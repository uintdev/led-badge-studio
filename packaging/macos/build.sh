#!/usr/bin/env bash
# Builds the macOS app bundle (Apple silicon, macOS 11 or later), installing
# anything missing.
#
#   ./packaging/macos/build.sh             # build into dist/
#   ./packaging/macos/build.sh --install   # ... and copy the app to ~/Applications
#   ./packaging/macos/build.sh --zip       # ... and also make a zip
#
# What it does:
#   1. Installs Apple's Command Line Tools if they're missing (a window asks
#      you to confirm; the full Xcode isn't needed).
#   2. Installs Rust (rustup, into your home folder) if it's missing.
#   3. Applies branding.toml (app name, copyright, icons; see
#      docs/branding.md), builds the app and badgectl, wraps them in
#      "<name>.app" with its icon and the .badge document type (badgectl in
#      Contents/MacOS, to run from Terminal; see docs/command-line.md), signs
#      it for this Mac (ad hoc), and puts it in a disk image for sharing:
#      dist/LED_Badge_Studio-macos-arm64.dmg (open it and drag the app to
#      Applications; laid out with dmgbuild, which is installed into
#      target/dmgbuild). With --zip, also
#      dist/LED_Badge_Studio-macos-arm64.zip: the app with the README, docs
#      and licenses. Everything is put together in target/package/, removed
#      again at the end; dist/ only gets the downloads.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
# Only Apple silicon is supported; macOS 11 was the first to run on it.
arch=arm64
export MACOSX_DEPLOYMENT_TARGET=11.0
if [[ "$(uname -m)" != arm64 ]]; then
    if [[ "$(sysctl -n hw.optional.arm64 2>/dev/null)" == 1 ]]; then
        echo "This terminal runs under Rosetta (Intel); run the script from a native one." >&2
    else
        echo "Only Macs with Apple silicon are supported." >&2
    fi
    exit 1
fi
base="LED_Badge_Studio-macos-$arch"
dist="$root/dist"
# Put together here; only the downloads go to dist/.
stage="$root/target/package/$base"
install=0 zip=0
for arg in "$@"; do
    case "$arg" in
        --install) install=1 ;;
        --zip) zip=1 ;;
        *) echo "Unknown option: $arg" >&2; exit 2 ;;
    esac
done

if ! xcode-select -p >/dev/null 2>&1; then
    echo "Installing Apple's Command Line Tools: click Install in the window that opens…"
    xcode-select --install >/dev/null 2>&1 || true
    until xcode-select -p >/dev/null 2>&1; do sleep 5; done
fi

[[ -f "$HOME/.cargo/env" ]] && . "$HOME/.cargo/env"
if ! command -v cargo >/dev/null; then
    echo "Installing Rust (rustup) into $HOME/.cargo…"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
    . "$HOME/.cargo/env"
fi
# The latest Rust.
if command -v rustup >/dev/null; then
    echo "Updating Rust (rustup update stable)…"
    rustup update stable --no-self-update >/dev/null || echo "Couldn't update Rust; using the version installed." >&2
fi

echo "Applying branding.toml (name, copyright, icons)…"
branding=(cargo run --quiet --release --locked --manifest-path "$root/packaging/branding/Cargo.toml" --)
"${branding[@]}"
name="$("${branding[@]}" --print name)"
app="$stage/$name.app"

echo "Building $name (the first build takes a few minutes)…"
(cd "$root" && cargo build --release -p badge-app -p badgectl)

echo "Assembling $name.app…"
rm -rf "$stage"
# Removed when the script ends, however it ends: Spotlight and Launch
# Services would otherwise list this copy as one more installed app (and
# could open .badge files with it). It's only needed until the downloads are
# made and --install has copied the app.
trap 'rm -rf "$stage"' EXIT
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$stage/font-licenses"
cp "$root/target/release/led-badge-studio" "$app/Contents/MacOS/led-badge-studio"
# The command-line tool travels inside the app (signed with it, below).
cp "$root/target/release/badgectl" "$app/Contents/MacOS/badgectl"
# The classic icon (all macOS versions) and Info.plist, both from branding.toml.
cp "$root/assets/icons/led-badge-studio.icns" "$app/Contents/Resources/led-badge-studio.icns"
# The .badge file icon, unless macOS draws it (see Info.plist).
doc_icon="$root/assets/icons/led-badge-studio-document.icns"
[[ -f $doc_icon ]] && cp "$doc_icon" "$app/Contents/Resources/led-badge-studio-document.icns"
cp "$root/packaging/macos/Info.plist" "$app/Contents/Info.plist"

