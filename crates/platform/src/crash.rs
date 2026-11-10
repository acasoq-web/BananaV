//! What is written down when the whole program crashes. On Windows that is
//! an exception no code handled (a graphics driver failing, say): one line
//! in a log file — when it happened, the exception and the module it came
//! from — written without taking memory, as nothing in the program can be
//! trusted then; and, while [`set_restart`] allows it, the program started
//! again with the same arguments. Elsewhere the panics the interface notes
//! down are what is left.

use std::ffi::OsString;
use std::path::Path;

/// From now on, a crash appends a line to `log`; while [`set_restart`]
/// allows it, `again` (the program and its arguments) is started too.
pub fn on_crash(log: &Path, again: Option<(&Path, &[OsString])>) {
    imp::on_crash(log, again);
}

/// Whether a crash starts the program again (it does while it is still
/// starting: the next start draws differently).
pub fn set_restart(on: bool) {
    imp::set_restart(on);
}

// Calls into the system's C functions, and a filter it calls.
#[cfg(windows)]
#[allow(unsafe_code)]
mod imp {
    use std::ffi::{OsString, c_void};
    use std::fmt::Write as _;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    /// The log's path and the command line, NUL-terminated wide strings
    /// made before any crash.
    static LOG: OnceLock<Box<[u16]>> = OnceLock::new();
    static AGAIN: OnceLock<Box<[u16]>> = OnceLock::new();
    static RESTART: AtomicBool = AtomicBool::new(false);
    /// The filter there was before, called after this one.
    static PREVIOUS: AtomicUsize = AtomicUsize::new(0);
    /// The command line's room when it is started again.
    const COMMAND_ROOM: usize = 8192;

    #[repr(C)]
    struct ExceptionRecord {
        code: u32,
        flags: u32,
        next: *mut ExceptionRecord,
        address: *mut c_void,
        count: u32,
        information: [usize; 15],
    }

    #[repr(C)]
    struct ExceptionPointers {
        record: *mut ExceptionRecord,
        context: *mut c_void,
    }

    type Filter = Option<unsafe extern "system" fn(*const ExceptionPointers) -> i32>;

