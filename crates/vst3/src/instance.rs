//! A VST 3 effect running: its component and audio processor and, when
//! asked for, its edit controller (the component itself, or an object of
//! its own connected to it as the specification describes); its buses set
//! for the channels given, blocks of sound processed with the parameter
//! changes queued (ramps too, sample by sample) and the timeline's place,
//! tempo and meter, its programs, its states read and given back, and its
//! editor. The plug-in's functions that run sound or states are called
//! behind the crash guard (`guard.rs`): a module that crashed is never
//! called again, and its effects let the sound through.

#![allow(unsafe_code)]

use std::ffi::{CStr, c_void};
use std::mem::ManuallyDrop;
use std::sync::Arc;

use bv_vst3_sys::{
    AUDIO, AudioBusBuffers, BusInfo, IAudioProcessorVtbl, IComponentVtbl, IConnectionPointVtbl,
    IEditControllerVtbl, INPUT, IPlugViewVtbl, IUnitInfoVtbl, NO_PROGRAM_LIST, OUTPUT,
    ParameterInfo, ProcessContext, ProcessData, ProcessSetup, ProgramListInfo, REALTIME, Raw,
    SAMPLE32, UnitInfo, ViewRect, context, iid, param_flags, result, speaker, text_of16,
};

use crate::com::{ComPtr, Own};
use crate::guard;
use crate::host::{ComponentHandler, Edit, ParameterChanges, PlugFrame, Resize, Stream};
use crate::module::{ClassInfo, Error, Module};

/// A parameter, as the plug-in's controller describes it.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamInfo {
    pub id: u32,
    pub title: String,
    pub short_title: String,
    pub units: String,
    /// 0 continuous, 1 on or off, else how many steps.
    pub steps: i32,
    /// Its default (normalized, 0–1).
    pub default: f64,
    /// The unit it belongs to (0: the plug-in itself).
    pub unit: i32,
    pub flags: i32,
}

impl ParamInfo {
    /// Shown to the user (not hidden).
    pub fn shown(&self) -> bool {
        self.flags & param_flags::HIDDEN == 0
    }

    /// Shown but not changed from outside the plug-in.
    pub fn read_only(&self) -> bool {
        self.flags & param_flags::READ_ONLY != 0
    }

    /// The plug-in's bypass.
    pub fn bypass(&self) -> bool {
        self.flags & param_flags::BYPASS != 0
    }

    /// The parameter that chooses the plug-in's program.
    pub fn program_change(&self) -> bool {
        self.flags & param_flags::PROGRAM_CHANGE != 0
    }
}

/// Where the timeline is, for plug-ins that follow it (tempo-synced
/// delays, LFOs): playing or not, the project's tempo and meter, and the
/// loop region (in quarter notes) when looping.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transport {
    pub playing: bool,
    /// Beats per minute (0: none told).
    pub tempo: f64,
    pub numerator: i32,
    pub denominator: i32,
    pub cycle: Option<(f64, f64)>,
}

impl Default for Transport {
    /// Stopped, at 120 beats per minute in 4/4, as hosts tell when they
    /// know nothing else.
    fn default() -> Transport {
        Transport {
            playing: false,
            tempo: 120.0,
            numerator: 4,
            denominator: 4,
            cycle: None,
        }
    }
}

/// The controller, with what ties it to the component.
struct Controller {
    edit: ComPtr<IEditControllerVtbl>,
    /// An object of its own (not the component).
    separate: bool,
    /// The component's and the controller's connection points, connected.
    points: Option<(ComPtr<IConnectionPointVtbl>, ComPtr<IConnectionPointVtbl>)>,
    handler: Own<ComponentHandler>,
}

/// The processing set up: the rate, the longest block, each audio bus's
/// channels (inputs, then outputs) and their buffers; where the next block
/// lies on the timeline, and the samples processed since it started.
struct Run {
    block: usize,
    ins: Vec<Vec<Vec<f32>>>,
    outs: Vec<Vec<Vec<f32>>>,
    position: i64,
    time: i64,
}

/// A plug-in's editor (its view), shown in a window the caller gives.
pub struct Editor {
    view: ComPtr<IPlugViewVtbl>,
    frame: Own<PlugFrame>,
    attached: bool,
}

impl std::fmt::Debug for Editor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Editor")
            .field("attached", &self.attached)
            .finish()
    }
}

impl Editor {
    /// Whether it can live in a window of kind `platform` ("HWND"…).
    pub fn supports(&self, platform: &CStr) -> bool {
        // SAFETY: the view's own function.
        let r = unsafe {
            (self.view.vt().is_platform_type_supported)(self.view.raw(), platform.as_ptr())
        };
        r == result::OK
    }

    /// Its size (pixels).
    pub fn size(&self) -> Option<(i32, i32)> {
        let mut r = ViewRect::default();
        // SAFETY: the view's own function, a rectangle to fill.
        let ok = unsafe { (self.view.vt().get_size)(self.view.raw(), &mut r) } == result::OK;
        ok.then_some((r.right - r.left, r.bottom - r.top))
    }

