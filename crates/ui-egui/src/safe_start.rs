//! Starts that end early. On some computers the graphics driver fails on
//! DirectX 12 and BananaV closed right after its window appeared. Each
//! start is noted down (`Start.json` in the settings folder) before the
//! window opens, and crossed out once the interface has drawn and the
//! graphics card has opened and compiled its kernels — or when BananaV
//! closes normally. A start never crossed out ended early: the next one
//! draws through the next graphics interface — on Windows Vulkan, then
//! OpenGL with the processor compositing video; on Linux OpenGL — and the
//! one that works is kept. On Windows a crash while starting starts BananaV
//! again at once, on the next interface. `Crash.log` in the logs folder says
//! what ended each start (panics, crashes, a window that could not open).
//! `WGPU_BACKEND` set by hand still picks the interface; deleting
//! `Start.json` goes back to the system's own.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// The graphics interface the interface (and the card compositing video)
/// draws through.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Graphics {
    /// The system's own: DirectX 12 on Windows, Metal on macOS, Vulkan on
    /// Linux.
    #[default]
    Preferred,
    /// Vulkan, for the interface and for video alike.
    Vulkan,
    /// OpenGL for the interface; video composited on the processor.
    OpenGl,
}

impl Graphics {
    /// The interface to try after a start that ended early on this one.
    fn next(self) -> Option<Graphics> {
        match self {
            Graphics::Preferred if cfg!(windows) => Some(Graphics::Vulkan),
            Graphics::Preferred if cfg!(target_os = "linux") => Some(Graphics::OpenGl),
            Graphics::Vulkan => Some(Graphics::OpenGl),
            _ => None,
        }
    }

    /// Its name, as the logs and the notice say it.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Graphics::Preferred if cfg!(windows) => "DirectX 12",
            Graphics::Preferred if cfg!(target_os = "macos") => "Metal",
            Graphics::Preferred | Graphics::Vulkan => "Vulkan",
            Graphics::OpenGl => "OpenGL",
        }
    }

    /// The interfaces the main window draws through (`None`: the window
    /// library's choice).
    pub(crate) fn window_backends(self) -> Option<eframe::wgpu::Backends> {
        use eframe::wgpu::Backends;
        match self {
            // Windows' own way to the screen: with monitors on two cards,
            // a window dragged from one to the other stays smooth.
            Graphics::Preferred if cfg!(windows) => Some(Backends::DX12 | Backends::GL),
            Graphics::Preferred => None,
            Graphics::Vulkan => Some(Backends::VULKAN),
            Graphics::OpenGl => Some(Backends::GL),
        }
    }

    /// The interfaces the card compositing video opens through.
    fn video(self) -> bv_engine::GpuInterfaces {
        match self {
            Graphics::Preferred => bv_engine::GpuInterfaces::All,
            Graphics::Vulkan => bv_engine::GpuInterfaces::Vulkan,
            Graphics::OpenGl => bv_engine::GpuInterfaces::None,
        }
    }
}

/// `Start.json`.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Record {
    graphics: Graphics,
    /// A start is under way: set before the window opens, cleared once it
    /// settled.
    starting: bool,
}

fn record_path() -> Option<PathBuf> {
    Some(
        bv_platform::config_dir()?
            .join("BananaV")
            .join("Start.json"),
    )
}

