# Installing

Download the file for your system from the project's **Releases** page on
GitHub (see the [README](../README.md#installing)), then follow the steps for
your system below. To build it yourself instead, see
[Building from source](building.md).

## Linux (Flatpak)

1. **Make sure Flatpak is set up.** It already is on SteamOS, Fedora, Linux
   Mint, Pop!_OS, Bazzite, elementary OS and many others. On **Ubuntu** (and
   Debian), install it and add Flathub, where the app's runtime comes from:

   ```bash
   sudo apt install flatpak
   ```

   ```bash
   flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
   ```

   On **Arch**: `sudo pacman -S flatpak`.

   If you've just installed Flatpak, **log out and back in once**: until
   then the desktop doesn't know where Flatpak apps are, so LED Badge Studio
   won't be in the app menu and the dock shows a generic icon (a gear on
   Ubuntu) for it. `./build.sh --install` says so when it's needed.

2. **Install LED Badge Studio**, in the folder you downloaded it to. Take
   the file for your computer's processor: `x86_64` for Intel and AMD,
   `arm64` for ARM (`uname -m` says `x86_64` or `aarch64`):

   ```bash
   flatpak install --user LED_Badge_Studio-linux-x86_64.flatpak
   ```

   (Or double-click the file to open it in your software center.) The first
   install also downloads the shared Freedesktop runtime from Flathub, a few
   hundred MB that other Flatpak apps share.

3. **Start it** from the app menu, or with
   `flatpak run dev.uint.LedBadgeStudio`.

4. **Allow access to the badge** (once per computer): plug in the badge and
   click **Enable Badge Access** in the bar that appears, or **File → Badge
   Access Help… → Install Rule**. Enter your password, and the badge is ready.
   On SteamOS, set a password for the `deck` user first: run `passwd` in
   Konsole (Desktop Mode). See [Badge access on Linux](linux-badge-access.md).

To update, install a newer `.flatpak` file the same way. To uninstall:

```bash
flatpak uninstall --user dev.uint.LedBadgeStudio
```

To use it from Steam's Game Mode, add it in Desktop Mode with *Add a
Non-Steam Game* in Steam.

**What the Flatpak may access**, and why:

| Permission | Why |
|---|---|
| All devices (`--device=all`) | The badge (`/dev/hidraw*`), serial badges and the GPU |
| Host network namespace (`--share=network`) | Noticing badges being plugged in: Linux only sends those events there. The app makes no network connections. |
| Your home folder (`--filesystem=home`) | Files named on the `badgectl` command line (the app itself opens files through the system file picker) |
| Running commands on the host (`--talk-name=org.freedesktop.Flatpak`) | **Install Rule** and **Remove Rule**: install or remove the udev rule, with the usual password prompt |

## Windows and macOS

On Windows, unzip the download anywhere: the `x64` zip for Intel and AMD
processors, the `arm64` one for ARM (such as Snapdragon; *Settings → System
→ About* shows which). On macOS (a Mac with Apple silicon
and macOS 11 or later), open the `.dmg` and drag **LED Badge Studio** onto
**Applications**.

The downloads aren't code-signed, so the first launch shows a warning:

- **Windows**: "Windows protected your PC". Click *More info → Run anyway*.
- **macOS**: "Apple could not verify "LED Badge Studio" is free of malware"
  (or "can't be opened"). Click *Done*, then open *System Settings →
  Privacy & Security*, scroll down to the message about LED Badge Studio and
  click *Open Anyway* (and confirm with your password). This is needed once.
  On macOS 14 and earlier, right-clicking the app and choosing *Open* also
  works. Or, in Terminal, clear the download flag:

  ```bash
  xattr -dr com.apple.quarantine "/Applications/LED Badge Studio.app"
  ```

No driver or permission setup is needed on either system. `badgectl`, the
[command-line tool](command-line.md), is in the Windows zip and inside the
macOS app.