    /// The user may resize it.
    pub fn resizable(&self) -> bool {
        // SAFETY: the view's own function.
        let r = unsafe { (self.view.vt().can_resize)(self.view.raw()) };
        r == result::OK
    }

    /// Shows it in window `parent`, of kind `platform`; `resize` resizes
    /// that window when the plug-in asks for another size.
    ///
    /// # Safety
    /// `parent` is a live window of kind `platform`, kept until
    /// [`Editor::detach`] (or the editor is dropped).
    pub unsafe fn attach(&mut self, parent: *mut c_void, platform: &CStr, resize: Resize) -> bool {
        if self.attached {
            return true;
        }
        self.frame.get().set_resize(Some(resize));
        // SAFETY: the caller's window.
        let ok = unsafe { (self.view.vt().attached)(self.view.raw(), parent, platform.as_ptr()) }
            == result::OK;
        self.attached = ok;
        ok
    }

    /// The user resized its window to `w` × `h`: the size it takes (the
    /// plug-in may change it).
    pub fn resized(&self, w: i32, h: i32) -> (i32, i32) {
        let mut r = ViewRect {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        };
        // SAFETY: the view's own functions, a rectangle each.
        unsafe {
            (self.view.vt().check_size_constraint)(self.view.raw(), &mut r);
            (self.view.vt().on_size)(self.view.raw(), &mut r);
        }
        (r.right - r.left, r.bottom - r.top)
    }

    /// Takes it out of its window (before the window goes).
    pub fn detach(&mut self) {
        if std::mem::take(&mut self.attached) {
            // SAFETY: the view's own function.
            unsafe { (self.view.vt().removed)(self.view.raw()) };
        }
        self.frame.get().set_resize(None);
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        self.detach();
        // SAFETY: the view lets go of the frame.
        unsafe { (self.view.vt().set_frame)(self.view.raw(), std::ptr::null_mut()) };
    }
}

/// A plug-in effect.
pub struct Instance {
    /// Dropped first: a view never outlives its controller.
    editor: Option<Editor>,
    module: Arc<Module>,
    class: ClassInfo,
    /// Let go of in `drop` (never when its module crashed: its code is not
    /// called again, not even to release).
    component: ManuallyDrop<ComPtr<IComponentVtbl>>,
    processor: ManuallyDrop<ComPtr<IAudioProcessorVtbl>>,
    controller: Option<Controller>,
    changes: Own<ParameterChanges>,
    /// Parameter changes for the next block (normalized): values from its
    /// first sample, and ramps (parameter, sample in the block, value).
    pending: Vec<(u32, f64)>,
    ramps: Vec<(u32, usize, f64)>,
    run: Option<Run>,
    transport: Transport,
    context: Box<ProcessContext>,
}

impl std::fmt::Debug for Instance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Instance")
            .field("class", &self.class.name)
            .field("running", &self.run.is_some())
            .finish()
    }
}

// SAFETY: an instance is used by one thread at a time (moving it between
// threads is what hosts do with the processing part).
unsafe impl Send for Instance {}

/// What `get_state` writes, read from a new stream; `None` when it fails or
/// crashes (`call` runs the plug-in's getState behind the guard with the
/// stream's pointer).
fn read_state(call: impl FnOnce(Raw) -> Option<i32>) -> Option<Vec<u8>> {
    let stream = Stream::new(Vec::new());
    (call(stream.raw(0)) == Some(result::OK)).then(|| stream.get().bytes())
}

impl Instance {
    /// Makes effect `class` of `module`, its controller too when
    /// `controller` (the parameters' names, values and texts, the programs
    /// and the editor need it).
    pub fn new(
        module: &Arc<Module>,
        class: &ClassInfo,
        controller: bool,
    ) -> Result<Instance, Error> {
        if module.crashed() {
            return Err(Error::Crashed);
        }
        let component: ComPtr<IComponentVtbl> = module
            .create(&class.cid, &iid::COMPONENT)
            .ok_or(Error::Refused("to make its component", 0))?;
        let host = module.host.raw(0);
        // SAFETY: initialize with the host application, which the module
        // keeps as long as the instance holds the module.
        let r = unsafe { guard::pp(component.vt().base.initialize, component.raw(), host) };
        match r {
            Ok(result::OK) => {}
            Ok(r) => return Err(Error::Refused("to start", r)),
            Err((code, at)) => {
                guard::crashed(module, &class.name, "initialize", code, at);
                std::mem::forget(component);
                return Err(Error::Crashed);
            }
        }
        let Some(processor) = component.query::<IAudioProcessorVtbl>(&iid::AUDIO_PROCESSOR) else {
            // SAFETY: undoing initialize.
            let _ = unsafe { guard::p(component.vt().base.terminate, component.raw()) };
            return Err(Error::Refused("to process sound", 0));
        };
        let mut inst = Instance {
            editor: None,
            module: Arc::clone(module),
            class: class.clone(),
            component: ManuallyDrop::new(component),
            processor: ManuallyDrop::new(processor),
            controller: None,
            changes: ParameterChanges::new(),
            pending: Vec::new(),
            ramps: Vec::new(),
            run: None,
            transport: Transport::default(),
            context: Box::default(),
        };
        if controller {
            inst.controller = inst.make_controller();
        }
        Ok(inst)
    }

