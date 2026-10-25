//! Device discovery and upload over USB HID or a serial port.

mod hotplug;
pub use hotplug::{DeviceWatcher, watch_devices};

use std::ffi::CString;
use std::io::Write;
use std::thread::sleep;
use std::time::Duration;

use badge_core::protocol::{self, USB_PRODUCT_ID, USB_VENDOR_ID};
use badge_core::{AddressMode, Project, Timestamp};
use chrono::{Datelike, Timelike};

/// udev rule that grants the logged-in user access to the badge. It must sort
/// before systemd's 73-seat-late.rules, which is what applies `uaccess`.
pub const UDEV_RULE: &str =
    "SUBSYSTEM==\"hidraw\", ATTRS{idVendor}==\"0416\", ATTRS{idProduct}==\"5020\", MODE=\"0660\", TAG+=\"uaccess\"\n";
pub const UDEV_RULE_PATH: &str = "/etc/udev/rules.d/70-led-badge.rules";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DeviceKind {
    Usb,
    Serial,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DeviceInfo {
    pub kind: DeviceKind,
    pub id: String,
    /// Human-readable name for pickers.
    pub label: String,
}

#[derive(Debug, thiserror::Error)]
pub enum SendError {
    #[error("nothing to send: select at least one message with content")]
    Empty,
    #[cfg_attr(
        target_os = "linux",
        error(
            "permission denied opening the badge: set up badge access (File → Badge Access Help…, or `badgectl udev install`)"
        )
    )]
    #[cfg_attr(
        not(target_os = "linux"),
        error("permission denied opening the badge (is another program using it?)")
    )]
    PermissionDenied,
    #[error("badge not found (was it unplugged?)")]
    NotFound,
    #[error("the device isn't taking the data; it may not be a badge")]
    NotResponding,
    #[error("USB error: {0}")]
    Hid(String),
    #[error("serial port error: {0}")]
    Serial(#[from] serialport::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("canceled")]
    Canceled,
}

impl From<hidapi::HidError> for SendError {
    fn from(e: hidapi::HidError) -> Self {
        let s = e.to_string();
        if s.contains("ermission") || s.contains("EACCES") || s.contains("os error 13") {
            SendError::PermissionDenied
        } else {
            SendError::Hid(s)
        }
    }
}

/// True when this process may write to the device (always true off Linux,
/// where no special permissions are needed).
pub fn can_write(device: &DeviceInfo) -> bool {
    #[cfg(target_os = "linux")]
    {
        if device.kind == DeviceKind::Usb && device.id.starts_with('/') {
            return std::fs::OpenOptions::new().write(true).open(&device.id).is_ok();
        }
    }
    let _ = device;
    true
}

/// Why setting up access failed.
#[derive(Debug, thiserror::Error)]
pub enum AccessSetupError {
    #[error("canceled")]
    Canceled,
    #[error("pkexec (polkit) is not available; run the commands from Badge Access Help in a terminal")]
    NoPkexec,
    #[error(
        "this Flatpak can't run commands on your system (it lacks the org.freedesktop.Flatpak permission); \
         run the commands from Badge Access Help in a terminal"
    )]
    NoHostAccess,
    #[error("authorization failed - if your account has no password (e.g. SteamOS), set one with `passwd` first")]
    NotAuthorized,
    #[error("{0}")]
    Other(String),
}

/// Rule files from development builds, named so they sorted after
/// 73-seat-late.rules. Removed on install and uninstall.
const LEGACY_UDEV_RULE_PATHS: &[&str] = &["/etc/udev/rules.d/99-led-badge.rules"];

/// State of the badge's udev rule on this system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UdevRuleState {
    /// The current rule is installed.
    Installed,
    /// A rule file exists but differs from (or is named differently than)
    /// the one this version installs.
    Outdated,
    NotInstalled,
    /// Not applicable (not Linux).
    NotNeeded,
    /// Can't be checked: running in a Flatpak that may not look at the
    /// host system (see [`can_set_up_access`]).
    Unknown,
}

pub fn in_flatpak() -> bool {
    cfg!(target_os = "linux") && std::path::Path::new("/.flatpak-info").exists()
}

