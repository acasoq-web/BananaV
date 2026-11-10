//! A VST 2 plug-in running in this process: made from its library, set up
//! for a rate and a block, blocks of sound processed with the parameter
//! changes queued (ramps cut the block at their points) and the timeline's
//! place, tempo and meter; its parameters, programs and state; its editor
//! in a window the caller gives. Every call into it goes behind the crash
//! guard: a library that crashed is not called again, its plug-ins pass the
//! sound.

#![allow(unsafe_code)]

use std::ffi::c_void;
use std::sync::Arc;

use crate::abi::{PluginObject, Rect, TEXT, TimeInfo, flags, op, text_of, time};
use crate::guard;
use crate::host::{Edit, HostData, Resize};
use crate::module::{Error, Module};
use crate::{Description, Param, Transport};

/// The processing set up: the longest block, the plug-in's input and
/// output buffers, where the next block lies on the timeline.
#[derive(Debug)]
struct Run {
    block: usize,
    ins: Vec<Vec<f32>>,
    outs: Vec<Vec<f32>>,
}

/// A plug-in in this process.
pub struct Local {
    module: Arc<Module>,
    plugin: *mut PluginObject,
    /// Pointed to by the plug-in's object: kept until it is closed.
    host: Box<HostData>,
    name: String,
    run: Option<Run>,
    /// Parameter changes for the next block: from its first sample, and
    /// ramps' points (parameter, sample, value).
    pending: Vec<(u32, f32)>,
    ramps: Vec<(u32, usize, f32)>,
    transport: Transport,
    position: i64,
    editor_open: bool,
    /// The program last chosen through BananaV (for plug-ins that do not
    /// tell theirs).
    chosen: Option<usize>,
}

impl std::fmt::Debug for Local {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Local")
            .field("module", &self.module.path())
            .field("name", &self.name)
            .field("running", &self.run.is_some())
            .finish()
    }
}

// SAFETY: an instance is used by one thread at a time (moving it between
// threads is what hosts do with the processing part).
unsafe impl Send for Local {}

/// The bank (0) or the program (1) as a chunk.
const BANK: i32 = 0;

/// BananaV's state of a plug-in: "BVV2", a version, the program, then its
/// parameters' values or its bank's chunk.
const STATE_MARK: &[u8; 4] = b"BVV2";
const STATE_PARAMS: u8 = 0;
const STATE_CHUNK: u8 = 1;

impl Local {
    /// Plug-in `id` of `module` (the shell's plug-in of that number, or
    /// the library's own with 0), made and opened.
    pub fn new(module: &Arc<Module>, id: i32) -> Result<Local, Error> {
        let plugin = module.make(id)?;
        let host = HostData::new();
        // SAFETY: the plug-in's object; its host field is BananaV's.
        unsafe { (*plugin).host_data[0] = &*host as *const HostData as isize };
        let mut local = Local {
            module: Arc::clone(module),
            plugin,
            host,
            name: String::new(),
            run: None,
            pending: Vec::new(),
            ramps: Vec::new(),
            transport: Transport::default(),
            position: 0,
            editor_open: false,
            chosen: None,
        };
        local
            .dispatch("open", op::OPEN, 0, 0, std::ptr::null_mut(), 0.0)
            .ok_or(Error::Crashed)?;
        local.name = local.describe().name;
        Ok(local)
    }

    fn object(&self) -> &PluginObject {
        // SAFETY: the plug-in's object, alive while it is open.
        unsafe { &*self.plugin }
    }

    /// A request to the plug-in behind the guard: its answer, or `None`
    /// when its library is switched off or crashes now.
    fn dispatch(
        &self,
        doing: &str,
        request: i32,
        index: i32,
        value: isize,
        ptr: *mut c_void,
        number: f32,
    ) -> Option<isize> {
        if self.module.crashed() {
            return None;
        }
        let f = self.object().dispatcher?;
        // SAFETY: the plug-in's own dispatcher; `ptr` what the request
        // takes (the callers give buffers of `TEXT` bytes for texts).
        match unsafe { guard::dispatch(f, self.plugin, request, index, value, ptr, number) } {
            Ok(r) => Some(r),
            Err((code, at)) => {
                self.module.crash(&self.name, doing, code, at);
                None
            }
        }
    }

