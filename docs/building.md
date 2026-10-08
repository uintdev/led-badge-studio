# Building from source

1. **Get the source**: `git clone` the repository, or use GitHub's *Code →
   Download ZIP* and unzip it.
2. **Build and install**, from the project folder:

   **Linux and macOS** (in a terminal):

   ```bash
   ./build.sh --install
   ```

   **Windows**: open a terminal in the project folder and run
   `build.cmd --install`, or double-click `build.cmd` to only build.

That's all. The script works out which system it's running on, installs
whatever the build needs that is missing (compiler, libraries, Rust; it asks
for your password or permission where needed), builds, and installs:

| System | What gets built | `--install` puts it |
|---|---|---|
| Linux (any distribution, including SteamOS, Silverblue, Bazzite) | A Flatpak: `dist/LED_Badge_Studio-linux-<arch>.flatpak` | In your app menu, installed for your user |
| macOS | An app and disk image: `dist/LED_Badge_Studio-macos-arm64.dmg` (with `--zip`, also a `.zip`) | In `~/Applications` |
| Windows | The programs and a zip: `dist\LED_Badge_Studio-windows-<arch>.zip` | In `%LOCALAPPDATA%\Programs`, with a Start menu shortcut, and `.badge` files open with it |

`<arch>` is the processor it's built for: on Linux `x86_64` or `arm64`, on
Windows `x64` or `arm64`. Builds are for the processor of the computer they
run on; on Windows, `--arch` builds for the other one too.

Leave out `--install` to only build (for example, to hand the file in `dist/`
to someone else). `dist/` only gets the finished downloads; the builds put
them together in `target/` first. The first build takes a while: on Linux it downloads the
Flatpak runtime and Rust toolchain (about 1.5 GB, once); everywhere, compiling
takes a few minutes. Later builds are much faster.

All options (`./build.sh --help` lists them too):

| Option | What it does |
|---|---|
| (none) | Build for this system |
| `--install` | Build, then install |
| `--uninstall` | Remove the installed app (Linux and macOS; your settings stay) |
| `--udev` | Linux: also allow your account to write to the badge (asks for your password). You can also do this later from the app. |
| `--native` | Linux: build and install directly into `~/.local` instead of as a Flatpak (see below) |
| `--arch x64` or `--arch arm64` | Windows: build for that processor instead of this computer's (also `x86_64`, `amd64`, `aarch64`) |

Every build first applies `branding.toml`: the app's name, copyright and
icons. To build the app under your own name or with your own logo, see
[Branding: name, copyright and icons](branding.md).

Run the tests with `cargo test --workspace`. The sections below describe
what the script does on each system, and how to build by hand.

## Linux: Flatpak (the default)

`./build.sh` runs `packaging/flatpak/build.sh`. This works on any
distribution, including read-only ones like SteamOS (run it in Konsole in
Desktop Mode): nothing is installed outside your home folder apart from
Flatpak itself, if it was missing. It:

1. installs Flatpak with your package manager if needed, and adds Flathub;
2. uses `flatpak-builder` if it's installed, or else installs Flathub's
   `org.flatpak.Builder`;
3. builds the app inside the Freedesktop SDK with the same Rust as outside
   Flatpak: it updates your Rust to the latest stable release first
   (`rustup update stable`), reads its version with `rustc --version`, and
   pins that official rust-lang.org release, with its checksums, in
   `packaging/flatpak/rust.yml` (the latest stable release if Rust isn't
   installed). That Rust is only used for building and isn't in the app;
4. writes `dist/LED_Badge_Studio-linux-<arch>.flatpak`, for this computer's
   processor (`x86_64` or `arm64`; Flatpak calls the latter `aarch64`),
   ready to attach to a GitHub release, and with `--install` installs it.

If Flatpak was only just installed, log out and back in once so the app
appears in the menu. Start it from the menu or with
`flatpak run dev.uint.LedBadgeStudio`; `badgectl` is inside it
(`flatpak run --command=badgectl dev.uint.LedBadgeStudio list`).

The manifest is `packaging/flatpak/dev.uint.LedBadgeStudio.yml`. Before
building, the script downloads the Rust crates into `.flatpak-cargo/` in the
project folder (about 170 MB, kept for the next build), which the Flatpak
build then uses; anything missing is downloaded during the build. Publishing
on Flathub would need the crates listed as sources instead (with
`flatpak-cargo-generator`).

## Linux: without Flatpak (`--native`)

```bash
./build.sh --native --udev
```

This runs `packaging/linux/install.sh`, which builds for this computer's
processor (x86_64 or arm64) and installs that. If the build tools are missing, it
first runs `packaging/linux/install-build-deps.sh`, which installs the
compiler and libraries with your package manager (apt, dnf, pacman or zypper;
asks for your password) and Rust with rustup (in your home folder). Then it
builds and installs into `~/.local`: `led-badge-studio` and `badgectl` in
`~/.local/bin` (which needs to be on your `PATH`), the menu entry, icons, and
the README, these docs and the licenses in
`~/.local/share/doc/led-badge-studio/`. `--udev` also installs the badge
access rule; leave it out to do that from the app later. Remove it again with
`./build.sh --native --uninstall`.

Use one or the other: the Flatpak and the direct install share a menu entry.
`./build.sh --install` removes a direct install it finds, and `--native`
tells you if the Flatpak is installed. `install.sh` also takes
`PREFIX=/some/folder` to install elsewhere, and `--remove-udev` to remove
only the rule.

To build by hand instead:

```bash
./packaging/linux/install-build-deps.sh
```

```bash
cargo build --release -p badge-app -p badgectl
```

The programs are then `target/release/led-badge-studio` and `target/release/badgectl`.

