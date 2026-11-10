//! A VST 2 plug-in, wherever it runs: in this process when its library is
//! made for this processor ([`Local`]), in BananaV's 32-bit bridge when it
//! is a 32-bit Windows library ([`Bridged`]). Both answer alike.

#![allow(unsafe_code)]

use std::ffi::c_void;
use std::path::Path;
use std::sync::Arc;

use crate::bridge::Bridged;
use crate::host::{Edit, Resize};
use crate::local::Local;
use crate::module::{Error, Machine, Module, load, machine, this_machine};
use crate::{Description, Param, Transport};

/// A plug-in running.
#[derive(Debug)]
pub enum Instance {
    Local(Box<Local>),
    Bridged(Box<Bridged>),
}

/// Forwards a call to whichever runs it.
macro_rules! either {
    ($self:expr, $p:ident => $e:expr) => {
        match $self {
            Instance::Local($p) => $e,
            Instance::Bridged($p) => $e,
        }
    };
}

impl Instance {
    /// Plug-in `id` (a shell's plug-in; 0 for the library's own) of the
    /// library at `path`: here, or in the bridge for a 32-bit library.
    pub fn new(path: &Path, id: i32) -> Result<Instance, Error> {
        match machine(path) {
            Some(Machine::X86) if this_machine() != Machine::X86 && cfg!(windows) => {
                Bridged::spawn(path, id).map(|b| Instance::Bridged(Box::new(b)))
            }
            Some(m) if m != this_machine() && m != Machine::Other => Err(Error::Load(format!(
                "the library is made for another processor ({m:?})"
            ))),
            _ => {
                let module = load(path)?;
                Local::new(&module, id).map(|l| Instance::Local(Box::new(l)))
            }
        }
    }

    /// Plug-in `id` of the library at `path` in bridge program `exe`, with
    /// environment variables `env` (the tests').
    pub fn bridged_with(
        exe: &Path,
        path: &Path,
        id: i32,
        env: &[(String, String)],
    ) -> Result<Instance, Error> {
        Bridged::spawn_with(exe, path, id, env).map(|b| Instance::Bridged(Box::new(b)))
    }

    /// A plug-in of a library already loaded (the tests' linked ones).
    pub fn of(module: &Arc<Module>, id: i32) -> Result<Instance, Error> {
        Local::new(module, id).map(|l| Instance::Local(Box::new(l)))
    }

    /// Whether it runs in the 32-bit bridge.
    pub fn bridged(&self) -> bool {
        matches!(self, Instance::Bridged(_))
    }

    pub fn crashed(&self) -> bool {
        either!(self, p => p.crashed())
    }

    pub fn describe(&self) -> Description {
        either!(self, p => p.describe())
    }

    pub fn setup(&mut self, rate: f64, block: usize, channels: usize) -> Result<(), Error> {
        either!(self, p => p.setup(rate, block, channels))
    }

    pub fn stop(&mut self) {
        either!(self, p => p.stop())
    }

    pub fn running(&self) -> bool {
        either!(self, p => p.running())
    }

    /// The plug-in's channels (in, out).
    pub fn channels(&self) -> (usize, usize) {
        either!(self, p => p.channels())
    }

    pub fn set_transport(&mut self, t: Transport) {
        either!(self, p => p.set_transport(t))
    }

    pub fn locate(&mut self, position: i64) {
        either!(self, p => p.locate(position))
    }

    pub fn process(&mut self, channels: &mut [Vec<f32>]) {
        either!(self, p => p.process(channels))
    }

    pub fn queue(&mut self, id: u32, value: f64) {
        either!(self, p => p.queue(id, value))
    }

    pub fn queue_ramp(&mut self, id: u32, points: &[(usize, f64)]) {
        either!(self, p => p.queue_ramp(id, points))
    }

    pub fn has_queued(&self) -> bool {
        either!(self, p => p.has_queued())
    }

    pub fn flush(&mut self) {
        either!(self, p => p.flush())
    }

    pub fn set(&mut self, id: u32, value: f64) {
        either!(self, p => p.set(id, value))
    }

    pub fn params(&self) -> Vec<Param> {
        either!(self, p => p.params())
    }

    pub fn value(&self, id: u32) -> Option<f32> {
        either!(self, p => p.value(id))
    }

    /// Parameter `id`'s current value as the plug-in writes it.
    pub fn display(&self, id: u32) -> String {
        either!(self, p => p.display(id))
    }

    pub fn can_automate(&self, id: u32) -> bool {
        either!(self, p => p.can_automate(id))
    }

    pub fn programs(&self) -> Vec<String> {
        either!(self, p => p.programs())
    }

    pub fn program(&self) -> Option<usize> {
        either!(self, p => p.program())
    }

    pub fn set_program(&mut self, index: usize) -> bool {
        either!(self, p => p.set_program(index))
    }

    pub fn state(&self) -> Option<Vec<u8>> {
        either!(self, p => p.state())
    }

    pub fn set_state(&mut self, bytes: &[u8]) -> bool {
        either!(self, p => p.set_state(bytes))
    }

    /// The program as a preset file (`.fxp`).
    pub fn preset_file(&self) -> Option<Vec<u8>> {
        match self {
            Instance::Local(p) => p.preset().map(|p| crate::preset::write(&p)),
            Instance::Bridged(p) => p.preset_file(),
        }
    }

    /// All its programs as a bank file (`.fxb`).
    pub fn bank_file(&mut self) -> Option<Vec<u8>> {
        match self {
            Instance::Local(p) => p.bank().map(|p| crate::preset::write(&p)),
            Instance::Bridged(p) => p.bank_file(),
        }
    }

    pub fn latency(&self) -> u32 {
        either!(self, p => p.latency())
    }

    pub fn tail(&self) -> Option<u32> {
        either!(self, p => p.tail())
    }

    pub fn has_editor(&self) -> bool {
        either!(self, p => p.has_editor())
    }

    /// Its editor's size, as it tells (a bridged one's, as it told last).
    pub fn editor_size(&self) -> Option<(i32, i32)> {
        match self {
            Instance::Local(p) => p.editor_size(),
            Instance::Bridged(p) => p.editor_size(),
        }
    }

    /// Shows its editor in window `parent` (resized by `resize`) — a
    /// bridged one's window a child of it too, in the bridge's process —,
    /// or, with no `parent`, a bridged one's in a window of the bridge's
    /// own titled `title`.
    ///
    /// # Safety
    /// `parent` is a live window of this system's kind kept until
    /// [`Instance::close_editor`].
    pub unsafe fn open_editor(&mut self, parent: *mut c_void, resize: Resize, title: &str) -> bool {
        match self {
            // SAFETY: the caller's.
            Instance::Local(p) => unsafe { p.open_editor(parent, resize) },
            Instance::Bridged(p) => {
                p.open_editor(title, (!parent.is_null()).then_some(parent as isize))
            }
        }
    }

    /// A frame of its editor's life: idle time here, its window's news from
    /// the bridge. Whether the editor is still open.
    pub fn follow_editor(&mut self) -> bool {
        match self {
            Instance::Local(p) => {
                p.idle_editor();
                p.editor_open()
            }
            Instance::Bridged(p) => p.poll_editor(),
        }
    }

    pub fn close_editor(&mut self) {
        either!(self, p => p.close_editor())
    }

    pub fn editor_open(&self) -> bool {
        either!(self, p => p.editor_open())
    }

    pub fn take_edits(&self) -> Vec<Edit> {
        either!(self, p => p.take_edits())
    }
}