    /// A text the plug-in writes for `request`.
    fn text(&self, doing: &str, request: i32, index: i32) -> String {
        let mut buf = [0u8; TEXT];
        self.dispatch(doing, request, index, 0, buf.as_mut_ptr().cast(), 0.0);
        text_of(&buf)
    }

    pub fn crashed(&self) -> bool {
        self.module.crashed()
    }

    /// What it is.
    pub fn describe(&self) -> Description {
        let o = self.object();
        let tail = self.tail();
        let mut name = self.text("its name", op::EFFECT_NAME, 0);
        let product = self.text("its product", op::PRODUCT, 0);
        if name.is_empty() {
            name.clone_from(&product);
        }
        if name.is_empty() {
            name = self
                .module
                .path()
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
        }
        let version = self
            .dispatch(
                "its version",
                op::VENDOR_VERSION,
                0,
                0,
                std::ptr::null_mut(),
                0.0,
            )
            .unwrap_or(0);
        Description {
            name,
            vendor: self.text("its maker", op::VENDOR, 0),
            product,
            version: version as i64,
            unique_id: o.unique_id,
            category: self
                .dispatch(
                    "its category",
                    op::PLUG_CATEGORY,
                    0,
                    0,
                    std::ptr::null_mut(),
                    0.0,
                )
                .unwrap_or(0) as i32,
            inputs: o.inputs.max(0) as u32,
            outputs: o.outputs.max(0) as u32,
            params: o.params.max(0) as u32,
            programs: o.programs.max(0) as u32,
            latency: o.latency.max(0) as u32,
            tail,
            editor: o.flags & flags::HAS_EDITOR != 0,
            synth: o.flags & flags::IS_SYNTH != 0,
            chunks: o.flags & flags::PROGRAM_CHUNKS != 0,
        }
    }

    /// Sets it up for `rate` and blocks of at most `block` samples (its own
    /// channels; `channels` the host's), then it runs.
    pub fn setup(&mut self, rate: f64, block: usize, _channels: usize) -> Result<(), Error> {
        self.stop();
        if self.crashed() {
            return Err(Error::Crashed);
        }
        self.host.set_rate(rate);
        self.host.set_block(block);
        let none = std::ptr::null_mut();
        self.dispatch("its rate", op::SET_SAMPLE_RATE, 0, 0, none, rate as f32)
            .ok_or(Error::Crashed)?;
        self.dispatch(
            "its block",
            op::SET_BLOCK_SIZE,
            0,
            block as isize,
            none,
            0.0,
        )
        .ok_or(Error::Crashed)?;
        self.dispatch("resume", op::MAINS, 0, 1, none, 0.0)
            .ok_or(Error::Crashed)?;
        self.dispatch("start", op::START_PROCESS, 0, 0, none, 0.0)
            .ok_or(Error::Crashed)?;
        let o = self.object();
        let (ins, outs) = (o.inputs.max(0) as usize, o.outputs.max(0) as usize);
        if o.process.is_none() && o.process_adding.is_none() {
            return Err(Error::Refused("to process sound"));
        }
        self.run = Some(Run {
            block: block.max(1),
            ins: vec![vec![0.0; block.max(1)]; ins],
            outs: vec![vec![0.0; block.max(1)]; outs],
        });
        Ok(())
    }

    /// Stops processing (it starts again with [`Local::setup`]).
    pub fn stop(&mut self) {
        if self.run.take().is_some() {
            let none = std::ptr::null_mut();
            self.dispatch("stop", op::STOP_PROCESS, 0, 0, none, 0.0);
            self.dispatch("suspend", op::MAINS, 0, 0, none, 0.0);
        }
    }

    pub fn running(&self) -> bool {
        self.run.is_some()
    }

    /// The plug-in's channels (in, out).
    pub fn channels(&self) -> (usize, usize) {
        let o = self.object();
        (o.inputs.max(0) as usize, o.outputs.max(0) as usize)
    }

    pub fn set_transport(&mut self, t: Transport) {
        self.transport = t;
    }

    /// Where the next block's first sample lies on the timeline.
    pub fn locate(&mut self, position: i64) {
        self.position = position;
    }