/// Whether this Flatpak may run commands on the host (through
/// `flatpak-spawn --host`, which needs the org.freedesktop.Flatpak
/// permission). Checked once.
fn flatpak_host_access() -> bool {
    static ACCESS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ACCESS.get_or_init(|| {
        std::process::Command::new("flatpak-spawn")
            .args(["--host", "true"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    })
}

/// A command that runs on the host system: directly, or through
/// `flatpak-spawn --host` inside a Flatpak.
fn host_command(program: &str) -> std::process::Command {
    if in_flatpak() {
        let mut c = std::process::Command::new("flatpak-spawn");
        c.args(["--host", program]);
        c
    } else {
        std::process::Command::new(program)
    }
}

/// Whether the udev rule can be installed or removed from here (with a
/// password prompt). False off Linux and in a Flatpak without host access.
pub fn can_set_up_access() -> bool {
    cfg!(target_os = "linux") && (!in_flatpak() || flatpak_host_access())
}

/// Reads a file of the host system (`/etc` inside a Flatpak is the
/// runtime's, not the host's).
fn read_host_file(path: &str) -> Option<String> {
    if !in_flatpak() {
        return std::fs::read_to_string(path).ok();
    }
    let out = host_command("cat")
        .arg(path)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn host_file_exists(path: &str) -> bool {
    if !in_flatpak() {
        return std::path::Path::new(path).exists();
    }
    host_command("test")
        .args(["-e", path])
        .status()
        .is_ok_and(|s| s.success())
}

/// Checks the badge's udev rule. Inside a Flatpak this runs commands on the
/// host, so call it off the UI thread.
pub fn udev_rule_state() -> UdevRuleState {
    if !cfg!(target_os = "linux") {
        return UdevRuleState::NotNeeded;
    }
    if in_flatpak() && !flatpak_host_access() {
        return UdevRuleState::Unknown;
    }
    let legacy = LEGACY_UDEV_RULE_PATHS.iter().any(|p| host_file_exists(p));
    match read_host_file(UDEV_RULE_PATH) {
        Some(s) if s == UDEV_RULE && !legacy => UdevRuleState::Installed,
        Some(_) => UdevRuleState::Outdated,
        None if legacy => UdevRuleState::Outdated,
        None => UdevRuleState::NotInstalled,
    }
}

/// Re-applies rules to hidraw devices so permission changes take effect
/// without re-plugging.
const UDEV_RELOAD: &str = "udevadm control --reload-rules && \
     udevadm trigger --subsystem-match=hidraw --action=change && udevadm settle --timeout=5";

/// Installs [`UDEV_RULE`] with `pkexec` (a password prompt) and reloads udev,
/// so the badge becomes writable without re-plugging. Blocks until the user
/// answers.
pub fn install_udev_rule() -> Result<(), AccessSetupError> {
    let script = format!(
        "rm -f {legacy} && printf '%s' \"$1\" > {path} && chmod 644 {path} && {UDEV_RELOAD}",
        legacy = LEGACY_UDEV_RULE_PATHS.join(" "),
        path = UDEV_RULE_PATH
    );
    run_privileged(&script, &[UDEV_RULE])
}

/// Removes the udev rule with `pkexec`. A plugged-in badge may stay writable
/// until it's re-plugged.
pub fn remove_udev_rule() -> Result<(), AccessSetupError> {
    let script = format!(
        "rm -f {path} {legacy} && {UDEV_RELOAD}",
        legacy = LEGACY_UDEV_RULE_PATHS.join(" "),
        path = UDEV_RULE_PATH
    );
    run_privileged(&script, &[])
}

/// Runs `/bin/sh -c script sh args…` as root through `pkexec`, on the host
/// system when inside a Flatpak.
#[allow(unused_variables)]
fn run_privileged(script: &str, args: &[&str]) -> Result<(), AccessSetupError> {
    #[cfg(target_os = "linux")]
    {
        if in_flatpak() {
            if !flatpak_host_access() {
                return Err(AccessSetupError::NoHostAccess);
            }
            // flatpak-spawn can't report a missing program apart from other
            // failures, so check for pkexec first.
            let has_pkexec = host_command("sh")
                .args(["-c", "command -v pkexec"])
                .stdout(std::process::Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            if !has_pkexec {
                return Err(AccessSetupError::NoPkexec);
            }
        }
        let out = host_command("pkexec")
            .args(["/bin/sh", "-c", script, "sh"])
            .args(args)
            .output()
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => AccessSetupError::NoPkexec,
                _ => AccessSetupError::Other(e.to_string()),
            })?;
        return match out.status.code() {
            Some(0) => Ok(()),
            // pkexec: 126 = dialog dismissed, 127 = not authorized / no agent.
            Some(126) => Err(AccessSetupError::Canceled),
            Some(127) => Err(AccessSetupError::NotAuthorized),
            _ => Err(AccessSetupError::Other(
                String::from_utf8_lossy(&out.stderr).trim().to_string(),
            )),
        };
    }
    #[allow(unreachable_code)]
    Ok(())
}

pub fn now() -> Timestamp {
    let t = chrono::Local::now();
    Timestamp {
        year: t.year().clamp(0, u16::MAX as i32) as u16,
        month: t.month() as u8,
        day: t.day() as u8,
        hour: t.hour() as u8,
        minute: t.minute() as u8,
        second: t.second() as u8,
    }
}

/// Lists connected badges (USB) and plausible serial ports.
pub fn list_devices() -> Vec<DeviceInfo> {
    let mut out = list_usb();
    out.extend(list_serial());
    out
}

type HidJob = Box<dyn FnOnce() + Send>;

/// Runs `f` on the one thread that lists and opens USB devices, which lives
/// as long as the program.
///
/// On macOS hidapi ties its device manager to the run loop of the thread
/// that first lists devices. Once that thread ends, the next listing on any
/// other thread uses its freed run loop and the app crashes (SIGTRAP in
/// `CFRunLoopAddSource`); the app's background threads end when idle, so it
/// crashed while in the background. One thread also keeps hidapi's shared
/// state from being used by two threads at once. Writing to an open device
/// can happen on any thread.
fn on_hid_thread<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> R {
    static JOBS: std::sync::OnceLock<std::sync::mpsc::Sender<HidJob>> = std::sync::OnceLock::new();
    let jobs = JOBS.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<HidJob>();
        std::thread::Builder::new()
            .name("usb-devices".into())
            .spawn(move || rx.into_iter().for_each(|job| job()))
            .expect("couldn't start the USB device thread");
        tx
    });
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    jobs.send(Box::new(move || {
        let _ = tx.send(f());
    }))
    .expect("the USB device thread stopped");
    rx.recv().expect("the USB device thread stopped")
}

