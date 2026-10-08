# Troubleshooting

- **The badge keeps showing its old messages after sending.** An upload was
  interrupted (for example by unplugging mid-send) and the badge is waiting for
  the rest of it. It ignores everything until it is power-cycled: unplug it,
  plug it back in, and send again. Normally the badge blanks and restarts its
  display the moment an upload finishes. (The app asks before closing or
  quitting mid-send for this reason; choosing **Quit Anyway** has the same
  effect as unplugging.)
- **Large uploads are slow.** The badge stores incoming data slowly (about one
  64-byte packet per 200-300 ms) and has a small buffer, so long uploads are
  paced at 325 ms per packet. Short messages fit in the buffer and upload in
  about a second.
- **Sending fails on Windows with "WaitTimedOut", "not functioning" (os
  error 31) or "cannot find the file specified" (os error 2).** Another
  program listed the computer's HID devices mid-upload, which interrupts the
  badge. The app starts such an upload again by itself (up to 3 times); if
  it still fails, send again. See
  [the badge protocol](protocol.md#each-system).
- **"Repair …?" or "Couldn't open …" when opening a project.** The file was
  edited by hand or damaged; the dialog lists exactly what. See
  [Damaged or hand-edited files](project-files.md#damaged-or-hand-edited-files).
  `badgectl check <file>` shows the same from a terminal.
- **"Your settings were reset" at startup.** The settings file couldn't be
  read. The old one is kept as `settings.json.damaged` in the settings folder
  (`~/.config/led-badge-studio/`; in the Flatpak
  `~/.var/app/dev.uint.LedBadgeStudio/config/led-badge-studio/`; on Windows
  `%APPDATA%\led-badge-studio\`; on macOS
  `~/Library/Application Support/led-badge-studio/`).
- **Blank or crashing window on a Linux VM.** Some virtual GPU drivers (for
  example virtio-gpu "Venus") crash GPUI. The app detects this on first launch
  and falls back to Mesa's software renderer automatically. Install
  `mesa-vulkan-drivers` (lavapipe) if it isn't there. Override with
  `LED_BADGE_STUDIO_GPU=hardware` or `LED_BADGE_STUDIO_GPU=software`.
