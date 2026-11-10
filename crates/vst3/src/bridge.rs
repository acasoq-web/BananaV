//! VST 3 plug-ins in BananaV's 32-bit bridge (`bv-bridge`): a 32-bit
//! module runs in `bananav-bridge32.exe`, started with `--vst3-serve
//! <module> <class> <controller>`, which BananaV talks to through its
//! standard input and output: blocks of sound with their parameter changes (ramps too) and the
//! timeline's place and tempo, parameters, programs, states, the editor in
//! a window of the bridge's own. A plug-in that crashes or hangs there
//! costs that process only: BananaV stops waiting after a while, the sound
//! goes through, and the interface is told. Modules made for BananaV's own
//! processor never come here: they run in BananaV ([`crate::Instance`]).

#![allow(unsafe_code)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use bv_bridge::wire::{In, Out};
use bv_bridge::{Lost, Process, done, failed};

use crate::guard;
use crate::host::Edit;
use crate::instance::{Instance, ParamInfo, Transport};
use crate::module::{ClassInfo, Error, load};

/// The argument that starts the bridge serving a VST 3 effect.
pub const SERVE_FLAG: &str = "--vst3-serve";

/// How long BananaV waits for the bridge: to start the plug-in, for a block
/// of sound, for anything else.
const OPEN_WAIT: Duration = Duration::from_secs(60);
const BLOCK_WAIT: Duration = Duration::from_secs(10);
const CALL_WAIT: Duration = Duration::from_secs(30);

/// The requests' kinds.
mod req {
    pub const SETUP: u8 = 2;
    pub const STOP: u8 = 3;
    pub const PROCESS: u8 = 4;
    pub const PARAMS: u8 = 5;
    pub const VALUE: u8 = 6;
    pub const TEXT: u8 = 7;
    pub const SET: u8 = 8;
    pub const PROGRAMS: u8 = 9;
    pub const PROGRAM: u8 = 10;
    pub const SET_PROGRAM: u8 = 11;
    pub const STATE: u8 = 12;
    pub const SET_STATE: u8 = 13;
    pub const CONTROLLER_STATE: u8 = 14;
    pub const LATENCY: u8 = 15;
    pub const TAIL: u8 = 16;
    pub const EDITOR_OPEN: u8 = 17;
    pub const EDITOR_POLL: u8 = 18;
    pub const EDITOR_CLOSE: u8 = 19;
    pub const QUIT: u8 = 20;
    pub const FLUSH: u8 = 21;
}

fn put_transport(o: &mut Out, t: &Transport) {
    o.u8(u8::from(t.playing))
        .f64(t.tempo)
        .i32(t.numerator)
        .i32(t.denominator);
    match t.cycle {
        Some((a, b)) => o.u8(1).f64(a).f64(b),
        None => o.u8(0),
    };
}

fn get_transport(r: &mut In<'_>) -> Option<Transport> {
    let playing = r.u8()? != 0;
    let tempo = r.f64()?;
    let numerator = r.i32()?;
    let denominator = r.i32()?;
    let cycle = if r.u8()? != 0 {
        Some((r.f64()?, r.f64()?))
    } else {
        None
    };
    Some(Transport {
        playing,
        tempo,
        numerator,
        denominator,
        cycle,
    })
}

fn put_class(o: &mut Out, c: &ClassInfo) {
    o.str(&c.id())
        .str(&c.category)
        .str(&c.name)
        .str(&c.vendor)
        .str(&c.sub_categories)
        .str(&c.version);
}

fn get_class(r: &mut In<'_>) -> Option<ClassInfo> {
    Some(ClassInfo {
        cid: bv_vst3_sys::uid_from_hex(&r.str()?)?,
        category: r.str()?,
        name: r.str()?,
        vendor: r.str()?,
        sub_categories: r.str()?,
        version: r.str()?,
    })
}

fn put_param(o: &mut Out, p: &ParamInfo) {
    o.u32(p.id)
        .str(&p.title)
        .str(&p.short_title)
        .str(&p.units)
        .i32(p.steps)
        .f64(p.default)
        .i32(p.unit)
        .i32(p.flags);
}

