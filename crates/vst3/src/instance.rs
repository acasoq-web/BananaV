//! A VST 3 effect running: its component and audio processor and, when
//! asked for, its edit controller (the component itself, or an object of
//! its own connected to it as the specification describes); its buses set
//! for the channels given, blocks of sound processed with the parameter
//! changes queued, its states read and given back, and its editor.

#![allow(unsafe_code)]

use std::ffi::{CStr, c_void};
use std::sync::Arc;

use bv_vst3_sys::{
    AUDIO, AudioBusBuffers, BusInfo, IAudioProcessorVtbl, IComponentVtbl, IConnectionPointVtbl,
    IEditControllerVtbl, INPUT, IPlugViewVtbl, OUTPUT, ParameterInfo, ProcessContext, ProcessData,
    ProcessSetup, REALTIME, Raw, SAMPLE32, ViewRect, context, iid, param_flags, result, speaker,
    text_of16,
};

use crate::com::{ComPtr, Own};
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
/// channels (inputs, then outputs) and their buffers.
struct Run {
    block: usize,
    ins: Vec<Vec<Vec<f32>>>,
    outs: Vec<Vec<Vec<f32>>>,
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
    component: ComPtr<IComponentVtbl>,
    processor: ComPtr<IAudioProcessorVtbl>,
    controller: Option<Controller>,
    changes: Own<ParameterChanges>,
    /// Parameter changes for the next block (normalized).
    pending: Vec<(u32, f64)>,
    run: Option<Run>,
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

/// What `get_state` writes, read from a new stream.
///
/// # Safety
/// `get` calls a plug-in's getState with the stream's pointer.
unsafe fn read_state(get: impl FnOnce(Raw) -> i32) -> Option<Vec<u8>> {
    let stream = Stream::new(Vec::new());
    (get(stream.raw(0)) == result::OK).then(|| stream.get().bytes())
}

impl Instance {
    /// Makes effect `class` of `module`, its controller too when
    /// `controller` (the parameters' names, values and texts, and the
    /// editor, need it).
    pub fn new(
        module: &Arc<Module>,
        class: &ClassInfo,
        controller: bool,
    ) -> Result<Instance, Error> {
        let component: ComPtr<IComponentVtbl> = module
            .create(&class.cid, &iid::COMPONENT)
            .ok_or(Error::Refused("to make its component", 0))?;
        let host = module.host.raw(0);
        // SAFETY: initialize with the host application, which the module
        // keeps as long as the instance holds the module.
        let r = unsafe { (component.vt().base.initialize)(component.raw(), host) };
        if r != result::OK {
            return Err(Error::Refused("to start", r));
        }
        let Some(processor) = component.query::<IAudioProcessorVtbl>(&iid::AUDIO_PROCESSOR) else {
            // SAFETY: undoing initialize.
            unsafe { (component.vt().base.terminate)(component.raw()) };
            return Err(Error::Refused("to process sound", 0));
        };
        let mut inst = Instance {
            editor: None,
            module: Arc::clone(module),
            class: class.clone(),
            component,
            processor,
            controller: None,
            changes: ParameterChanges::new(),
            pending: Vec::new(),
            run: None,
            context: Box::default(),
        };
        if controller {
            inst.controller = inst.make_controller();
        }
        Ok(inst)
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
                if unsafe { (edit.vt().base.initialize)(edit.raw(), host) } != result::OK {
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
        unsafe {
            if let Some(state) = read_state(|s| (comp.vt().get_state)(comp.raw(), s)) {
                let stream = Stream::new(state);
                (edit.vt().set_component_state)(edit.raw(), stream.raw(0));
            }
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

    /// Whether it has a controller.
    pub fn has_controller(&self) -> bool {
        self.controller.is_some()
    }

    /// Sets it up to process `channels` channels (its main buses set to
    /// them when it takes that; else its own) at `rate`, in blocks of at
    /// most `block` samples; then it runs.
    pub fn setup(&mut self, rate: f64, block: usize, channels: usize) -> Result<(), Error> {
        self.stop();
        let comp = &self.component;
        let proc_ = &self.processor;
        // SAFETY: the component's and the processor's own functions, with
        // arrays as long as the counts given and structures to fill.
        unsafe {
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
            let mut setup = ProcessSetup {
                process_mode: REALTIME,
                symbolic_sample_size: SAMPLE32,
                max_samples_per_block: block as i32,
                sample_rate: rate,
            };
            let r = (proc_.vt().setup_processing)(proc_.raw(), &mut setup);
            if r != result::OK {
                return Err(Error::Refused("the sample rate or the block size", r));
            }
            let r = (comp.vt().set_active)(comp.raw(), 1);
            if r != result::OK {
                return Err(Error::Refused("to start processing", r));
            }
            // Hosts may skip setProcessing's answer (NotImplemented).
            (proc_.vt().set_processing)(proc_.raw(), 1);
            let buffers = |counts: &[usize]| -> Vec<Vec<Vec<f32>>> {
                counts.iter().map(|&n| vec![vec![0.0; block]; n]).collect()
            };
            self.context.sample_rate = rate;
            self.run = Some(Run {
                block,
                ins: buffers(&ins),
                outs: buffers(&outs),
                time: 0,
            });
        }
        Ok(())
    }

    /// Stops processing (it starts again with [`Instance::setup`]).
    pub fn stop(&mut self) {
        if self.run.take().is_some() {
            // SAFETY: the processor's and the component's own functions.
            unsafe {
                (self.processor.vt().set_processing)(self.processor.raw(), 0);
                (self.component.vt().set_active)(self.component.raw(), 0);
            }
        }
    }

    /// Whether it is set up.
    pub fn running(&self) -> bool {
        self.run.is_some()
    }

    /// The tempo and transport plug-ins are told.
    pub fn set_transport(&mut self, tempo: f64, playing: bool) {
        self.context.tempo = tempo;
        self.context.state = if playing { context::PLAYING } else { 0 };
    }

    /// Processes `channels` (planar, the first ones the main bus's) in
    /// place, in blocks; the parameter changes queued go with the first.
    /// Channels it has no output for stay as they are; one channel through
    /// a stereo plug-in is both its inputs and the mean of its outputs.
    pub fn process(&mut self, channels: &mut [Vec<f32>]) {
        let Instance {
            run,
            changes,
            pending,
            context,
            processor,
            ..
        } = self;
        let Some(run) = run.as_mut() else {
            return;
        };
        let host = channels.len();
        let n = channels.first().map_or(0, Vec::len);
        if host == 0 || n == 0 {
            return;
        }
        let mut start = 0;
        while start < n {
            let len = (n - start).min(run.block);
            if let Some(main) = run.ins.first_mut() {
                for (k, buf) in main.iter_mut().enumerate() {
                    buf[..len].copy_from_slice(&channels[k.min(host - 1)][start..start + len]);
                }
            }
            changes
                .get()
                .set(if start == 0 { pending.as_slice() } else { &[] });
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
            context.project_time_samples = run.time;
            context.continous_time_samples = run.time;
            context.state |= context::CONT_TIME_VALID;
            if context.tempo > 0.0 {
                context.state |= context::TEMPO_VALID;
            }
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
                input_parameter_changes: changes.raw(0),
                output_parameter_changes: std::ptr::null_mut(),
                input_events: std::ptr::null_mut(),
                output_events: std::ptr::null_mut(),
                process_context: &mut **context,
            };
            // SAFETY: the processor's process, with buffers of `len`
            // samples for each channel of each bus, kept until it returns.
            unsafe { (processor.vt().process)(processor.raw(), &mut data) };
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
        }
        pending.clear();
        changes.get().set(&[]);
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
        // SAFETY: the processor's process, no buffers.
        unsafe { (self.processor.vt().process)(self.processor.raw(), &mut data) };
        self.changes.get().set(&[]);
    }

    /// Queues parameter `id`'s new value (normalized) for the processor.
    pub fn queue(&mut self, id: u32, value: f64) {
        self.pending.retain(|&(p, _)| p != id);
        self.pending.push((id, value.clamp(0.0, 1.0)));
    }

    /// Sets parameter `id` (normalized): the controller and, with the
    /// next block (or [`Instance::flush`]), the processor.
    pub fn set(&mut self, id: u32, value: f64) {
        let value = value.clamp(0.0, 1.0);
        if let Some(c) = &self.controller {
            // SAFETY: the controller's own function.
            unsafe { (c.edit.vt().set_param_normalized)(c.edit.raw(), id, value) };
        }
        self.queue(id, value);
    }

    /// The parameters (with a controller).
    pub fn params(&self) -> Vec<ParamInfo> {
        let Some(c) = &self.controller else {
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
                            flags: p.flags,
                        },
                    )
                })
                .collect()
        }
    }

    /// Parameter `id`'s value (normalized), as the controller has it.
    pub fn value(&self, id: u32) -> Option<f64> {
        let c = self.controller.as_ref()?;
        // SAFETY: the controller's own function.
        Some(unsafe { (c.edit.vt().get_param_normalized)(c.edit.raw(), id) })
    }

    /// Value `value` of parameter `id` as the plug-in writes it.
    pub fn text(&self, id: u32, value: f64) -> Option<String> {
        let c = self.controller.as_ref()?;
        let mut s = [0u16; 128];
        // SAFETY: the controller's own function, a String128 to fill.
        let r = unsafe {
            (c.edit.vt().get_param_string_by_value)(c.edit.raw(), id, value, s.as_mut_ptr())
        };
        (r == result::OK).then(|| text_of16(&s))
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
        unsafe { read_state(|s| (comp.vt().get_state)(comp.raw(), s)) }
    }

    /// The controller's own state.
    pub fn controller_state(&self) -> Option<Vec<u8>> {
        let c = self.controller.as_ref()?;
        // SAFETY: getState with a stream of the host.
        unsafe { read_state(|s| (c.edit.vt().get_state)(c.edit.raw(), s)) }
    }

    /// Gives it states it had: the component's (also the controller's to
    /// read) and the controller's own (none when empty).
    pub fn set_state(&mut self, component: &[u8], controller: &[u8]) -> bool {
        if component.is_empty() {
            return false;
        }
        let comp = &self.component;
        // SAFETY: setState and setComponentState with streams of the host.
        unsafe {
            let stream = Stream::new(component.to_vec());
            let ok = (comp.vt().set_state)(comp.raw(), stream.raw(0)) == result::OK;
            if let Some(c) = &self.controller {
                let stream = Stream::new(component.to_vec());
                (c.edit.vt().set_component_state)(c.edit.raw(), stream.raw(0));
                if !controller.is_empty() {
                    let stream = Stream::new(controller.to_vec());
                    (c.edit.vt().set_state)(c.edit.raw(), stream.raw(0));
                }
            }
            ok
        }
    }

    /// Its latency (samples).
    pub fn latency(&self) -> u32 {
        // SAFETY: the processor's own function.
        unsafe { (self.processor.vt().get_latency_samples)(self.processor.raw()) }
    }

    /// Its editor, made the first time (with a controller that has one).
    pub fn editor(&mut self) -> Option<&mut Editor> {
        if self.editor.is_none() {
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

    /// Closes its editor (after taking it out of its window).
    pub fn close_editor(&mut self) {
        self.editor = None;
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        self.editor = None;
        self.stop();
        if let Some(c) = self.controller.take() {
            // SAFETY: the plug-in's own functions, undoing what was done.
            unsafe {
                if let Some((a, b)) = &c.points {
                    (a.vt().disconnect)(a.raw(), b.raw());
                    (b.vt().disconnect)(b.raw(), a.raw());
                }
                (c.edit.vt().set_component_handler)(c.edit.raw(), std::ptr::null_mut());
                if c.separate {
                    (c.edit.vt().base.terminate)(c.edit.raw());
                }
            }
        }
        // SAFETY: the component's own function.
        unsafe { (self.component.vt().base.terminate)(self.component.raw()) };
    }
}
