# Development notes

## Project layout

| Crate | Purpose |
|---|---|
| `badge-core` | Data model, framebuffer and byte-exact upload encoders (USB and serial) |
| `badge-io` | Device discovery, USB HID / serial upload and pacing, Linux access setup |
| `badge-render` | Built-in pixel fonts, system fonts, icons, image import/export, animation preview |
| `badge-formats` | `.badge` project files, the last-sent record and per-user folders |
| `badgectl` | Command-line tool |
| `badge-app` | The GPUI application |

`vendor/` holds patched copies of dependencies, used in place of the
published versions until the fix is released upstream; its README says what
each patch fixes and how to update it.

The main window is `crates/badge-app/src/app/`: `mod.rs` holds its state,
layout and key bindings, and each part has a file of its own: `toolbar.rs`,
`message.rs` (the message list, text, font, symbols and effect),
`editor.rs` (the Pixels section and image import), `preview_section.rs`
(the preview, Pop Out and Export), `menus.rs`, `dialogs.rs`, `files.rs`
(new, open, save, repair), `device.rs` (finding badges, badge access,
sending), `history.rs` (undo and redo), `recovery.rs` (the copy kept of
unsaved work) and `versions.rs` (File → Revert To). On macOS only:
`menu_bar.rs` (the main window's side of the macOS menu bar, whose menus
are made in `app_menu` in `main.rs`), `native_document.rs` (the window as a
Mac document: its title, "Edited", and answering macOS when it asks to
quit) and `dock_progress.rs` (the send's progress on the Dock icon). On
Windows only: `taskbar.rs` (the same on the taskbar button).

`window_tests.rs` there tests the window's behavior in GPUI's test mode,
without a real window, badge or settings file: the modified indicator,
undo and redo, saving and opening, the repair and discard dialogs, missing
fonts, typing over a drawing, image import, quitting during a send, the
preview playlist and menu placement. Run
everything with `cargo test --workspace`, and format with `cargo fmt --all`
(settings in `rustfmt.toml`); [CONTRIBUTING.md](../CONTRIBUTING.md) has the
checks a pull request needs.

The interface's icons (dropdown arrows, Save, Undo/Redo, play/pause, the
title-bar buttons and so on) are SVG files in `crates/badge-app/assets/icons`,
drawn on a 16 x 16 grid in a single color (the menu icon on 14 x 14, the
size it is shown at, so its three bars fall exactly on whole pixels). They
are built into the program and drawn by GPUI as vectors, on whole pixels, in
the color of the text around them, so they
look the same on every system and in every build, at any size and display
scale. To change an icon, edit its SVG (keep `viewBox="0 0 16 16"`; colors
are ignored, only the shape counts). To add one, add the file, list it in
`crates/badge-app/src/icons.rs` and give it an `Icon` name there. Font
symbols (▶, ✓, emoji…) would come from whatever fonts each system has, so a
test (`interface_text_has_no_symbol_characters`) fails if one is used in the
interface; another (`every_icon_has_its_svg`) checks that every icon has its
file.

How LEDs look (shape, colors, the hot core, glow, brightness) is described
once, in `crates/badge-render/src/led_look.rs`, and both the app's preview
(drawn on the GPU) and the exported images (drawn by `preview_export.rs`)
draw from it, so an export always shows what the preview shows. To add an
effect, add a field to `LedLook` with its math there and set it in the app's
`led_look()`: the build fails until the preview (`led_view.rs`) and the
export both handle it, and the test `every_part_of_the_look_shows_in_the_export`
checks the export draws it.

The code uses the Rust 2024 edition and needs Rust 1.96 or newer. Every
build script updates stable Rust with rustup first, so native builds and the
Flatpak all build with the latest Rust, the same version.

`build.sh` and `build.cmd` in the project folder are the entry points; they
run the build for your system from `packaging/`:

| Folder | Contents |
|---|---|
| `linux/` | `install.sh`, `install-build-deps.sh`, the udev rule and desktop entry |
| `flatpak/` | Flatpak manifest, AppStream metadata and `build.sh` |
| `windows/` | `build.ps1` |
| `macos/` | `build.sh` (makes the `.app` and the `.dmg`), `dmg-settings.py` (the disk image's layout, for dmgbuild) and the generated `Info.plist` |
| `branding/` | The branding tool: applies `branding.toml` to the icons and packaging files |
| `fonts/`, `licenses/` | Generators for the font tables and the third-party notices |

Branding sources live in `assets/branding/` (the logo and the `.badge`
file icon artwork, their hand-drawn small sizes, and per-system icon
overrides); the icons made from them are in `assets/icons/`.

## How the protocol was worked out

The upload format comes from decompiling the vendor's .NET application
(`BmpBadge.exe`). Pacing, the trailing padding packet and the end-of-upload
hold come from testing on real hardware: the badge acknowledges every USB
packet immediately but stores them slowly, and silently loses data that
arrives too fast. [The badge protocol](protocol.md) has the details: the
byte layout, the pacing and what's safe on each system.

The vendor app also does something unexplained: on every start it connects to
port 1234 on the same computer (`127.0.0.1`) and sends its version, the date
it was first run and the computer's MAC address. If a program there answers,
its reply sets one byte in the header of every upload. Nothing normally
listens on that port, so the byte stays 0. What it was for is unknown, perhaps
a separate vendor tool. LED Badge Studio leaves the connection out. The byte
is still part of every upload, since the header has a fixed size, but it
stays 0: the same as the vendor app sends on a normal computer, and the badge
works with it.