    /// The timeline as plug-ins read it, for a block at `position`.
    fn time_info(&self, position: i64) -> TimeInfo {
        let rate = self.host.rate();
        let t = self.transport;
        let mut info = TimeInfo {
            sample_pos: position as f64,
            sample_rate: rate,
            ..TimeInfo::default()
        };
        if t.playing {
            info.flags |= time::PLAYING;
        }
        if t.tempo > 0.0 && rate > 0.0 {
            let quarters = position as f64 / rate * t.tempo / 60.0;
            info.tempo = t.tempo;
            info.quarters = quarters;
            info.flags |= time::TEMPO_VALID | time::QUARTERS_VALID;
            if t.numerator > 0 && t.denominator > 0 {
                let bar = f64::from(t.numerator) * 4.0 / f64::from(t.denominator);
                info.bar_start = (quarters / bar).floor() * bar;
                info.numerator = t.numerator;
                info.denominator = t.denominator;
                info.flags |= time::BARS_VALID | time::SIGNATURE_VALID;
            }
            if let Some((a, b)) = t.cycle {
                info.cycle_start = a;
                info.cycle_end = b;
                info.flags |= time::CYCLE_VALID | time::CYCLE_ACTIVE;
            }
        }
        info
    }

    /// Sets parameter `index` to `value` behind the guard.
    fn set_parameter(&self, index: u32, value: f32) {
        if self.crashed() {
            return;
        }
        let Some(f) = self.object().set_parameter else {
            return;
        };
        // SAFETY: the plug-in's own setter.
        if let Err((code, at)) = unsafe { guard::set(f, self.plugin, index as i32, value) } {
            self.module.crash(&self.name, "setParameter", code, at);
        }
    }

    /// Processes `channels` (planar, the host's) in place: its inputs
    /// take the host's channels (one channel on both sides of a stereo
    /// plug-in), its outputs give them back (a stereo pair's mean for one
    /// channel). The parameter changes queued go with the first sample,
    /// ramps' points each at its sample (the block cut there). A library
    /// that crashed leaves the sound as it came.
    pub fn process(&mut self, channels: &mut [Vec<f32>]) {
        if self.run.is_none() || self.crashed() {
            return;
        }
        let host = channels.len();
        let n = channels.first().map_or(0, Vec::len);
        if host == 0 || n == 0 {
            return;
        }
        for (id, v) in std::mem::take(&mut self.pending) {
            self.set_parameter(id, v);
        }
        let mut ramps = std::mem::take(&mut self.ramps);
        ramps.sort_by_key(|r| r.1);
        let mut next = 0;
        let mut start = 0;
        while start < n {
            // The ramps' points due by now.
            while let Some(&(id, at, v)) = ramps.get(next)
                && at <= start
            {
                self.set_parameter(id, v);
                next += 1;
            }
            let Some(run) = self.run.as_ref() else {
                return;
            };
            let mut end = (start + run.block).min(n);
            if let Some(&(_, at, _)) = ramps.get(next)
                && at < end
            {
                end = at;
            }
            if !self.piece(channels, start, end) {
                return;
            }
            start = end;
        }
        for &(id, _, v) in &ramps[next..] {
            self.set_parameter(id, v);
        }
    }