    /// Calls a plug-in's function of an object and a pointer behind the
    /// guard, doing `doing`: its result, or `None` when the module is
    /// switched off or crashes now.
    ///
    /// # Safety
    /// `f` is `this`'s function, `arg` what it takes.
    unsafe fn call<A>(
        &self,
        doing: &'static str,
        f: unsafe extern "system" fn(Raw, *mut A) -> i32,
        this: Raw,
        arg: *mut A,
    ) -> Option<i32> {
        if self.module.crashed() {
            return None;
        }
        // SAFETY: the caller's.
        match unsafe { guard::pp(f, this, arg) } {
            Ok(r) => Some(r),
            Err((code, at)) => {
                guard::crashed(&self.module, &self.class.name, doing, code, at);
                None
            }
        }
    }

    /// [`Instance::call`] for a function of an object and a byte.
    ///
    /// # Safety
    /// `f` is `this`'s function.
    unsafe fn call_byte(
        &self,
        doing: &'static str,
        f: unsafe extern "system" fn(Raw, u8) -> i32,
        this: Raw,
        arg: u8,
    ) -> Option<i32> {
        if self.module.crashed() {
            return None;
        }
        // SAFETY: the caller's.
        match unsafe { guard::pb(f, this, arg) } {
            Ok(r) => Some(r),
            Err((code, at)) => {
                guard::crashed(&self.module, &self.class.name, doing, code, at);
                None
            }
        }
    }

    fn make_controller(&self) -> Option<Controller> {
        let comp = &self.component;
        let host = self.module.host.raw(0);
        let (edit, separate) = match comp.query::<IEditControllerVtbl>(&iid::EDIT_CONTROLLER) {
            Some(edit) => (edit, false),
            None => {
                let mut cid = [0u8; 16];
                // SAFETY: the component's own function, 16 bytes to fill.
                let r = unsafe { (comp.vt().get_controller_class_id)(comp.raw(), &mut cid) };
                if r != result::OK || cid == [0; 16] {
                    return None;
                }
                let edit: ComPtr<IEditControllerVtbl> =
                    self.module.create(&cid, &iid::EDIT_CONTROLLER)?;
                // SAFETY: initialize with the host application.
                let r = unsafe {
                    self.call(
                        "the controller's initialize",
                        edit.vt().base.initialize,
                        edit.raw(),
                        host,
                    )
                };
                if r != Some(result::OK) {
                    return None;
                }
                (edit, true)
            }
        };
        let handler = ComponentHandler::new();
        // SAFETY: the controller's own functions; the handler stays while
        // the controller holds it (it counts its reference).
        unsafe { (edit.vt().set_component_handler)(edit.raw(), handler.raw(0)) };
        // Two objects talk through their connection points.
        let points = if separate {
            let a = comp.query::<IConnectionPointVtbl>(&iid::CONNECTION_POINT);
            let b = edit.query::<IConnectionPointVtbl>(&iid::CONNECTION_POINT);
            match (a, b) {
                (Some(a), Some(b)) => {
                    // SAFETY: each point's own connect, with the other.
                    unsafe {
                        (a.vt().connect)(a.raw(), b.raw());
                        (b.vt().connect)(b.raw(), a.raw());
                    }
                    Some((a, b))
                }
                _ => None,
            }
        } else {
            None
        };
        // The controller learns the component's state.
        // SAFETY: getState and setComponentState with streams of the host.
        let state =
            read_state(|s| unsafe { self.call("getState", comp.vt().get_state, comp.raw(), s) });
        if let Some(state) = state {
            let stream = Stream::new(state);
            // SAFETY: as above.
            unsafe {
                self.call(
                    "setComponentState",
                    edit.vt().set_component_state,
                    edit.raw(),
                    stream.raw(0),
                )
            };
        }
        Some(Controller {
            edit,
            separate,
            points,
            handler,
        })
    }

    pub fn class(&self) -> &ClassInfo {
        &self.class
    }

    pub fn module(&self) -> &Arc<Module> {
        &self.module
    }

    /// Whether its module crashed (it is switched off: sound goes through
    /// untouched).
    pub fn crashed(&self) -> bool {
        self.module.crashed()
    }

    /// Whether it has a controller.
    pub fn has_controller(&self) -> bool {
        self.controller.is_some()
    }

