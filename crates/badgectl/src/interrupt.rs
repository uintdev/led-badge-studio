//! Ctrl+C during an upload: the upload finishes first. Stopping it partway
//! leaves the badge ignoring every upload until it's unplugged.

use std::sync::atomic::{AtomicBool, Ordering};

static PRESSED: AtomicBool = AtomicBool::new(false);

/// Said once Ctrl+C was pressed during an upload.
pub const FINISHING: &str = "Finishing the upload first: stopping it partway would leave the badge \
                             ignoring uploads until it's unplugged and plugged back in.";

/// The exit status after an interrupted run, as shells report one.
pub const EXIT_STATUS: i32 = 130;

/// While it's held, Ctrl+C is noted instead of ending the program.
pub struct Deferred(());

impl Deferred {
    pub fn new() -> Self {
        PRESSED.store(false, Ordering::Relaxed);
        install();
        Deferred(())
    }

    pub fn pressed(&self) -> bool {
        PRESSED.load(Ordering::Relaxed)
    }
}

impl Drop for Deferred {
    fn drop(&mut self) {
        uninstall();
    }
}

#[cfg(unix)]
extern "C" fn on_interrupt(_: libc::c_int) {
    PRESSED.store(true, Ordering::Relaxed);
}

#[cfg(unix)]
fn install() {
    // SAFETY: the handler only stores to an atomic, which is safe in a
    // signal handler.
    unsafe {
        libc::signal(libc::SIGINT, on_interrupt as *const () as libc::sighandler_t);
    }
}

#[cfg(unix)]
fn uninstall() {
    // SAFETY: puts back the default handler.
    unsafe {
        libc::signal(libc::SIGINT, libc::SIG_DFL);
    }
}

#[cfg(windows)]
unsafe extern "system" fn on_ctrl(kind: u32) -> windows_sys::core::BOOL {
    use windows_sys::Win32::System::Console::{CTRL_BREAK_EVENT, CTRL_C_EVENT};
    if kind == CTRL_C_EVENT || kind == CTRL_BREAK_EVENT {
        PRESSED.store(true, Ordering::Relaxed);
        1
    } else {
        0
    }
}

#[cfg(windows)]
fn install() {
    // SAFETY: adds a handler that only stores to an atomic.
    unsafe {
        windows_sys::Win32::System::Console::SetConsoleCtrlHandler(Some(on_ctrl), 1);
    }
}

#[cfg(windows)]
fn uninstall() {
    // SAFETY: removes the handler `install` added.
    unsafe {
        windows_sys::Win32::System::Console::SetConsoleCtrlHandler(Some(on_ctrl), 0);
    }
}

#[cfg(not(any(unix, windows)))]
fn install() {}

#[cfg(not(any(unix, windows)))]
fn uninstall() {}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn ctrl_c_while_held_is_noted_not_fatal() {
        let held = Deferred::new();
        assert!(!held.pressed());
        // SAFETY: raises SIGINT in this process, whose handler is ours.
        unsafe {
            libc::raise(libc::SIGINT);
        }
        assert!(held.pressed(), "still running, and it's noted");
    }
}
