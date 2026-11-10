//! What BananaV's 32-bit bridge (`bananav-bridge32.exe`) and the plug-in
//! hosts that use it (`bv-vst2`, `bv-vst3`) share: a 32-bit plug-in runs in
//! a process of its own, fed through that process's standard input and
//! output.
//!
//! - [`wire`]: the frames they exchange.
//! - [`Process`]: the host's side — the bridge started, requests answered
//!   within deadlines; a bridge that crashes, hangs or ends is gone for
//!   good (the plug-in's sound goes through).
//! - [`serve`]: the bridge's side — the requests answered one by one, the
//!   windows' messages run between them.
//! - [`window`] (Windows): the bridge's window for a plug-in's editor.

use std::ffi::OsString;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Mutex, mpsc};
use std::time::Duration;

#[cfg(windows)]
pub mod window;
pub mod wire;

use wire::{In, Out, read_frame, write_frame};

/// The bridge's program's name.
pub const BRIDGE_EXE: &str = "bananav-bridge32.exe";

/// An answer's first byte: it failed (its reason follows), it is done (its
/// values follow), the plug-in crashed in the bridge (what it was doing,
/// the exception's code and where follow).
pub mod answer {
    pub const FAILED: u8 = 0;
    pub const DONE: u8 = 1;
    pub const CRASHED: u8 = 2;
}

static ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

thread_local! {
    /// This thread has windows a bridged plug-in's may live in: while it
    /// waits for a bridge it answers the messages other threads send them
    /// (see [`windows_here`]).
    static WINDOWS_HERE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Marks this thread (the interface's) as one whose windows a bridged
/// plug-in's editor lives in: while it waits for a bridge, it answers the
/// messages the bridge's windows send them (a window made, focus moving),
/// which would otherwise wait for it as it waits for them.
pub fn windows_here() {
    WINDOWS_HERE.with(|w| w.set(true));
}

/// Answers the messages other threads sent this thread's windows (none
/// is taken from its queue).
#[cfg(windows)]
#[allow(unsafe_code)]
fn answer_sent() {
    #[repr(C)]
    struct Msg {
        hwnd: isize,
        message: u32,
        wparam: usize,
        lparam: isize,
        time: u32,
        pt: [i32; 2],
        private: u32,
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn PeekMessageW(msg: *mut Msg, h: isize, min: u32, max: u32, remove: u32) -> i32;
    }
    const PM_NOREMOVE: u32 = 0;
    const PM_QS_SENDMESSAGE: u32 = 0x0040 << 16;
    let mut msg = std::mem::MaybeUninit::<Msg>::zeroed();
    // SAFETY: a peek that removes nothing: the system answers the sent
    // messages meanwhile.
    unsafe { PeekMessageW(msg.as_mut_ptr(), 0, 0, 0, PM_NOREMOVE | PM_QS_SENDMESSAGE) };
}

/// Turns 32-bit plug-ins on or off (Options › Preferences › VST Effects).
pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// The bridge's program: `BV_BRIDGE32` when set, else beside this program,
/// else (a build of the sources) in the build folder's 32-bit Windows
/// target — a 32-bit program only (a build of the whole sources leaves one
/// for this processor beside BananaV, which could not run 32-bit plug-ins).
pub fn exe() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("BV_BRIDGE32").map(PathBuf::from)
        && is_x86(&p)
    {
        return Some(p);
    }
    let me = std::env::current_exe().ok()?;
    let dir = me.parent()?;
    let beside = dir.join(BRIDGE_EXE);
    if is_x86(&beside) {
        return Some(beside);
    }
    for up in dir.ancestors().take(4) {
        for profile in ["release", "debug"] {
            let p = up
                .join("i686-pc-windows-msvc")
                .join(profile)
                .join(BRIDGE_EXE);
            if is_x86(&p) {
                return Some(p);
            }
        }
    }
    None
}

/// Whether `path` is a 32-bit x86 Windows program (its header's processor).
fn is_x86(path: &Path) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    let machine = || -> Option<u16> {
        let mut f = std::fs::File::open(path).ok()?;
        let mut head = [0u8; 64];
        f.read_exact(&mut head).ok()?;
        if &head[..2] != b"MZ" {
            return None;
        }
        let pe = u32::from_le_bytes(head[60..64].try_into().ok()?);
        f.seek(SeekFrom::Start(u64::from(pe))).ok()?;
        let mut sig = [0u8; 6];
        f.read_exact(&mut sig).ok()?;
        (&sig[..4] == b"PE\0\0").then(|| u16::from_le_bytes([sig[4], sig[5]]))
    };
    path.is_file() && machine() == Some(0x014C)
}

