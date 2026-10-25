//! Notifications when devices are plugged in or removed, so the device list
//! can be refreshed on demand instead of polled.
//!
//! - Linux: a netlink socket receiving kernel and udev uevents for HID, USB and
//!   serial (tty) devices. No extra libraries or permissions are needed.
//! - Windows: `CM_Register_Notification` for the HID, USB device and COM port
//!   interface classes (Windows 8 and later). Unlike `WM_DEVICECHANGE`, which
//!   the vendor software uses, this needs no window.
//! - macOS: IOKit matching notifications for badges (HID devices with the
//!   badge's USB ID) and serial ports, delivered to a run loop on a thread of
//!   their own.
//! - Elsewhere [`watch_devices`] returns `None` and callers should poll.
//!
//! The callback may fire several times for one plug-in (one event per
//! interface, plus the kernel's and udev's copy on Linux) and runs on a
//! background thread; callers should debounce before rescanning.

/// Keeps device notifications running; dropping it stops them.
pub struct DeviceWatcher {
    #[allow(dead_code)]
    inner: imp::Watcher,
}

/// Calls `on_change` whenever a relevant device appears or disappears.
/// Returns `None` if notifications aren't available on this system.
pub fn watch_devices(on_change: impl Fn() + Send + Sync + 'static) -> Option<DeviceWatcher> {
    imp::watch(Box::new(on_change)).map(|inner| DeviceWatcher { inner })
}

type Callback = Box<dyn Fn() + Send + Sync>;

