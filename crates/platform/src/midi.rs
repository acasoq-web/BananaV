//! MIDI ports (Options › Preferences › MIDI): the system's MIDI outputs
//! and inputs, by name; an output open for sending (from any thread),
//! and an input open handing what comes in to a function.

/// The MIDI output ports, by name, in the system's order.
pub fn midi_outputs() -> Vec<String> {
    imp::outputs()
}

/// The MIDI input ports, by name, in the system's order.
pub fn midi_inputs() -> Vec<String> {
    imp::inputs()
}

/// An open MIDI output port (the first of its name). Messages may be sent
/// from any thread; it closes when dropped.
#[derive(Debug)]
pub struct MidiOut {
    imp: std::sync::Mutex<imp::Out>,
}

impl MidiOut {
    pub fn open(name: &str) -> Result<MidiOut, String> {
        Ok(MidiOut {
            imp: std::sync::Mutex::new(imp::Out::open(name)?),
        })
    }

    /// Sends a short message: a status byte and up to two data bytes.
    pub fn send(&self, bytes: &[u8]) {
        let mut word = 0u32;
        for (i, &b) in bytes.iter().take(3).enumerate() {
            word |= (b as u32) << (8 * i);
        }
        if let Ok(mut out) = self.imp.lock() {
            out.short(word);
        }
    }

    /// Sends a system-exclusive message (from `F0` to `F7`), waiting until
    /// it is out.
    pub fn send_sysex(&self, bytes: &[u8]) -> Result<(), String> {
        match self.imp.lock() {
            Ok(mut out) => out.long(bytes),
            Err(_) => Err("the MIDI output is unusable".into()),
        }
    }
}

/// What is done with each short message an input brings (status and data
/// bytes, unused ones zero). It runs on the system's MIDI thread: it must
/// be quick, and may send on outputs.
pub type MidiHandler = Box<dyn FnMut([u8; 3]) + Send>;

/// An open MIDI input port (the first of its name) handing each short
/// message to its handler; it closes when dropped.
#[derive(Debug)]
pub struct MidiIn {
    _imp: imp::In,
}

impl MidiIn {
    pub fn open(name: &str, handler: MidiHandler) -> Result<MidiIn, String> {
        Ok(MidiIn {
            _imp: imp::In::open(name, handler)?,
        })
    }
}

