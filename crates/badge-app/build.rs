//! Applies branding.toml: name, copyright etc. for the app, and on Windows
//! the .exe's version info and icon (see packaging/branding/src/exe_metadata.rs).

#[path = "../../packaging/branding/src/config.rs"]
mod config;
#[path = "../../packaging/branding/src/exe_metadata.rs"]
mod exe_metadata;

fn main() {
    exe_metadata::embed(
        "led-badge-studio.exe",
        None,
        &[
            "assets/icons/led-badge-studio.ico",
            // Resource ID 2: the .badge file icon, which the Windows file
            // association points to.
            "assets/icons/led-badge-studio-document.ico",
        ],
    );
}