    #[repr(C)]
    struct SystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }

    #[repr(C)]
    struct StartupInfo {
        cb: u32,
        reserved: *mut u16,
        desktop: *mut u16,
        title: *mut u16,
        x: u32,
        y: u32,
        x_size: u32,
        y_size: u32,
        x_chars: u32,
        y_chars: u32,
        fill: u32,
        flags: u32,
        show_window: u16,
        reserved2_len: u16,
        reserved2: *mut u8,
        std_input: *mut c_void,
        std_output: *mut c_void,
        std_error: *mut c_void,
    }

    #[repr(C)]
    struct ProcessInformation {
        process: *mut c_void,
        thread: *mut c_void,
        process_id: u32,
        thread_id: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetUnhandledExceptionFilter(filter: Filter) -> Filter;
        fn GetModuleHandleExW(flags: u32, name: *const c_void, module: *mut *mut c_void) -> i32;
        fn GetModuleFileNameW(module: *mut c_void, name: *mut u16, size: u32) -> u32;
        fn GetSystemTime(time: *mut SystemTime);
        fn CreateFileW(
            name: *const u16,
            access: u32,
            share: u32,
            security: *const c_void,
            disposition: u32,
            flags: u32,
            template: *mut c_void,
        ) -> *mut c_void;
        fn WriteFile(
            file: *mut c_void,
            buffer: *const u8,
            len: u32,
            written: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
        fn CreateProcessW(
            application: *const u16,
            command_line: *mut u16,
            process_security: *const c_void,
            thread_security: *const c_void,
            inherit: i32,
            flags: u32,
            environment: *const c_void,
            directory: *const u16,
            startup: *const StartupInfo,
            info: *mut ProcessInformation,
        ) -> i32;
    }

    fn wide(s: &std::ffi::OsStr) -> Box<[u16]> {
        s.encode_wide().chain([0]).collect()
    }

    /// `arg` quoted for a command line as programs split it again.
    fn quoted(arg: &std::ffi::OsStr, out: &mut Vec<u16>) {
        let a: Vec<u16> = arg.encode_wide().collect();
        let plain = !a.is_empty()
            && !a
                .iter()
                .any(|&c| c == u16::from(b' ') || c == u16::from(b'\t') || c == u16::from(b'"'));
        if plain {
            out.extend(&a);
            return;
        }
        out.push(u16::from(b'"'));
        let mut slashes = 0;
        for &c in &a {
            if c == u16::from(b'\\') {
                slashes += 1;
                continue;
            }
            if c == u16::from(b'"') {
                out.extend(std::iter::repeat_n(u16::from(b'\\'), slashes * 2 + 1));
            } else {
                out.extend(std::iter::repeat_n(u16::from(b'\\'), slashes));
            }
            slashes = 0;
            out.push(c);
        }
        out.extend(std::iter::repeat_n(u16::from(b'\\'), slashes * 2));
        out.push(u16::from(b'"'));
    }

    pub(super) fn on_crash(log: &Path, again: Option<(&Path, &[OsString])>) {
        let _ = LOG.set(wide(log.as_os_str()));
        if let Some((program, args)) = again {
            let mut line = Vec::new();
            quoted(program.as_os_str(), &mut line);
            for a in args {
                line.push(u16::from(b' '));
                quoted(a, &mut line);
            }
            line.push(0);
            if line.len() <= COMMAND_ROOM {
                let _ = AGAIN.set(line.into_boxed_slice());
            }
        }
        // SAFETY: installs a function of the right signature, kept for
        // the program's life; the one before it is kept to be called.
        let previous = unsafe { SetUnhandledExceptionFilter(Some(filter)) };
        PREVIOUS.store(previous.map_or(0, |f| f as usize), Ordering::SeqCst);
    }

    pub(super) fn set_restart(on: bool) {
        RESTART.store(on, Ordering::SeqCst);
    }

    /// A line built in place: no memory taken.
    struct Line {
        bytes: [u8; 1024],
        len: usize,
    }

    impl std::fmt::Write for Line {
        fn write_str(&mut self, s: &str) -> std::fmt::Result {
            let room = self.bytes.len() - self.len;
            let n = s.len().min(room);
            self.bytes[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
            self.len += n;
            Ok(())
        }
    }

    /// The module holding `address` and the offset in it, into `line`.
    ///
    /// # Safety
    /// Only looks the address up; nothing is read from it.
    unsafe fn place(address: *mut c_void, line: &mut Line) {
        const FROM_ADDRESS: u32 = 0x4;
        const UNCHANGED_REFCOUNT: u32 = 0x2;
        let mut module = std::ptr::null_mut();
        // SAFETY: a lookup of the module holding an address, its count of
        // references unchanged; then its file's name into a buffer of the
        // length given.
        unsafe {
            if GetModuleHandleExW(FROM_ADDRESS | UNCHANGED_REFCOUNT, address, &mut module) == 0 {
                let _ = write!(line, "{address:p}");
                return;
            }
            let mut buf = [0u16; 512];
            let n = GetModuleFileNameW(module, buf.as_mut_ptr(), buf.len() as u32) as usize;
            let name = &buf[..n.min(buf.len())];
            let file = name
                .iter()
                .rposition(|&c| c == u16::from(b'\\') || c == u16::from(b'/'))
                .map_or(name, |i| &name[i + 1..]);
            for c in char::decode_utf16(file.iter().copied()) {
                let _ = line.write_char(c.unwrap_or('?'));
            }
            let _ = write!(line, "+{:#x}", address as usize - module as usize);
        }
    }

    unsafe extern "system" fn filter(info: *const ExceptionPointers) -> i32 {
        const CONTINUE_SEARCH: i32 = 0;
        // Once: a crash while writing does not come back here.
        static ONCE: AtomicBool = AtomicBool::new(false);
        if ONCE.swap(true, Ordering::SeqCst) {
            return CONTINUE_SEARCH;
        }
        let restart = RESTART.load(Ordering::SeqCst) && AGAIN.get().is_some();
        let mut line = Line {
            bytes: [0; 1024],
            len: 0,
        };
        // SAFETY: the system's pointers to the exception, read once; the
        // calls below take only buffers of this frame.
        unsafe {
            let mut t: SystemTime = std::mem::zeroed();
            GetSystemTime(&mut t);
            let _ = write!(
                line,
                "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC  BananaV crashed: exception ",
                t.year, t.month, t.day, t.hour, t.minute, t.second
            );
            match info.as_ref().and_then(|p| p.record.as_ref()) {
                Some(r) => {
                    let _ = write!(line, "{:#010x} at ", r.code);
                    place(r.address, &mut line);
                }
                None => {
                    let _ = line.write_str("(unknown)");
                }
            }
            if restart {
                let _ = line.write_str("; started again");
            }
            let _ = line.write_str("\r\n");
            if let Some(log) = LOG.get() {
                const APPEND: u32 = 0x4;
                const SHARE: u32 = 0x1 | 0x2;
                const OPEN_ALWAYS: u32 = 4;
                const NORMAL: u32 = 0x80;
                let file = CreateFileW(
                    log.as_ptr(),
                    APPEND,
                    SHARE,
                    std::ptr::null(),
                    OPEN_ALWAYS,
                    NORMAL,
                    std::ptr::null_mut(),
                );
                if file as isize != -1 {
                    let mut written = 0;
                    WriteFile(
                        file,
                        line.bytes.as_ptr(),
                        line.len as u32,
                        &mut written,
                        std::ptr::null_mut(),
                    );
                    CloseHandle(file);
                }
            }
            if restart && let Some(again) = AGAIN.get() {
                // The command line goes in a buffer the call may change.
                let mut command = [0u16; COMMAND_ROOM];
                command[..again.len()].copy_from_slice(again);
                let mut startup: StartupInfo = std::mem::zeroed();
                startup.cb = std::mem::size_of::<StartupInfo>() as u32;
                let mut process: ProcessInformation = std::mem::zeroed();
                if CreateProcessW(
                    std::ptr::null(),
                    command.as_mut_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    0,
                    0,
                    std::ptr::null(),
                    std::ptr::null(),
                    &startup,
                    &mut process,
                ) != 0
                {
                    CloseHandle(process.thread);
                    CloseHandle(process.process);
                }
            }
            match PREVIOUS.load(Ordering::SeqCst) {
                0 => CONTINUE_SEARCH,
                f => {
                    let f: unsafe extern "system" fn(*const ExceptionPointers) -> i32 =
                        std::mem::transmute(f);
                    f(info)
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::quoted;
        use std::ffi::OsString;
        use std::path::PathBuf;

        /// Set in the processes the crash test starts: the folder of their
        /// files.
        const CHILD: &str = "BV_CRASH_TEST_DIR";
        const AGAIN: &str = "crash::imp::tests::started_again_after_the_crash";

        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn SetErrorMode(mode: u32) -> u32;
        }

        /// A crash in a process of its own (this test run again): a line
        /// in the log with the exception and the module it came from, and
        /// the program it names started again.
        #[test]
        fn a_crash_writes_its_line_and_starts_the_program_again() {
            let exe = std::env::current_exe().unwrap();
            if let Some(dir) = std::env::var_os(CHILD) {
                let dir = PathBuf::from(dir);
                let args = [OsString::from(AGAIN), OsString::from("--exact")];
                super::on_crash(&dir.join("Crash.log"), Some((&exe, &args)));
                super::set_restart(true);
                // SAFETY: no error box for the crash; then the crash itself.
                unsafe {
                    SetErrorMode(0x1 | 0x2);
                    std::ptr::null_mut::<u8>().write_volatile(1);
                }
                unreachable!("the write crashes");
            }
            let dir = std::env::temp_dir().join(format!("bv crash {}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let status = std::process::Command::new(&exe)
                .args([
                    "crash::imp::tests::a_crash_writes_its_line_and_starts_the_program_again",
                    "--exact",
                    "--test-threads=1",
                ])
                .env(CHILD, &dir)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap();
            assert!(!status.success());
            let log = std::fs::read_to_string(dir.join("Crash.log")).unwrap();
            assert!(
                log.contains("BananaV crashed: exception 0xc0000005 at "),
                "{log}"
            );
            assert!(
                log.contains(".exe+0x") && log.ends_with("; started again\r\n"),
                "{log}"
            );
            let mark = dir.join("again.txt");
            for _ in 0..100 {
                if mark.exists() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            assert!(mark.exists(), "the program was started again");
            let _ = std::fs::remove_dir_all(&dir);
        }

        /// Run by the crash test's process when it starts the program
        /// again: leaves its mark (nothing elsewhere).
        #[test]
        fn started_again_after_the_crash() {
            if let Some(dir) = std::env::var_os(CHILD) {
                std::fs::write(PathBuf::from(dir).join("again.txt"), "again").unwrap();
            }
        }

        fn line(args: &[&str]) -> String {
            let mut out = Vec::new();
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push(u16::from(b' '));
                }
                quoted(std::ffi::OsStr::new(a), &mut out);
            }
            String::from_utf16(&out).unwrap()
        }

        #[test]
        fn arguments_are_quoted_as_programs_split_them() {
            assert_eq!(line(&["a.exe", "plain"]), "a.exe plain");
            assert_eq!(
                line(&[
                    r"C:\Program Files\BananaV\BananaV.exe",
                    r"D:\My Videos\x.bvproj"
                ]),
                r#""C:\Program Files\BananaV\BananaV.exe" "D:\My Videos\x.bvproj""#
            );
            assert_eq!(line(&[r#"say "hi""#]), r#""say \"hi\"""#);
            assert_eq!(line(&[r"C:\dir with space\"]), r#""C:\dir with space\\""#);
            assert_eq!(line(&[""]), r#""""#);
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::ffi::OsString;
    use std::path::Path;

    pub(super) fn on_crash(_log: &Path, _again: Option<(&Path, &[OsString])>) {}

    pub(super) fn set_restart(_on: bool) {}
}
