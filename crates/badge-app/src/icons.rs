//! The interface's icons: single-color SVGs (assets/icons) built into the
//! program and drawn as vectors. Font symbols would differ between systems
//! and can turn into color emoji.

use std::borrow::Cow;

use gpui::prelude::*;
use gpui::{Bounds, Hsla, Pixels, SharedString, TransformationMatrix, canvas, point, px};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    ChevronDown,
    ChevronRight,
    Play,
    Pause,
    Restart,
    Return,
    Pencil,
    Eraser,
    Menu,
    Warning,
    Check,
    Close,
    Maximize,
    Restore,
    Minimize,
    Image,
    Minus,
    Plus,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Save,
    PopOut,
    Undo,
    Redo,
    Info,
    /// The info "i" in a circle, for notes in running text (`Info` is for
    /// round buttons, which draw their own circle).
    InfoCircle,
    Settings,
    Keyboard,
    More,
    Plug,
}

#[cfg(test)]
const ALL: [Icon; 32] = {
    use Icon::*;
    [
        ChevronDown,
        ChevronRight,
        Play,
        Pause,
        Restart,
        Return,
        Pencil,
        Eraser,
        Menu,
        Warning,
        Check,
        Close,
        Maximize,
        Restore,
        Minimize,
        Image,
        Minus,
        Plus,
        ArrowLeft,
        ArrowRight,
        ArrowUp,
        ArrowDown,
        Save,
        PopOut,
        Undo,
        Redo,
        Info,
        InfoCircle,
        Settings,
        Keyboard,
        More,
        Plug,
    ]
};

const FILES: &[(&str, &[u8])] = &[
    ("arrow-down", include_bytes!("../assets/icons/arrow-down.svg")),
    ("arrow-left", include_bytes!("../assets/icons/arrow-left.svg")),
    ("arrow-right", include_bytes!("../assets/icons/arrow-right.svg")),
    ("arrow-up", include_bytes!("../assets/icons/arrow-up.svg")),
    ("check", include_bytes!("../assets/icons/check.svg")),
    ("chevron-down", include_bytes!("../assets/icons/chevron-down.svg")),
    ("chevron-right", include_bytes!("../assets/icons/chevron-right.svg")),
    ("close", include_bytes!("../assets/icons/close.svg")),
    ("eraser", include_bytes!("../assets/icons/eraser.svg")),
    ("image", include_bytes!("../assets/icons/image.svg")),
    ("info", include_bytes!("../assets/icons/info.svg")),
    ("info-circle", include_bytes!("../assets/icons/info-circle.svg")),
    ("maximize", include_bytes!("../assets/icons/maximize.svg")),
    ("menu", include_bytes!("../assets/icons/menu.svg")),
    ("minimize", include_bytes!("../assets/icons/minimize.svg")),
    ("minus", include_bytes!("../assets/icons/minus.svg")),
    ("pause", include_bytes!("../assets/icons/pause.svg")),
    ("pencil", include_bytes!("../assets/icons/pencil.svg")),
    ("play", include_bytes!("../assets/icons/play.svg")),
    ("plus", include_bytes!("../assets/icons/plus.svg")),
    ("pop-out", include_bytes!("../assets/icons/pop-out.svg")),
    ("redo", include_bytes!("../assets/icons/redo.svg")),
    ("restart", include_bytes!("../assets/icons/restart.svg")),
    ("restore", include_bytes!("../assets/icons/restore.svg")),
    ("return", include_bytes!("../assets/icons/return.svg")),
    ("save", include_bytes!("../assets/icons/save.svg")),
    ("undo", include_bytes!("../assets/icons/undo.svg")),
    ("warning", include_bytes!("../assets/icons/warning.svg")),
    ("settings", include_bytes!("../assets/icons/settings.svg")),
    ("keyboard", include_bytes!("../assets/icons/keyboard.svg")),
    ("more", include_bytes!("../assets/icons/more.svg")),
    ("plug", include_bytes!("../assets/icons/plug.svg")),
];

fn name(icon: Icon) -> &'static str {
    match icon {
        Icon::ChevronDown => "chevron-down",
        Icon::ChevronRight => "chevron-right",
        Icon::Play => "play",
        Icon::Pause => "pause",
        Icon::Restart => "restart",
        Icon::Return => "return",
        Icon::Pencil => "pencil",
        Icon::Eraser => "eraser",
        Icon::Menu => "menu",
        Icon::Warning => "warning",
        Icon::Check => "check",
        Icon::Close => "close",
        Icon::Maximize => "maximize",
        Icon::Restore => "restore",
        Icon::Minimize => "minimize",
        Icon::Image => "image",
        Icon::Minus => "minus",
        Icon::Plus => "plus",
        Icon::ArrowLeft => "arrow-left",
        Icon::ArrowRight => "arrow-right",
        Icon::ArrowUp => "arrow-up",
        Icon::ArrowDown => "arrow-down",
        Icon::Save => "save",
        Icon::PopOut => "pop-out",
        Icon::Undo => "undo",
        Icon::Redo => "redo",
        Icon::Info => "info",
        Icon::InfoCircle => "info-circle",
        Icon::Settings => "settings",
        Icon::Keyboard => "keyboard",
        Icon::More => "more",
        Icon::Plug => "plug",
    }
}