## Windows

`build.cmd` runs `packaging\windows\build.ps1`. It builds for the processor
Windows runs on (x64, or arm64 on ARM computers such as Snapdragon ones),
or with `--arch x64` or `--arch arm64` for that one: Windows on ARM can build
the x64 app, and the other way round. It uses `winget` (included in Windows
10 and 11) to install what's missing:

1. **Visual Studio 2022 Build Tools** with the C++ workload, which includes
   the Windows SDK, and the C++ tools for the processor it builds for
   (`MSVC … x64/x86 build tools` or `MSVC … ARM64 build tools`). If the Build
   Tools are installed without those, it adds them with the Visual Studio
   Installer. Windows asks for permission, and the first download is large.
2. **Rust** (rustup), with its target for that processor
   (`x86_64-pc-windows-msvc` or `aarch64-pc-windows-msvc`).

It then finds the Windows SDK's `fxc.exe`, which GPUI needs to compile its
DirectX shaders, and builds (into `target\<target>\release\`). The programs, README, docs and licenses are put
together in `target\package\LED_Badge_Studio-windows-<arch>\` (deleted again
when the build ends) and zipped into `dist\LED_Badge_Studio-windows-<arch>.zip`. With
`--install`, the app is copied to `%LOCALAPPDATA%\Programs\<app name>`, gets a
Start menu shortcut, and `.badge` files open with it (for your account; no
administrator rights needed). To uninstall, delete that folder and the
shortcut. From Git Bash, `./build.sh` works too.

Building by hand: install the
[Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
with **Desktop development with C++** (on ARM, add the **MSVC ARM64 build
tools** component) and [Rust](https://rustup.rs/), then
run `cargo build --release -p badge-app -p badgectl`. If the build can't
find `fxc.exe`, set `GPUI_FXC_PATH` to its full path (it's in
`C:\Program Files (x86)\Windows Kits\10\bin\<version>\x64\`).

The app has to be built on Windows itself (for either processor): GPUI
compiles its shaders only on Windows, so it can't be cross-compiled from
Linux or macOS. `badgectl` can be
(`cargo build --release --target x86_64-pc-windows-gnu -p badgectl` with
MinGW-w64).

## macOS

The app is for Macs with Apple silicon and macOS 11 or later; on an Intel
Mac, or in a terminal running under Rosetta, the build stops and says so.
`./build.sh` runs `packaging/macos/build.sh`. It installs Apple's Command
Line Tools if needed (a window asks you to confirm; the full Xcode isn't
needed, except for [Icon Composer icons](branding.md#macos-icon-composer-and-liquid-glass))
and Rust, builds, and assembles the app with its icon. The app is signed for
this Mac only (ad hoc), with `badgectl` inside it (in `Contents/MacOS`), and
put in a disk image, `dist/LED_Badge_Studio-macos-arm64.dmg`: the app, a
link to Applications to drag it onto, and a [background](branding.md#macos-the-disk-images-background)
with an arrow between them. The app is put together with the README, docs and
licenses in `target/package/LED_Badge_Studio-macos-arm64/`; `--zip` also
makes `dist/LED_Badge_Studio-macos-arm64.zip` of that folder. With
`--install`, the app is copied to `~/Applications`. That folder is deleted
when the build ends, so Spotlight and Finder's *Open With* don't list a
second copy of the app.

The disk image is laid out with [dmgbuild](https://github.com/dmgbuild/dmgbuild)
(settings in `packaging/macos/dmg-settings.py`). The first build installs it
from PyPI into `target/dmgbuild`, with the Python that comes with the Command
Line Tools, so that build needs an internet connection.

By hand: `xcode-select --install`, install [Rust](https://rustup.rs/), then
`cargo build --release -p badge-app -p badgectl`.

## Automatic builds on GitHub

[`.github/workflows/build.yml`](../.github/workflows/build.yml) runs on
GitHub Actions once the project is on GitHub:

- **Every push and pull request**: the formatting check, clippy and the
  tests, on Linux.
- **A version tag** (for example `git tag v0.2.0 && git push --tags`), or
  **by hand** (*Actions → Build → Run workflow*): the tests, then the
  Flatpaks and Windows zips, each for x64 and arm64, built on GitHub's
  x64 and ARM runners, and the macOS disk image (Apple silicon). Download them
  from the run's page (*Artifacts*, kept for 30 days) and attach them to a
  release on the *Releases* page yourself.

Each build uses the same script as a build on your own computer. The
Flatpak runtime and SDK are cached between runs (refreshed weekly, and
updated from Flathub on every run), and so are the Rust crates (until
`Cargo.lock` changes), so they aren't downloaded each time. The
Flatpak build also checks that the Flatpak can still ask the host for your
password, so **Install Rule** in Badge Access works without typing commands.

GitHub Actions is free for public repositories. Private repositories get a
monthly allowance of minutes, in which macOS minutes count ten times and
Windows minutes twice, so a full release build uses up a good part of it.

## Regenerating generated files

These are checked in, so you only need this when changing them:

| What | Command | Needs |
|---|---|---|
| CJK font tables (`assets/fonts/cjk-*.lbg`) | `python3 packaging/fonts/build_fonts.py` | Python 3, internet access |
| App icons and packaging metadata | applied by every build script, or see [Branding](branding.md) | Rust |
| Third-party notices (`THIRD_PARTY_NOTICES.txt`) | see below | cargo-about |

Regenerate the third-party notices whenever dependencies change:

```bash
cargo install cargo-about --features cli
```

```bash
cargo about generate --fail -c packaging/licenses/about.toml --workspace -o THIRD_PARTY_NOTICES.txt packaging/licenses/notices.hbs
```