    /// Sets it up to process `channels` channels (its main buses set to
    /// them when it takes that; else its own) at `rate`, in blocks of at
    /// most `block` samples; then it runs.
    pub fn setup(&mut self, rate: f64, block: usize, channels: usize) -> Result<(), Error> {
        self.stop();
        if self.module.crashed() {
            return Err(Error::Crashed);
        }
        let comp = &self.component;
        let proc_ = &self.processor;
        // SAFETY: the component's and the processor's own functions, with
        // arrays as long as the counts given and structures to fill.
        let (ins, outs) = unsafe {
            let count = |dir| (comp.vt().get_bus_count)(comp.raw(), AUDIO, dir).max(0) as usize;
            let (n_in, n_out) = (count(INPUT), count(OUTPUT));
            if n_out == 0 {
                return Err(Error::Refused("to give sound out (it has no outputs)", 0));
            }
            let want = match channels {
                1 => speaker::MONO,
                6 => speaker::SURROUND_51,
                _ => speaker::STEREO,
            };
            let current = |dir, i: usize| {
                let mut a = 0u64;
                (proc_.vt().get_bus_arrangement)(proc_.raw(), dir, i as i32, &mut a);
                a
            };
            let mut ins: Vec<u64> = (0..n_in)
                .map(|i| if i == 0 { want } else { current(INPUT, i) })
                .collect();
            let mut outs: Vec<u64> = (0..n_out)
                .map(|i| if i == 0 { want } else { current(OUTPUT, i) })
                .collect();
            (proc_.vt().set_bus_arrangements)(
                proc_.raw(),
                if ins.is_empty() {
                    std::ptr::null_mut()
                } else {
                    ins.as_mut_ptr()
                },
                n_in as i32,
                outs.as_mut_ptr(),
                n_out as i32,
            );
            // What it took.
            let channels_of = |dir, i: usize| {
                let mut info: BusInfo = std::mem::zeroed();
                if (comp.vt().get_bus_info)(comp.raw(), AUDIO, dir, i as i32, &mut info)
                    == result::OK
                {
                    info.channel_count.max(0) as usize
                } else {
                    0
                }
            };
            let ins: Vec<usize> = (0..n_in).map(|i| channels_of(INPUT, i)).collect();
            let outs: Vec<usize> = (0..n_out).map(|i| channels_of(OUTPUT, i)).collect();
            if n_in > 0 {
                (comp.vt().activate_bus)(comp.raw(), AUDIO, INPUT, 0, 1);
            }
            (comp.vt().activate_bus)(comp.raw(), AUDIO, OUTPUT, 0, 1);
            let r = (proc_.vt().can_process_sample_size)(proc_.raw(), SAMPLE32);
            if r != result::OK {
                return Err(Error::Refused("32-bit samples", r));
            }
            (ins, outs)
        };
        let mut setup = ProcessSetup {
            process_mode: REALTIME,
            symbolic_sample_size: SAMPLE32,
            max_samples_per_block: block as i32,
            sample_rate: rate,
        };
        // SAFETY: the processor's and the component's own functions, a
        // structure to read.
        unsafe {
            match self.call(
                "setupProcessing",
                proc_.vt().setup_processing,
                proc_.raw(),
                &mut setup,
            ) {
                Some(result::OK) => {}
                Some(r) => return Err(Error::Refused("the sample rate or the block size", r)),
                None => return Err(Error::Crashed),
            }
            match self.call_byte("setActive", comp.vt().set_active, comp.raw(), 1) {
                Some(result::OK) => {}
                Some(r) => return Err(Error::Refused("to start processing", r)),
                None => return Err(Error::Crashed),
            }
            // Hosts may skip setProcessing's answer (NotImplemented).
            if self
                .call_byte("setProcessing", proc_.vt().set_processing, proc_.raw(), 1)
                .is_none()
            {
                return Err(Error::Crashed);
            }
        }
        let buffers = |counts: &[usize]| -> Vec<Vec<Vec<f32>>> {
            counts.iter().map(|&n| vec![vec![0.0; block]; n]).collect()
        };
        self.context.sample_rate = rate;
        self.run = Some(Run {
            block,
            ins: buffers(&ins),
            outs: buffers(&outs),
            position: 0,
            time: 0,
        });
        Ok(())
    }

    /// Stops processing (it starts again with [`Instance::setup`]).
    pub fn stop(&mut self) {
        if self.run.take().is_some() {
            // SAFETY: the processor's and the component's own functions.
            unsafe {
                let (p, c) = (&self.processor, &self.component);
                self.call_byte("setProcessing", p.vt().set_processing, p.raw(), 0);
                self.call_byte("setActive", c.vt().set_active, c.raw(), 0);
            }
        }
    }

    /// Whether it is set up.
    pub fn running(&self) -> bool {
        self.run.is_some()
    }

    /// Its main buses' channels (in, out) as set up; none before.
    pub fn main_channels(&self) -> (usize, usize) {
        self.run.as_ref().map_or((0, 0), |r| {
            (
                r.ins.first().map_or(0, Vec::len),
                r.outs.first().map_or(0, Vec::len),
            )
        })
    }

    /// The timeline's transport plug-ins are told from the next block on.
    pub fn set_transport(&mut self, t: Transport) {
        self.transport = t;
    }