fn path_of(name: &str) -> String {
    format!("icons/{name}.svg")
}

fn path(icon: Icon) -> SharedString {
    path_of(name(icon)).into()
}

/// An icon's file, for the app's asset source.
pub fn load(path: &str) -> Option<Cow<'static, [u8]>> {
    let name = path.strip_prefix("icons/")?.strip_suffix(".svg")?;
    FILES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, bytes)| Cow::Borrowed(*bytes))
}

/// The paths of all icon files.
pub fn paths() -> Vec<SharedString> {
    FILES.iter().map(|(n, _)| path_of(n).into()).collect()
}

/// An icon `size` pixels square in `color`.
pub fn icon(icon: Icon, size: f32, color: impl Into<Hsla>) -> gpui::Div {
    let color: Hsla = color.into();
    gpui::div().flex_none().size(px(size)).child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                // Snapped to whole pixels, or symmetric shapes come out uneven.
                let sf = window.scale_factor();
                let snap = |v: Pixels| px((f32::from(v) * sf).round() / sf);
                let bounds = Bounds {
                    origin: point(snap(bounds.origin.x), snap(bounds.origin.y)),
                    size: bounds.size,
                };
                window
                    .paint_svg(bounds, path(icon), None, TransformationMatrix::unit(), color, cx)
                    .ok();
            },
        )
        .size_full(),
    )
}

#[cfg(test)]
mod tests {
    /// The interface's source files.
    const SOURCES: &[(&str, &str)] = &[
        ("app/mod.rs", include_str!("app/mod.rs")),
        ("app/device.rs", include_str!("app/device.rs")),
        ("app/dialogs.rs", include_str!("app/dialogs.rs")),
        ("app/editor.rs", include_str!("app/editor.rs")),
        ("app/files.rs", include_str!("app/files.rs")),
        ("app/history.rs", include_str!("app/history.rs")),
        ("app/menus.rs", include_str!("app/menus.rs")),
        ("app/message.rs", include_str!("app/message.rs")),
        ("app/preview_section.rs", include_str!("app/preview_section.rs")),
        ("app/toolbar.rs", include_str!("app/toolbar.rs")),
        ("chrome.rs", include_str!("chrome.rs")),
        ("widgets.rs", include_str!("widgets.rs")),
        ("text_input.rs", include_str!("text_input.rs")),
        ("led_view.rs", include_str!("led_view.rs")),
        ("main.rs", include_str!("main.rs")),
        ("system_theme.rs", include_str!("system_theme.rs")),
        ("preview_window.rs", include_str!("preview_window.rs")),
    ];

    /// Characters that every font has: punctuation, plus the macOS key
    /// symbols, which only appear on macOS (where the system font has them).
    const ALLOWED: &str = "…·•—“”‘’⌘⇧⌥⌃";

    /// The string literals on a line, with `\u{…}` escapes decoded.
    fn literals(line: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '"' {
                continue;
            }
            let mut s = String::new();
            while let Some(c) = chars.next() {
                match c {
                    '"' => break,
                    '\\' => match chars.next() {
                        Some('u') => {
                            let hex: String = chars.by_ref().skip(1).take_while(|&c| c != '}').collect();
                            if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                                s.push(ch);
                            }
                        }
                        Some(other) => s.push(other),
                        None => {}
                    },
                    c => s.push(c),
                }
            }
            out.push(s);
        }
        out
    }

    /// Icons must be drawn (see this module), not font symbols, which look
    /// different on every system and can turn into color emoji.
    #[test]
    fn interface_text_has_no_symbol_characters() {
        let mut found = Vec::new();
        for (file, text) in SOURCES {
            for (n, line) in text.lines().enumerate() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                for lit in literals(line) {
                    for c in lit.chars().filter(|c| !c.is_ascii() && !ALLOWED.contains(*c)) {
                        found.push(format!("{file}:{}: {c:?} (U+{:04X})", n + 1, c as u32));
                    }
                }
            }
        }
        assert!(
            found.is_empty(),
            "use an icon from icons.rs instead of these symbol characters:\n{}",
            found.join("\n")
        );
    }

    /// Every icon has its SVG file, and each file is an icon drawn in
    /// one color (GPUI uses only its shape and tints it).
    #[test]
    fn every_icon_has_its_svg() {
        for &(name, bytes) in super::FILES {
            let text = std::str::from_utf8(bytes).unwrap();
            // The menu icon is drawn for the 14 px it is shown at (see
            // its file); the others on a 16 x 16 grid.
            let grid = if name == "menu" {
                r#"viewBox="0 0 14 14""#
            } else {
                r#"viewBox="0 0 16 16""#
            };
            assert!(text.contains(grid), "{name}");
            assert!(super::load(&super::path_of(name)).is_some(), "{name}");
        }
        for icon in super::ALL {
            assert!(super::load(&super::path(icon)).is_some(), "{icon:?}");
        }
    }
}
