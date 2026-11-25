//! Stop requests during an upload (Ctrl+C, the terminal closing, `kill`):
//! the upload finishes first. Stopping it partway leaves the badge ignoring
//! every upload until it's unplugged.

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

/// The signal asking to stop (on Windows, 2 for Ctrl+C or Break, 1 for
/// closing the console), 0 while none has.
static STOP: AtomicI32 = AtomicI32::new(0);
static HELD: AtomicBool = AtomicBool::new(false);

/// Said once, as soon as a stop is asked for.
const FINISHING: &str = "\nFinishing the upload first: stopping it partway would leave the badge \
                         ignoring uploads until it's unplugged and plugged back in.\n";

/// While it's held, a request to stop is noted instead of ending the
/// program, and output that can't be written doesn't end it either.
pub struct Deferred {
    terminal: Option<Terminal>,
}

impl Deferred {
    pub fn new() -> Self {
        STOP.store(0, Ordering::Relaxed);
        HELD.store(true, Ordering::Relaxed);
        install();
        Deferred {
            terminal: Terminal::hold(),
        }
    }

    /// Ends the hold, once the upload is done: the exit status to stop
    /// with, if a stop was asked for (130 for Ctrl+C, as shells report it).
    pub fn finish(self) -> Option<i32> {
        drop(self);
        match STOP.load(Ordering::Relaxed) {
            0 => None,
            signal => Some(128 + signal),
        }
    }
}

impl Drop for Deferred {
    fn drop(&mut self) {
        HELD.store(false, Ordering::Relaxed);
        if let Some(terminal) = self.terminal.take() {
            terminal.release();
        }
        uninstall();
    }
}

#[cfg(unix)]
mod platform {
    use std::fs::{File, OpenOptions};
    use std::io::Read;
    use std::os::fd::{AsRawFd, RawFd};
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread::JoinHandle;

    use super::{FINISHING, HELD, STOP};

    const SIGNALS: [libc::c_int; 4] = [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGPIPE];
    static PREVIOUS: [AtomicUsize; 4] = [const { AtomicUsize::new(0) }; 4];

    extern "C" fn on_signal(signal: libc::c_int) {
        if STOP
            .compare_exchange(0, signal, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            // SAFETY: write(2) is safe in a signal handler; a failed write
            // is ignored.
            unsafe {
                libc::write(2, FINISHING.as_ptr().cast(), FINISHING.len());
            }
        }
    }

    pub fn install() {
        for (signal, previous) in SIGNALS.into_iter().zip(&PREVIOUS) {
            let handler = if signal == libc::SIGPIPE {
                libc::SIG_IGN
            } else {
                on_signal as *const () as libc::sighandler_t
            };
            // SAFETY: the handler only uses an atomic and write(2), which
            // are safe in a signal handler.
            let was = unsafe { libc::signal(signal, handler) };
            previous.store(was, Ordering::Relaxed);
            if was == libc::SIG_IGN {
                // SAFETY: as above.
                unsafe {
                    libc::signal(signal, libc::SIG_IGN);
                }
            }
        }
    }

    pub fn uninstall() {
        for (signal, previous) in SIGNALS.into_iter().zip(&PREVIOUS) {
            // SAFETY: puts back the handler there was before `install`.
            unsafe {
                libc::signal(signal, previous.load(Ordering::Relaxed));
            }
        }
    }

    /// In the Flatpak, Ctrl+C would also end `flatpak run`, outside the
    /// sandbox, at once: the upload goes on, but the shell stops waiting
    /// for it. So while it's held there, the terminal doesn't turn Ctrl+C
    /// into a signal, and the key is watched for instead.
    pub struct Terminal {
        tty: File,
        saved: libc::termios,
        watcher: JoinHandle<()>,
    }