    /// Where the next block's first sample lies on the timeline (samples
    /// from its start): its place, as plug-ins read it in quarter notes
    /// too.
    pub fn locate(&mut self, position: i64) {
        if let Some(run) = &mut self.run {
            run.position = position;
        }
    }

    /// The process context for a block starting `position` samples into
    /// the timeline, `time` into the processing.
    fn fill_context(&mut self, position: i64, time: i64) {
        let t = self.transport;
        let c = &mut *self.context;
        c.project_time_samples = position;
        c.continous_time_samples = time;
        c.state = context::CONT_TIME_VALID;
        if t.playing {
            c.state |= context::PLAYING;
        }
        if t.tempo > 0.0 && c.sample_rate > 0.0 {
            let quarters = position as f64 / c.sample_rate * t.tempo / 60.0;
            c.tempo = t.tempo;
            c.project_time_music = quarters;
            c.state |= context::TEMPO_VALID | context::PROJECT_TIME_MUSIC_VALID;
            if t.numerator > 0 && t.denominator > 0 {
                c.time_sig_numerator = t.numerator;
                c.time_sig_denominator = t.denominator;
                let bar = f64::from(t.numerator) * 4.0 / f64::from(t.denominator);
                c.bar_position_music = (quarters / bar).floor() * bar;
                c.state |= context::TIME_SIG_VALID | context::BAR_POSITION_VALID;
            }
            if let Some((a, b)) = t.cycle {
                c.cycle_start_music = a;
                c.cycle_end_music = b;
                c.state |= context::CYCLE_VALID | context::CYCLE_ACTIVE;
            }
        }
    }

    /// Processes `channels` (planar, the first ones the main bus's) in
    /// place, in blocks; the parameter changes queued go with the first,
    /// the ramps' points each with its block. Channels it has no output
    /// for stay as they are; one channel through a stereo plug-in is both
    /// its inputs and the mean of its outputs. A module that crashed (now
    /// or before) leaves the sound as it came.
    pub fn process(&mut self, channels: &mut [Vec<f32>]) {
        if self.run.is_none() || self.module.crashed() {
            return;
        }
        let host = channels.len();
        let n = channels.first().map_or(0, Vec::len);
        if host == 0 || n == 0 {
            return;
        }
        let pending = std::mem::take(&mut self.pending);
        let ramps = std::mem::take(&mut self.ramps);
        let mut start = 0;
        while start < n {
            let Some(run) = self.run.as_mut() else {
                return;
            };
            let len = (n - start).min(run.block);
            if let Some(main) = run.ins.first_mut() {
                for (k, buf) in main.iter_mut().enumerate() {
                    buf[..len].copy_from_slice(&channels[k.min(host - 1)][start..start + len]);
                }
            }
            let (position, time) = (run.position, run.time);
            // This block's points: the values queued from its first sample
            // (the first block's), the ramps' that fall in it.
            let mut points: Vec<(u32, i32, f64)> = if start == 0 {
                pending.iter().map(|&(id, v)| (id, 0, v)).collect()
            } else {
                Vec::new()
            };
            points.extend(
                ramps
                    .iter()
                    .filter(|&&(_, at, _)| at >= start && at < start + len)
                    .map(|&(id, at, v)| (id, (at - start) as i32, v)),
            );
            self.changes.get().set_points(&points);
            self.fill_context(position, time);
            let Some(run) = self.run.as_mut() else {
                return;
            };
            let mut in_ptrs: Vec<Vec<*mut f32>> = run
                .ins
                .iter_mut()
                .map(|b| b.iter_mut().map(|c| c.as_mut_ptr()).collect())
                .collect();
            let mut out_ptrs: Vec<Vec<*mut f32>> = run
                .outs
                .iter_mut()
                .map(|b| b.iter_mut().map(|c| c.as_mut_ptr()).collect())
                .collect();
            let bus = |p: &mut Vec<*mut f32>| AudioBusBuffers {
                num_channels: p.len() as i32,
                silence_flags: 0,
                buffers: p.as_mut_ptr(),
            };
            let mut in_buses: Vec<AudioBusBuffers> = in_ptrs.iter_mut().map(bus).collect();
            let mut out_buses: Vec<AudioBusBuffers> = out_ptrs.iter_mut().map(bus).collect();
            let mut data = ProcessData {
                process_mode: REALTIME,
                symbolic_sample_size: SAMPLE32,
                num_samples: len as i32,
                num_inputs: in_buses.len() as i32,
                num_outputs: out_buses.len() as i32,
                inputs: if in_buses.is_empty() {
                    std::ptr::null_mut()
                } else {
                    in_buses.as_mut_ptr()
                },
                outputs: out_buses.as_mut_ptr(),
                input_parameter_changes: self.changes.raw(0),
                output_parameter_changes: std::ptr::null_mut(),
                input_events: std::ptr::null_mut(),
                output_events: std::ptr::null_mut(),
                process_context: &mut *self.context,
            };
            let p = &self.processor;
            // SAFETY: the processor's process, with buffers of `len`
            // samples for each channel of each bus, kept until it returns.
            let done = unsafe { self.call("process", p.vt().process, p.raw(), &mut data) };
            if done.is_none() {
                // Crashed: what is left goes through as it came.
                self.changes.get().set(&[]);
                return;
            }
            let Some(run) = self.run.as_mut() else {
                return;
            };
            let out = &run.outs[0];
            match (host, out.len()) {
                (_, 0) => {}
                (1, m) if m >= 2 => {
                    for (i, s) in channels[0][start..start + len].iter_mut().enumerate() {
                        *s = 0.5 * (out[0][i] + out[1][i]);
                    }
                }
                (h, m) => {
                    for k in 0..h.min(m) {
                        channels[k][start..start + len].copy_from_slice(&out[k][..len]);
                    }
                }
            }
            start += len;
            run.time += len as i64;
            run.position += len as i64;
        }
        self.changes.get().set(&[]);
    }