fn get_param(r: &mut In<'_>) -> Option<ParamInfo> {
    Some(ParamInfo {
        id: r.u32()?,
        title: r.str()?,
        short_title: r.str()?,
        units: r.str()?,
        steps: r.i32()?,
        default: r.f64()?,
        unit: r.i32()?,
        flags: r.i32()?,
    })
}

/// A VST 3 effect in the bridge.
pub struct Bridged {
    path: PathBuf,
    class: ClassInfo,
    has_editor: bool,
    process: Process,
    noted: AtomicBool,
    running: bool,
    pending: Vec<(u32, f64)>,
    ramps: Vec<(u32, usize, f64)>,
    transport: Transport,
    position: i64,
    editor_open: bool,
    /// Its editor's size as the plug-in last told it (pixels).
    editor_size: Option<(i32, i32)>,
    edits: Mutex<Vec<Edit>>,
}

impl std::fmt::Debug for Bridged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bridged")
            .field("path", &self.path)
            .field("class", &self.class.name)
            .finish()
    }
}

impl Bridged {
    /// Effect `class_id` (32 hexadecimal digits; empty: the module's first)
    /// of the 32-bit module at `path`, started in a bridge, its controller
    /// too when `controller` (as [`Instance::new`]).
    pub fn spawn(path: &Path, class_id: &str, controller: bool) -> Result<Bridged, Error> {
        if !bv_bridge::enabled() {
            return Err(Error::NeedsBridge);
        }
        let exe = bv_bridge::exe().ok_or(Error::NeedsBridge)?;
        Bridged::spawn_with(&exe, path, class_id, controller, &[])
    }

    /// The same in bridge program `exe` with environment variables `env`
    /// (the tests').
    pub fn spawn_with(
        exe: &Path,
        path: &Path,
        class_id: &str,
        controller: bool,
        env: &[(String, String)],
    ) -> Result<Bridged, Error> {
        let class_arg = if class_id.trim().is_empty() {
            "-".to_string()
        } else {
            class_id.trim().to_string()
        };
        let args: Vec<OsString> = vec![
            SERVE_FLAG.into(),
            std::path::absolute(path)
                .unwrap_or_else(|_| path.to_path_buf())
                .into(),
            class_arg.into(),
            if controller { "1" } else { "0" }.into(),
        ];
        let (process, hello) = Process::spawn(exe, &args, env, OPEN_WAIT).map_err(Error::Bridge)?;
        let mut r = In::new(&hello);
        let class = get_class(&mut r).ok_or_else(|| Error::Bridge("a garbled answer".into()))?;
        let has_editor = r.u8().unwrap_or(0) != 0;
        Ok(Bridged {
            path: path.to_path_buf(),
            class,
            has_editor,
            process,
            noted: AtomicBool::new(false),
            running: false,
            pending: Vec::new(),
            ramps: Vec::new(),
            transport: Transport::default(),
            position: 0,
            editor_open: false,
            editor_size: None,
            edits: Mutex::new(Vec::new()),
        })
    }

    /// A request and its answer's values (`None`: it failed, or the bridge
    /// is gone — noted once as a crash).
    fn call(&self, out: &Out, wait: Duration) -> Option<Vec<u8>> {
        match self.process.call(&out.0, wait) {
            Ok(values) => Some(values),
            Err(lost) => {
                if lost.fatal() && !self.noted.swap(true, Ordering::AcqRel) {
                    let (code, at) = match &lost {
                        Lost::Crashed { code, at, .. } => {
                            (*code, format!("{at} (in the 32-bit bridge)"))
                        }
                        _ => (0, "the 32-bit bridge".to_string()),
                    };
                    guard::note(guard::Crash {
                        module: self.path.clone(),
                        effect: self.class.name.clone(),
                        doing: lost.doing(),
                        code,
                        at,
                    });
                }
                None
            }
        }
    }

    fn ask(&self, kind: u8) -> Option<Vec<u8>> {
        self.call(&Out::new(kind), CALL_WAIT)
    }

    pub fn class(&self) -> &ClassInfo {
        &self.class
    }

    pub fn crashed(&self) -> bool {
        self.process.gone()
    }

