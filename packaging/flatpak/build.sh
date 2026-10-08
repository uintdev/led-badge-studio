#!/usr/bin/env bash
# Builds the LED Badge Studio Flatpak and writes
# dist/LED_Badge_Studio-linux-<arch>.flatpak (x86_64 or arm64, this
# computer's), a single file you can install on any Linux distribution on
# that kind of processor, or attach to a GitHub release.
#
#   ./packaging/flatpak/build.sh             # build dist/LED_Badge_Studio-linux-<arch>.flatpak
#   ./packaging/flatpak/build.sh --install   # ... and install it for your user
#
# Everything needed is set up automatically: Flatpak itself (with your
# distribution's package manager, asks for your password) if it's missing,
# the Flathub remote, flatpak-builder (Flathub's org.flatpak.Builder if it
# isn't installed, which also works on SteamOS), the runtime, and Rust: the
# same version as outside Flatpak (updated to the latest with rustup first).
# Downloads about 1.5 GB the first time.
set -euo pipefail

app_id=dev.uint.LedBadgeStudio
flathub=https://dl.flathub.org/repo/flathub.flatpakrepo
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
work="$root/target/flatpak"
install=0
[[ "${1:-}" == "--install" ]] && install=1

if ! command -v flatpak >/dev/null; then
    echo "Flatpak isn't installed; installing it (needs your password)…"
    if command -v apt-get >/dev/null; then
        sudo apt-get update && sudo apt-get install -y flatpak
    elif command -v dnf >/dev/null; then
        sudo dnf install -y flatpak
    elif command -v pacman >/dev/null; then
        sudo pacman -S --needed --noconfirm flatpak
    elif command -v zypper >/dev/null; then
        sudo zypper --non-interactive install flatpak
    else
        echo "Please install Flatpak (https://flatpak.org/setup/) and run this again." >&2
        exit 1
    fi
fi

flatpak remote-add --user --if-not-exists flathub "$flathub"
# This computer's architecture, named as in the Windows and macOS downloads
# (Flatpak's own name for arm64 is aarch64).
arch="$(flatpak --default-arch)"
[[ $arch == aarch64 ]] && arch=arm64
out="$root/dist/LED_Badge_Studio-linux-$arch.flatpak"

if command -v flatpak-builder >/dev/null; then
    builder=(flatpak-builder)
else
    echo "Installing flatpak-builder from Flathub (first time only)…"
    flatpak install --user --noninteractive flathub org.flatpak.Builder
    builder=(flatpak run org.flatpak.Builder)
    # Custom installation folders (FLATPAK_USER_DIR, FLATPAK_SYSTEM_DIR) must
    # be visible inside the builder's sandbox too.
    for var in FLATPAK_USER_DIR FLATPAK_SYSTEM_DIR; do
        if [[ -n "${!var:-}" ]]; then
            builder=("${builder[@]:0:2}" "--env=$var=${!var}" "--filesystem=${!var}" "${builder[@]:2}")
        fi
    done
fi

# The Rust the Flatpak is built with: the same release as the Rust outside
# Flatpak, brought up to date first, so both builds use the latest. Without
# Rust installed (or with a nightly or beta), the latest stable release.
# rust.yml pins it with checksums from rust-lang.org.
[[ -f "$HOME/.cargo/env" ]] && . "$HOME/.cargo/env"
if command -v rustup >/dev/null; then
    echo "Updating Rust (rustup update stable)…"
    rustup update stable --no-self-update >/dev/null || echo "Couldn't update Rust; using the version installed." >&2
fi
# Prints a URL's contents, with curl or wget (Ubuntu and Debian have only
# wget by default); fails without either.
fetch() {
    if command -v curl >/dev/null; then
        curl -fsSL "$1"
    elif command -v wget >/dev/null; then
        wget -qO- "$1"
    else
        return 1
    fi
}
rust=""
command -v rustc >/dev/null && rust="$(rustc --version | awk '{ print $2 }')"
if [[ -z $rust || $rust == *-* ]]; then
    rust=""
    if channel="$(fetch https://static.rust-lang.org/dist/channel-rust-stable.toml)"; then
        rust="$(awk '/^\[pkg\.rust\]/ { f = 1 } f && /^version = / { gsub(/"/, "", $3); print $3; exit }' <<<"$channel")"
    fi