    impl Terminal {
        pub fn hold() -> Option<Self> {
            if !Path::new("/.flatpak-info").exists() {
                return None;
            }
            let tty = OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_NOCTTY | libc::O_CLOEXEC)
                .open("/dev/tty")
                .ok()?;
            let fd = tty.as_raw_fd();
            if !foreground(fd) {
                return None;
            }
            // SAFETY: a termios is plain data, filled in by tcgetattr.
            let mut saved: libc::termios = unsafe { std::mem::zeroed() };
            // SAFETY: `fd` is open, and `saved` is a termios.
            if unsafe { libc::tcgetattr(fd, &mut saved) } != 0 {
                return None;
            }
            let reader = tty.try_clone().ok()?;
            let mut quiet = saved;
            quiet.c_lflag &= !(libc::ISIG | libc::ICANON);
            quiet.c_cc[libc::VMIN] = 0;
            quiet.c_cc[libc::VTIME] = 0;
            // SAFETY: `fd` is open, and `quiet` is a termios.
            if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &quiet) } != 0 {
                return None;
            }
            let intr = saved.c_cc[libc::VINTR];
            let watcher = std::thread::spawn(move || watch(reader, intr));
            Some(Terminal { tty, saved, watcher })
        }

        pub fn release(self) {
            let _ = self.watcher.join();
            // SAFETY: the terminal is open, and `saved` is how it was.
            unsafe {
                libc::tcsetattr(self.tty.as_raw_fd(), libc::TCSANOW, &self.saved);
            }
        }
    }

    /// Whether this process is in the terminal's foreground: reading from
    /// the background fails instead (with SIGTTIN ignored, which would
    /// otherwise stop it), and reading nothing takes no input.
    fn foreground(fd: RawFd) -> bool {
        let mut byte = 0u8;
        // SAFETY: SIGTTIN is ignored only for this read, and the read
        // writes nothing into `byte`.
        unsafe {
            let was = libc::signal(libc::SIGTTIN, libc::SIG_IGN);
            let read = libc::read(fd, (&raw mut byte).cast(), 0);
            libc::signal(libc::SIGTTIN, was);
            read == 0
        }
    }

    /// Notes Ctrl+C typed while it's held, then once more what was typed
    /// up to the end.
    fn watch(mut tty: File, intr: u8) {
        let mut buf = [0u8; 256];
        loop {
            let held = HELD.load(Ordering::Relaxed);
            let mut poll = libc::pollfd {
                fd: tty.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: `poll` is one pollfd for an open file.
            let ready = unsafe { libc::poll(&mut poll, 1, if held { 100 } else { 0 }) };
            if ready > 0 && poll.revents & libc::POLLIN == 0 {
                return;
            }
            while ready > 0
                && let Ok(n @ 1..) = tty.read(&mut buf)
            {
                if buf[..n].contains(&intr) {
                    on_signal(libc::SIGINT);
                }
            }
            if !held {
                return;
            }
        }
    }
}

#[cfg(windows)]
mod platform {
    use std::io::Write;
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    use windows_sys::Win32::System::Console::{CTRL_BREAK_EVENT, CTRL_C_EVENT, SetConsoleCtrlHandler};

    use super::{FINISHING, HELD, STOP};

    unsafe extern "system" fn on_ctrl(kind: u32) -> windows_sys::core::BOOL {
        let interrupt = kind == CTRL_C_EVENT || kind == CTRL_BREAK_EVENT;
        let signal = if interrupt { 2 } else { 1 };
        if STOP
            .compare_exchange(0, signal, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            let _ = std::io::stderr().write_all(FINISHING.as_bytes());
        }
        if !interrupt {
            let until = Instant::now() + Duration::from_secs(30);
            while HELD.load(Ordering::Relaxed) && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        1
    }

    pub fn install() {
        // SAFETY: adds a handler that only uses atomics and stderr.
        unsafe {
            SetConsoleCtrlHandler(Some(on_ctrl), 1);
        }
    }

    pub fn uninstall() {
        // SAFETY: removes the handler `install` added.
        unsafe {
            SetConsoleCtrlHandler(Some(on_ctrl), 0);
        }
    }

    pub struct Terminal;

    impl Terminal {
        pub fn hold() -> Option<Self> {
            None
        }

        pub fn release(self) {}
    }
}

#[cfg(not(any(unix, windows)))]
mod platform {
    pub fn install() {}
    pub fn uninstall() {}

    pub struct Terminal;

    impl Terminal {
        pub fn hold() -> Option<Self> {
            None
        }

        pub fn release(self) {}
    }
}

use platform::{Terminal, install, uninstall};

/// Writes to stderr, ignoring a failure (a reader that went away), which
/// mustn't stop an upload.
pub fn note(args: std::fmt::Arguments) {
    use std::io::Write;
    let _ = std::io::stderr().write_fmt(args);
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn stop_requests_while_held_are_noted_not_fatal() {
        let held = Deferred::new();
        // SAFETY: raises signals in this process, whose handlers are ours.
        unsafe {
            libc::raise(libc::SIGPIPE);
            libc::raise(libc::SIGTERM);
            libc::raise(libc::SIGINT);
        }
        assert_eq!(
            held.finish(),
            Some(128 + libc::SIGTERM),
            "still running, the first noted"
        );

        // SAFETY: changes this process's SIGHUP handling, put back below.
        let was = unsafe { libc::signal(libc::SIGHUP, libc::SIG_IGN) };
        let held = Deferred::new();
        // SAFETY: reads SIGHUP's handling without changing it.
        let during = unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            libc::sigaction(libc::SIGHUP, std::ptr::null(), &mut action);
            action.sa_sigaction
        };
        assert_eq!(during, libc::SIG_IGN);
        drop(held);
        // SAFETY: as above.
        let after = unsafe { libc::signal(libc::SIGHUP, was) };
        assert_eq!(after, libc::SIG_IGN);
    }
}
