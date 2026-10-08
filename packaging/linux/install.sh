#!/usr/bin/env bash
# Installs LED Badge Studio for the current user (no root needed), optionally plus
# the udev rule that lets you write to the badge (asks for your password).
#
#   ./packaging/linux/install.sh            # build and install to ~/.local
#                                           # (installs build tools and Rust first if needed)
#   ./packaging/linux/install.sh --udev     # ... and install the udev rule system-wide
#   ./packaging/linux/install.sh --remove-udev   # remove only the udev rule
#   ./packaging/linux/install.sh --uninstall
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
prefix="${PREFIX:-$HOME/.local}"
rule_dst=/etc/udev/rules.d/70-led-badge.rules
app_id=dev.uint.LedBadgeStudio

# Files from versions before the app was called LED Badge Studio.
remove_old_names() {
    rm -f "$prefix/bin/led-badge" "$prefix/share/applications/led-badge.desktop" \
          "$prefix"/share/icons/hicolor/*/apps/led-badge.*
    rm -rf "$prefix/share/doc/led-badge" "$prefix/share/led-badge/fontlib"
}

reload_udev() {
    sudo udevadm control --reload-rules
    sudo udevadm trigger --subsystem-match=hidraw --action=change
}

if [[ "${1:-}" == "--remove-udev" ]]; then
    echo "Removing udev rule (needs your password)…"
    sudo rm -f "$rule_dst" /etc/udev/rules.d/99-led-badge.rules
    reload_udev
    echo "Removed. A badge plugged in now may keep access until it is unplugged."
    exit 0
fi

# Refreshes the caches that make the menu entry, icons and the .badge file
# type show up (all optional; desktops also notice changes on their own).
refresh_desktop() {
    command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$prefix/share/icons/hicolor" 2>/dev/null || true
    command -v update-desktop-database >/dev/null && update-desktop-database "$prefix/share/applications" 2>/dev/null || true
    command -v update-mime-database >/dev/null && update-mime-database "$prefix/share/mime" 2>/dev/null || true
    return 0
}

if [[ "${1:-}" == "--uninstall" ]]; then
    rm -f "$prefix/bin/led-badge-studio" "$prefix/bin/badgectl" \
          "$prefix/share/applications/$app_id.desktop" \
          "$prefix/share/mime/packages/$app_id.xml" \
          "$prefix"/share/icons/hicolor/*/*/"$app_id".*
    rm -rf "$prefix/share/doc/led-badge-studio"
    remove_old_names
    refresh_desktop
    echo "Removed LED Badge Studio from $prefix."
    [[ -f "$rule_dst" ]] && echo "The udev rule $rule_dst was left in place (remove it with: $0 --remove-udev)."
    exit 0
fi

# Install the compiler, libraries and Rust first if anything is missing.
[[ -f "$HOME/.cargo/env" ]] && . "$HOME/.cargo/env"
if ! command -v cargo >/dev/null || ! command -v cc >/dev/null || ! command -v pkg-config >/dev/null \
    || ! pkg-config --exists xkbcommon xkbcommon-x11 xcb fontconfig freetype2; then
    "$here/install-build-deps.sh"
    . "$HOME/.cargo/env"
fi
# The latest Rust (the Flatpak build uses the same version).
if command -v rustup >/dev/null; then
    echo "Updating Rust (rustup update stable)…"
    rustup update stable --no-self-update >/dev/null || echo "Couldn't update Rust; using the version installed." >&2
fi

echo "Applying branding.toml (name, copyright, icons)…"
cargo run --quiet --release --locked --manifest-path "$root/packaging/branding/Cargo.toml"
# For this computer's processor, named as in the downloads.
arch="$(uname -m)"
[[ $arch == aarch64 ]] && arch=arm64
echo "Building release binaries for $arch (the first build takes a few minutes)…"
(cd "$root" && cargo build --release -p badge-app -p badgectl)

remove_old_names
install -Dm755 "$root/target/release/led-badge-studio" "$prefix/bin/led-badge-studio"
install -Dm755 "$root/target/release/badgectl" "$prefix/bin/badgectl"
install -Dm644 "$here/$app_id.desktop" "$prefix/share/applications/$app_id.desktop"
install -Dm644 "$here/$app_id.xml" "$prefix/share/mime/packages/$app_id.xml"   # the .badge file type
# App icon (apps/) and .badge file icon (mimetypes/).
for icon in "$root"/assets/icons/hicolor/*/*/"$app_id".*; do
    rel="${icon#"$root"/assets/icons/}"
    install -Dm644 "$icon" "$prefix/share/icons/$rel"
done
doc="$prefix/share/doc/led-badge-studio"
rm -rf "$doc"
install -Dm644 "$root/README.md" "$doc/README.md"
install -Dm644 "$root/LICENSE" "$doc/LICENSE"
mkdir -p "$doc/docs"
install -m644 "$root"/docs/*.md "$doc/docs/"
install -Dm644 "$root/THIRD_PARTY_NOTICES.txt" "$doc/THIRD_PARTY_NOTICES.txt"
mkdir -p "$doc/font-licenses"
install -m644 "$root"/assets/fonts/licenses/* "$doc/font-licenses/"
refresh_desktop
echo "Installed to $prefix (make sure $prefix/bin is on your PATH)."

if [[ "${1:-}" == "--udev" ]]; then
    echo "Installing udev rule (needs your password)…"
    sudo rm -f /etc/udev/rules.d/99-led-badge.rules   # misnamed rule from early versions
    sudo install -Dm644 "$here/70-led-badge.rules" "$rule_dst"
    reload_udev
    echo "Badge access set up. Plug in (or re-plug) the badge."
else
    echo "Tip: run with --udev, or use File > Badge Access Help > Install Rule in the app, to allow writing to the badge."
fi
