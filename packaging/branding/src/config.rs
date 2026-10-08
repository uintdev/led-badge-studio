//! `branding.toml`: shared by the branding tool and the apps' build scripts
//! (which include this file with `#[path]`).

use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)] // the build scripts only read `app`
pub struct Branding {
    pub app: App,
    pub icon: Icon,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct App {
    pub name: String,
    pub description: String,
    pub publisher: String,
    pub copyright: String,
    #[serde(default)]
    pub trademarks: String,
    pub homepage: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)] // the build scripts only read `app`
pub struct Icon {
    /// SVG or PNG, relative to the project folder.
    pub logo: String,
    /// Folder of `<pixels>.png` files used as-is; empty for none.
    #[serde(default)]
    pub pixel_sizes: String,
    /// The `.badge` file icon: SVG or PNG, like `logo`. Empty: the logo on a
    /// plain page.
    #[serde(default)]
    pub document: String,
    /// Hand-made sizes of the document icon; empty for none.
    #[serde(default)]
    pub document_pixel_sizes: String,
}

impl Branding {
    pub fn load(path: &Path) -> Result<Branding, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
        let b: Branding = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        if b.app.name.trim().is_empty() {
            return Err(format!("{}: app.name can't be empty", path.display()));
        }
        Ok(b)
    }
}
