//! Keeping the computer from going to sleep (or hibernating) by itself
//! while long work goes on: a render counts as nobody using it.
#![allow(unsafe_code)]

/// While one lives the computer does not go to sleep by itself (the
/// screen may turn off). The system lists it with the reason given
/// (Windows: `powercfg /requests`).
#[derive(Debug)]
pub struct KeepAwake {
    _held: Option<imp::Held>,
}

impl KeepAwake {
    /// From now on, because of `why` (one line, in English).
    pub fn new(why: &str) -> KeepAwake {
        KeepAwake {
            _held: imp::hold(why),
        }
    }

    /// Whether the system took it.
    pub fn held(&self) -> bool {
        self._held.is_some()
    }
}

#[cfg(windows)]
mod imp {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Power::{
        POWER_REQUEST_TYPE, PowerClearRequest, PowerCreateRequest, PowerRequestExecutionRequired,
        PowerRequestSystemRequired, PowerSetRequest,
    };
    use windows::Win32::System::Threading::{
        POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT, REASON_CONTEXT_0,
    };
    use windows::core::PWSTR;

    /// Awake, and (computers that sleep while on, "modern standby") this
    /// program running.
    const KINDS: [POWER_REQUEST_TYPE; 2] =
        [PowerRequestSystemRequired, PowerRequestExecutionRequired];

    #[derive(Debug)]
    pub struct Held(HANDLE);

    // SAFETY: a power request's handle may be used from any thread.
    unsafe impl Send for Held {}
    // SAFETY: as above; nothing is changed through a shared reference.
    unsafe impl Sync for Held {}

    pub fn hold(why: &str) -> Option<Held> {
        let mut text: Vec<u16> = why.encode_utf16().chain([0]).collect();
        let context = REASON_CONTEXT {
            // POWER_REQUEST_CONTEXT_VERSION.
            Version: 0,
            Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
            Reason: REASON_CONTEXT_0 {
                SimpleReasonString: PWSTR(text.as_mut_ptr()),
            },
        };
        // SAFETY: the context and its text live through the call, which
        // copies them; the handle is closed once, when dropped.
        unsafe {
            let held = Held(PowerCreateRequest(&context).ok()?);
            let set = KINDS.map(|k| PowerSetRequest(held.0, k).is_ok());
            set[0].then_some(held)
        }
    }

    impl Drop for Held {
        fn drop(&mut self) {
            // SAFETY: the request made above.
            unsafe {
                for k in KINDS {
                    let _ = PowerClearRequest(self.0, k);
                }
                let _ = CloseHandle(self.0);
            }
        }
    }
}

/// Linux and macOS: the system's own program for it, run meanwhile.
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod imp {
    use std::process::{Child, Command, Stdio};

    #[derive(Debug)]
    pub struct Held(Child);

    pub fn hold(why: &str) -> Option<Held> {
        let mut c = if cfg!(target_os = "macos") {
            // It also ends when BananaV does.
            let mut c = Command::new("caffeinate");
            c.args(["-i", "-w", &std::process::id().to_string()]);
            c
        } else {
            let mut c = Command::new("systemd-inhibit");
            c.args(["--what=sleep:idle", "--who=BananaV", "--mode=block"]);
            c.arg(format!("--why={why}"));
            // Ends by itself should BananaV end without stopping it.
            let end = format!(
                "while kill -0 {} 2>/dev/null; do sleep 5; done",
                std::process::id()
            );
            c.args(["sh", "-c", &end]);
            c
        };
        c.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()
            .map(Held)
    }

    impl Drop for Held {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod imp {
    #[derive(Debug)]
    pub struct Held(());

    pub fn hold(_why: &str) -> Option<Held> {
        None
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn the_computer_is_kept_awake_and_let_go() {
        let a = KeepAwake::new("BananaV's tests");
        let b = KeepAwake::new("BananaV's tests, again");
        assert!(a.held() && b.held());
        drop(a);
        assert!(b.held());
    }
}
