//! `bananav-cli`: BananaV's command line from a terminal.
//!
//! On Windows BananaV's program is a window program: a terminal neither
//! waits for it nor shows what it writes. This small console program starts
//! it from beside itself with `--cli` and the same arguments, passes its
//! input and output along as they come, ends it if it is ended itself
//! (Ctrl+C), and exits with its exit code. Elsewhere it becomes BananaV's
//! program (which works from a terminal as it is).

use std::path::PathBuf;
use std::process::{Command, ExitCode};

/// BananaV's program, beside this one.
fn program() -> Option<PathBuf> {
    let me = std::env::current_exe().ok()?;
    let dir = me.parent()?;
    let name = if cfg!(windows) {
        "bananav.exe"
    } else {
        "bananav"
    };
    let path = dir.join(name);
    path.is_file().then_some(path)
}

fn main() -> ExitCode {
    let Some(program) = program() else {
        eprintln!("bananav-cli: BananaV's program is not beside this one.");
        return ExitCode::from(2);
    };
    let mut command = Command::new(program);
    command.arg("--cli").args(std::env::args_os().skip(1));
    run(command)
}

#[cfg(unix)]
fn run(mut command: Command) -> ExitCode {
    use std::os::unix::process::CommandExt;
    let e = command.exec();
    eprintln!("bananav-cli: {e}");
    ExitCode::from(2)
}

#[cfg(not(unix))]
fn run(mut command: Command) -> ExitCode {
    use std::io::{Read, Write};
    use std::process::Stdio;

    /// What one end reads, written to the other as it comes.
    fn pass(mut from: impl Read, mut to: impl Write) {
        let mut buf = [0u8; 16 * 1024];
        loop {
            match from.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if to.write_all(&buf[..n]).is_err() || to.flush().is_err() {
                        break;
                    }
                }
            }
        }
    }

    let mut child = match command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("bananav-cli: {e}");
            return ExitCode::from(2);
        }
    };
    bv_platform::end_with_this_process(&child);
    let input = child.stdin.take();
    let output = child.stdout.take();
    let errors = child.stderr.take();
    // The input's thread may wait on the terminal forever: left behind.
    if let Some(input) = input {
        std::thread::spawn(move || pass(std::io::stdin(), input));
    }
    let out = output.map(|o| std::thread::spawn(move || pass(o, std::io::stdout())));
    let err = errors.map(|e| std::thread::spawn(move || pass(e, std::io::stderr())));
    let status = child.wait();
    for t in [out, err].into_iter().flatten() {
        let _ = t.join();
    }
    match status {
        Ok(s) => ExitCode::from(s.code().map_or(1, |c| c.clamp(0, 255) as u8)),
        Err(e) => {
            eprintln!("bananav-cli: {e}");
            ExitCode::from(2)
        }
    }
}
