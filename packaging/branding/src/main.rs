//! Applies `branding.toml` to the app: makes every platform's icons from the
//! logo, draws the macOS disk image's background, and writes the name, description, publisher and copyright into the
//! packaging files. The executables get the same details at compile time
//! (see `crates/badge-app/build.rs`).
//!
//!   cargo run --release --manifest-path packaging/branding/Cargo.toml
//!   … -- --print name      prints one value from branding.toml (for scripts)
//!
//! Files are only rewritten when their contents change.

mod config;
mod dmg;
mod icons;

use std::path::{Path, PathBuf};

use config::Branding;

/// Fixed identity (not part of branding: changing it would orphan users'
/// settings and Flatpak installs).
const APP_ID: &str = "dev.uint.LedBadgeStudio";

fn main() {
    if let Err(e) = run() {
        eprintln!("branding: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = root.canonicalize().unwrap_or(root);
    let branding = Branding::load(&root.join("branding.toml"))?;

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        [] => apply(&root, &branding),
        ["--print", key] => {
            let a = &branding.app;
            let value = match *key {
                "name" => &a.name,
                "description" => &a.description,
                "publisher" => &a.publisher,
                "copyright" => &a.copyright,
                "trademarks" => &a.trademarks,
                "homepage" => &a.homepage,
                other => return Err(format!("unknown key {other:?}")),
            };
            println!("{value}");
            Ok(())
        }
        _ => Err("usage: branding [--print <key>]".into()),
    }
}

fn apply(root: &Path, b: &Branding) -> Result<(), String> {
    let mut changed = Vec::new();
    icons::write_all(root, b, APP_ID, &mut changed)?;
    dmg::write(root, &mut changed)?;
    write_metadata(root, b, &mut changed)?;
    if changed.is_empty() {
        println!("Branding is up to date.");
    } else {
        println!("Applied branding.toml:");
        for p in &changed {
            println!("  {}", p.strip_prefix(root).unwrap_or(p).display());
        }
    }
    Ok(())
}

/// Writes `bytes` to `path` unless it already holds exactly that.
pub fn write_if_changed(path: &Path, bytes: &[u8], changed: &mut Vec<PathBuf>) -> Result<(), String> {
    if std::fs::read(path).is_ok_and(|old| old == bytes) {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("writing {}: {e}", path.display()))?;
    changed.push(path.to_path_buf());
    Ok(())
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The version from the workspace's Cargo.toml.
fn version(root: &Path) -> Result<String, String> {
    let path = root.join("Cargo.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let doc: toml::Table = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    doc.get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| format!("{}: no [workspace.package] version", path.display()))
}

/// Replaces the text between `start` and the next `end` after it.
fn replace_between(text: &str, start: &str, end: &str, value: &str) -> Result<String, String> {
    let a = text.find(start).ok_or_else(|| format!("{start:?} not found"))? + start.len();
    let b = a + text[a..].find(end).ok_or_else(|| format!("{end:?} not found"))?;
    Ok(format!("{}{value}{}", &text[..a], &text[b..]))
}

fn update_file(path: &Path, edits: &[(&str, &str, String)], changed: &mut Vec<PathBuf>) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let text = edit_text(&text, edits).map_err(|e| format!("{}: {e}", path.display()))?;
    write_if_changed(path, text.as_bytes(), changed)
}

/// Makes `edits` to `text`, whose lines may end in `\r\n` (as Git checks
/// text files out on Windows): the edits are written for `\n`, and the
/// file keeps its own line endings.
fn edit_text(text: &str, edits: &[(&str, &str, String)]) -> Result<String, String> {
    let crlf = text.contains("\r\n");
    let mut text = text.replace("\r\n", "\n");
    for (start, end, value) in edits {
        text = replace_between(&text, start, end, value)?;
    }
    Ok(if crlf { text.replace('\n', "\r\n") } else { text })
}

fn write_metadata(root: &Path, b: &Branding, changed: &mut Vec<PathBuf>) -> Result<(), String> {
    let a = &b.app;
    let one_line = |s: &str| s.replace(['\n', '\r'], " ");

    // Linux: menu entry, file type and software-center listing.
    update_file(
        &root.join(format!("packaging/linux/{APP_ID}.desktop")),
        &[
            ("\nName=", "\n", one_line(&a.name)),
            ("\nComment=", "\n", one_line(&a.description)),
        ],
        changed,
    )?;
    update_file(
        &root.join(format!("packaging/linux/{APP_ID}.xml")),
        &[("<comment>", "</comment>", xml(&format!("{} project", a.name)))],
        changed,
    )?;
    update_file(
        &root.join(format!("packaging/flatpak/{APP_ID}.metainfo.xml")),
        &[
            ("<name>", "</name>", xml(&a.name)),
            ("<summary>", "</summary>", xml(&a.description)),
            ("<developer id=\"dev.uint\">\n    <name>", "</name>", xml(&a.publisher)),
            ("<url type=\"homepage\">", "</url>", xml(&a.homepage)),
        ],
        changed,
    )?;

    // macOS: the app bundle's Info.plist. Without a document icon of our own,
    // macOS draws one from the app icon.
    let version = version(root)?;
    let (type_icon, uti_icon) = if icons::macos_draws_document(root, b) {
        ("<key>CFBundleTypeIconSystemGenerated</key><true/>", "")
    } else {
        (
            "<key>CFBundleTypeIconFile</key><string>led-badge-studio-document</string>",
            "\n      <key>UTTypeIconFile</key><string>led-badge-studio-document</string>",
        )
    };
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!-- Generated from branding.toml by packaging/branding; edit that instead. -->
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>{name}</string>
  <key>CFBundleDisplayName</key><string>{name}</string>
  <key>CFBundleIdentifier</key><string>{APP_ID}</string>
  <key>CFBundleExecutable</key><string>led-badge-studio</string>
  <key>CFBundleIconFile</key><string>led-badge-studio</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>{version}</string>
  <key>CFBundleVersion</key><string>{version}</string>
  <key>NSHumanReadableCopyright</key><string>{copyright}</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeName</key><string>{name} project</string>
      <key>CFBundleTypeRole</key><string>Editor</string>
      <key>LSHandlerRank</key><string>Owner</string>
      <key>LSItemContentTypes</key><array><string>dev.uint.led-badge</string></array>
      {type_icon}
    </dict>
  </array>
  <key>UTExportedTypeDeclarations</key>
  <array>
    <dict>
      <key>UTTypeIdentifier</key><string>dev.uint.led-badge</string>
      <key>UTTypeDescription</key><string>{name} project</string>
      <key>UTTypeConformsTo</key><array><string>public.json</string></array>{uti_icon}
      <key>UTTypeTagSpecification</key>
      <dict>
        <key>public.filename-extension</key><array><string>badge</string></array>
        <key>public.mime-type</key><array><string>application/x-led-badge</string></array>
      </dict>
    </dict>
  </array>
</dict>
</plist>
"#,
        name = xml(&a.name),
        copyright = xml(&a.copyright),
    );
    write_if_changed(&root.join("packaging/macos/Info.plist"), plist.as_bytes(), changed)
}

#[cfg(test)]
mod tests {
    use super::edit_text;

    #[test]
    fn files_keep_their_line_endings() {
        let edits = [
            ("<developer>\n  <name>", "</name>", "New".to_string()),
            ("\nName=", "\n", "App".to_string()),
        ];
        let lf = "<developer>\n  <name>Old</name>\nName=Old\n";
        assert_eq!(
            edit_text(lf, &edits).unwrap(),
            "<developer>\n  <name>New</name>\nName=App\n"
        );
        let crlf = lf.replace('\n', "\r\n");
        assert_eq!(
            edit_text(&crlf, &edits).unwrap(),
            "<developer>\r\n  <name>New</name>\r\nName=App\r\n"
        );
    }
}