fn read() -> Record {
    record_path()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn write(r: &Record) {
    let Some(path) = record_path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(r) {
        let _ = std::fs::write(path, bytes);
    }
}

/// This start: what it draws through, and what the last one ended on.
struct Run {
    graphics: Graphics,
    /// `WGPU_BACKEND` picks the interface: nothing is changed.
    by_hand: bool,
    /// The interface the last start ended early on.
    after: Option<Graphics>,
    started: Instant,
}

static RUN: Mutex<Option<Run>> = Mutex::new(None);
static SETTLED: AtomicBool = AtomicBool::new(false);
static FRAMES: AtomicU32 = AtomicU32::new(0);

/// The start begins: reads how the last one ended, picks the graphics
/// interface, notes the start down, and makes panics and crashes write
/// `Crash.log`. Returns the interface the window draws through.
pub(crate) fn begin() -> Graphics {
    rotate_crash_log();
    let by_hand = std::env::var_os("WGPU_BACKEND").is_some();
    let mut record = read();
    let mut after = None;
    if record.starting && !by_hand {
        match record.graphics.next() {
            Some(next) => {
                crate::logs::crash(&format!(
                    "The last start ended before it settled, drawing through {}: this one draws through {}.",
                    record.graphics.name(),
                    next.name()
                ));
                after = Some(record.graphics);
                record.graphics = next;
            }
            None => crate::logs::crash(&format!(
                "The last start ended before it settled, drawing through {}: there is no other interface to try.",
                record.graphics.name()
            )),
        }
    }
    let graphics = record.graphics;
    if !by_hand {
        record.starting = true;
        write(&record);
        bv_engine::set_gpu_interfaces(graphics.video());
    }
    *RUN.lock().unwrap_or_else(|e| e.into_inner()) = Some(Run {
        graphics,
        by_hand,
        after,
        started: Instant::now(),
    });
    note_panics();
    if let Some(log) = crate::logs::crash_log() {
        if let Some(dir) = log.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let program = std::env::current_exe().ok();
        let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
        let again = program.as_deref().map(|p| (p, args.as_slice()));
        bv_platform::on_crash(&log, again);
        // Started again only while there is another interface to try.
        bv_platform::set_restart(!by_hand && graphics.next().is_some());
    }
    graphics
}

/// Each frame of the main window, until the start settled: three frames
/// drawn and the card compositing video open with its kernels compiled
/// (three minutes at most).
pub(crate) fn frame(engine: &bv_engine::Engine) {
    if SETTLED.load(Ordering::Relaxed) {
        return;
    }
    let frames = FRAMES.fetch_add(1, Ordering::Relaxed) + 1;
    let started = RUN
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map_or_else(Instant::now, |r| r.started);
    let long = started.elapsed() > Duration::from_secs(180);
    if frames >= 3 && (!engine.gpu_starting() || long) {
        settle();
    }
}

/// The start went well (or BananaV closes normally): crossed out.
pub(crate) fn settle() {
    if SETTLED.swap(true, Ordering::SeqCst) {
        return;
    }
    bv_platform::set_restart(false);
    let run = RUN.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(r) = run.as_ref().filter(|r| !r.by_hand) {
        write(&Record {
            graphics: r.graphics,
            starting: false,
        });
    }
}

/// The main window could not open (`error`): noted down, and BananaV
/// started again on the next interface when there is one. Returns whether
/// it was.
pub(crate) fn window_failed(error: &dyn std::fmt::Display) -> bool {
    let run = RUN.lock().unwrap_or_else(|e| e.into_inner());
    let Some(r) = run.as_ref() else {
        return false;
    };
    let next = r.graphics.next().filter(|_| !r.by_hand);
    crate::logs::crash(&format!(
        "The window could not open, drawing through {}: {error}",
        r.graphics.name()
    ));
    let Some(next) = next else {
        return false;
    };
    write(&Record {
        graphics: next,
        starting: false,
    });
    SETTLED.store(true, Ordering::SeqCst);
    let Ok(program) = std::env::current_exe() else {
        return false;
    };
    crate::logs::crash(&format!("Started again, drawing through {}.", next.name()));
    std::process::Command::new(program)
        .args(std::env::args_os().skip(1))
        .spawn()
        .is_ok()
}

/// What to tell the user once the window shows: the last start ended
/// early and this one draws through another interface.
pub(crate) fn notice() -> Option<String> {
    let run = RUN.lock().unwrap_or_else(|e| e.into_inner());
    let r = run.as_ref()?;
    let failed = r.after?;
    Some(trf!(
        "The last start of BananaV ended early on {}; it now draws through {}. Crash.log in the logs folder says what happened.",
        failed.name(),
        r.graphics.name()
    ))
}

/// Panics write `Crash.log` (twenty at most), then go on as before.
fn note_panics() {
    static ONCE: OnceLock<()> = OnceLock::new();
    if ONCE.set(()).is_err() {
        return;
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        if COUNT.fetch_add(1, Ordering::Relaxed) < 20 {
            let thread = std::thread::current();
            let name = thread.name().unwrap_or("unnamed");
            let message = info
                .payload()
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
                .unwrap_or("(no message)");
            let at = info
                .location()
                .map(|l| format!("{}:{}", l.file(), l.line()))
                .unwrap_or_default();
            let trace = std::backtrace::Backtrace::force_capture().to_string();
            let trace: Vec<&str> = trace.lines().take(40).collect();
            crate::logs::crash(&format!(
                "Panic on thread '{name}' at {at}: {message}\n{}",
                trace.join("\n")
            ));
        }
        previous(info);
    }));
}

/// `Crash.log` past a megabyte becomes `Crash.old.log` (the one before
/// goes).
fn rotate_crash_log() {
    let Some(log) = crate::logs::crash_log() else {
        return;
    };
    if std::fs::metadata(&log).is_ok_and(|m| m.len() > 1 << 20) {
        let _ = std::fs::rename(&log, log.with_file_name("Crash.old.log"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_failed_interface_has_a_next_until_the_last() {
        if cfg!(windows) {
            assert_eq!(Graphics::Preferred.next(), Some(Graphics::Vulkan));
            assert_eq!(Graphics::Vulkan.next(), Some(Graphics::OpenGl));
        }
        if cfg!(target_os = "linux") {
            assert_eq!(Graphics::Preferred.next(), Some(Graphics::OpenGl));
        }
        if cfg!(target_os = "macos") {
            assert_eq!(Graphics::Preferred.next(), None);
        }
        assert_eq!(Graphics::OpenGl.next(), None);
        // OpenGL leaves video to the processor.
        assert_eq!(Graphics::OpenGl.video(), bv_engine::GpuInterfaces::None);
        assert_eq!(Graphics::Vulkan.video(), bv_engine::GpuInterfaces::Vulkan);
    }

    #[test]
    fn the_record_reads_back_and_old_files_read_as_new() {
        let r = Record {
            graphics: Graphics::Vulkan,
            starting: true,
        };
        let json = serde_json::to_string(&r).unwrap();
        let back: Record = serde_json::from_str(&json).unwrap();
        assert_eq!((back.graphics, back.starting), (Graphics::Vulkan, true));
        let empty: Record = serde_json::from_str("{}").unwrap();
        assert_eq!(
            (empty.graphics, empty.starting),
            (Graphics::Preferred, false)
        );
    }
}