    pub fn setup(&mut self, rate: f64, block: usize, channels: usize) -> Result<(), Error> {
        let mut out = Out::new(req::SETUP);
        out.f64(rate).u32(block as u32).u32(channels as u32);
        match self.call(&out, OPEN_WAIT) {
            Some(_) => {
                self.running = true;
                Ok(())
            }
            None if self.crashed() => Err(Error::Crashed),
            None => Err(Error::Refused("to start processing", 0)),
        }
    }

    pub fn stop(&mut self) {
        if std::mem::take(&mut self.running) {
            self.ask(req::STOP);
        }
    }

    pub fn running(&self) -> bool {
        self.running
    }

    pub fn set_transport(&mut self, t: Transport) {
        self.transport = t;
    }

    pub fn locate(&mut self, position: i64) {
        self.position = position;
    }

    /// Processes `channels` in the bridge (as [`Instance::process`]).
    pub fn process(&mut self, channels: &mut [Vec<f32>]) {
        if !self.running || self.crashed() {
            return;
        }
        let host = channels.len();
        let n = channels.first().map_or(0, Vec::len);
        if host == 0 || n == 0 {
            return;
        }
        let mut out = Out::new(req::PROCESS);
        out.i64(self.position);
        put_transport(&mut out, &self.transport);
        let pending = std::mem::take(&mut self.pending);
        out.u32(pending.len() as u32);
        for (id, v) in pending {
            out.u32(id).f64(v);
        }
        let ramps = std::mem::take(&mut self.ramps);
        out.u32(ramps.len() as u32);
        for (id, at, v) in ramps {
            out.u32(id).u32(at as u32).f64(v);
        }
        out.u32(host as u32).u32(n as u32);
        let flat: Vec<f32> = channels
            .iter()
            .flat_map(|c| c[..n].iter().copied())
            .collect();
        out.f32s(&flat);
        let Some(answer) = self.call(&out, BLOCK_WAIT) else {
            return;
        };
        let Some(data) = In::new(&answer).f32s() else {
            return;
        };
        if data.len() == host * n {
            for (c, chunk) in channels.iter_mut().zip(data.chunks_exact(n)) {
                c[..n].copy_from_slice(chunk);
            }
        }
        self.position += n as i64;
    }

    pub fn queue(&mut self, id: u32, value: f64) {
        self.pending.retain(|&(p, _)| p != id);
        self.pending.push((id, value.clamp(0.0, 1.0)));
    }

    pub fn queue_ramp(&mut self, id: u32, points: &[(usize, f64)]) {
        self.ramps.retain(|&(p, _, _)| p != id);
        self.ramps
            .extend(points.iter().map(|&(at, v)| (id, at, v.clamp(0.0, 1.0))));
    }

    pub fn has_queued(&self) -> bool {
        !self.pending.is_empty() || !self.ramps.is_empty()
    }

    /// Hands the queued changes to the processor now (one request).
    pub fn flush(&mut self) {
        let pending = std::mem::take(&mut self.pending);
        let mut out = Out::new(req::FLUSH);
        out.u32(pending.len() as u32);
        for (id, v) in pending {
            out.u32(id).f64(v);
        }
        self.call(&out, CALL_WAIT);
    }

    /// Sets parameter `id` (the controller and the processor).
    pub fn set(&mut self, id: u32, value: f64) {
        self.pending.retain(|&(p, _)| p != id);
        let mut out = Out::new(req::SET);
        out.u32(id).f64(value.clamp(0.0, 1.0));
        self.call(&out, CALL_WAIT);
    }

    pub fn params(&self) -> Vec<ParamInfo> {
        let Some(answer) = self.ask(req::PARAMS) else {
            return Vec::new();
        };
        let mut r = In::new(&answer);
        let n = r.u32().unwrap_or(0);
        (0..n).map_while(|_| get_param(&mut r)).collect()
    }

    pub fn value(&self, id: u32) -> Option<f64> {
        let mut out = Out::new(req::VALUE);
        out.u32(id);
        In::new(&self.call(&out, CALL_WAIT)?).f64()
    }