/// The bridge's thread joins a single-threaded apartment with OLE, as
/// BananaV's interface thread (plug-ins' editors may use drag and drop or
/// the clipboard). Called before a plug-in is loaded.
#[allow(unsafe_code)]
pub fn apartment() {
    #[cfg(windows)]
    {
        #[link(name = "ole32")]
        unsafe extern "system" {
            fn OleInitialize(reserved: *mut std::ffi::c_void) -> i32;
        }
        // SAFETY: a plain call; the apartment ends with the process.
        unsafe { OleInitialize(std::ptr::null_mut()) };
    }
}

/// A done answer, its values from `fill`.
pub fn done(fill: impl FnOnce(&mut Out)) -> Vec<u8> {
    let mut out = Out(vec![answer::DONE]);
    fill(&mut out);
    out.0
}

/// A failed answer, with `why`.
pub fn failed(why: &str) -> Vec<u8> {
    let mut out = Out(vec![answer::FAILED]);
    out.str(why);
    out.0
}

/// The answer once the plug-in crashed in the bridge.
pub fn crashed(doing: &str, code: u32, at: &str) -> Vec<u8> {
    let mut out = Out(vec![answer::CRASHED]);
    out.str(doing).u32(code).str(at);
    out.0
}

/// Why a request got no values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lost {
    /// The plug-in refused it (its reason): the bridge goes on.
    Failed(String),
    /// The plug-in crashed in the bridge (caught there): what it was
    /// doing, the exception's code, where.
    Crashed {
        doing: String,
        code: u32,
        at: String,
    },
    /// The bridge's pipe broke.
    Broke,
    /// No answer in time: the bridge hung.
    Hung,
    /// The bridge ended (it crashed).
    Ended,
    /// The bridge was gone already.
    Gone,
}

impl Lost {
    /// Whether the bridge is gone with it.
    pub fn fatal(&self) -> bool {
        !matches!(self, Lost::Failed(_))
    }

    /// What the bridge was doing, as a crash's note says it.
    pub fn doing(&self) -> String {
        match self {
            Lost::Failed(why) => why.clone(),
            Lost::Crashed { doing, .. } => doing.clone(),
            Lost::Broke => "the bridge's pipe broke".into(),
            Lost::Hung => "no answer in time (it hung)".into(),
            Lost::Ended | Lost::Gone => "the bridge ended (it crashed)".into(),
        }
    }
}

/// A bridge process, from the host's side.
pub struct Process {
    child: Mutex<Child>,
    to: Mutex<ChildStdin>,
    from: Mutex<Receiver<Vec<u8>>>,
    gone: AtomicBool,
}

impl std::fmt::Debug for Process {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Process")
            .field("gone", &self.gone())
            .finish()
    }
}

impl Process {
    /// Starts `exe` with `args` and `env` and waits `wait` at most for its
    /// first answer: the process and that answer's values (or its reason
    /// when it failed).
    pub fn spawn(
        exe: &Path,
        args: &[OsString],
        env: &[(String, String)],
        wait: Duration,
    ) -> Result<(Process, Vec<u8>), String> {
        let mut child = Command::new(exe)
            .args(args)
            .envs(env.iter().map(|(k, v)| (k, v)))
            .current_dir(std::env::temp_dir())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        let (Some(to), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            return Err("no pipes".into());
        };
        let (tx, rx) = mpsc::channel();
        let _ = std::thread::Builder::new()
            .name("bv-bridge-answers".into())
            .spawn(move || {
                let mut r = BufReader::new(stdout);
                while let Ok(frame) = read_frame(&mut r) {
                    if tx.send(frame).is_err() {
                        break;
                    }
                }
            });
        let hello = match rx.recv_timeout(wait) {
            Ok(frame) => frame,
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("no answer".into());
            }
        };
        match hello.first() {
            Some(&answer::DONE) => Ok((
                Process {
                    child: Mutex::new(child),
                    to: Mutex::new(to),
                    from: Mutex::new(rx),
                    gone: AtomicBool::new(false),
                },
                hello[1..].to_vec(),
            )),
            _ => {
                let why = In::new(hello.get(1..).unwrap_or(&[]))
                    .str()
                    .unwrap_or_default();
                let _ = child.wait();
                Err(why)
            }
        }
    }

    pub fn gone(&self) -> bool {
        self.gone.load(Ordering::Acquire)
    }

    /// Ends the bridge (it is gone from now on).
    pub fn end(&self) {
        self.gone.store(true, Ordering::Release);
        if let Ok(mut c) = self.child.lock() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    /// Request `request` and its answer's values within `wait`, or why
    /// there are none; a crash, a hang or an end leaves the bridge gone.
    pub fn call(&self, request: &[u8], wait: Duration) -> Result<Vec<u8>, Lost> {
        if self.gone() {
            return Err(Lost::Gone);
        }
        let outcome = {
            let mut to = self.to.lock().unwrap_or_else(|e| e.into_inner());
            let from = self.from.lock().unwrap_or_else(|e| e.into_inner());
            if write_frame(&mut *to, request).is_err() {
                Err(Lost::Broke)
            } else {
                match receive(&from, wait) {
                    Ok(frame) => match frame.first() {
                        Some(&answer::DONE) => Ok(frame[1..].to_vec()),
                        Some(&answer::CRASHED) => {
                            let mut r = In::new(&frame[1..]);
                            Err(Lost::Crashed {
                                doing: r.str().unwrap_or_default(),
                                code: r.u32().unwrap_or(0),
                                at: r.str().unwrap_or_default(),
                            })
                        }
                        _ => Err(Lost::Failed(
                            In::new(frame.get(1..).unwrap_or(&[]))
                                .str()
                                .unwrap_or_default(),
                        )),
                    },
                    Err(RecvTimeoutError::Timeout) => Err(Lost::Hung),
                    Err(RecvTimeoutError::Disconnected) => Err(Lost::Ended),
                }
            }
        };
        if let Err(lost) = &outcome
            && lost.fatal()
        {
            self.end();
        }
        outcome
    }
}