    /// Processes samples `[start, end)` of `channels`; whether it went
    /// without a crash.
    fn piece(&mut self, channels: &mut [Vec<f32>], start: usize, end: usize) -> bool {
        let host = channels.len();
        let len = end - start;
        let time = self.time_info(self.position);
        // SAFETY: the plug-in reads it on this thread while processing,
        // not now.
        unsafe { self.host.set_time(time) };
        let (replace, adding) = {
            let o = self.object();
            (o.process, o.process_adding)
        };
        let Some(run) = self.run.as_mut() else {
            return false;
        };
        // The host's channels on its first inputs (one channel on both
        // sides of a pair); silence on the others (a sidechain's).
        for (k, buf) in run.ins.iter_mut().enumerate() {
            if k < host {
                buf[..len].copy_from_slice(&channels[k][start..end]);
            } else if host == 1 && k == 1 {
                buf[..len].copy_from_slice(&channels[0][start..end]);
            } else {
                buf[..len].fill(0.0);
            }
        }
        if replace.is_none() {
            for buf in &mut run.outs {
                buf[..len].fill(0.0);
            }
        }
        let mut ins: Vec<*mut f32> = run.ins.iter_mut().map(|b| b.as_mut_ptr()).collect();
        let mut outs: Vec<*mut f32> = run.outs.iter_mut().map(|b| b.as_mut_ptr()).collect();
        // A null pointer for an empty list (generators, analyzers).
        let ptr = |v: &mut Vec<*mut f32>| {
            if v.is_empty() {
                std::ptr::null_mut()
            } else {
                v.as_mut_ptr()
            }
        };
        let (pin, pout) = (ptr(&mut ins), ptr(&mut outs));
        let Some(f) = replace.or(adding) else {
            return false;
        };
        // SAFETY: the plug-in's processing, with `len` samples for each of
        // its inputs and outputs, kept until it returns.
        let done = unsafe { guard::process(f, self.plugin, pin, pout, len as i32) };
        if let Err((code, at)) = done {
            self.module.crash(&self.name, "process", code, at);
            return false;
        }
        let Some(run) = self.run.as_ref() else {
            return false;
        };
        let out = &run.outs;
        match (host, out.len()) {
            (_, 0) => {}
            (1, m) if m >= 2 => {
                for (i, s) in channels[0][start..end].iter_mut().enumerate() {
                    *s = 0.5 * (out[0][i] + out[1][i]);
                }
            }
            (h, m) => {
                for k in 0..h.min(m) {
                    channels[k][start..end].copy_from_slice(&out[k][..len]);
                }
            }
        }
        self.position += len as i64;
        true
    }

    /// Queues parameter `id`'s new value (normalized) for the next block.
    pub fn queue(&mut self, id: u32, value: f64) {
        self.pending.retain(|&(p, _)| p != id);
        self.pending.push((id, value.clamp(0.0, 1.0) as f32));
    }

    /// Queues parameter `id`'s values along the next block (sample,
    /// value).
    pub fn queue_ramp(&mut self, id: u32, points: &[(usize, f64)]) {
        self.ramps.retain(|&(p, _, _)| p != id);
        self.ramps.extend(
            points
                .iter()
                .map(|&(at, v)| (id, at, v.clamp(0.0, 1.0) as f32)),
        );
    }

    pub fn has_queued(&self) -> bool {
        !self.pending.is_empty() || !self.ramps.is_empty()
    }

    /// Hands the queued changes over now.
    pub fn flush(&mut self) {
        for (id, v) in std::mem::take(&mut self.pending) {
            self.set_parameter(id, v);
        }
    }

    /// Sets parameter `id` (normalized) now.
    pub fn set(&mut self, id: u32, value: f64) {
        self.pending.retain(|&(p, _)| p != id);
        self.set_parameter(id, value.clamp(0.0, 1.0) as f32);
    }

    /// Its parameters: their names and units.
    pub fn params(&self) -> Vec<Param> {
        if self.crashed() {
            return Vec::new();
        }
        (0..self.object().params.max(0))
            .map(|i| Param {
                index: i as u32,
                name: self.text("a parameter's name", op::PARAM_NAME, i),
                label: self.text("a parameter's units", op::PARAM_LABEL, i),
            })
            .collect()
    }

    /// Parameter `id`'s value (normalized).
    pub fn value(&self, id: u32) -> Option<f32> {
        if self.crashed() {
            return None;
        }
        let f = self.object().get_parameter?;
        // SAFETY: the plug-in's own getter.
        match unsafe { guard::get(f, self.plugin, id as i32) } {
            Ok(v) => Some(v),
            Err((code, at)) => {
                self.module.crash(&self.name, "getParameter", code, at);
                None
            }
        }
    }

    /// Parameter `id`'s value as the plug-in writes it.
    pub fn display(&self, id: u32) -> String {
        self.text("a parameter's text", op::PARAM_DISPLAY, id as i32)
    }