    /// Hands the queued parameter changes to the processor without sound
    /// (a block of no samples, as the specification allows): its state
    /// then has them.
    pub fn flush(&mut self) {
        if self.pending.is_empty() || self.run.is_none() {
            return;
        }
        self.changes.get().set(&self.pending);
        self.pending.clear();
        let position = self.run.as_ref().map_or(0, |r| r.position);
        let time = self.run.as_ref().map_or(0, |r| r.time);
        self.fill_context(position, time);
        let mut data = ProcessData {
            process_mode: REALTIME,
            symbolic_sample_size: SAMPLE32,
            num_samples: 0,
            num_inputs: 0,
            num_outputs: 0,
            inputs: std::ptr::null_mut(),
            outputs: std::ptr::null_mut(),
            input_parameter_changes: self.changes.raw(0),
            output_parameter_changes: std::ptr::null_mut(),
            input_events: std::ptr::null_mut(),
            output_events: std::ptr::null_mut(),
            process_context: &mut *self.context,
        };
        let p = &self.processor;
        // SAFETY: the processor's process, no buffers.
        unsafe { self.call("process", p.vt().process, p.raw(), &mut data) };
        self.changes.get().set(&[]);
    }

    /// Whether parameter changes wait for the next block.
    pub fn has_queued(&self) -> bool {
        !self.pending.is_empty() || !self.ramps.is_empty()
    }

    /// Queues parameter `id`'s new value (normalized) for the processor.
    pub fn queue(&mut self, id: u32, value: f64) {
        self.pending.retain(|&(p, _)| p != id);
        self.pending.push((id, value.clamp(0.0, 1.0)));
    }

    /// Queues parameter `id`'s values along the next [`Instance::process`]
    /// call's samples (sample, normalized value): the plug-in goes from
    /// one to the next, sample by sample — automation as smooth as its
    /// envelope, not in steps.
    pub fn queue_ramp(&mut self, id: u32, points: &[(usize, f64)]) {
        self.ramps.retain(|&(p, _, _)| p != id);
        self.ramps
            .extend(points.iter().map(|&(at, v)| (id, at, v.clamp(0.0, 1.0))));
    }

    /// Sets parameter `id` (normalized): the controller and, with the
    /// next block (or [`Instance::flush`]), the processor.
    pub fn set(&mut self, id: u32, value: f64) {
        let value = value.clamp(0.0, 1.0);
        if let Some(c) = &self.controller
            && !self.module.crashed()
        {
            // SAFETY: the controller's own function.
            unsafe { (c.edit.vt().set_param_normalized)(c.edit.raw(), id, value) };
        }
        self.queue(id, value);
    }

    /// The parameters (with a controller).
    pub fn params(&self) -> Vec<ParamInfo> {
        let Some(c) = self.controller.as_ref().filter(|_| !self.module.crashed()) else {
            return Vec::new();
        };
        // SAFETY: the controller's own functions, a structure to fill.
        unsafe {
            let n = (c.edit.vt().get_parameter_count)(c.edit.raw()).max(0);
            (0..n)
                .filter_map(|i| {
                    let mut p: ParameterInfo = std::mem::zeroed();
                    ((c.edit.vt().get_parameter_info)(c.edit.raw(), i, &mut p) == result::OK).then(
                        || ParamInfo {
                            id: p.id,
                            title: text_of16(&p.title),
                            short_title: text_of16(&p.short_title),
                            units: text_of16(&p.units),
                            steps: p.step_count,
                            default: p.default_normalized_value,
                            unit: p.unit_id,
                            flags: p.flags,
                        },
                    )
                })
                .collect()
        }
    }

    /// Parameter `id`'s value (normalized), as the controller has it.
    pub fn value(&self, id: u32) -> Option<f64> {
        let c = self
            .controller
            .as_ref()
            .filter(|_| !self.module.crashed())?;
        // SAFETY: the controller's own function.
        Some(unsafe { (c.edit.vt().get_param_normalized)(c.edit.raw(), id) })
    }