fi
rust_sha() {
    fetch "https://static.rust-lang.org/dist/rust-$rust-$1-unknown-linux-gnu.tar.xz.sha256" | cut -d' ' -f1
}
pinned="$(sed -n 's|.*/rust-\([0-9.]*\)-x86_64-.*|\1|p' "$here/rust.yml")"
if [[ -n $rust && $rust != "$pinned" ]]; then
    if sha_x86="$(rust_sha x86_64)" && sha_arm="$(rust_sha aarch64)" && [[ -n $sha_x86 && -n $sha_arm ]]; then
        tmp="$(mktemp)"
        sed -e "s|rust-$pinned-|rust-$rust-|" "$here/rust.yml" \
            | awk -v x="$sha_x86" -v a="$sha_arm" '
                /only-arches: \[x86_64\]/ { arch = "x" } /only-arches: \[aarch64\]/ { arch = "a" }
                /^    sha256: / { sub(/sha256: .*/, "sha256: " (arch == "x" ? x : a)) } { print }' > "$tmp"
        mv "$tmp" "$here/rust.yml"
        pinned="$rust"
    else
        echo "Couldn't get Rust $rust from rust-lang.org; building with Rust $pinned." >&2
    fi
elif [[ -z $rust ]]; then
    echo "Couldn't find out the latest Rust; building with Rust $pinned." >&2
fi
# The crates, fetched here into .flatpak-cargo (copied into the build with
# the rest of the project) so the build doesn't download them every time.
if command -v cargo >/dev/null; then
    echo "Fetching the Rust crates…"
    for manifest in "$root/Cargo.toml" "$root/packaging/branding/Cargo.toml"; do
        CARGO_HOME="$root/.flatpak-cargo" cargo fetch --locked --quiet --manifest-path "$manifest" \
            || echo "Couldn't fetch the crates beforehand; the build downloads them." >&2
    done
    # Only the downloaded .crate files are needed (cargo unpacks them as it
    # builds); unpacked, they'd be ten times the size to copy and cache.
    rm -rf "$root/.flatpak-cargo/registry/src"
fi
echo "Building the Flatpak with Rust $pinned (the first build downloads the runtime and Rust)…"
mkdir -p "$work" "$(dirname "$out")"
"${builder[@]}" --user --install-deps-from=flathub --force-clean --disable-rofiles-fuse \
    --state-dir="$work/state" --repo="$work/repo" "$work/build" "$here/$app_id.yml"

flatpak build-bundle --runtime-repo="$flathub" "$work/repo" "$out" "$app_id"
echo "Built $out ($(du -h "$out" | cut -f1))."

if [[ $install == 1 ]]; then
    flatpak install --user --noninteractive --reinstall "$out"
    echo "Installed. Start LED Badge Studio from your app menu, or with: flatpak run $app_id"
    # The desktop finds Flatpak apps through a folder Flatpak adds to
    # XDG_DATA_DIRS when you log in; when Flatpak is newer than this login
    # (it was only just installed), the app isn't in the menu yet and the
    # dock shows a generic icon for it.
    exports="${FLATPAK_USER_DIR:-$HOME/.local/share/flatpak}/exports/share"
    if [[ ":${XDG_DATA_DIRS:-}:" != *":$exports:"* && ":${XDG_DATA_DIRS:-}:" != *":$exports/:"* ]]; then
        echo
        echo "Note: Flatpak is new since you logged in. Log out and back in (once) so"
        echo "      LED Badge Studio appears in your app menu, with its icon in the dock."
    fi
else
    echo "Install it with:  flatpak install --user \"$out\""
fi
