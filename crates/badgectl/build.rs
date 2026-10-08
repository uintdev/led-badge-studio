//! Applies branding.toml: copyright etc. for `badgectl --version`, and on
//! Windows the .exe's version info (see packaging/branding/src/exe_metadata.rs).

#[path = "../../packaging/branding/src/config.rs"]
mod config;
#[path = "../../packaging/branding/src/exe_metadata.rs"]
mod exe_metadata;

fn main() {
    exe_metadata::embed("badgectl.exe", Some("badgectl"), &["assets/icons/led-badge-studio.ico"]);
}