/// The bytes of a short message packed as the system gives them.
pub fn unpack(word: u32) -> [u8; 3] {
    [word as u8, (word >> 8) as u8, (word >> 16) as u8]
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod imp {
    use windows::Win32::Media::Audio::{
        CALLBACK_FUNCTION, CALLBACK_NULL, HMIDIIN, HMIDIOUT, MIDIHDR, MIDIINCAPSW, MIDIOUTCAPSW,
        midiInClose, midiInGetDevCapsW, midiInGetNumDevs, midiInOpen, midiInReset, midiInStart,
        midiInStop, midiOutClose, midiOutGetDevCapsW, midiOutGetNumDevs, midiOutLongMsg,
        midiOutOpen, midiOutPrepareHeader, midiOutReset, midiOutShortMsg, midiOutUnprepareHeader,
    };

    use super::MidiHandler;

    /// A short message came in (`MIM_DATA`).
    const MIM_DATA: u32 = 0x3C3;
    /// A long message is still going out (`MIDIERR_STILLPLAYING`).
    const STILL_PLAYING: u32 = 65;

    fn name(chars: &[u16]) -> String {
        let end = chars.iter().position(|&c| c == 0).unwrap_or(chars.len());
        String::from_utf16_lossy(&chars[..end])
    }

    pub fn outputs() -> Vec<String> {
        // SAFETY: no arguments.
        let n = unsafe { midiOutGetNumDevs() };
        (0..n as usize)
            .filter_map(|i| {
                let mut caps = MIDIOUTCAPSW::default();
                let size = std::mem::size_of::<MIDIOUTCAPSW>() as u32;
                // SAFETY: `caps` is writable and `size` bytes long.
                let r = unsafe { midiOutGetDevCapsW(i, &mut caps, size) };
                // A copy: the structure is packed.
                let chars = caps.szPname;
                (r == 0).then(|| name(&chars))
            })
            .collect()
    }

    pub fn inputs() -> Vec<String> {
        // SAFETY: no arguments.
        let n = unsafe { midiInGetNumDevs() };
        (0..n as usize)
            .filter_map(|i| {
                let mut caps = MIDIINCAPSW::default();
                let size = std::mem::size_of::<MIDIINCAPSW>() as u32;
                // SAFETY: `caps` is writable and `size` bytes long.
                let r = unsafe { midiInGetDevCapsW(i, &mut caps, size) };
                let chars = caps.szPname;
                (r == 0).then(|| name(&chars))
            })
            .collect()
    }

    #[derive(Debug)]
    pub struct Out(HMIDIOUT);

    // SAFETY: a MIDI output handle may be used from any thread; `MidiOut`
    // sends through it one message at a time.
    unsafe impl Send for Out {}

    impl Out {
        pub fn open(port: &str) -> Result<Out, String> {
            let id = outputs()
                .iter()
                .position(|n| n == port)
                .ok_or_else(|| format!("no MIDI output named {port}"))?;
            let mut h = HMIDIOUT::default();
            // SAFETY: `h` is writable; no callback.
            let r = unsafe { midiOutOpen(&mut h, id as u32, None, None, CALLBACK_NULL) };
            if r != 0 {
                return Err(format!("the MIDI output {port} did not open ({r})"));
            }
            Ok(Out(h))
        }

        pub fn short(&mut self, word: u32) {
            // SAFETY: the handle is open while `self` lives.
            unsafe { midiOutShortMsg(self.0, word) };
        }

        pub fn long(&mut self, bytes: &[u8]) -> Result<(), String> {
            let mut data = bytes.to_vec();
            let mut hdr = Box::new(MIDIHDR {
                lpData: windows::core::PSTR(data.as_mut_ptr()),
                dwBufferLength: data.len() as u32,
                ..Default::default()
            });
            let size = std::mem::size_of::<MIDIHDR>() as u32;
            // SAFETY: `hdr` and `data` stay alive and in place until the
            // header is unprepared below.
            let r = unsafe { midiOutPrepareHeader(self.0, &mut *hdr, size) };
            if r != 0 {
                return Err(format!("the MIDI message was not prepared ({r})"));
            }
            // SAFETY: as above; the header is prepared.
            let sent = unsafe { midiOutLongMsg(self.0, &*hdr, size) };
            // Unprepared once the driver is done with it (a second at most).
            for _ in 0..1000 {
                // SAFETY: as above.
                let r = unsafe { midiOutUnprepareHeader(self.0, &mut *hdr, size) };
                if r != STILL_PLAYING {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            drop(data);
            if sent != 0 {
                return Err(format!("the MIDI message was not sent ({sent})"));
            }
            Ok(())
        }
    }

    impl Drop for Out {
        fn drop(&mut self) {
            // SAFETY: opened by `open`, closed once.
            unsafe {
                midiOutReset(self.0);
                midiOutClose(self.0);
            }
        }
    }

    /// An open input, and its handler (where the system's callback finds
    /// it).
    #[derive(Debug)]
    pub struct In {
        handle: HMIDIIN,
        handler: *mut Handler,
    }

    struct Handler(MidiHandler);

    impl std::fmt::Debug for Handler {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("Handler")
        }
    }

    // SAFETY: the handler is `Send`; the handle is only closed on drop.
    unsafe impl Send for In {}

    /// Called by the system for what comes in; `instance` is the input's
    /// handler.
    unsafe extern "system" fn came_in(
        _input: HMIDIIN,
        msg: u32,
        instance: usize,
        param1: usize,
        _param2: usize,
    ) {
        if msg == MIM_DATA && instance != 0 {
            // SAFETY: `instance` is the `Handler` boxed by `In::open`,
            // freed only after the input is closed (no callbacks then).
            let handler = unsafe { &mut *(instance as *mut Handler) };
            (handler.0)(super::unpack(param1 as u32));
        }
    }

    impl In {
        pub fn open(port: &str, handler: MidiHandler) -> Result<In, String> {
            let id = inputs()
                .iter()
                .position(|n| n == port)
                .ok_or_else(|| format!("no MIDI input named {port}"))?;
            let handler = Box::into_raw(Box::new(Handler(handler)));
            let mut h = HMIDIIN::default();
            let callback = came_in as unsafe extern "system" fn(HMIDIIN, u32, usize, usize, usize);
            // SAFETY: `h` is writable; the callback has the signature the
            // system calls, and its instance is the handler, alive until
            // the input is closed.
            let r = unsafe {
                midiInOpen(
                    &mut h,
                    id as u32,
                    Some(callback as usize),
                    Some(handler as usize),
                    CALLBACK_FUNCTION,
                )
            };
            if r != 0 {
                // SAFETY: made above and never handed out.
                drop(unsafe { Box::from_raw(handler) });
                return Err(format!("the MIDI input {port} did not open ({r})"));
            }
            // SAFETY: `h` was just opened.
            unsafe { midiInStart(h) };
            Ok(In { handle: h, handler })
        }
    }

    impl Drop for In {
        fn drop(&mut self) {
            // SAFETY: opened by `open`, closed once; after closing no
            // callback runs, so the handler can go.
            unsafe {
                midiInStop(self.handle);
                midiInReset(self.handle);
                midiInClose(self.handle);
                drop(Box::from_raw(self.handler));
            }
        }
    }
}

/// Linux: the ALSA sequencer's ports, named "client: port" (the same
/// ports every MIDI program of the desktop sees).
#[cfg(target_os = "linux")]
mod imp {
    use super::MidiHandler;
    use alsa::seq::{Addr, ClientIter, MidiEvent, PortCap, PortIter, PortSubscribe, PortType, Seq};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// The system's ports with these capabilities: names and addresses.
    fn ports(caps: PortCap) -> Vec<(String, Addr)> {
        let Ok(seq) = Seq::open(None, None, false) else {
            return Vec::new();
        };
        let own = seq.client_id().ok();
        let mut out = Vec::new();
        for c in ClientIter::new(&seq) {
            let client = c.get_client();
            // The system's timer and announcements, and BananaV itself.
            if client == 0 || Some(client) == own {
                continue;
            }
            let name = c.get_name().unwrap_or("").to_string();
            for p in PortIter::new(&seq, client) {
                let cap = p.get_capability();
                if cap.contains(caps) && !cap.contains(PortCap::NO_EXPORT) {
                    let port = p.get_name().unwrap_or("");
                    out.push((
                        format!("{name}: {port}"),
                        Addr {
                            client,
                            port: p.get_port(),
                        },
                    ));
                }
            }
        }
        out
    }

    fn writable() -> PortCap {
        PortCap::WRITE | PortCap::SUBS_WRITE
    }

    fn readable() -> PortCap {
        PortCap::READ | PortCap::SUBS_READ
    }

    pub fn outputs() -> Vec<String> {
        ports(writable()).into_iter().map(|(n, _)| n).collect()
    }

    pub fn inputs() -> Vec<String> {
        ports(readable()).into_iter().map(|(n, _)| n).collect()
    }

    fn find(name: &str, caps: PortCap) -> Result<Addr, String> {
        ports(caps)
            .into_iter()
            .find(|(n, _)| n == name)
            .map(|(_, a)| a)
            .ok_or_else(|| format!("no MIDI port named {name}"))
    }

    /// BananaV's client with one port, subscribed to `other` (as the sender
    /// when `out`, else as the receiver).
    fn connect(other: Addr, out: bool) -> Result<(Seq, i32), String> {
        let err = |e: alsa::Error| e.to_string();
        let dir = if out {
            alsa::Direction::Playback
        } else {
            alsa::Direction::Capture
        };
        let seq = Seq::open(None, Some(dir), false).map_err(err)?;
        let _ = seq.set_client_name(c"BananaV");
        let (name, caps) = if out {
            (c"BananaV out", readable())
        } else {
            (c"BananaV in", writable())
        };
        let port = seq
            .create_simple_port(name, caps, PortType::MIDI_GENERIC | PortType::APPLICATION)
            .map_err(err)?;
        let own = Addr {
            client: seq.client_id().map_err(err)?,
            port,
        };
        let subs = PortSubscribe::empty().map_err(err)?;
        let (from, to) = if out { (own, other) } else { (other, own) };
        subs.set_sender(from);
        subs.set_dest(to);
        seq.subscribe_port(&subs).map_err(err)?;
        Ok((seq, port))
    }

    /// An open output: BananaV's port subscribed to it.
    pub struct Out {
        seq: Seq,
        port: i32,
    }

    impl std::fmt::Debug for Out {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Out").field("port", &self.port).finish()
        }
    }

    impl Out {
        pub fn open(name: &str) -> Result<Out, String> {
            let (seq, port) = connect(find(name, writable())?, true)?;
            Ok(Out { seq, port })
        }

        /// The bytes as the sequencer's events (an encoder each time: it
        /// cannot go to another thread, and MIDI comes slowly).
        fn send_bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
            let mut encoder =
                MidiEvent::new(bytes.len().max(16) as u32).map_err(|e| e.to_string())?;
            let mut rest = bytes;
            while !rest.is_empty() {
                let (used, event) = encoder.encode(rest).map_err(|e| e.to_string())?;
                if let Some(mut event) = event {
                    event.set_source(self.port);
                    event.set_subs();
                    event.set_direct();
                    self.seq
                        .event_output_direct(&mut event)
                        .map_err(|e| e.to_string())?;
                }
                if used == 0 {
                    break;
                }
                rest = &rest[used.min(rest.len())..];
            }
            Ok(())
        }

        pub fn short(&mut self, word: u32) {
            let b = super::unpack(word);
            // How many bytes the status byte takes.
            let len = match b[0] {
                0xC0..=0xDF | 0xF1 | 0xF3 => 2,
                0xF6 | 0xF8..=0xFF => 1,
                _ => 3,
            };
            let _ = self.send_bytes(&b[..len]);
        }

        pub fn long(&mut self, bytes: &[u8]) -> Result<(), String> {
            self.send_bytes(bytes)
        }
    }

    /// An open input: a thread waits for its events and hands each short
    /// message to the handler, until dropped.
    #[derive(Debug)]
    pub struct In {
        stop: Arc<AtomicBool>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl Drop for In {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
        }
    }

    impl In {
        pub fn open(name: &str, mut handler: MidiHandler) -> Result<In, String> {
            let (seq, _) = connect(find(name, readable())?, false)?;
            let stop = Arc::new(AtomicBool::new(false));
            let flag = stop.clone();
            let thread = std::thread::Builder::new()
                .name("MIDI in".into())
                .spawn(move || {
                    // Made here: it cannot go to another thread.
                    let Ok(decoder) = MidiEvent::new(256) else {
                        return;
                    };
                    decoder.enable_running_status(false);
                    let Ok(mut fds) =
                        alsa::poll::Descriptors::get(&(&seq, Some(alsa::Direction::Capture)))
                    else {
                        return;
                    };
                    let mut input = seq.input();
                    let mut bytes = [0u8; 16];
                    while !flag.load(Ordering::Relaxed) {
                        // A tenth of a second at most, to notice the stop.
                        if alsa::poll::poll(&mut fds, 100).unwrap_or(0) == 0 {
                            continue;
                        }
                        while input.event_input_pending(true).unwrap_or(0) > 0 {
                            let Ok(mut event) = input.event_input() else {
                                break;
                            };
                            if let Ok(n) = decoder.decode(&mut bytes, &mut event)
                                && (1..=3).contains(&n)
                            {
                                let mut msg = [0u8; 3];
                                msg[..n].copy_from_slice(&bytes[..n]);
                                handler(msg);
                            }
                        }
                    }
                })
                .map_err(|e| e.to_string())?;
            Ok(In {
                stop,
                thread: Some(thread),
            })
        }
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod imp {
    use super::MidiHandler;

    pub fn outputs() -> Vec<String> {
        Vec::new()
    }

    pub fn inputs() -> Vec<String> {
        Vec::new()
    }

    #[derive(Debug)]
    pub struct Out;

    impl Out {
        pub fn open(_port: &str) -> Result<Out, String> {
            Err("MIDI ports are not available on this system".into())
        }

        pub fn short(&mut self, _word: u32) {}

        pub fn long(&mut self, _bytes: &[u8]) -> Result<(), String> {
            Ok(())
        }
    }

    #[derive(Debug)]
    pub struct In;

    impl In {
        pub fn open(_port: &str, _handler: MidiHandler) -> Result<In, String> {
            Err("MIDI ports are not available on this system".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports_are_listed_by_name() {
        // Whatever this computer has: names, never empty ones.
        for name in midi_outputs().iter().chain(&midi_inputs()) {
            assert!(!name.trim().is_empty());
        }
        assert!(MidiOut::open("no such output").is_err());
        assert!(MidiIn::open("no such input", Box::new(|_| {})).is_err());
        assert_eq!(unpack(0x007F_3C90), [0x90, 0x3C, 0x7F]);
    }
}
