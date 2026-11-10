//! The main thread: AppKit's windows and views are touched there only. Work
//! asked for from another thread (a plug-in resizing its editor from its
//! own) is sent there through the main dispatch queue, which the program's
//! run loop drains.

use std::ffi::{c_int, c_void};

unsafe extern "C" {
    fn pthread_main_np() -> c_int;
    static _dispatch_main_q: u8;
    fn dispatch_async_f(
        queue: *const c_void,
        context: *mut c_void,
        work: unsafe extern "C" fn(*mut c_void),
    );
}

/// Whether this is the program's main thread.
pub fn is_main_thread() -> bool {
    // SAFETY: a plain query.
    unsafe { pthread_main_np() != 0 }
}

type Work = Box<dyn FnOnce() + Send>;

unsafe extern "C" fn run(context: *mut c_void) {
    // SAFETY: the box `on_main` gave the queue, taken back once.
    let work = unsafe { Box::from_raw(context.cast::<Work>()) };
    work();
}

/// Runs `work` on the main thread: now when this is it, else as soon as
/// the main thread's run loop gets to it.
pub fn on_main(work: impl FnOnce() + Send + 'static) {
    if is_main_thread() {
        work();
        return;
    }
    let boxed: Box<Work> = Box::new(Box::new(work));
    // SAFETY: the main queue's address; the box is taken back by `run`,
    // once.
    unsafe {
        dispatch_async_f(
            (&raw const _dispatch_main_q).cast(),
            Box::into_raw(boxed).cast(),
            run,
        );
    }
}
