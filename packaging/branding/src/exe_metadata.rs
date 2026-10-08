//! Branding for the executables, applied at compile time. Included by the
//! apps' build scripts (`crates/*/build.rs`) together with `config.rs`.
//!
//! - Every platform: `BRAND_NAME`, `BRAND_DESCRIPTION`, `BRAND_PUBLISHER`,
//!   `BRAND_COPYRIGHT` and `BRAND_HOMEPAGE` are set for `env!()`, so the app
//!   can show them (title bar, About, `badgectl --version`).
//! - Windows: the .exe gets a version resource (Properties > Details:
//!   product name, description, company, copyright, trademarks, version) and
//!   optionally the icon.

use std::path::PathBuf;

use crate::config::Branding;

/// `file_description` is what Windows shows as the program's name (Task
/// Manager, "Open with"; `None` = the app's name). `icons` are .ico files
/// relative to the project folder, embedded with resource IDs 1, 2, … (1 is
/// the program's own icon).
pub fn embed(original_filename: &str, file_description: Option<&str>, icons: &[&str]) {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let path = root.join("branding.toml");
    println!("cargo:rerun-if-changed={}", path.display());
    let b = Branding::load(&path).unwrap_or_else(|e| panic!("{e}"));
    let a = &b.app;
    for (key, value) in [
        ("BRAND_NAME", &a.name),
        ("BRAND_DESCRIPTION", &a.description),
        ("BRAND_PUBLISHER", &a.publisher),
        ("BRAND_COPYRIGHT", &a.copyright),
        ("BRAND_HOMEPAGE", &a.homepage),
    ] {
        println!("cargo:rustc-env={key}={}", value.replace(['\n', '\r'], " "));
    }

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    for (i, icon) in icons.iter().enumerate() {
        let icon = root.join(icon);
        println!("cargo:rerun-if-changed={}", icon.display());
        res.set_icon_with_id(&icon.to_string_lossy(), &(i + 1).to_string());
    }
    let internal = original_filename.trim_end_matches(".exe");
    res.set("ProductName", &a.name)
        .set("FileDescription", file_description.unwrap_or(&a.name))
        .set("Comments", &a.description)
        .set("CompanyName", &a.publisher)
        .set("LegalCopyright", &a.copyright)
        .set("OriginalFilename", original_filename)
        .set("InternalName", internal);
    if !a.trademarks.trim().is_empty() {
        res.set("LegalTrademarks", &a.trademarks);
    }
    if let Err(e) = res.compile() {
        // Don't fail the build over a missing resource compiler.
        println!("cargo:warning=could not embed Windows version info and icon: {e}");
    }
}