/// Windows: held while sending to a USB badge, and by listing, which waits
/// for a send to end. Listing there asks every device for its name and
/// serial number, and a badge asked in the middle of an upload drops it
/// (the write times out, and Windows resets the badge). macOS and Linux
/// list from what the system already knows.
#[cfg(windows)]
static USB_SENDING: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn list_usb() -> Vec<DeviceInfo> {
    #[cfg(windows)]
    let _sending = USB_SENDING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let Ok(api) = on_hid_thread(hidapi::HidApi::new) else {
        return Vec::new();
    };
    // Sorted, de-duplicated paths give stable numbering ("USB badge 1", 2, …).
    api.device_list()
        .filter(|d| d.vendor_id() == USB_VENDOR_ID && d.product_id() == USB_PRODUCT_ID)
        .map(|d| d.path().to_string_lossy().into_owned())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .enumerate()
        .map(|(i, path)| DeviceInfo {
            kind: DeviceKind::Usb,
            label: format!("USB badge {}", i + 1),
            id: path,
        })
        .collect()
}

pub fn list_serial() -> Vec<DeviceInfo> {
    let Ok(ports) = serialport::available_ports() else {
        return Vec::new();
    };
    ports
        .into_iter()
        .filter(|p| {
            matches!(p.port_type, serialport::SerialPortType::UsbPort(_))
                || p.port_name.contains("ttyUSB")
                || p.port_name.contains("ttyACM")
                || p.port_name.to_ascii_uppercase().starts_with("COM")
        })
        .map(|p| DeviceInfo {
            kind: DeviceKind::Serial,
            label: format!("Serial {}", p.port_name.rsplit('/').next().unwrap_or(&p.port_name)),
            id: p.port_name,
        })
        .collect()
}

/// Upload progress: `(done, total)` in packets. Return `false` to cancel.
pub type Progress<'a> = &'a mut dyn FnMut(usize, usize) -> bool;