    /// Its programs (the presets it holds), by name.
    pub fn programs(&self) -> Vec<String> {
        let n = self.object().programs.max(0);
        (0..n)
            .map(|i| {
                let mut buf = [0u8; TEXT];
                let ok = self.dispatch(
                    "a program's name",
                    op::PROGRAM_NAME_INDEXED,
                    i,
                    -1,
                    buf.as_mut_ptr().cast(),
                    0.0,
                ) == Some(1);
                let name = text_of(&buf);
                if ok && !name.is_empty() {
                    name
                } else {
                    format!("{}", i + 1)
                }
            })
            .collect()
    }

    /// The program chosen now: as the plug-in tells, else (it answers -1)
    /// the last one chosen through BananaV, the first at the start.
    pub fn program(&self) -> Option<usize> {
        if self.object().programs <= 0 {
            return None;
        }
        let p = self.dispatch(
            "its program",
            op::GET_PROGRAM,
            0,
            0,
            std::ptr::null_mut(),
            0.0,
        )?;
        usize::try_from(p).ok().or(Some(self.chosen.unwrap_or(0)))
    }

    /// Chooses program `index`; whether it has it.
    pub fn set_program(&mut self, index: usize) -> bool {
        if index >= self.object().programs.max(0) as usize {
            return false;
        }
        let done = self
            .dispatch(
                "a program",
                op::SET_PROGRAM,
                0,
                index as isize,
                std::ptr::null_mut(),
                0.0,
            )
            .is_some();
        if done {
            self.chosen = Some(index);
        }
        done
    }

    /// Its whole bank as the plug-in's chunk.
    fn chunk(&self) -> Option<Vec<u8>> {
        let mut data: *mut c_void = std::ptr::null_mut();
        let len = self.dispatch(
            "getChunk",
            op::GET_CHUNK,
            BANK,
            0,
            (&mut data as *mut *mut c_void).cast(),
            0.0,
        )?;
        if data.is_null() || len <= 0 {
            return None;
        }
        // SAFETY: the plug-in's chunk, `len` bytes, valid until its next
        // call.
        Some(unsafe { std::slice::from_raw_parts(data.cast::<u8>(), len as usize) }.to_vec())
    }

