//! Processes: a protocol's standard output kept for it alone, and a
//! process of BananaV's that ends with the one that started it.

/// The program's standard output kept for a protocol (MCP's messages, a
/// line each): a file writing to it, while everything else the program —
/// or a plug-in loaded into it, through the C runtime too — writes to its
/// standard output goes to the standard error from now on (or nowhere
/// without one), and the processes it starts never get it (a program a
/// plug-in starts would keep it open after BananaV ends, and its client
/// would wait for it).
pub fn protocol_output() -> std::io::Result<std::fs::File> {
    imp::protocol_output()
}

/// `child` ends when this process ends, however it ends (Windows: in a job
/// closed with this process; elsewhere nothing, the caller replacing
/// itself with the program instead).
pub fn end_with_this_process(child: &std::process::Child) {
    imp::end_with(child);
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod imp {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, IntoRawHandle};

    use windows::Win32::Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE};
    use windows::Win32::System::Console::{
        GetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
    };
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };
    use windows::Win32::System::Threading::GetCurrentProcess;
    use windows::core::PCWSTR;

    unsafe extern "C" {
        /// The C runtime's own descriptors (what `printf` writes to).
        fn _dup2(from: i32, to: i32) -> i32;
        fn _open(path: *const std::ffi::c_char, flags: i32, ...) -> i32;
        fn _close(fd: i32) -> i32;
    }

    const O_WRONLY: i32 = 0x0001;

    fn io(e: windows::core::Error) -> std::io::Error {
        std::io::Error::other(e.message())
    }

    pub fn protocol_output() -> std::io::Result<std::fs::File> {
        // SAFETY: the standard handles are this process's; the one kept is
        // a copy it owns (not inherited), handed to the file it closes.
        unsafe {
            let out = GetStdHandle(STD_OUTPUT_HANDLE).map_err(io)?;
            let me = GetCurrentProcess();
            let mut kept = HANDLE::default();
            DuplicateHandle(me, out, me, &mut kept, 0, false, DUPLICATE_SAME_ACCESS).map_err(io)?;
            // Where the rest goes: the standard error, else nowhere.
            let err = match GetStdHandle(STD_ERROR_HANDLE) {
                Ok(h) => h,
                Err(_) => {
                    let null = std::fs::OpenOptions::new().write(true).open("NUL")?;
                    HANDLE(null.into_raw_handle())
                }
            };
            SetStdHandle(STD_OUTPUT_HANDLE, err).map_err(io)?;
            if _dup2(2, 1) != 0 {
                let null = _open(c"NUL".as_ptr(), O_WRONLY);
                if null >= 0 {
                    _dup2(null, 1);
                    _close(null);
                }
            }
            Ok(std::fs::File::from_raw_handle(kept.0))
        }
    }

    pub fn end_with(child: &std::process::Child) {
        // SAFETY: the job is this process's own, left open on purpose (it
        // closes when the process ends, ending the child); the child's
        // handle is valid while `child` is.
        unsafe {
            let Ok(job) = CreateJobObjectW(None, PCWSTR::null()) else {
                return;
            };
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let set = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&raw const info).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if set.is_ok() {
                let _ = AssignProcessToJobObject(job, HANDLE(child.as_raw_handle()));
            }
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[allow(unsafe_code)]
mod imp {
    use std::os::fd::FromRawFd;

    pub fn protocol_output() -> std::io::Result<std::fs::File> {
        // SAFETY: descriptors 1 and 2 are this process's; the copy kept
        // (closed in the programs it starts) goes to the file that owns it.
        unsafe {
            let kept = libc::fcntl(1, libc::F_DUPFD_CLOEXEC, 3);
            if kept < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::dup2(2, 1) < 0 {
                let null = libc::open(c"/dev/null".as_ptr(), libc::O_WRONLY);
                if null >= 0 {
                    libc::dup2(null, 1);
                    libc::close(null);
                }
            }
            Ok(std::fs::File::from_raw_fd(kept))
        }
    }

    pub fn end_with(_child: &std::process::Child) {}
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod imp {
    pub fn protocol_output() -> std::io::Result<std::fs::File> {
        Err(std::io::ErrorKind::Unsupported.into())
    }

    pub fn end_with(_child: &std::process::Child) {}
}