pub fn send(project: &Project, address: AddressMode, device: &DeviceInfo, progress: Progress) -> Result<(), SendError> {
    if protocol::is_empty(project) {
        return Err(SendError::Empty);
    }
    match device.kind {
        DeviceKind::Usb => send_usb(&protocol::encode_usb(project, address, now()), &device.id, progress),
        DeviceKind::Serial => send_serial(&protocol::encode_serial(project, address, now()), &device.id, progress),
    }
}

/// Writes an encoded USB upload stream to the badge at `path`.
///
/// On Windows an upload that another program interrupts (by listing HID
/// devices, which asks the badge for its name; see [`USB_SENDING`]) is
/// started again: Windows resets the badge, which takes it away for a moment
/// and then accepts a new upload.
fn send_usb(stream: &[u8], path: &str, progress: Progress) -> Result<(), SendError> {
    #[cfg(windows)]
    {
        const RETRIES: u32 = 3;
        let _sending = USB_SENDING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut retry = 0;
        loop {
            match send_usb_once(stream, path, &mut *progress) {
                Err(e) if retry < RETRIES && interrupted(&e) => {
                    retry += 1;
                    log::warn!("sending was interrupted ({e}); starting again ({retry} of {RETRIES})");
                    sleep(Duration::from_secs(1));
                }
                result => return result,
            }
        }
    }
    #[cfg(not(windows))]
    send_usb_once(stream, path, progress)
}

/// Windows: whether an upload failed because the badge was reset or briefly
/// gone (see [`send_usb`]), so that trying again may work.
#[cfg(windows)]
fn interrupted(e: &SendError) -> bool {
    let SendError::Hid(text) = e else {
        return matches!(e, SendError::NotFound);
    };
    // A write that got no answer; the device isn't there (2, 1167) or not
    // working (31) while it's reset.
    ["WaitTimedOut", "(os error 2)", "(os error 31)", "(os error 1167)"]
        .iter()
        .any(|code| text.contains(code))
}

fn send_usb_once(stream: &[u8], path: &str, progress: Progress) -> Result<(), SendError> {
    let cpath = CString::new(path).map_err(|_| SendError::NotFound)?;
    let dev = on_hid_thread(move || hidapi::HidApi::new()?.open_path(&cpath)).map_err(|e| {
        let err = SendError::from(e);
        if matches!(err, SendError::Hid(_)) && !std::path::Path::new(path).exists() && path.starts_with('/') {
            SendError::NotFound
        } else {
            err
        }
    })?;
    let mut reports = protocol::usb_reports(stream);
    let pacing = Pacing::from_env();
    if pacing.trailing_pad {
        reports.push([0u8; protocol::REPORT_SIZE + 1]);
    }
    let total = reports.len();
    let gap = pacing.gap_for(total);
    for (i, r) in reports.iter().enumerate() {
        if i > 0 {
            sleep(gap);
        }
        let n = dev.write(r)?;
        // hidapi's Windows backend returns 0 when the write finished at once
        // (it only counts bytes for writes that completed in the background):
        // that's a complete write, not a short one.
        let n = if cfg!(windows) && n == 0 { r.len() } else { n };
        if n < r.len() - 1 {
            return Err(SendError::Hid(format!("short write ({n} of {} bytes)", r.len())));
        }
        if !progress(i + 1, total) {
            return Err(SendError::Canceled);
        }
    }
    // Keep the device open (and the kernel polling it) while the badge works
    // through its queue; closing early loses the last reports.
    sleep(pacing.tail_hold);
    drop(dev);
    Ok(())
}

/// Upload timing for USB badges.
///
/// Measured on a WCH CH546 badge (0416:5020): the badge acknowledges every
/// 64-byte report immediately but needs roughly 200-300 ms to store each one
/// and can queue only about 7. Reports arriving into a full queue are
/// corrupted or lost, and a lost report leaves the badge waiting forever for
/// the rest of the upload (it ignores everything until power-cycled). There is
/// no ready signal, so we pace by time:
///
/// * uploads that fit in the queue (`fast_limit` reports) go out quickly,
/// * longer ones are sent one report every `gap`,
/// * one extra all-zero report follows the data: the badge only finishes a
///   report when the next arrives, so without it the last one was sometimes
///   lost (it ignores bytes beyond the lengths in the header),
/// * the device stays open for `tail_hold` so the queue drains.
///
/// Observed with 82-report uploads: 250 ms gaps fail, 300 ms and 350 ms work;
/// the default leaves some margin.
#[derive(Clone, Copy, Debug)]
pub struct Pacing {
    pub fast_limit: usize,
    pub fast_gap: Duration,
    pub gap: Duration,
    pub tail_hold: Duration,
    pub trailing_pad: bool,
}