# A modern icon from assets/branding/macos (Icon Composer .icon, or an Xcode
# asset catalog), compiled with Xcode's actool. macOS 26 and later show it
# (with Liquid Glass); older versions keep using the .icns above.
icon_src="$(find "$root/assets/branding/macos" -maxdepth 1 -name '*.icon' | head -1)"
[[ -z "$icon_src" ]] && icon_src="$(find "$root/assets/branding/macos" -maxdepth 1 -name '*.xcassets' | head -1)"
if [[ -n "$icon_src" ]]; then
    if [[ "$icon_src" == *.icon ]]; then
        icon_name="$(basename "$icon_src" .icon)"
    else
        set_dir="$(find "$icon_src" -maxdepth 1 -name '*.appiconset' | head -1)"
        icon_name="$(basename "${set_dir:-AppIcon.appiconset}" .appiconset)"
    fi
    if xcrun --find actool >/dev/null 2>&1; then
        echo "Compiling the app icon $(basename "$icon_src") with actool…"
        partial="$(mktemp -d)/partial.plist"
        if xcrun actool "$icon_src" --compile "$app/Contents/Resources" \
            --platform macosx --target-device mac --minimum-deployment-target "$MACOSX_DEPLOYMENT_TARGET" \
            --app-icon "$icon_name" --include-all-app-icons \
            --output-partial-info-plist "$partial" >"$partial.log" 2>&1; then
            plutil -replace CFBundleIconName -string "$icon_name" "$app/Contents/Info.plist"
            # actool also writes an .icns from the catalog for older macOS.
            if [[ -f "$app/Contents/Resources/$icon_name.icns" ]]; then
                plutil -replace CFBundleIconFile -string "$icon_name" "$app/Contents/Info.plist"
            fi
        else
            cat "$partial.log" >&2
            echo "Note: actool couldn't compile $(basename "$icon_src"); using the .icns icon instead." >&2
        fi
    else
        echo "Note: $(basename "$icon_src") needs Xcode's actool (install Xcode from the App Store," >&2
        echo "      then: sudo xcode-select -s /Applications/Xcode.app). Using the .icns icon instead." >&2
    fi
fi

cp "$root/README.md" "$root/LICENSE" "$root/THIRD_PARTY_NOTICES.txt" "$stage/"
mkdir -p "$stage/docs"
cp "$root"/docs/*.md "$stage/docs/"
cp "$root"/assets/fonts/licenses/* "$stage/font-licenses/"

# Ad-hoc signature: enough to run on this Mac. Other Macs block the first
# launch until it's allowed in Privacy & Security (docs/installing.md),
# unless it's signed with a Developer ID and notarized.
codesign --force --deep --sign - "$app"

# The disk image: the app, a link to Applications to drag it onto, and the
# background with the arrow between them, laid out by dmgbuild (see
# dmg-settings.py). Earlier outputs go first, so an old zip can't pass for a
# new one.
dmg="$dist/$base.dmg"
zip_file="$dist/$base.zip"
mkdir -p "$dist"
# Earlier builds assembled the app in dist/ itself.
rm -rf "${dist:?}/$base"
rm -f "$dmg" "$zip_file"

# dmgbuild (a Python tool) in its own environment under target/. These are
# the last versions that run on the Command Line Tools' Python 3.9.
dmgbuild_env="$root/target/dmgbuild"
if [[ ! -x "$dmgbuild_env/bin/dmgbuild" ]]; then
    echo "Installing dmgbuild (for the disk image) into target/dmgbuild…"
    python3 -m venv "$dmgbuild_env"
fi
"$dmgbuild_env/bin/python" -m pip install --quiet --disable-pip-version-check \
    "dmgbuild==1.6.5" "ds_store==1.3.1" "mac_alias==2.2.2"

# The background at both sizes in one file, so Retina screens get the sharp one.
dmg_tmp="$(mktemp -d)"
tiffutil -cathidpicheck "$root/assets/icons/dmg-background.png" "$root/assets/icons/dmg-background@2x.png" \
    -out "$dmg_tmp/background.tiff" >/dev/null 2>&1

echo "Making the disk image…"
# hdiutil, which dmgbuild runs, now and then fails with "Resource busy"
# (often on GitHub's runners); a moment later it works. Its output is only
# shown when an attempt fails: on recent macOS, hdiutil warns that the
# options dmgbuild uses are deprecated (in favour of `diskutil image`, which
# older macOS doesn't have), though they still work.
for attempt in 1 2 3; do
    if "$dmgbuild_env/bin/dmgbuild" -s "$here/dmg-settings.py" \
        -D app="$app" -D background="$dmg_tmp/background.tiff" "$name" "$dmg" >"$dmg_tmp/dmgbuild.log" 2>&1; then
        break
    fi
    cat "$dmg_tmp/dmgbuild.log" >&2
    [[ $attempt == 3 ]] && { echo "Couldn't make the disk image." >&2; exit 1; }
    echo "Making the disk image failed; trying again…" >&2
    sleep 5
done
rm -rf "$dmg_tmp"
echo "Built $dmg"

if [[ $zip == 1 ]]; then
    # --norsrc: without it, ditto keeps macOS file attributes as "._" files
    # in the zip, which are clutter when it's unzipped on Windows or Linux.
    # The app's signature is in its files, so it isn't affected.
    ditto -c -k --norsrc --keepParent "$stage" "$zip_file"
    echo "  and $zip_file"
fi

if [[ $install == 1 ]]; then
    mkdir -p "$HOME/Applications"
    rm -rf "$HOME/Applications/$name.app"
    cp -R "$app" "$HOME/Applications/"
    echo "Installed to ~/Applications/$name.app."
    echo "badgectl is in ~/Applications/$name.app/Contents/MacOS/badgectl (see docs/command-line.md)."
fi