// One `imp` per system: Linux, Windows, macOS, or a stand-in that reports
// notifications aren't available.
cfg_select! {
    target_os = "linux" => {
        mod imp {
            use super::Callback;
            use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
            use std::sync::atomic::{AtomicBool, Ordering};
            use std::sync::Arc;
            use std::thread::JoinHandle;

            /// Kernel uevents (sent before udev has run its rules).
            const GROUP_KERNEL: u32 = 1;
            /// udev's re-broadcast, sent after its rules ran (permissions applied).
            const GROUP_UDEV: u32 = 2;
            const SUBSYSTEMS: &[&[u8]] = &[b"hidraw", b"usb", b"tty"];

            pub struct Watcher {
                stop: Arc<AtomicBool>,
                thread: Option<JoinHandle<()>>,
            }

            impl Drop for Watcher {
                fn drop(&mut self) {
                    self.stop.store(true, Ordering::Relaxed);
                    if let Some(t) = self.thread.take() {
                        let _ = t.join();
                    }
                }
            }

            pub fn watch(on_change: Callback) -> Option<Watcher> {
                // Device events are only sent to the host's network namespace: a
                // Flatpak without --share=network gets a socket that never receives
                // anything, so report "unavailable" and let the caller poll.
                if crate::in_flatpak() && !flatpak_shares_network() {
                    return None;
                }
                let fd = open_socket()?;
                let stop = Arc::new(AtomicBool::new(false));
                let thread = std::thread::Builder::new()
                    .name("device-hotplug".into())
                    .spawn({
                        let stop = stop.clone();
                        move || run(fd, &stop, &on_change)
                    })
                    .ok()?;
                Some(Watcher {
                    stop,
                    thread: Some(thread),
                })
            }

            /// Reads `shared=…` from the `[Context]` section of /.flatpak-info.
            fn flatpak_shares_network() -> bool {
                let Ok(info) = std::fs::read_to_string("/.flatpak-info") else {
                    return false;
                };
                let mut in_context = false;
                for line in info.lines() {
                    let line = line.trim();
                    if line.starts_with('[') {
                        in_context = line == "[Context]";
                    } else if let Some(v) = line.strip_prefix("shared=").filter(|_| in_context) {
                        return v.split(';').any(|s| s == "network");
                    }
                }
                false
            }

            fn open_socket() -> Option<OwnedFd> {
                // SAFETY: plain socket/bind calls; the fd is owned right away.
                unsafe {
                    let raw = libc::socket(
                        libc::AF_NETLINK,
                        libc::SOCK_DGRAM | libc::SOCK_CLOEXEC,
                        libc::NETLINK_KOBJECT_UEVENT,
                    );
                    if raw < 0 {
                        return None;
                    }
                    let fd = OwnedFd::from_raw_fd(raw);
                    let mut addr: libc::sockaddr_nl = std::mem::zeroed();
                    addr.nl_family = libc::AF_NETLINK as libc::sa_family_t;
                    addr.nl_groups = GROUP_KERNEL | GROUP_UDEV;
                    let ok = libc::bind(
                        fd.as_raw_fd(),
                        &addr as *const libc::sockaddr_nl as *const libc::sockaddr,
                        std::mem::size_of::<libc::sockaddr_nl>() as libc::socklen_t,
                    ) == 0;
                    ok.then_some(fd)
                }
            }

            fn run(fd: OwnedFd, stop: &AtomicBool, on_change: &Callback) {
                let mut buf = vec![0u8; 16 * 1024];
                while !stop.load(Ordering::Relaxed) {
                    let mut pfd = libc::pollfd {
                        fd: fd.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    };
                    // SAFETY: one valid pollfd; the timeout lets `stop` be noticed.
                    let n = unsafe { libc::poll(&mut pfd, 1, 500) };
                    if n < 0 {
                        if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                            continue;
                        }
                        return;
                    }
                    if n == 0 {
                        continue;
                    }
                    // SAFETY: reads into a buffer we own, bounded by its length.
                    let len = unsafe { libc::recv(fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len(), 0) };
                    if len < 0 {
                        // The receive buffer overflowed and events were dropped:
                        // report a change so the caller rescans.
                        if std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOBUFS) {
                            on_change();
                        }
                        continue;
                    }
                    if len == 0 {
                        continue;
                    }
                    if is_relevant(&buf[..len as usize]) {
                        on_change();
                    }
                }
            }

            /// Whether a uevent (kernel `ACTION@devpath\0KEY=VALUE\0…`, or udev's
            /// `libudev\0<header>KEY=VALUE\0…`) is an add/remove in a subsystem badges
            /// show up in.
            pub(super) fn is_relevant(msg: &[u8]) -> bool {
                // udev's header (struct udev_monitor_netlink_header) gives the offset
                // of the properties at byte 16.
                let msg = match msg.strip_prefix(b"libudev\0") {
                    Some(_) if msg.len() >= 20 => {
                        let off = u32::from_ne_bytes([msg[16], msg[17], msg[18], msg[19]]) as usize;
                        msg.get(off..).unwrap_or_default()
                    }
                    Some(_) => return false,
                    None => msg,
                };
                let mut action = None;
                let mut subsystem = None;
                for field in msg.split(|&b| b == 0) {
                    if let Some(v) = field.strip_prefix(b"ACTION=") {
                        action = Some(v);
                    } else if let Some(v) = field.strip_prefix(b"SUBSYSTEM=") {
                        subsystem = Some(v);
                    }
                }
                matches!(action, Some(b"add" | b"remove"))
                    && subsystem.is_some_and(|s| SUBSYSTEMS.contains(&s))
            }
        }
    }
    windows => {
        mod imp {
            use super::Callback;
            use windows_sys::core::GUID;
            use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
                CM_Register_Notification, CM_Unregister_Notification, CM_NOTIFY_ACTION,
                CM_NOTIFY_ACTION_DEVICEINTERFACEARRIVAL, CM_NOTIFY_ACTION_DEVICEINTERFACEREMOVAL,
                CM_NOTIFY_EVENT_DATA, CM_NOTIFY_FILTER, CM_NOTIFY_FILTER_0, CM_NOTIFY_FILTER_0_0,
                CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE, CR_SUCCESS, HCMNOTIFICATION,
            };

            /// GUID_DEVINTERFACE_HID
            const HID: GUID = GUID::from_u128(0x4d1e55b2_f16f_11cf_88cb_001111000030);
            /// GUID_DEVINTERFACE_USB_DEVICE
            const USB_DEVICE: GUID = GUID::from_u128(0xa5dcbf10_6530_11d2_901f_00c04fb951ed);
            /// GUID_DEVINTERFACE_COMPORT
            const COMPORT: GUID = GUID::from_u128(0x86e0d1e0_8089_11d0_9ce4_08003e301f73);

            pub struct Watcher {
                handles: Vec<HCMNOTIFICATION>,
                // Boxed twice so the context pointer stays put; freed after the
                // registrations are gone (unregistering waits for running callbacks).
                callback: *mut Callback,
            }

            // SAFETY: the handles and callback pointer are only used in `drop`, and
            // the callback itself is `Send + Sync`.
            unsafe impl Send for Watcher {}
            unsafe impl Sync for Watcher {}

            impl Drop for Watcher {
                fn drop(&mut self) {
                    // SAFETY: each handle came from a successful registration, and
                    // the callback box is freed only after all of them are removed.
                    unsafe {
                        for &h in &self.handles {
                            CM_Unregister_Notification(h);
                        }
                        drop(Box::from_raw(self.callback));
                    }
                }
            }

            unsafe extern "system" fn notify(
                _: HCMNOTIFICATION,
                context: *const core::ffi::c_void,
                action: CM_NOTIFY_ACTION,
                _: *const CM_NOTIFY_EVENT_DATA,
                _: u32,
            ) -> u32 {
                if action == CM_NOTIFY_ACTION_DEVICEINTERFACEARRIVAL
                    || action == CM_NOTIFY_ACTION_DEVICEINTERFACEREMOVAL
                {
                    // SAFETY: `context` is the `*mut Callback` registered below, which
                    // outlives every registration.
                    let cb = unsafe { &*(context as *const Callback) };
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(cb));
                }
                0 // ERROR_SUCCESS
            }

            pub fn watch(on_change: Callback) -> Option<Watcher> {
                let callback = Box::into_raw(Box::new(on_change));
                let mut watcher = Watcher {
                    handles: Vec::new(),
                    callback,
                };
                for class in [HID, USB_DEVICE, COMPORT] {
                    let filter = CM_NOTIFY_FILTER {
                        cbSize: std::mem::size_of::<CM_NOTIFY_FILTER>() as u32,
                        Flags: 0,
                        FilterType: CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE,
                        Reserved: 0,
                        u: CM_NOTIFY_FILTER_0 {
                            DeviceInterface: CM_NOTIFY_FILTER_0_0 { ClassGuid: class },
                        },
                    };
                    let mut handle: HCMNOTIFICATION = std::ptr::null_mut();
                    // SAFETY: valid filter and out pointer; `callback` lives until drop.
                    let cr = unsafe {
                        CM_Register_Notification(&filter, callback as *const _, Some(notify), &mut handle)
                    };
                    if cr == CR_SUCCESS {
                        watcher.handles.push(handle);
                    }
                }
                // Without HID notifications badges wouldn't be noticed; poll instead.
                if watcher.handles.is_empty() {
                    return None;
                }
                Some(watcher)
            }
        }
    }
    target_os = "macos" => {
        mod imp {
            use super::Callback;
            use core_foundation_sys::base::{CFRelease, kCFAllocatorDefault};
            use core_foundation_sys::dictionary::{CFDictionarySetValue, CFMutableDictionaryRef};
            use core_foundation_sys::number::{CFNumberCreate, kCFNumberSInt32Type};
            use core_foundation_sys::runloop::{
                CFRunLoopAddSource, CFRunLoopGetCurrent, CFRunLoopRef, CFRunLoopRunInMode, CFRunLoopStop,
                kCFRunLoopDefaultMode,
            };
            use core_foundation_sys::string::{CFStringCreateWithCString, kCFStringEncodingUTF8};
            use io_kit_sys::hid::keys::{kIOHIDDeviceKey, kIOHIDProductIDKey, kIOHIDVendorIDKey};
            use io_kit_sys::serial::keys::kIOSerialBSDServiceValue;
            use io_kit_sys::keys::{kIOFirstMatchNotification, kIOTerminatedNotification};
            use io_kit_sys::types::io_iterator_t;
            use io_kit_sys::{
                IOIteratorNext, IONotificationPortCreate, IONotificationPortDestroy,
                IONotificationPortGetRunLoopSource, IOObjectRelease, IOServiceAddMatchingNotification,
                IOServiceMatching,
            };
            use std::ffi::{c_char, c_void};
            use std::sync::atomic::{AtomicBool, Ordering};
            use std::sync::Arc;
            use std::thread::JoinHandle;

            pub struct Watcher {
                stop: Arc<AtomicBool>,
                run_loop: RunLoop,
                thread: Option<JoinHandle<()>>,
            }

            /// The watcher thread's run loop, stopped from whichever thread
            /// drops the watcher (CFRunLoopStop may be called from any thread).
            struct RunLoop(CFRunLoopRef);
            // SAFETY: only used for CFRunLoopStop, which is thread-safe; the
            // thread owning the run loop is joined before the watcher is gone.
            unsafe impl Send for RunLoop {}
            unsafe impl Sync for RunLoop {}

            impl Drop for Watcher {
                fn drop(&mut self) {
                    self.stop.store(true, Ordering::Relaxed);
                    // SAFETY: the run loop belongs to the thread joined below,
                    // which is still running here.
                    unsafe { CFRunLoopStop(self.run_loop.0) };
                    if let Some(t) = self.thread.take() {
                        let _ = t.join();
                    }
                }
            }

            pub fn watch(on_change: Callback) -> Option<Watcher> {
                let stop = Arc::new(AtomicBool::new(false));
                let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
                let thread = std::thread::Builder::new()
                    .name("device-hotplug".into())
                    .spawn({
                        let stop = stop.clone();
                        move || run(on_change, &stop, ready_tx)
                    })
                    .ok()?;
                // `None` when the notifications couldn't be set up; the
                // thread has ended then.
                let Ok(Some(run_loop)) = ready_rx.recv() else {
                    let _ = thread.join();
                    return None;
                };
                Some(Watcher {
                    stop,
                    run_loop,
                    thread: Some(thread),
                })
            }

            fn run(
                on_change: Callback,
                stop: &AtomicBool,
                ready: std::sync::mpsc::SyncSender<Option<RunLoop>>,
            ) {
                // SAFETY: IOKit and Core Foundation calls on objects created
                // here; every notification is registered with this thread's run
                // loop and torn down (port destroyed, iterators released) before
                // `on_change`, which they point to, is dropped.
                unsafe {
                    let port = IONotificationPortCreate(0);
                    if port.is_null() {
                        let _ = ready.send(None);
                        return;
                    }
                    let run_loop = CFRunLoopGetCurrent();
                    CFRunLoopAddSource(run_loop, IONotificationPortGetRunLoopSource(port), kCFRunLoopDefaultMode);
                    let refcon = &on_change as *const Callback as *mut c_void;
                    let mut iterators = Vec::new();
                    // Badges, by their USB ID, and serial ports (for the
                    // older serial badges), each when they appear and when
                    // they go.
                    for kind in [Kind::Badge, Kind::Serial] {
                        for event in [kIOFirstMatchNotification, kIOTerminatedNotification] {
                            let Some(matching) = kind.matching() else { continue };
                            let mut it: io_iterator_t = 0;
                            // Takes over the matching dictionary.
                            let kr = IOServiceAddMatchingNotification(
                                port,
                                event as *mut c_char,
                                matching as _,
                                notify,
                                refcon,
                                &mut it,
                            );
                            if kr == 0 {
                                // Going through what's there already arms
                                // the notification (and isn't a change).
                                drain(it);
                                iterators.push(it);
                            }
                        }
                    }
                    // Without badge notifications they wouldn't be noticed;
                    // poll instead.
                    if iterators.len() < 2 {
                        iterators.into_iter().for_each(|it| _ = IOObjectRelease(it));
                        IONotificationPortDestroy(port);
                        let _ = ready.send(None);
                        return;
                    }
                    let _ = ready.send(Some(RunLoop(run_loop)));
                    // In one-second turns: a CFRunLoopStop that comes just
                    // before the run loop starts is lost, and `stop` is
                    // then noticed at the end of the turn.
                    while !stop.load(Ordering::Relaxed) {
                        CFRunLoopRunInMode(kCFRunLoopDefaultMode, 1.0, 0);
                    }
                    iterators.into_iter().for_each(|it| _ = IOObjectRelease(it));
                    IONotificationPortDestroy(port);
                }
            }

            #[derive(Clone, Copy)]
            enum Kind {
                Badge,
                Serial,
            }

            impl Kind {
                /// A new matching dictionary (each registration takes one).
                unsafe fn matching(self) -> Option<CFMutableDictionaryRef> {
                    // SAFETY: plain Core Foundation calls; created objects are
                    // released once they're in the dictionary, which retains them.
                    unsafe {
                        match self {
                            Kind::Serial => Some(IOServiceMatching(kIOSerialBSDServiceValue)).filter(|d| !d.is_null()),
                            Kind::Badge => {
                                let dict = IOServiceMatching(kIOHIDDeviceKey);
                                if dict.is_null() {
                                    return None;
                                }
                                for (key, value) in [
                                    (kIOHIDVendorIDKey, crate::USB_VENDOR_ID),
                                    (kIOHIDProductIDKey, crate::USB_PRODUCT_ID),
                                ] {
                                    let key = CFStringCreateWithCString(kCFAllocatorDefault, key, kCFStringEncodingUTF8);
                                    let value = i32::from(value);
                                    let number = CFNumberCreate(
                                        kCFAllocatorDefault,
                                        kCFNumberSInt32Type,
                                        &value as *const i32 as *const c_void,
                                    );
                                    CFDictionarySetValue(dict, key as *const c_void, number as *const c_void);
                                    CFRelease(key as *const c_void);
                                    CFRelease(number as *const c_void);
                                }
                                Some(dict)
                            }
                        }
                    }
                }
            }

            /// Releases everything an iterator has (which re-arms its
            /// notification); returns whether there was anything.
            unsafe fn drain(it: io_iterator_t) -> bool {
                let mut any = false;
                // SAFETY: `it` is a live iterator from a registration.
                unsafe {
                    loop {
                        let obj = IOIteratorNext(it);
                        if obj == 0 {
                            return any;
                        }
                        IOObjectRelease(obj);
                        any = true;
                    }
                }
            }

            unsafe extern "C" fn notify(refcon: *mut c_void, it: io_iterator_t) {
                // SAFETY: `refcon` is the callback registered in `run`, which
                // outlives the registrations; `it` is that registration's.
                unsafe {
                    if drain(it) {
                        let cb = &*(refcon as *const Callback);
                        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(cb));
                    }
                }
            }
        }
    }
    _ => {
        mod imp {
            use super::Callback;

            pub struct Watcher;

            pub fn watch(_: Callback) -> Option<Watcher> {
                None
            }
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::imp::is_relevant;

    #[test]
    fn filters_uevents() {
        assert!(is_relevant(
            b"add@/devices/pci/usb1/1-1/1-1:1.0/0003:0416:5020.0001/hidraw/hidraw0\0ACTION=add\0DEVPATH=/x\0SUBSYSTEM=hidraw\0SEQNUM=1\0"
        ));
        // udev: magic, header size, properties offset (40), length, hashes.
        let mut udev = b"libudev\0".to_vec();
        for v in [0xfeedcafeu32, 40, 40, 0, 1, 2, 0xffff_ffff, 0xffff_ffff] {
            udev.extend_from_slice(&v.to_ne_bytes());
        }
        udev.extend_from_slice(b"ACTION=remove\0SUBSYSTEM=tty\0DEVNAME=/dev/ttyUSB0\0");
        assert!(is_relevant(&udev));
        udev[40..47].copy_from_slice(b"ACTION=");
        assert!(!is_relevant(&udev[..30]));
        assert!(!is_relevant(b"change@/x\0ACTION=change\0SUBSYSTEM=hidraw\0"));
        assert!(!is_relevant(b"add@/x\0ACTION=add\0SUBSYSTEM=net\0"));
    }
}

#[cfg(all(test, target_os = "macos"))]
mod macos_tests {
    #[test]
    fn notifications_start_and_stop() {
        // Twice, so stopping is known to leave nothing behind that breaks
        // the next watcher.
        for _ in 0..2 {
            let watcher = super::watch_devices(|| {});
            assert!(watcher.is_some(), "IOKit notifications should be available");
            drop(watcher);
        }
    }
}