impl Default for Pacing {
    fn default() -> Self {
        Pacing {
            fast_limit: 6,
            fast_gap: Duration::from_millis(20),
            gap: Duration::from_millis(325),
            tail_hold: Duration::from_millis(1200),
            trailing_pad: true,
        }
    }
}

impl Pacing {
    /// Defaults, overridable for experiments with `LED_BADGE_STUDIO_FAST_LIMIT`,
    /// `LED_BADGE_STUDIO_REPORT_GAP_MS`, `LED_BADGE_STUDIO_TAIL_HOLD_MS` and
    /// `LED_BADGE_STUDIO_TRAILING_PAD`.
    pub fn from_env() -> Self {
        let num = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<u64>().ok());
        let d = Pacing::default();
        Pacing {
            fast_limit: num("LED_BADGE_STUDIO_FAST_LIMIT").map_or(d.fast_limit, |v| v as usize),
            fast_gap: d.fast_gap,
            gap: num("LED_BADGE_STUDIO_REPORT_GAP_MS").map_or(d.gap, Duration::from_millis),
            tail_hold: num("LED_BADGE_STUDIO_TAIL_HOLD_MS").map_or(d.tail_hold, Duration::from_millis),
            trailing_pad: num("LED_BADGE_STUDIO_TRAILING_PAD").map_or(d.trailing_pad, |v| v != 0),
        }
    }

    /// Pause between reports for an upload of `total` reports (padding included).
    fn gap_for(&self, total: usize) -> Duration {
        if total <= self.fast_limit {
            self.fast_gap
        } else {
            self.gap
        }
    }

    /// Expected duration of an upload of `data_reports` data reports.
    pub fn estimate(&self, data_reports: usize) -> Duration {
        let total = data_reports + usize::from(self.trailing_pad);
        self.gap_for(total) * total.saturating_sub(1) as u32 + self.tail_hold
    }
}

pub fn send_serial(plan: &protocol::SerialPlan, port_name: &str, progress: Progress) -> Result<(), SendError> {
    // How long one byte takes on the wire: a start bit, 8 data bits and the
    // stop bits.
    let bits: u32 = if plan.two_stop_bits { 11 } else { 10 };
    let byte_time = Duration::from_secs(bits.into()) / plan.baud_rate;
    let mut port = serialport::new(port_name, plan.baud_rate)
        .data_bits(serialport::DataBits::Eight)
        .parity(serialport::Parity::None)
        .stop_bits(if plan.two_stop_bits {
            serialport::StopBits::Two
        } else {
            serialport::StopBits::One
        })
        .timeout(write_timeout(plan, byte_time))
        .open()?;
    port.write_data_terminal_ready(true)?;
    let total = plan.packets.len();
    for (i, p) in plan.packets.iter().enumerate() {
        sleep(Duration::from_millis(plan.gap_ms));
        let sent = port
            .write_all(p)
            .map_err(not_responding_on_timeout)
            .and_then(|()| wait_until_sent(|| port.bytes_to_write().map_err(Into::into), p.len(), byte_time));
        if let Err(e) = sent {
            // Throw away what the device didn't take: closing the port would
            // otherwise wait for it to go out (up to 30 s on Linux).
            let _ = port.clear(serialport::ClearBuffer::Output);
            return Err(e);
        }
        if i == 1 {
            sleep(Duration::from_millis(plan.settle_ms));
        }
        if !progress(i + 1, total) {
            return Err(SendError::Canceled);
        }
    }
    Ok(())
}

/// How long a write may take: 10 seconds, or more for packets that take
/// longer than that on the wire (a mono badge's rows at 1200 baud: up to
/// 2560 bytes, about 21 s), where writes wait for the data to go out.
fn write_timeout(plan: &protocol::SerialPlan, byte_time: Duration) -> Duration {
    let longest = plan.packets.iter().map(Vec::len).max().unwrap_or(0);
    (byte_time * longest as u32 * 2 + Duration::from_secs(1)).max(Duration::from_secs(10))
}

