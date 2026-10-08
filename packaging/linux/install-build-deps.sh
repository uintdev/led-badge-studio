#!/usr/bin/env bash
# Installs everything needed to build LED Badge Studio on Linux: the system libraries
# (with your distribution's package manager, so it asks for your password)
# and Rust (with rustup, into your home folder).
#
#   ./packaging/linux/install-build-deps.sh
#
# Supports Debian, Ubuntu and derivatives (apt), Fedora and RHEL-likes (dnf),
# Arch and derivatives (pacman) and openSUSE (zypper). On read-only systems
# such as SteamOS, build the Flatpak instead (packaging/flatpak/build.sh).
set -euo pipefail

. /etc/os-release 2>/dev/null || true
ids=" ${ID:-} ${ID_LIKE:-} "

sudo_cmd=()
[[ $EUID -ne 0 ]] && sudo_cmd=(sudo)

if command -v apt-get >/dev/null; then
    echo "Installing build packages with apt…"
    "${sudo_cmd[@]}" apt-get update
    "${sudo_cmd[@]}" env DEBIAN_FRONTEND=noninteractive apt-get install -y \
        build-essential pkg-config curl xz-utils ca-certificates \
        libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libx11-dev libxcb1-dev \
        libx11-xcb-dev libfontconfig-dev libfreetype-dev libvulkan-dev mesa-vulkan-drivers
elif command -v dnf >/dev/null; then
    echo "Installing build packages with dnf…"
    "${sudo_cmd[@]}" dnf install -y \
        gcc pkgconf-pkg-config curl xz \
        libxkbcommon-devel libxkbcommon-x11-devel wayland-devel libX11-devel libxcb-devel \
        fontconfig-devel freetype-devel vulkan-loader-devel mesa-vulkan-drivers
elif command -v pacman >/dev/null && [[ $ids != *" steamos "* ]]; then
    echo "Installing build packages with pacman…"
    "${sudo_cmd[@]}" pacman -S --needed --noconfirm \
        base-devel curl xz \
        libxkbcommon libxkbcommon-x11 wayland libx11 libxcb fontconfig freetype2 \
        vulkan-icd-loader vulkan-swrast
elif command -v zypper >/dev/null; then
    echo "Installing build packages with zypper…"
    "${sudo_cmd[@]}" zypper --non-interactive install \
        gcc pkgconf-pkg-config curl xz \
        libxkbcommon-devel libxkbcommon-x11-devel wayland-devel libX11-devel libxcb-devel \
        fontconfig-devel freetype2-devel vulkan-devel
else
    cat >&2 <<'EOF'
Couldn't find a supported package manager (apt, dnf, pacman or zypper).
Install these with your system's tools, then run this script again:
  a C compiler, pkg-config, curl, xz, and the development files for
  xkbcommon, xkbcommon-x11, wayland, X11, xcb, fontconfig, freetype and Vulkan.
On SteamOS and other read-only systems, build the Flatpak instead:
  ./packaging/flatpak/build.sh --install
EOF
    exit 1
fi

if ! command -v cargo >/dev/null && [[ ! -x "$HOME/.cargo/bin/cargo" ]]; then
    echo "Installing Rust (rustup) into $HOME/.cargo…"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
fi
echo "Build dependencies are installed."
