#!/usr/bin/env bash
# Builds LED Badge Studio for the system you run it on, installing anything
# the build needs first.
#
#   ./build.sh              build the app for this system
#   ./build.sh --install    build it and install it
#   ./build.sh --uninstall  remove an installed copy
#
# What it makes:
#   Linux    a Flatpak (dist/LED_Badge_Studio-linux-<arch>.flatpak), any distribution
#   macOS    an app bundle and disk image (dist/LED_Badge_Studio-macos-arm64.dmg)
#   Windows  (Git Bash) the programs and a zip (dist\LED_Badge_Studio-windows-<arch>.zip);
#            from Explorer or cmd, use build.cmd instead
#
# macOS only:
#   --zip      also make a zip of the app (dist/LED_Badge_Studio-macos-arm64.zip)
#
# Windows only:
#   --arch ARCH  build for x64 or arm64 instead of this computer's processor
#
# Linux only:
#   --udev     also allow your account to write to the badge (asks for your
#              password; the app can do this later too)
#   --native   build and install directly into ~/.local instead of as a
#              Flatpak (always installs; needs your distribution's build tools)
set -euo pipefail
# Arrays are passed on as ${args[@]+"${args[@]}"}: macOS's bash 3.2 calls an
# empty "${args[@]}" unbound under set -u.

root="$(cd "$(dirname "$0")" && pwd)"
app_id=dev.uint.LedBadgeStudio

install=0 uninstall=0 udev=0 native=0 zip=0 arch=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --install) install=1 ;;
        --uninstall) uninstall=1 ;;
        --udev) udev=1 ;;
        --native) native=1 ;;
        --zip) zip=1 ;;
        --arch) arch="${2:-}"; [[ $# -gt 1 ]] && shift ;;
        --arch=*) arch="${1#--arch=}" ;;
        -h|--help) awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$0"; exit 0 ;;
        *) echo "Unknown option: $1 (see ./build.sh --help)" >&2; exit 2 ;;
    esac
    shift
done

case "$(uname -s)" in
    Linux) system=linux ;;
    Darwin) system=macos ;;
    MINGW*|MSYS*|CYGWIN*) system=windows ;;
    *) echo "Unsupported system: $(uname -s)" >&2; exit 1 ;;
esac

if [[ $system != linux && ( $udev == 1 || $native == 1 ) ]]; then
    echo "--udev and --native are for Linux only." >&2
    exit 2
fi
if [[ $system != macos && $zip == 1 ]]; then
    echo "--zip is for macOS only." >&2
    exit 2
fi
if [[ -n $arch && $system != windows ]]; then
    echo "--arch is for Windows only." >&2
    exit 2
fi

native_entry="$HOME/.local/share/applications/$app_id.desktop"
flatpak_installed() { command -v flatpak >/dev/null && flatpak info --user "$app_id" >/dev/null 2>&1; }

case "$system" in
linux)
    if [[ $native == 1 ]]; then
        if [[ $uninstall == 1 ]]; then
            exec "$root/packaging/linux/install.sh" --uninstall
        fi
        echo "== Linux: building and installing directly into ~/.local"
        args=()
        [[ $udev == 1 ]] && args+=(--udev)
        "$root/packaging/linux/install.sh" ${args[@]+"${args[@]}"}
        if flatpak_installed; then
            echo
            echo "Note: the Flatpak is installed too, and both use the same menu entry."
            echo "      Remove one: ./build.sh --uninstall (Flatpak) or ./build.sh --native --uninstall."
        fi
        exit 0
    fi

    if [[ $uninstall == 1 ]]; then
        flatpak uninstall --user --noninteractive "$app_id"
        echo "Removed the LED Badge Studio Flatpak."
        exit 0
    fi
    echo "== Linux: building the Flatpak (for a direct install instead, use --native)"
    args=()
    [[ $install == 1 ]] && args+=(--install)
    "$root/packaging/flatpak/build.sh" ${args[@]+"${args[@]}"}
    if [[ $install == 1 && -f "$native_entry" ]]; then
        # Same menu entry name; the one in ~/.local would hide the Flatpak.
        echo
        echo "A direct install (from --native) is also present and would hide the Flatpak"
        echo "in the app menu. Removing it (your settings stay)…"
        "$root/packaging/linux/install.sh" --uninstall
    fi
    if [[ $udev == 1 ]]; then
        if [[ $install == 1 ]]; then
            echo "Setting up badge access (asks for your password)…"
            flatpak run --command=badgectl "$app_id" udev install
        else
            echo "Note: --udev needs --install (or set up access later from the app)." >&2
        fi
    fi
    [[ $install == 1 ]] || echo "Next: ./build.sh --install to install it for your user."
    ;;

macos)
    if [[ $uninstall == 1 ]]; then
        name="$(cargo run --quiet --release --locked --manifest-path "$root/packaging/branding/Cargo.toml" -- --print name)"
        rm -rf "$HOME/Applications/$name.app"
        echo "Removed ~/Applications/$name.app."
        exit 0
    fi
    echo "== macOS: building the app"
    args=()
    [[ $install == 1 ]] && args+=(--install)
    [[ $zip == 1 ]] && args+=(--zip)
    "$root/packaging/macos/build.sh" ${args[@]+"${args[@]}"}
    [[ $install == 1 ]] || echo "Next: ./build.sh --install to copy it to ~/Applications."
    ;;

windows)
    if [[ $uninstall == 1 ]]; then
        echo "On Windows, delete %LOCALAPPDATA%\\Programs\\<app name> and its Start menu shortcut." >&2
        exit 1
    fi
    echo "== Windows: building the app"
    args=()
    [[ $install == 1 ]] && args+=(-Install)
    [[ -n $arch ]] && args+=(-Arch "$arch")
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$(cygpath -w "$root/packaging/windows/build.ps1")" ${args[@]+"${args[@]}"}
    ;;
esac