/// A write that timed out: the device stopped taking data.
fn not_responding_on_timeout(e: std::io::Error) -> SendError {
    if e.kind() == std::io::ErrorKind::TimedOut {
        SendError::NotResponding
    } else {
        SendError::Io(e)
    }
}

/// Waits until the port has sent the `len` bytes just written, as `flush`
/// does, but with a time limit: a device that doesn't take the data (one
/// that isn't a badge) would keep `flush` waiting forever. `queued` reports
/// how many bytes are still waiting to go out.
fn wait_until_sent(
    queued: impl Fn() -> std::io::Result<u32>,
    len: usize,
    byte_time: Duration,
) -> Result<(), SendError> {
    let on_wire = byte_time * len as u32;
    // Twice the time the bytes take on the wire, and a second to spare.
    let deadline = std::time::Instant::now() + on_wire * 2 + Duration::from_secs(1);
    loop {
        match queued() {
            Ok(0) => break,
            Ok(_) if std::time::Instant::now() >= deadline => return Err(SendError::NotResponding),
            Ok(_) => sleep(Duration::from_millis(5)),
            // Not known on this system: wait as long as the bytes take.
            Err(_) => {
                sleep(on_wire);
                break;
            }
        }
    }
    // The serial chip's own buffer (16 bytes on common ones) still goes out
    // after the queue is empty; `flush` waited for that too.
    sleep(byte_time * 16);
    Ok(())
}

#[cfg(test)]
mod serial_tests {
    use super::{SendError, wait_until_sent};
    use std::cell::Cell;
    use std::time::{Duration, Instant};

    /// 100 bytes at 1 ms a byte: 0.1 s on the wire, so a 1.2 s deadline.
    const BYTE: Duration = Duration::from_millis(1);

    #[test]
    fn the_write_timeout_covers_the_longest_packet() {
        let plan = |len| badge_core::protocol::SerialPlan {
            baud_rate: 1200,
            two_stop_bits: false,
            packets: vec![vec![0; 6], vec![0; len]],
            gap_ms: 30,
            settle_ms: 1000,
        };
        let byte = Duration::from_secs(10) / 1200;
        assert_eq!(super::write_timeout(&plan(47), byte), Duration::from_secs(10));
        // 2560 bytes: about 21.3 s on the wire, so about 43.7 s.
        let long = super::write_timeout(&plan(2560), byte);
        assert!(
            long > Duration::from_secs(43) && long < Duration::from_secs(44),
            "{long:?}"
        );
    }

    #[test]
    fn a_queue_that_empties_is_sent() {
        let left = Cell::new(3u32);
        let r = wait_until_sent(|| Ok(left.replace(left.get().saturating_sub(1))), 100, BYTE);
        assert!(r.is_ok());
        assert_eq!(left.get(), 0);
    }

    #[test]
    fn a_queue_that_never_empties_gives_up_at_the_deadline() {
        let start = Instant::now();
        let r = wait_until_sent(|| Ok(100), 100, BYTE);
        assert!(matches!(r, Err(SendError::NotResponding)), "{r:?}");
        let waited = start.elapsed();
        assert!(
            waited >= Duration::from_millis(1200) && waited < Duration::from_secs(3),
            "{waited:?}"
        );
    }

    #[test]
    fn without_a_queue_count_it_waits_as_long_as_the_bytes_take() {
        let start = Instant::now();
        let r = wait_until_sent(|| Err(std::io::ErrorKind::Unsupported.into()), 100, BYTE);
        assert!(r.is_ok());
        // The bytes' time on the wire, then the chip's 16 bytes.
        assert!(start.elapsed() >= Duration::from_millis(116), "{:?}", start.elapsed());
    }
}

#[cfg(test)]
mod tests {
    /// Listing on a thread that has ended, then on others, some at once,
    /// crashed on macOS before `on_hid_thread`. Uses whatever HID devices
    /// this computer has.
    #[test]
    fn listing_usb_from_several_threads_at_once() {
        std::thread::spawn(super::list_usb).join().unwrap();
        for _ in 0..50 {
            let threads: Vec<_> = (0..4).map(|_| std::thread::spawn(super::list_usb)).collect();
            for t in threads {
                t.join().unwrap();
            }
        }
    }
}
