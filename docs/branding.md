# Branding: name, copyright and icons

The app's name, publisher details and icon all come from one file,
[`branding.toml`](../branding.toml), and one logo. Change them and build with
the usual command for your system ([Building from source](building.md));
each build script applies the branding before it compiles.

## Name, copyright and other details

To update them:

1. Edit the `[app]` section of `branding.toml` (table below).
2. For a new version, see
   [Releasing a new version](building.md#releasing-a-new-version).
3. Build with your system's build script. It applies the details everywhere
   listed below; nothing else needs editing.

The `[app]` settings:

| Setting | Example | Where it shows up |
|---|---|---|
| `name` | `"LED Badge Studio"` | Title bar, About, app menus, Start menu, the macOS app's name, Windows file properties (*Product name*, and *File description*, which Task Manager shows) |
| `description` | `"Design messages and send them to USB LED name badges"` | Linux menu comment and software-center summary, Windows *Comments* |
| `publisher` | `"Your Name"` | Windows *Company*, software-center developer |
| `copyright` | `"Copyright © 2026 Your Name"` | About, `badgectl --version`, Windows *Copyright*, macOS Finder's Get Info |
| `trademarks` | `""` (none) | Windows *Legal trademarks* |
| `homepage` | `"https://example.com"` | About, `badgectl --version`, software-center listing |

The version comes from `version` in [`Cargo.toml`](../Cargo.toml)
(`[workspace.package]`) and is used everywhere the same way, as is the
license (`license`; also update `<project_license>` in the metainfo file
if you change it). The app ID and file names are not part of
`branding.toml`; see [Package names and IDs](#package-names-and-ids).

What each system gets:

- **Windows**: `led-badge-studio.exe` and `badgectl.exe` carry the details
  in their version information (right-click → *Properties* → *Details*),
  along with the icon. `build.cmd --install` also names the Start menu
  shortcut, install folder and `.badge` file type after `name`.
- **macOS**: the app bundle's `Info.plist` (name, copyright, version) is
  generated as [`packaging/macos/Info.plist`](../packaging/macos/Info.plist),
  and the bundle is called `<name>.app`.
- **Linux**: the menu entry (`packaging/linux/dev.uint.LedBadgeStudio.desktop`),
  the `.badge` file type and the Flatpak's software-center listing
  (`packaging/flatpak/dev.uint.LedBadgeStudio.metainfo.xml`) are updated.
  Linux programs have no built-in metadata section, so that is where it lives.
- **Everywhere**: the app shows the name, copyright and homepage in its title
  bar and About box, and `badgectl --version` prints them. These are built
  in at compile time.

## Icons from one logo

Set `logo` in the `[icon]` section to your logo:

- **An SVG** (best): every icon size is drawn from the vector art, so each
  is as sharp as it can be at its size. Linux also gets it as the scalable
  icon. Text in the SVG uses your installed fonts; for a predictable result,
  convert text to paths first.
- **A PNG** of at least 1024 x 1024 pixels (the largest size macOS uses).
  Smaller sizes are scaled down from it with a high-quality (Lanczos) filter.
  A non-square image is centered on a transparent square. Smaller PNGs work
  but the large icons come out blurry, and the tool warns about it.

From it the tool makes every size each system uses:

| System | File | Sizes |
|---|---|---|
| Windows | `assets/icons/led-badge-studio.ico` (built into both `.exe` files) | 16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 80, 96, 128, 256 |
| macOS | `assets/icons/led-badge-studio.icns` | 16 to 1024, including the Retina (@2x) versions |
| Linux | `assets/icons/hicolor/<N>x<N>/apps/dev.uint.LedBadgeStudio.png` (+ `scalable/` for an SVG logo) | 16, 22, 24, 32, 48, 64, 96, 128, 256, 512 |

Tiny icons often look better drawn by hand on their pixel grid than scaled
down. Put such sizes in the `pixel_sizes` folder (`assets/branding/icon-sizes/`
by default) as `<pixels>.png`, e.g. `16.png` and `32.png`: they are used as
they are, on every system, for exactly that size; the folder is empty now,
so every size comes from the logo. When you replace the logo, delete the
files in that folder (or set `pixel_sizes = ""`), or the old artwork stays at
those sizes.

## The .badge file icon

`.badge` project files have their own icon, shown for them in file managers,
Finder and Explorer. It's set in the `[icon]` section of `branding.toml`:

| Setting | Now | Meaning |
|---|---|---|
| `document` | `""` | The document icon's artwork. Empty: the app logo on a plain page. Otherwise a file, like `logo`: an SVG (best) or a PNG of at least 1024 x 1024 |
| `document_pixel_sizes` | `assets/branding/document-sizes` (empty) | Like `pixel_sizes`: hand-drawn `<pixels>.png` used as they are |

**Without document art** (`document = ""`, as now):

- **macOS** draws the icon itself: the app icon on the system's own document
  page (`Info.plist` asks for that, and no document `.icns` is made).
- **Windows and Linux** can't do that, so the build draws the logo on a plain
  page with a folded corner, the usual shape of a document icon. On Linux
  it comes in PNG sizes only, without a `scalable/` version.

**To use your own artwork:**

1. Save it as `assets/branding/document.svg` (or a PNG of at least
   1024 x 1024, any name).
2. Set `document = "assets/branding/document.svg"` in `branding.toml`.
3. Optionally, put hand-drawn small sizes in `assets/branding/document-sizes/`
   as `<pixels>.png`.
4. Build, or apply the branding on its own (see
   [below](#applying-the-branding-without-building)).

To change it on one system only, see the next section.

Either way, the tool makes every size each system uses (the same sizes as
the app icon):

| System | File | Where it's used |
|---|---|---|
| Windows | `assets/icons/led-badge-studio-document.ico` | Built into `led-badge-studio.exe` as its second icon; `build.cmd --install` points the `.badge` file type to it |
| macOS | `assets/icons/led-badge-studio-document.icns` (only with document art, or a `document.icns` override) | Put in the app bundle; `Info.plist` names it as the icon of the `.badge` document type |
| Linux | `assets/icons/hicolor/<N>x<N>/mimetypes/dev.uint.LedBadgeStudio.Project.png` (+ `scalable/` for SVG artwork) | Installed with the app; the `application/x-led-badge` file type names it. It's named after the app ID because a Flatpak may only add icons with names that start with it. |

The icon appears once the app is installed (on Linux, file managers may need
a moment, or a log-out, to pick up a changed icon).

## Icons for one system only

What the artwork produces is the fallback. To use different icon files on
one system, place them in that system's folder; anything you don't provide
still comes from the artwork. This works the same for the app icon and the
`.badge` file icon:

| Folder | App icon | `.badge` file icon |
|---|---|---|
| `assets/branding/windows/` | One `.ico` file (any name but `document.ico`): replaces the whole Windows icon | `document.ico` |
| `assets/branding/macos/` | One `.icns` file (any name but `document.icns`): replaces the classic macOS icon | `document.icns` |
| `assets/branding/macos/` | An Icon Composer `.icon` file, or an Xcode asset catalog (`.xcassets` with an app icon set): the icon on macOS 26 and later, including Liquid Glass (see below) | (macOS draws document icons itself from the `.icns`) |
| `assets/branding/linux/` | `<N>.png` for any of the Linux sizes above, and/or `scalable.svg`: replaces those sizes | The same names in `assets/branding/linux/document/` |

For example, to give `.badge` files a hand-made Windows icon and a
different 16 px Linux icon, add `assets/branding/windows/document.ico` and
`assets/branding/linux/document/16.png`, then build.

Each folder has a short README with these details. Files there are checked
(an `.ico` must really be one, a `48.png` must be 48 x 48) and the tool
says which overrides it used.

## macOS: Icon Composer and Liquid Glass

macOS 26 draws app icons with Liquid Glass (layers, translucency, light and
dark and tinted versions), which needs an icon made in Apple's **Icon
Composer** (part of Xcode 26) or an app icon set in an Xcode asset catalog.
To use one:

1. Design the icon in Icon Composer and save it as, for example,
   `AppIcon.icon`, or take the `Assets.xcassets` folder from an Xcode
   project.
2. Copy it into `assets/branding/macos/` as it is. Nothing needs converting,
   so you can keep editing it in Icon Composer or Xcode and copy it over
   again.
3. Build with `./build.sh` on a Mac with **Xcode**
   installed (not only the Command Line Tools), selected with
   `sudo xcode-select -s /Applications/Xcode.app`.

The build compiles it with Xcode's `actool` into the app (`Assets.car`, named
after the `.icon` file or the app icon set) and points `Info.plist` at it.
Older macOS versions keep showing the `.icns` icon (yours from
`assets/branding/macos/`, or the one made from the logo). If Xcode isn't
installed, or `actool` fails, the build says so and uses the `.icns` icon.
With both an `.icon` and an `.xcassets`, the `.icon` is used.

## macOS: the disk image's background

The macOS download is a disk image whose window shows the app, a link to
Applications and, behind them, `assets/branding/dmg-background.svg`: an arrow
from the app to Applications, with no text. To change it, edit that SVG (or
replace it) and keep it 640 x 400, the window's size; the app's icon is
centered at (160, 155) and the Applications link at (480, 155)
(`packaging/macos/dmg-settings.py`). Applying the branding draws it into
`assets/icons/dmg-background.png` and `dmg-background@2x.png` (for Retina
screens), which the build combines.

Finder writes the icons' names over the background, in black in light mode
and white in dark mode, so keep it a mid tone that both are readable on (the
default grey gives about 4.5:1 for both).

## Package names and IDs

These identify the app to each system rather than describe it. They aren't in
`branding.toml`, because changing them makes the app a different one: existing
installs keep the old one alongside, settings aren't carried over and file
associations must be set up again. If you publish your own build under your
own name, change them once, before your first release, and keep them after.

| Identifier | Current value | Used for |
|---|---|---|
| App ID (reverse domain name you control) | `dev.uint.LedBadgeStudio` | Flatpak ID, Linux desktop entry, icon and window matching, macOS bundle identifier |
| Program file names | `led-badge-studio`, `badgectl` | The executables (`.exe` on Windows) |
| Per-user folder | `led-badge-studio` | Settings, last-sent record, unsaved-work copy, earlier versions (Linux, Windows) and GPU cache (`~/.config/…`, `%APPDATA%\…`, `~/Library/Application Support/…`) |
| macOS file type ID | `dev.uint.led-badge` | The `.badge` document type on macOS |
| Windows file type ID | `LedBadgeStudio.Project` | The `.badge` file association on Windows |
| File type (MIME) | `application/x-led-badge` | The `.badge` file type on Linux, macOS and Windows |
| Download names | `LED_Badge_Studio…` | `dist/` files from the build scripts |

Where each one lives:

- **App ID**: `APP_ID` in `crates/badge-app/src/main.rs` and in
  `packaging/branding/src/main.rs`; `app-id` and the file names in
  `packaging/flatpak/dev.uint.LedBadgeStudio.yml`; `app_id` in
  `packaging/flatpak/build.sh` and `packaging/linux/install.sh`; `<id>` and
  `<launchable>` in the metainfo file, and its `<developer id>` (your
  reverse domain); `Icon=` and `StartupWMClass=` in the desktop entry. Then
  rename the three files named after it: `packaging/linux/<id>.desktop`,
  `packaging/linux/<id>.xml` and `packaging/flatpak/<id>.metainfo.xml` (and
  the manifest `packaging/flatpak/<id>.yml`), and run the branding tool,
  which writes the icons under the new name. Delete the old
  `assets/icons/hicolor/*/apps/dev.uint.LedBadgeStudio.*` files.
- **Program file names**: `name` under `[[bin]]` in
  `crates/badge-app/Cargo.toml` (the app) and the package name in
  `crates/badgectl/Cargo.toml` (`badgectl`); the file names passed to
  `exe_metadata::embed` in each crate's `build.rs`; `CFBundleExecutable` in
  `packaging/branding/src/main.rs`; `Exec=` in the desktop entry and
  `command:` in the Flatpak manifest; and the copy/install lines in
  `packaging/linux/install.sh`, the Flatpak manifest,
  `packaging/macos/build.sh` and `packaging/windows/build.ps1`. The icon
  files `led-badge-studio.ico` and `.icns` can keep their names; to rename
  them too, change them in `packaging/branding/src/icons.rs`, both `build.rs`
  files and `packaging/macos/build.sh` (and `CFBundleIconFile` in
  `packaging/branding/src/main.rs`).
- **Per-user folder**: `APP_DIR` in `crates/badge-formats/src/dirs.rs`. To
  carry users' settings over, set `LEGACY_APP_DIR` there to the old name;
  files are moved over on first use.
- **macOS file type ID**: `dev.uint.led-badge` (twice) in
  `packaging/branding/src/main.rs`.
- **Windows file type ID**: `$progId` in `packaging/windows/build.ps1`.
- **MIME type**: `packaging/linux/<id>.xml`, `MimeType=` in the desktop entry,
  and `application/x-led-badge` in `packaging/branding/src/main.rs` and
  `packaging/windows/build.ps1`.
- **Download names**: `out=` in `packaging/flatpak/build.sh`, `stage=` in
  `packaging/macos/build.sh`, and `$name` in `packaging/windows/build.ps1`.

To find every remaining use of an old identifier afterwards:

```bash
grep -rnI --exclude-dir=target --exclude-dir=dist -e dev.uint.LedBadgeStudio -e led-badge-studio -e LED_Badge_Studio .
```

Then build and run `cargo test --workspace`. The app's displayed name is
separate: that is `name` in `branding.toml`.

## Applying the branding without building

The build scripts do this for you. To apply it on its own, for example to
look at the icons first:

```bash
cargo run --release --manifest-path packaging/branding/Cargo.toml
```

It rewrites only files whose contents change, and lists them. A plain
`cargo build` doesn't run it: the name and copyright in the programs are
always current, but the icons are only what was last applied, so run it
after changing the logo or the override folders. The tool lives in
[`packaging/branding/`](../packaging/branding/); it is a separate Cargo project,
so its libraries aren't counted as part of the app (or in its third-party
notices).

On Linux the icon shows in the dock and app menu once the app is installed
(with `install.sh` or as a Flatpak), which adds the desktop entry
`dev.uint.LedBadgeStudio.desktop` that the window is matched to.
