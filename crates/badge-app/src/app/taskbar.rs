//! Windows: the taskbar button's progress bar while sending, and the recent
//! files in its Jump List (right-click the button). Only built on Windows.
//!
//! Both are COM interfaces, which windows-sys doesn't wrap: their method
//! tables are declared here, as far as the methods used.

use std::cell::Cell;
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
};
use windows_sys::Win32::UI::Shell::{ApplicationDestinations, SHARD_PATHW, SHAddToRecentDocs, TaskbarList};
use windows_sys::core::{GUID, HRESULT};

use super::*;

/// IID_ITaskbarList3
const ITASKBARLIST3: GUID = GUID::from_u128(0xea1afb91_9e28_4b86_90e9_9e9f8a5eefaf);
/// IID_IApplicationDestinations
const IAPPLICATIONDESTINATIONS: GUID = GUID::from_u128(0x12337d35_94c6_48a0_bce7_6a9c69d4d600);

/// TBPF_NOPROGRESS, TBPF_NORMAL
const NO_PROGRESS: i32 = 0;
const NORMAL: i32 = 2;

type Unused = usize;

/// ITaskbarList3's methods, in order (from ITaskbarList and
/// ITaskbarList2), up to the ones used.
#[repr(C)]
struct TaskbarListMethods {
    _query_interface: Unused,
    _add_ref: Unused,
    _release: Unused,
    hr_init: unsafe extern "system" fn(*mut c_void) -> HRESULT,
    _add_tab: Unused,
    _delete_tab: Unused,
    _activate_tab: Unused,
    _set_active_alt: Unused,
    _mark_fullscreen_window: Unused,
    set_progress_value: unsafe extern "system" fn(*mut c_void, HWND, u64, u64) -> HRESULT,
    set_progress_state: unsafe extern "system" fn(*mut c_void, HWND, i32) -> HRESULT,
}

/// IApplicationDestinations' methods, in order.
#[repr(C)]
struct ApplicationDestinationsMethods {
    _query_interface: Unused,
    _add_ref: Unused,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    _set_app_id: Unused,
    _remove_destination: Unused,
    remove_all_destinations: unsafe extern "system" fn(*mut c_void) -> HRESULT,
}

thread_local! {
    /// The main window, set when it opens.
    static WINDOW: Cell<HWND> = const { Cell::new(std::ptr::null_mut()) };
    /// The taskbar, once asked for; kept for as long as the app runs.
    static TASKBAR: Cell<*mut c_void> = const { Cell::new(std::ptr::null_mut()) };
}

/// Notes the main window, whose taskbar button shows the progress.
pub(super) fn set_window(window: &Window) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    if let Ok(handle) = HasWindowHandle::window_handle(window)
        && let RawWindowHandle::Win32(h) = handle.as_raw()
    {
        WINDOW.set(h.hwnd.get() as HWND);
    }
}

/// Creates a COM object; null if it can't be.
fn create(class: &GUID, interface: &GUID) -> *mut c_void {
    let mut object = std::ptr::null_mut();
    // SAFETY: plain COM calls with valid GUIDs and an out pointer. COM may
    // already be set up on this thread (GPUI does); that's fine.
    unsafe {
        CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
        if CoCreateInstance(
            class,
            std::ptr::null_mut(),
            CLSCTX_INPROC_SERVER,
            interface,
            &mut object,
        ) < 0
        {
            return std::ptr::null_mut();
        }
    }
    object
}

/// Shows `fraction` (0 to 1) on the taskbar button, or no progress with
/// `None`.
pub(super) fn set_progress(fraction: Option<f64>) {
    let window = WINDOW.get();
    if window.is_null() {
        return;
    }
    let mut taskbar = TASKBAR.get();
    if taskbar.is_null() {
        taskbar = create(&TaskbarList, &ITASKBARLIST3);
        if taskbar.is_null() {
            return;
        }
        // SAFETY: an ITaskbarList3 just created; its first field is its
        // method table.
        unsafe {
            let methods = *(taskbar as *const *const TaskbarListMethods);
            ((*methods).hr_init)(taskbar);
        }
        TASKBAR.set(taskbar);
    }
    // SAFETY: as above; `window` is the main window's handle.
    unsafe {
        let methods = *(taskbar as *const *const TaskbarListMethods);
        match fraction {
            Some(f) => {
                ((*methods).set_progress_state)(taskbar, window, NORMAL);
                ((*methods).set_progress_value)(taskbar, window, (f.clamp(0.0, 1.0) * 1000.0) as u64, 1000);
            }
            None => {
                ((*methods).set_progress_state)(taskbar, window, NO_PROGRESS);
            }
        }
    }
}

/// Makes the Jump List's recent files `files` (newest first). Windows fills
/// that list itself from the files the app reports, for file types it's
/// registered to open (installing does that for `.badge` files).
pub(super) fn set_recent_files(files: &[PathBuf]) {
    let destinations = create(&ApplicationDestinations, &IAPPLICATIONDESTINATIONS);
    if !destinations.is_null() {
        // SAFETY: an IApplicationDestinations just created, released after
        // use. This only clears this app's Jump List.
        unsafe {
            let methods = *(destinations as *const *const ApplicationDestinationsMethods);
            ((*methods).remove_all_destinations)(destinations);
            ((*methods).release)(destinations);
        }
    }
    // Oldest first, so the newest ends up first.
    for file in files.iter().rev() {
        let wide: Vec<u16> = file.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        // SAFETY: a nul-terminated path.
        unsafe { SHAddToRecentDocs(SHARD_PATHW as u32, wide.as_ptr().cast()) };
    }
}