    /// Value `value` of parameter `id` as the plug-in writes it.
    pub fn text(&self, id: u32, value: f64) -> Option<String> {
        let c = self
            .controller
            .as_ref()
            .filter(|_| !self.module.crashed())?;
        let mut s = [0u16; 128];
        // SAFETY: the controller's own function, a String128 to fill.
        let r = unsafe {
            (c.edit.vt().get_param_string_by_value)(c.edit.raw(), id, value, s.as_mut_ptr())
        };
        (r == result::OK).then(|| text_of16(&s))
    }

    /// The parameter that chooses the plug-in's program, and how many
    /// programs it has.
    fn program_param(&self) -> Option<(ParamInfo, usize)> {
        let p = self.params().into_iter().find(ParamInfo::program_change)?;
        let count = usize::try_from(p.steps).ok().filter(|&s| s > 0)? + 1;
        Some((p, count))
    }

    /// The program list of unit `unit` (its id and its programs' count),
    /// as the controller's units tell.
    fn program_list(&self, unit: i32) -> Option<(i32, usize)> {
        let c = self.controller.as_ref()?;
        let units: ComPtr<IUnitInfoVtbl> = c.edit.query(&iid::UNIT_INFO)?;
        // SAFETY: the units' own functions, structures to fill.
        unsafe {
            let list = (0..(units.vt().get_unit_count)(units.raw()))
                .filter_map(|i| {
                    let mut u: UnitInfo = std::mem::zeroed();
                    ((units.vt().get_unit_info)(units.raw(), i, &mut u) == result::OK).then_some(u)
                })
                .find(|u| u.id == unit && u.program_list_id != NO_PROGRAM_LIST)?
                .program_list_id;
            (0..(units.vt().get_program_list_count)(units.raw())).find_map(|i| {
                let mut l: ProgramListInfo = std::mem::zeroed();
                ((units.vt().get_program_list_info)(units.raw(), i, &mut l) == result::OK
                    && l.id == list)
                    .then(|| (list, l.program_count.max(0) as usize))
            })
        }
    }

    /// The plug-in's programs (the presets it holds), by name, in order:
    /// those of the program list of the unit its program parameter belongs
    /// to, else that parameter's texts. None without a program parameter.
    pub fn programs(&self) -> Vec<String> {
        let Some((p, count)) = self.program_param() else {
            return Vec::new();
        };
        let named = self.program_list(p.unit).and_then(|(list, n)| {
            let c = self.controller.as_ref()?;
            let units: ComPtr<IUnitInfoVtbl> = c.edit.query(&iid::UNIT_INFO)?;
            Some(
                (0..n.min(count))
                    .map(|i| {
                        let mut s = [0u16; 128];
                        // SAFETY: the units' own function, a String128 to
                        // fill.
                        let r = unsafe {
                            (units.vt().get_program_name)(
                                units.raw(),
                                list,
                                i as i32,
                                s.as_mut_ptr(),
                            )
                        };
                        if r == result::OK {
                            text_of16(&s)
                        } else {
                            String::new()
                        }
                    })
                    .collect::<Vec<String>>(),
            )
        });
        match named {
            Some(names) if !names.is_empty() => names,
            _ => (0..count)
                .map(|i| {
                    self.text(p.id, i as f64 / (count - 1) as f64)
                        .unwrap_or_else(|| format!("{}", i + 1))
                })
                .collect(),
        }
    }

    /// The program chosen now.
    pub fn program(&self) -> Option<usize> {
        let (p, count) = self.program_param()?;
        let v = self.value(p.id)?;
        Some(((v * (count - 1) as f64).round().max(0.0) as usize).min(count - 1))
    }

    /// Chooses program `index`: the program parameter set (the controller
    /// and the processor, which loads it). Whether the plug-in has it.
    pub fn set_program(&mut self, index: usize) -> bool {
        let Some((p, count)) = self.program_param() else {
            return false;
        };
        if index >= count {
            return false;
        }
        self.set(p.id, index as f64 / (count - 1) as f64);
        self.flush();
        true
    }

    /// What the controller reported since last asked (its editor's
    /// edits).
    pub fn take_edits(&self) -> Vec<Edit> {
        self.controller
            .as_ref()
            .map(|c| c.handler.get().take())
            .unwrap_or_default()
    }

    /// The component's state.
    pub fn state(&self) -> Option<Vec<u8>> {
        let comp = &self.component;
        // SAFETY: getState with a stream of the host.
        read_state(|s| unsafe { self.call("getState", comp.vt().get_state, comp.raw(), s) })
    }

    /// The controller's own state.
    pub fn controller_state(&self) -> Option<Vec<u8>> {
        let c = self.controller.as_ref()?;
        // SAFETY: getState with a stream of the host.
        read_state(|s| unsafe {
            self.call(
                "the controller's getState",
                c.edit.vt().get_state,
                c.edit.raw(),
                s,
            )
        })
    }