    /// Its state: the program and its parameters' values, or its bank's
    /// chunk when it keeps one.
    pub fn state(&self) -> Option<Vec<u8>> {
        if self.crashed() {
            return None;
        }
        let o = self.object();
        let program = self.program().map_or(-1, |p| p as i32);
        let mut out = STATE_MARK.to_vec();
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&program.to_le_bytes());
        if o.flags & flags::PROGRAM_CHUNKS != 0
            && let Some(chunk) = self.chunk()
        {
            out.push(STATE_CHUNK);
            out.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
            out.extend_from_slice(&chunk);
            return Some(out);
        }
        let values: Vec<f32> = (0..o.params.max(0) as u32)
            .map(|i| self.value(i).unwrap_or(0.0))
            .collect();
        out.push(STATE_PARAMS);
        out.extend_from_slice(&((values.len() * 4) as u32).to_le_bytes());
        for v in values {
            out.extend_from_slice(&v.to_le_bytes());
        }
        Some(out)
    }

    /// Gives it a state it had ([`Local::state`]'s, or a preset file's:
    /// `.fxp`, `.fxb`). Whether it took it.
    pub fn set_state(&mut self, bytes: &[u8]) -> bool {
        if self.crashed() {
            return false;
        }
        if let Some(preset) = crate::preset::read(bytes) {
            return self.set_preset(&preset);
        }
        if bytes.get(..4) != Some(STATE_MARK) || bytes.len() < 17 {
            return false;
        }
        let program = i32::from_le_bytes(bytes[8..12].try_into().unwrap_or_default());
        let kind = bytes[12];
        let len = u32::from_le_bytes(bytes[13..17].try_into().unwrap_or_default()) as usize;
        let Some(data) = bytes.get(17..17 + len) else {
            return false;
        };
        if let Ok(p) = usize::try_from(program) {
            self.set_program(p);
        }
        match kind {
            STATE_CHUNK => self.set_chunk(BANK, data),
            STATE_PARAMS => {
                for (i, v) in data.as_chunks::<4>().0.iter().enumerate() {
                    let v = f32::from_le_bytes(*v);
                    self.set_parameter(i as u32, v);
                }
                true
            }
            _ => false,
        }
    }

    fn set_chunk(&self, which: i32, data: &[u8]) -> bool {
        let mut copy = data.to_vec();
        self.dispatch(
            "setChunk",
            op::SET_CHUNK,
            which,
            copy.len() as isize,
            copy.as_mut_ptr().cast(),
            0.0,
        )
        .is_some()
    }

    /// A preset file's settings: a program's values or chunk, or a bank's.
    pub(crate) fn set_preset(&mut self, p: &crate::preset::Preset) -> bool {
        use crate::preset::Body;
        match &p.body {
            Body::Program { values, .. } => {
                for (i, &v) in values.iter().enumerate() {
                    self.set_parameter(i as u32, v);
                }
                true
            }
            Body::ProgramChunk { chunk, .. } => self.set_chunk(1, chunk),
            Body::BankChunk { chunk, .. } => self.set_chunk(BANK, chunk),
            Body::Bank { programs, current } => {
                for (k, values) in programs.iter().enumerate() {
                    self.set_program(k);
                    for (i, &v) in values.iter().enumerate() {
                        self.set_parameter(i as u32, v);
                    }
                }
                self.set_program(*current as usize);
                true
            }
        }
    }

    /// The program as a preset file's body.
    pub(crate) fn preset(&self) -> Option<crate::preset::Preset> {
        use crate::preset::{Body, Preset};
        let o = self.object();
        let name = self
            .program()
            .and_then(|p| self.programs().get(p).cloned())
            .unwrap_or_default();
        let body = if o.flags & flags::PROGRAM_CHUNKS != 0 {
            let mut data: *mut c_void = std::ptr::null_mut();
            let len = self.dispatch(
                "getChunk",
                op::GET_CHUNK,
                1,
                0,
                (&mut data as *mut *mut c_void).cast(),
                0.0,
            )?;
            if data.is_null() || len <= 0 {
                return None;
            }
            // SAFETY: as in `chunk`.
            let chunk = unsafe { std::slice::from_raw_parts(data.cast::<u8>(), len as usize) };
            Body::ProgramChunk {
                name,
                chunk: chunk.to_vec(),
            }
        } else {
            Body::Program {
                name,
                values: (0..o.params.max(0) as u32)
                    .map(|i| self.value(i).unwrap_or(0.0))
                    .collect(),
            }
        };
        Some(Preset {
            unique_id: o.unique_id,
            version: o.version,
            params: o.params.max(0) as u32,
            body,
        })
    }

    /// All its programs as a bank file's body: the bank's chunk where the
    /// plug-in keeps chunks, else each program's values (each program
    /// chosen in turn to read them, the current one chosen again after).
    pub(crate) fn bank(&mut self) -> Option<crate::preset::Preset> {
        use crate::preset::{Body, Preset};
        let (chunks, params, unique_id, version) = {
            let o = self.object();
            (
                o.flags & flags::PROGRAM_CHUNKS != 0,
                o.params.max(0) as u32,
                o.unique_id,
                o.version,
            )
        };
        let current = self.program().unwrap_or(0);
        let body = if chunks {
            let mut data: *mut c_void = std::ptr::null_mut();
            let len = self.dispatch(
                "getChunk",
                op::GET_CHUNK,
                BANK,
                0,
                (&mut data as *mut *mut c_void).cast(),
                0.0,
            )?;
            if data.is_null() || len <= 0 {
                return None;
            }
            // SAFETY: as in `chunk`.
            let chunk = unsafe { std::slice::from_raw_parts(data.cast::<u8>(), len as usize) };
            Body::BankChunk {
                chunk: chunk.to_vec(),
                current: current as i32,
            }
        } else {
            let count = self.programs().len().max(1);
            let mut programs = Vec::with_capacity(count);
            for k in 0..count {
                self.set_program(k);
                programs.push((0..params).map(|i| self.value(i).unwrap_or(0.0)).collect());
            }
            self.set_program(current);
            Body::Bank {
                programs,
                current: current as i32,
            }
        };
        Some(Preset {
            unique_id,
            version,
            params,
            body,
        })
    }

    /// Its latency (samples).
    pub fn latency(&self) -> u32 {
        self.object().latency.max(0) as u32
    }

    /// How long its sound lasts after its input stops (samples; 0 when it
    /// does not say).
    pub fn tail(&self) -> Option<u32> {
        let t = self
            .dispatch("its tail", op::TAIL_SIZE, 0, 0, std::ptr::null_mut(), 0.0)
            .unwrap_or(0);
        Some(if t <= 1 {
            0
        } else {
            t.min(u32::MAX as isize) as u32
        })
    }

    pub fn has_editor(&self) -> bool {
        self.object().flags & flags::HAS_EDITOR != 0
    }

    /// Its editor's size (pixels), when it tells.
    pub fn editor_size(&self) -> Option<(i32, i32)> {
        let mut rect: *mut Rect = std::ptr::null_mut();
        self.dispatch(
            "its editor's size",
            op::EDITOR_RECT,
            0,
            0,
            (&mut rect as *mut *mut Rect).cast(),
            0.0,
        )?;
        if rect.is_null() {
            return None;
        }
        // SAFETY: the plug-in's rectangle.
        let r = unsafe { *rect };
        let (w, h) = (
            i32::from(r.right) - i32::from(r.left),
            i32::from(r.bottom) - i32::from(r.top),
        );
        (w > 0 && h > 0).then_some((w, h))
    }

    /// Shows its editor in window `parent`; `resize` resizes that window
    /// when the plug-in asks for another size.
    ///
    /// # Safety
    /// `parent` is a live window of this system's kind (Windows' HWND,
    /// X11's window), kept until [`Local::close_editor`].
    pub unsafe fn open_editor(&mut self, parent: *mut c_void, resize: Resize) -> bool {
        if self.editor_open {
            return true;
        }
        if !self.has_editor() {
            return false;
        }
        self.host.set_resize(Some(resize));
        let ok = self
            .dispatch("its editor", op::EDITOR_OPEN, 0, 0, parent, 0.0)
            .is_some();
        self.editor_open = ok;
        ok
    }

    /// Lets its editor do its work (a frame's idle).
    pub fn idle_editor(&self) {
        if self.editor_open {
            self.dispatch(
                "its editor's idle",
                op::EDITOR_IDLE,
                0,
                0,
                std::ptr::null_mut(),
                0.0,
            );
        }
    }

    /// Takes its editor out of its window (before the window goes).
    pub fn close_editor(&mut self) {
        if std::mem::take(&mut self.editor_open) {
            self.dispatch(
                "closing its editor",
                op::EDITOR_CLOSE,
                0,
                0,
                std::ptr::null_mut(),
                0.0,
            );
        }
        self.host.set_resize(None);
    }

    pub fn editor_open(&self) -> bool {
        self.editor_open
    }

    /// What its editor reported since last asked.
    pub fn take_edits(&self) -> Vec<Edit> {
        self.host.take_edits()
    }

    /// A shell's plug-ins: their numbers and names.
    pub fn shell_plugins(&self) -> Vec<(i32, String)> {
        let mut out = Vec::new();
        for _ in 0..4096 {
            let mut buf = [0u8; TEXT];
            let id = self
                .dispatch(
                    "a shell's plug-in",
                    op::SHELL_NEXT,
                    0,
                    0,
                    buf.as_mut_ptr().cast(),
                    0.0,
                )
                .unwrap_or(0);
            if id == 0 {
                break;
            }
            out.push((id as i32, text_of(&buf)));
        }
        out
    }

    /// Whether it asks for a parameter to be automated.
    pub fn can_automate(&self, id: u32) -> bool {
        self.dispatch(
            "can be automated",
            op::CAN_BE_AUTOMATED,
            id as i32,
            0,
            std::ptr::null_mut(),
            0.0,
        )
        .is_none_or(|r| r != 0)
    }
}

impl Drop for Local {
    fn drop(&mut self) {
        if self.crashed() {
            // Never called again, not even to close: its object is left.
            return;
        }
        self.close_editor();
        self.stop();
        // The object is the plug-in's to free from now on.
        self.dispatch("close", op::CLOSE, 0, 0, std::ptr::null_mut(), 0.0);
    }
}
