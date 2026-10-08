//! Picks a working Vulkan driver on Linux.
//!
//! Hardware rendering is preferred. Some drivers (notably the virtio-gpu
//! "Venus" driver used by many VMs) abort the whole process while GPUI builds
//! its render pipelines, so before starting the UI we probe the default
//! driver in a child process: it opens a hidden window, which makes GPUI build
//! the same pipelines, then exits. If the child crashes or hangs we switch to
//! Mesa's software renderer (lavapipe). The verdict is cached per driver
//! configuration so the probe normally runs only once.
//!
//! Overrides: `LED_BADGE_STUDIO_GPU=hardware` (no probe, never fall back),
//! `LED_BADGE_STUDIO_GPU=software` (always lavapipe), or set `VK_DRIVER_FILES`
//! yourself.

/// Environment variable that turns the process into a probe.
const PROBE_ENV: &str = "LED_BADGE_STUDIO_GPU_PROBE";

/// Runs the probe if this process is one. Never returns in that case.
pub fn run_probe_if_requested() {
    if std::env::var_os(PROBE_ENV).is_none() {
        return;
    }
    use gpui::{App, AppContext, Bounds, WindowBounds, WindowOptions, px, size};
    struct Empty;
    impl gpui::Render for Empty {
        fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
            gpui::div()
        }
    }
    gpui_platform::application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(64.), px(64.)), cx);
        let ok = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    show: false,
                    focus: false,
                    ..Default::default()
                },
                |_, cx| cx.new(|_| Empty),
            )
            .is_ok();
        std::process::exit(if ok { 0 } else { 3 });
    });
    std::process::exit(4);
}

/// Whether rendering uses Mesa's software renderer (lavapipe), set by
/// [`configure`]. The UI then redraws animations less often to save CPU.
static SOFTWARE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn is_software() -> bool {
    SOFTWARE.load(std::sync::atomic::Ordering::Relaxed)
}

