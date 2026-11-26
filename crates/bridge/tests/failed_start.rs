//! A bridge whose first answer is not the one expected is ended, never
//! waited for: its answer garbled, it may be alive and serving, its input
//! still open. This test is the host and, started again by itself, that
//! bridge — so it has a `main` of its own (the tests' harness writes to the
//! standard output before a test runs, which would be the first answer).

use std::io::Read;
use std::time::Duration;

use bv_bridge::Process;
use bv_bridge::wire::write_frame;

/// Set for the test started again as the bridge.
const ROLE: &str = "BV_BRIDGE_TEST_ROLE";

/// The bridge: a first answer that is none of the known ones, then it
/// serves until its input closes.
fn a_bridge_that_garbles_its_first_answer() {
    write_frame(&mut std::io::stdout().lock(), &[0x7F]).expect("its answer written");
    let mut requests = Vec::new();
    let _ = std::io::stdin().lock().read_to_end(&mut requests);
}

fn a_garbled_first_answer_ends_the_bridge_instead_of_waiting_for_it() {
    let me = std::env::current_exe().expect("this test's program");
    let env = [(ROLE.to_string(), "bridge".to_string())];
    let patience = Duration::from_secs(20);
    let (done, wait) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let started = Process::spawn(&me, &[], &env, patience).map(|_| ());
        let _ = done.send(started);
    });
    match wait.recv_timeout(patience) {
        Ok(started) => assert!(started.is_err(), "no bridge started: {started:?}"),
        Err(_) => panic!("the start waited for a bridge that was still serving"),
    }
}

fn main() {
    if std::env::var_os(ROLE).is_some() {
        a_bridge_that_garbles_its_first_answer();
    } else {
        a_garbled_first_answer_ends_the_bridge_instead_of_waiting_for_it();
    }
}