    pub fn text(&self, id: u32, value: f64) -> Option<String> {
        let mut out = Out::new(req::TEXT);
        out.u32(id).f64(value);
        In::new(&self.call(&out, CALL_WAIT)?).str()
    }

    pub fn programs(&self) -> Vec<String> {
        let Some(answer) = self.ask(req::PROGRAMS) else {
            return Vec::new();
        };
        let mut r = In::new(&answer);
        let n = r.u32().unwrap_or(0);
        (0..n).map_while(|_| r.str()).collect()
    }

    pub fn program(&self) -> Option<usize> {
        let p = In::new(&self.ask(req::PROGRAM)?).i32()?;
        usize::try_from(p).ok()
    }

    pub fn set_program(&mut self, index: usize) -> bool {
        let mut out = Out::new(req::SET_PROGRAM);
        out.u32(index as u32);
        self.call(&out, CALL_WAIT)
            .and_then(|a| In::new(&a).u8())
            .is_some_and(|v| v != 0)
    }

    pub fn state(&self) -> Option<Vec<u8>> {
        In::new(&self.ask(req::STATE)?).bytes()
    }

    pub fn controller_state(&self) -> Option<Vec<u8>> {
        In::new(&self.ask(req::CONTROLLER_STATE)?).bytes()
    }

    pub fn set_state(&mut self, component: &[u8], controller: &[u8]) -> bool {
        let mut out = Out::new(req::SET_STATE);
        out.bytes(component).bytes(controller);
        self.call(&out, OPEN_WAIT)
            .and_then(|a| In::new(&a).u8())
            .is_some_and(|v| v != 0)
    }

    pub fn latency(&self) -> u32 {
        self.ask(req::LATENCY)
            .and_then(|a| In::new(&a).u32())
            .unwrap_or(0)
    }

    pub fn tail(&self) -> Option<u32> {
        let t = self.ask(req::TAIL).and_then(|a| In::new(&a).i64())?;
        u32::try_from(t).ok()
    }

    pub fn has_editor(&self) -> bool {
        self.has_editor
    }

    /// Shows its editor: inside window `parent` of BananaV's when given
    /// (a system's handle: the plug-in's view a child of it, in this other
    /// process), else in a window of the bridge's own titled `title`.
    pub fn open_editor(&mut self, title: &str, parent: Option<isize>) -> bool {
        if self.editor_open {
            return true;
        }
        let mut out = Out::new(req::EDITOR_OPEN);
        out.str(title).u64(parent.map_or(0, |p| p as u64));
        let Some(answer) = self.call(&out, OPEN_WAIT) else {
            return false;
        };
        let mut r = In::new(&answer);
        let ok = r.u8().is_some_and(|v| v != 0);
        self.editor_size = size_of(&mut r);
        self.editor_open = ok;
        ok
    }

    /// Its editor's size as the plug-in last told it.
    pub fn editor_size(&self) -> Option<(i32, i32)> {
        self.editor_size
    }

    /// Whether its editor's window is still open; what it reported is kept
    /// for [`Bridged::take_edits`], the size it asks for for
    /// [`Bridged::editor_size`].
    pub fn poll_editor(&mut self) -> bool {
        if !self.editor_open {
            return false;
        }
        let Some(answer) = self.ask(req::EDITOR_POLL) else {
            self.editor_open = false;
            return false;
        };
        let mut r = In::new(&answer);
        let open = r.u8().unwrap_or(0) != 0;
        if let Some(size) = size_of(&mut r) {
            self.editor_size = Some(size);
        }
        let n = r.u32().unwrap_or(0);
        let mut edits = self.edits.lock().unwrap_or_else(|e| e.into_inner());
        for _ in 0..n {
            let (Some(kind), Some(id), Some(value), Some(flags)) =
                (r.u8(), r.u32(), r.f64(), r.i32())
            else {
                break;
            };
            edits.push(match kind {
                0 => Edit::Perform(id, value),
                1 => Edit::Begin(id),
                2 => Edit::End(id),
                3 => Edit::Restart(flags),
                _ => Edit::Dirty,
            });
        }
        self.editor_open = open;
        open
    }