/// Notes what the window really renders with (GPUI says on Linux): the
/// probe only checks that the default driver starts, and wgpu can still end
/// up on Mesa's software renderer itself, as in VMs whose GPU it can't use.
pub fn note_renderer(window: &gpui::Window) {
    let Some(specs) = window.gpu_specs() else {
        return;
    };
    log::info!(
        "Rendering with {} ({}{})",
        specs.device_name,
        specs.driver_name,
        if specs.is_software_emulated { ", on the CPU" } else { "" }
    );
    if specs.is_software_emulated {
        SOFTWARE.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(target_os = "linux")]
fn set_software(software: bool) {
    SOFTWARE.store(software, std::sync::atomic::Ordering::Relaxed);
}

/// Must run at the start of `main`, before any other thread exists.
pub fn configure() {
    #[cfg(target_os = "linux")]
    linux::configure();
}

#[cfg(target_os = "linux")]
mod linux {
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    pub fn configure() {
        let chosen = std::env::var_os("VK_DRIVER_FILES").or_else(|| std::env::var_os("VK_ICD_FILENAMES"));
        if let Some(files) = chosen {
            // The user picked the driver; lavapipe's manifest is lvp_icd*.json.
            super::set_software(files.to_string_lossy().contains("lvp_icd"));
            return;
        }
        skip_unusable_amd_driver();
        let software = match std::env::var("LED_BADGE_STUDIO_GPU").unwrap_or_default().as_str() {
            "hardware" => false,
            "software" => true,
            _ => !hardware_works(),
        };
        if !software {
            log::info!("The default GPU driver passed the probe");
            return;
        }
        match lavapipe_icd() {
            Some(icd) => {
                log::warn!(
                    "Hardware Vulkan driver unusable; using software renderer ({})",
                    icd.display()
                );
                // SAFETY: called at the start of main, before any other thread exists.
                unsafe { std::env::set_var("VK_DRIVER_FILES", &icd) };
                super::set_software(true);
            }
            None => log::warn!(
                "Hardware Vulkan driver failed the probe and Mesa lavapipe is not installed \
                 (package mesa-vulkan-drivers); trying hardware anyway"
            ),
        }
    }

    /// Tells the Vulkan loader to skip Mesa's AMD driver (radv) where it
    /// can't work: no AMD GPU, and a virtual GPU (virtio-gpu, as in VMs)
    /// without the "native contexts" radv could use it through. radv still
    /// checks such a GPU, printing three errors each time Vulkan starts
    /// (several times per launch), then leaves itself out.
    fn skip_unusable_amd_driver() {
        if std::env::var_os("VK_LOADER_DRIVERS_DISABLE").is_some()
            || std::env::var_os("VK_LOADER_DRIVERS_SELECT").is_some()
        {
            return;
        }
        let Ok(entries) = std::fs::read_dir("/sys/class/drm") else {
            return;
        };
        let mut virtio_without_contexts = false;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            // GPUs, not their outputs (card0-HDMI-A-1 and the like).
            if !name.starts_with("renderD") {
                continue;
            }
            let vendor = std::fs::read_to_string(entry.path().join("device/vendor")).unwrap_or_default();
            match vendor.trim() {
                // AMD: radv is the driver to use.
                "0x1002" => return,
                // virtio-gpu.
                "0x1af4" => {
                    if virtio_native_contexts(&Path::new("/dev/dri").join(&name)) {
                        return;
                    }
                    virtio_without_contexts = true;
                }
                _ => {}
            }
        }
        if virtio_without_contexts {
            log::info!("Skipping the AMD Vulkan driver: no AMD GPU, and the virtual GPU has no native contexts");
            // SAFETY: called at the start of main, before any other thread exists.
            unsafe { std::env::set_var("VK_LOADER_DRIVERS_DISABLE", "*radeon*") };
        }
    }

    /// Whether a virtio-gpu render node offers contexts beyond OpenGL
    /// (`VIRTGPU_PARAM_CONTEXT_INIT`), which native contexts need. Unknown
    /// counts as yes, so radv stays where it might work.
    fn virtio_native_contexts(node: &Path) -> bool {
        use std::os::fd::AsRawFd;
        #[repr(C)]
        struct GetParam {
            param: u64,
            value: u64,
        }
        const VIRTGPU_PARAM_CONTEXT_INIT: u64 = 6;
        // DRM_IOWR(DRM_COMMAND_BASE + DRM_VIRTGPU_GETPARAM, struct drm_virtgpu_getparam)
        const DRM_IOCTL_VIRTGPU_GETPARAM: libc::c_ulong = 0xc010_6443;
        let Ok(file) = std::fs::File::open(node) else {
            return true;
        };
        let mut value: u64 = 0;
        let mut request = GetParam {
            param: VIRTGPU_PARAM_CONTEXT_INIT,
            value: &raw mut value as u64,
        };
        // SAFETY: a valid open file, and a request whose `value` points to
        // memory big enough for the reply (the kernel writes an int).
        let result = unsafe { libc::ioctl(file.as_raw_fd(), DRM_IOCTL_VIRTGPU_GETPARAM as _, &raw mut request) };
        match result {
            0 => value != 0,
            // Not supported by this kernel or device: no such contexts.
            _ if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINVAL) => false,
            _ => true,
        }
    }

    /// Cached result of probing the default driver.
    fn hardware_works() -> bool {
        let key = config_key();
        let cache = cache_path();
        if let Some(cache) = &cache
            && let Ok(s) = std::fs::read_to_string(cache)
            && let Some(verdict) = s.strip_prefix(&format!("{key}\n"))
        {
            return verdict.trim() == "hardware";
        }
        let ok = probe(None);
        if !ok {
            // Only trust the failure if software rendering works here;
            // otherwise the problem is environmental (no display, …) and
            // nothing is cached.
            match lavapipe_icd() {
                Some(icd) if probe(Some(&icd)) => {}
                _ => return true,
            }
        }
        if let Some(cache) = &cache {
            if let Some(dir) = cache.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(cache, format!("{key}\n{}\n", if ok { "hardware" } else { "software" }));
        }
        ok
    }

    /// Runs the probe child, optionally forcing a Vulkan ICD.
    fn probe(icd: Option<&Path>) -> bool {
        let Ok(exe) = std::env::current_exe() else {
            return true;
        };
        let mut cmd = Command::new(exe);
        if let Some(icd) = icd {
            cmd.env("VK_DRIVER_FILES", icd);
        }
        let Ok(mut child) = cmd
            .env(super::PROBE_ENV, "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            return true;
        };
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    log::info!(
                        "GPU probe ({}) finished: {status}",
                        icd.map_or_else(|| "default driver".into(), |p| p.display().to_string())
                    );
                    return status.success();
                }
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    log::warn!("GPU probe timed out");
                    return false;
                }
            }
        }
    }

    /// Identifies the driver setup: installed ICDs, kernel and GPU devices.
    fn config_key() -> String {
        let mut parts = Vec::new();
        // Every folder the driver search uses, including a Flatpak's GL
        // extensions. Files there all have the same mtime (ostree), so each
        // driver is also identified by its size and its library's size.
        for dir in icd_dirs() {
            if let Ok(rd) = std::fs::read_dir(&dir) {
                let mut names: Vec<String> = rd
                    .flatten()
                    .map(|e| {
                        let meta = e.metadata().ok();
                        let mtime = meta
                            .as_ref()
                            .and_then(|m| m.modified().ok())
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map_or(0, |d| d.as_secs());
                        let size = meta.map_or(0, |m| m.len());
                        let lib = icd_library_size(&e.path());
                        format!("{}@{mtime}:{size}:{lib}", e.file_name().to_string_lossy())
                    })
                    .collect();
                names.sort();
                parts.push(format!("{}={}", dir.display(), names.join(",")));
            }
        }
        if let Ok(rd) = std::fs::read_dir("/sys/class/drm") {
            let mut ids: Vec<String> = rd
                .flatten()
                .filter_map(|e| {
                    let v = std::fs::read_to_string(e.path().join("device/vendor")).ok()?;
                    let d = std::fs::read_to_string(e.path().join("device/device")).unwrap_or_default();
                    Some(format!("{}:{}", v.trim(), d.trim()))
                })
                .collect();
            ids.sort();
            ids.dedup();
            parts.push(ids.join(","));
        }
        parts.push(
            std::fs::read_to_string("/proc/sys/kernel/osrelease")
                .unwrap_or_default()
                .trim()
                .to_string(),
        );
        parts.push(
            std::env::var("WAYLAND_DISPLAY")
                .map_or("x11", |_| "wayland")
                .to_string(),
        );
        parts.push(env!("CARGO_PKG_VERSION").to_string());
        parts.join("|").replace('\n', " ")
    }

    /// Size of the driver library an ICD manifest points to, when it is
    /// given as a path (0 for a bare name the loader searches for).
    fn icd_library_size(manifest: &Path) -> u64 {
        let Ok(text) = std::fs::read_to_string(manifest) else {
            return 0;
        };
        let lib = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v["ICD"]["library_path"].as_str().map(PathBuf::from));
        match lib {
            Some(lib) if lib.components().count() > 1 => {
                let lib = manifest.parent().map_or(lib.clone(), |d| d.join(&lib));
                std::fs::metadata(lib).map_or(0, |m| m.len())
            }
            _ => 0,
        }
    }

    fn cache_path() -> Option<PathBuf> {
        use badge_formats::dirs;
        Some(dirs::cache_base()?.join(dirs::APP_DIR).join("gpu-probe"))
    }

    const ICD_DIRS: &[&str] = &[
        "/usr/share/vulkan/icd.d",
        "/etc/vulkan/icd.d",
        "/usr/local/share/vulkan/icd.d",
    ];

    /// Where Vulkan driver manifests may be: the usual system folders, every
    /// `$XDG_DATA_DIRS/vulkan/icd.d`, and inside a Flatpak the runtime's GL
    /// extensions (`/usr/lib/<arch>/GL/<name>/share/vulkan/icd.d`).
    fn icd_dirs() -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = ICD_DIRS.iter().map(PathBuf::from).collect();
        if let Some(data) = std::env::var_os("XDG_DATA_DIRS") {
            dirs.extend(std::env::split_paths(&data).map(|d| d.join("vulkan/icd.d")));
        }
        let gl_roots = std::fs::read_dir("/usr/lib")
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path().join("GL"))
            .filter(|p| p.is_dir());
        for gl in gl_roots {
            for ext in std::fs::read_dir(&gl).into_iter().flatten().flatten() {
                dirs.push(ext.path().join("share/vulkan/icd.d"));
            }
        }
        dirs
    }

    fn lavapipe_icd() -> Option<PathBuf> {
        icd_dirs()
            .iter()
            .filter_map(|d| std::fs::read_dir(d).ok())
            .flat_map(std::iter::Iterator::flatten)
            .map(|e| e.path())
            .find(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("lvp_icd") && n.ends_with(".json"))
            })
    }
}