/// The next answer within `wait`; on a thread with windows a bridged
/// plug-in's live in, the messages sent to them answered meanwhile.
fn receive(from: &Receiver<Vec<u8>>, wait: Duration) -> Result<Vec<u8>, RecvTimeoutError> {
    if !WINDOWS_HERE.with(std::cell::Cell::get) {
        return from.recv_timeout(wait);
    }
    let until = std::time::Instant::now() + wait;
    loop {
        let left = until.saturating_duration_since(std::time::Instant::now());
        match from.recv_timeout(left.min(Duration::from_millis(2))) {
            Err(RecvTimeoutError::Timeout) if !left.is_zero() => {
                #[cfg(windows)]
                answer_sent();
            }
            other => return other,
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if let Ok(mut c) = self.child.lock() {
            if c.try_wait().ok().flatten().is_none() {
                let _ = c.kill();
            }
            let _ = c.wait();
        }
    }
}

/// The bridge's side: sends `hello`, then answers each request with
/// `handle` (its answer, and whether to quit) until told to quit or its
/// input closes; between requests `idle` runs (it says whether a window is
/// open, to come back soon for it) and, on Windows, the windows' messages.
pub fn serve(
    hello: &[u8],
    mut handle: impl FnMut(&[u8]) -> (Vec<u8>, bool),
    mut idle: impl FnMut() -> bool,
) -> i32 {
    #[cfg(windows)]
    let main = window::queue();
    let mut stdout = std::io::stdout().lock();
    if write_frame(&mut stdout, hello).is_err() {
        return 3;
    }
    let (tx, rx) = mpsc::channel::<Option<Vec<u8>>>();
    let _ = std::thread::Builder::new()
        .name("bv-bridge-requests".into())
        .spawn(move || {
            let mut stdin = std::io::stdin().lock();
            loop {
                let frame = read_frame(&mut stdin).ok();
                let end = frame.is_none();
                if tx.send(frame).is_err() || end {
                    break;
                }
                #[cfg(windows)]
                window::wake(main);
            }
            #[cfg(windows)]
            window::wake(main);
        });
    #[cfg(windows)]
    let mut wait = u32::MAX;
    loop {
        #[cfg(windows)]
        window::wait(wait);
        #[cfg(windows)]
        let next = rx.try_recv();
        #[cfg(not(windows))]
        let next = rx.recv().map_err(|_| mpsc::TryRecvError::Disconnected);
        match next {
            Ok(Some(frame)) => {
                let (reply, quit) = handle(&frame);
                if write_frame(&mut stdout, &reply).is_err() || quit {
                    return 0;
                }
                // More may wait: no sleep before them.
                #[cfg(windows)]
                window::wake(main);
            }
            Ok(None) | Err(mpsc::TryRecvError::Disconnected) => return 0,
            Err(mpsc::TryRecvError::Empty) => {}
        }
        let open = idle();
        #[cfg(windows)]
        {
            wait = if open { 15 } else { u32::MAX };
        }
        #[cfg(not(windows))]
        let _ = open;
    }
}