    /// Gives it states it had: the component's (also the controller's to
    /// read) and the controller's own (none when empty).
    pub fn set_state(&mut self, component: &[u8], controller: &[u8]) -> bool {
        if component.is_empty() {
            return false;
        }
        let comp = &self.component;
        let stream = Stream::new(component.to_vec());
        // SAFETY: setState and setComponentState with streams of the host.
        let ok = unsafe { self.call("setState", comp.vt().set_state, comp.raw(), stream.raw(0)) }
            == Some(result::OK);
        if let Some(c) = &self.controller {
            let stream = Stream::new(component.to_vec());
            // SAFETY: as above.
            unsafe {
                self.call(
                    "setComponentState",
                    c.edit.vt().set_component_state,
                    c.edit.raw(),
                    stream.raw(0),
                );
                if !controller.is_empty() {
                    let stream = Stream::new(controller.to_vec());
                    self.call(
                        "the controller's setState",
                        c.edit.vt().set_state,
                        c.edit.raw(),
                        stream.raw(0),
                    );
                }
            }
        }
        ok
    }

    /// Its latency (samples).
    pub fn latency(&self) -> u32 {
        if self.module.crashed() {
            return 0;
        }
        // SAFETY: the processor's own function.
        unsafe { (self.processor.vt().get_latency_samples)(self.processor.raw()) }
    }

    /// How long its sound goes on after its input stops (samples; `None`
    /// for ever, as an infinite reverb or a looper).
    pub fn tail(&self) -> Option<u32> {
        if self.module.crashed() {
            return Some(0);
        }
        // SAFETY: the processor's own function.
        let t = unsafe { (self.processor.vt().get_tail_samples)(self.processor.raw()) };
        (t != bv_vst3_sys::INFINITE_TAIL).then_some(t)
    }

    /// Its editor, made the first time (with a controller that has one).
    pub fn editor(&mut self) -> Option<&mut Editor> {
        if self.editor.is_none() {
            if self.module.crashed() {
                return None;
            }
            let c = self.controller.as_ref()?;
            // SAFETY: createView("editor"); the view comes with a
            // reference of ours.
            let view: ComPtr<IPlugViewVtbl> = unsafe {
                ComPtr::from_raw((c.edit.vt().create_view)(
                    c.edit.raw(),
                    bv_vst3_sys::VIEW_EDITOR.as_ptr(),
                ))
            }?;
            let frame = PlugFrame::new();
            // SAFETY: the view's own function, the frame counted by it.
            unsafe { (view.vt().set_frame)(view.raw(), frame.raw(0)) };
            self.editor = Some(Editor {
                view,
                frame,
                attached: false,
            });
        }
        self.editor.as_mut()
    }

    /// Whether its controller makes an editor (one is made and closed).
    pub fn has_editor(&mut self) -> bool {
        let made = self.editor().is_some();
        self.close_editor();
        made
    }

    /// Closes its editor (after taking it out of its window).
    pub fn close_editor(&mut self) {
        self.editor = None;
    }
}

impl Instance {
    /// Ends `this` (its terminate, `f`) behind the guard.
    ///
    /// # Safety
    /// `f` is `this`'s terminate.
    unsafe fn end(&self, doing: &'static str, f: unsafe extern "system" fn(Raw) -> i32, this: Raw) {
        if self.module.crashed() {
            return;
        }
        // SAFETY: the caller's.
        if let Err((code, at)) = unsafe { guard::p(f, this) } {
            guard::crashed(&self.module, &self.class.name, doing, code, at);
        }
    }

    /// Its objects left as they are when its module crashed: none of its
    /// code is called again, not even to let go of them.
    fn abandon(&mut self) {
        if let Some(e) = self.editor.take() {
            std::mem::forget(e);
        }
        if let Some(c) = self.controller.take() {
            std::mem::forget(c);
        }
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        if !self.module.crashed() {
            self.editor = None;
            self.stop();
        }
        if self.module.crashed() {
            self.abandon();
            return;
        }
        if let Some(c) = self.controller.take() {
            // SAFETY: the plug-in's own functions, undoing what was done.
            unsafe {
                if let Some((a, b)) = &c.points {
                    (a.vt().disconnect)(a.raw(), b.raw());
                    (b.vt().disconnect)(b.raw(), a.raw());
                }
                (c.edit.vt().set_component_handler)(c.edit.raw(), std::ptr::null_mut());
                if c.separate {
                    self.end(
                        "the controller's terminate",
                        c.edit.vt().base.terminate,
                        c.edit.raw(),
                    );
                }
            }
            if self.module.crashed() {
                std::mem::forget(c);
                return;
            }
        }
        // SAFETY: the component's own function.
        unsafe {
            self.end(
                "terminate",
                self.component.vt().base.terminate,
                self.component.raw(),
            );
        }
        if !self.module.crashed() {
            // SAFETY: dropped once, here; the processor before the
            // component it came from.
            unsafe {
                ManuallyDrop::drop(&mut self.processor);
                ManuallyDrop::drop(&mut self.component);
            }
        }
    }
}