    pub fn close_editor(&mut self) {
        if std::mem::take(&mut self.editor_open) {
            self.ask(req::EDITOR_CLOSE);
        }
    }

    pub fn editor_open(&self) -> bool {
        self.editor_open
    }

    pub fn take_edits(&self) -> Vec<Edit> {
        std::mem::take(&mut *self.edits.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

impl Drop for Bridged {
    fn drop(&mut self) {
        if !self.crashed() {
            let _ = self
                .process
                .call(&Out::new(req::QUIT).0, Duration::from_secs(2));
        }
    }
}

/// A size an answer gives (two numbers; none when they are not both
/// above 0).
fn size_of(r: &mut In<'_>) -> Option<(i32, i32)> {
    let (w, h) = (r.i32()?, r.i32()?);
    (w > 0 && h > 0).then_some((w, h))
}

// ---- The bridge's side -------------------------------------------------------------

/// The editor in the bridge: its window of the bridge's own, or a window
/// of BananaV's it lives in; the size the plug-in asked for.
#[derive(Debug, Default)]
struct EditorState {
    #[cfg(windows)]
    window: Option<bv_bridge::window::EditorWindow>,
    embedded: bool,
    asked: std::sync::Arc<Mutex<Option<(i32, i32)>>>,
}

impl EditorState {
    #[cfg(windows)]
    fn open(&self) -> bool {
        self.window.is_some()
    }

    #[cfg(not(windows))]
    fn open(&self) -> bool {
        false
    }

    /// The size the plug-in asked for last.
    fn asked(&self) -> Option<(i32, i32)> {
        *self.asked.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// The answer once the plug-in crashed here.
fn crashed_reply() -> Vec<u8> {
    match guard::take_crashes().pop() {
        Some(c) => bv_bridge::crashed(&c.doing, c.code, &c.at),
        None => bv_bridge::crashed("", 0, ""),
    }
}

/// Serves effect `class_id` ("-": the module's first) of the module at
/// `path`, its controller too when `controller`, until told to quit or its
/// input closes (the bridge program's `--vst3-serve`).
pub fn serve(path: &Path, class_id: &str, controller: bool) -> i32 {
    bv_bridge::apartment();
    let id = if class_id == "-" { "" } else { class_id };
    let made = load(path).and_then(|m| {
        let class = m.effect(id).cloned().ok_or(Error::NoClass)?;
        Instance::new(&m, &class, controller)
    });
    let mut inst = match made {
        Ok(i) => i,
        Err(e) => {
            let mut stdout = std::io::stdout().lock();
            let _ = bv_bridge::wire::write_frame(&mut stdout, &failed(&e.to_string()));
            return 2;
        }
    };
    let has_editor = inst.has_editor();
    let hello = done(|o| {
        put_class(o, inst.class());
        o.u8(u8::from(has_editor));
    });
    let editor = std::cell::RefCell::new(EditorState::default());
    let inst = std::cell::RefCell::new(&mut inst);
    bv_bridge::serve(
        &hello,
        |frame| handle(&mut inst.borrow_mut(), &mut editor.borrow_mut(), frame),
        || {
            #[cfg(windows)]
            {
                let mut inst = inst.borrow_mut();
                let mut editor = editor.borrow_mut();
                if editor.window.as_ref().is_some_and(|w| w.closed()) {
                    inst.close_editor();
                    editor.window = None;
                }
            }
            editor.borrow().open()
        },
    )
}

/// One request: its answer, and whether the bridge quits. Once the
/// plug-in crashed, every answer says so.
fn handle(inst: &mut Instance, editor: &mut EditorState, frame: &[u8]) -> (Vec<u8>, bool) {
    if inst.crashed() {
        return (crashed_reply(), false);
    }
    let (reply, quit) = handle_request(inst, editor, frame);
    if inst.crashed() {
        return (crashed_reply(), quit);
    }
    (reply, quit)
}

fn handle_request(inst: &mut Instance, editor: &mut EditorState, frame: &[u8]) -> (Vec<u8>, bool) {
    let mut r = In::new(frame);
    let Some(kind) = r.u8() else {
        return (failed("an empty request"), false);
    };
    let reply = match kind {
        req::SETUP => {
            let (Some(rate), Some(block), Some(channels)) = (r.f64(), r.u32(), r.u32()) else {
                return (failed("a garbled request"), false);
            };
            match inst.setup(rate, block as usize, channels as usize) {
                Ok(()) => done(|_| {}),
                Err(e) => failed(&e.to_string()),
            }
        }
        req::STOP => {
            inst.stop();
            done(|_| {})
        }
        req::PROCESS => match process(inst, &mut r) {
            Some(data) => done(|o| {
                o.f32s(&data);
            }),
            None => failed("a garbled block"),
        },
        req::PARAMS => {
            let params = inst.params();
            done(|o| {
                o.u32(params.len() as u32);
                for p in &params {
                    put_param(o, p);
                }
            })
        }
        req::VALUE => match r.u32().and_then(|i| inst.value(i)) {
            Some(v) => done(|o| {
                o.f64(v);
            }),
            None => failed("no value"),
        },
        req::TEXT => {
            let (Some(id), Some(value)) = (r.u32(), r.f64()) else {
                return (failed("a garbled request"), false);
            };
            match inst.text(id, value) {
                Some(t) => done(|o| {
                    o.str(&t);
                }),
                None => failed("no text"),
            }
        }
        req::SET => {
            if let (Some(i), Some(v)) = (r.u32(), r.f64()) {
                inst.set(i, v);
            }
            done(|_| {})
        }
        req::FLUSH => {
            for _ in 0..r.u32().unwrap_or(0) {
                let (Some(id), Some(v)) = (r.u32(), r.f64()) else {
                    break;
                };
                inst.queue(id, v);
            }
            inst.flush();
            done(|_| {})
        }
        req::PROGRAMS => {
            let names = inst.programs();
            done(|o| {
                o.u32(names.len() as u32);
                for n in &names {
                    o.str(n);
                }
            })
        }
        req::PROGRAM => {
            let p = inst.program().map_or(-1, |p| p as i32);
            done(|o| {
                o.i32(p);
            })
        }
        req::SET_PROGRAM => {
            let ok = inst.set_program(r.u32().unwrap_or(0) as usize);
            done(|o| {
                o.u8(u8::from(ok));
            })
        }
        req::STATE => match inst.state() {
            Some(s) => done(|o| {
                o.bytes(&s);
            }),
            None => failed("no state"),
        },
        req::CONTROLLER_STATE => match inst.controller_state() {
            Some(s) => done(|o| {
                o.bytes(&s);
            }),
            None => failed("no controller state"),
        },
        req::SET_STATE => {
            let (Some(component), Some(controller)) = (r.bytes(), r.bytes()) else {
                return (failed("a garbled request"), false);
            };
            let ok = inst.set_state(&component, &controller);
            done(|o| {
                o.u8(u8::from(ok));
            })
        }
        req::LATENCY => done(|o| {
            o.u32(inst.latency());
        }),
        req::TAIL => {
            let t = inst.tail().map_or(-1, i64::from);
            done(|o| {
                o.i64(t);
            })
        }
        req::EDITOR_OPEN => {
            let title = r.str().unwrap_or_default();
            let parent = r.u64().unwrap_or(0) as isize;
            let ok = open_editor(inst, editor, &title, parent);
            let (w, h) = editor.asked().unwrap_or((0, 0));
            done(|o| {
                o.u8(u8::from(ok)).i32(w).i32(h);
            })
        }
        req::EDITOR_POLL => {
            let open = editor.open();
            let (w, h) = editor.asked().unwrap_or((0, 0));
            let edits = inst.take_edits();
            done(|o| {
                o.u8(u8::from(open)).i32(w).i32(h).u32(edits.len() as u32);
                for e in &edits {
                    match *e {
                        Edit::Perform(id, v) => o.u8(0).u32(id).f64(v).i32(0),
                        Edit::Begin(id) => o.u8(1).u32(id).f64(0.0).i32(0),
                        Edit::End(id) => o.u8(2).u32(id).f64(0.0).i32(0),
                        Edit::Restart(flags) => o.u8(3).u32(0).f64(0.0).i32(flags),
                        Edit::Dirty => o.u8(4).u32(0).f64(0.0).i32(0),
                    };
                }
            })
        }
        req::EDITOR_CLOSE => {
            inst.close_editor();
            #[cfg(windows)]
            {
                editor.window = None;
            }
            editor.embedded = false;
            done(|_| {})
        }
        req::QUIT => return (done(|_| {}), true),
        _ => failed("an unknown request"),
    };
    (reply, false)
}

/// A block of sound: its place, transport, changes and samples read from
/// `r`, processed; the samples out.
fn process(inst: &mut Instance, r: &mut In<'_>) -> Option<Vec<f32>> {
    let position = r.i64()?;
    let transport = get_transport(r)?;
    for _ in 0..r.u32()? {
        let (id, v) = (r.u32()?, r.f64()?);
        inst.queue(id, v);
    }
    let mut ramps: Vec<(u32, usize, f64)> = Vec::new();
    for _ in 0..r.u32()? {
        ramps.push((r.u32()?, r.u32()? as usize, r.f64()?));
    }
    let mut ids: Vec<u32> = ramps.iter().map(|r| r.0).collect();
    ids.sort_unstable();
    ids.dedup();
    for id in ids {
        let points: Vec<(usize, f64)> = ramps
            .iter()
            .filter(|r| r.0 == id)
            .map(|r| (r.1, r.2))
            .collect();
        inst.queue_ramp(id, &points);
    }
    let host = r.u32()? as usize;
    let n = r.u32()? as usize;
    let flat = r.f32s()?;
    if host == 0 || flat.len() != host * n {
        return None;
    }
    let mut channels: Vec<Vec<f32>> = flat.chunks_exact(n.max(1)).map(<[f32]>::to_vec).collect();
    inst.set_transport(transport);
    inst.locate(position);
    inst.process(&mut channels);
    Some(channels.concat())
}

/// Opens the plug-in's editor: inside BananaV's window `parent` when
/// given (its view a child of that one), else in a window of the bridge's
/// own.
#[cfg(windows)]
fn open_editor(inst: &mut Instance, editor: &mut EditorState, title: &str, parent: isize) -> bool {
    if editor.window.is_some() || editor.embedded {
        return true;
    }
    let Some(ed) = inst.editor() else {
        return false;
    };
    if !ed.supports(crate::PLATFORM_HWND) {
        inst.close_editor();
        return false;
    }
    let (w, h) = ed.size().unwrap_or((400, 300));
    *editor.asked.lock().unwrap_or_else(|e| e.into_inner()) = Some((w, h));
    if parent != 0 {
        // In a window of the bridge's own (the plug-in deals with this
        // process's windows only), then put inside BananaV's.
        let Some(win) = bv_bridge::window::EditorWindow::open(title, w, h) else {
            inst.close_editor();
            return false;
        };
        let asked = std::sync::Arc::clone(&editor.asked);
        let resize = win.resizer();
        // SAFETY: the window lives until the editor is closed.
        let ok = unsafe {
            ed.attach(
                win.handle(),
                crate::PLATFORM_HWND,
                Box::new(move |w, h| {
                    resize(w, h);
                    *asked.lock().unwrap_or_else(|e| e.into_inner()) = Some((w, h));
                }),
            )
        };
        if !ok || !win.embed(parent, w, h) {
            inst.close_editor();
            return false;
        }
        editor.window = Some(win);
        editor.embedded = true;
        return true;
    }
    let Some(win) = bv_bridge::window::EditorWindow::open(title, w, h) else {
        inst.close_editor();
        return false;
    };
    // SAFETY: the window lives until the editor is closed (the bridge
    // closes the editor before it drops the window).
    let ok = unsafe { ed.attach(win.handle(), crate::PLATFORM_HWND, Box::new(win.resizer())) };
    if !ok {
        inst.close_editor();
        return false;
    }
    win.show();
    editor.window = Some(win);
    true
}

#[cfg(not(windows))]
fn open_editor(_: &mut Instance, _: &mut EditorState, _: &str, _: isize) -> bool {
    false
}
