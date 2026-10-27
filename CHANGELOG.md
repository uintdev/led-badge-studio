# Changelog

## 0.2.2 (2026-10-27)

### All platforms

- Oversized projects say what gets cut, and ask before sending.
- Load What Was Last Sent shows only what reached the badge.
- The Send shortcut waits while a dialog is open.
- badgectl warns about cut messages; `check --strict` fails on them.

### Linux

- Closing the window no longer leaves a lock file behind.

### macOS

- Closing a new project asks only the app's question.
- Closing, or quitting from the Dock, no longer leaves a lock file.

## 0.2.1 (2026-10-26)

### All platforms

- Editing no longer stutters while unsaved work is kept.
- Work lost in a crash is offered in one window only.
- Recovered work is kept on disk until it's safe again.
- No lock files are left behind after quitting.
- A failed Save Image keeps the earlier file.
- A loading image follows its message when it's moved.
- Changing image options while it loads no longer drops it.
- No stray letter in the text box when a dialog appears.
- Two windows saving settings at once no longer damage them.
- Non-UTF-8 repairs name the field that changed.

### Linux

- Unsaved work is offered after a crash on NFS home folders.
- Each Flatpak window keeps its own unsaved work.
- Saving never writes through links other users planted.
- Saving works where file permissions can't be copied.

### macOS

- Saving never writes through links other users planted.
- Saving works where file permissions can't be copied.

## 0.2.0 (2026-10-25)

### All platforms

- Each open copy of the app keeps its own unsaved work for crash recovery.
- Unsaved work is still kept while recovered work is offered.
- A slow image import no longer lands in the wrong message.
- Drawings keep all their rows in damaged files.
- Text that isn't UTF-8 is now reported as a repair.
- Saving and exporting keep symlinks and file permissions.
- Export uses the format of the typed file extension.
- A failed export no longer destroys the earlier file.
- An Off LED color in the settings goes back to red.
- Fixed stray letters in the text box when typing over a drawing.
- Less work per frame while the preview plays.

### Linux

- The taskbar button shows send progress (KDE Plasma, Ubuntu's dock).
- Empty or relative XDG folders are ignored.

### macOS

- Fixed a possible hang when stopping the device watcher.

### Windows

- Long uploads to mono serial badges no longer time out.

## 0.1.2 (2026-10-10)

- A USB badge is now picked over a computer's own serial port (such as the Steam Deck's), unless you choose the port yourself.
- Sending to a serial device that doesn't take the data now stops with an error after a moment, instead of hanging.

## 0.1.1 (2026-10-08)

- Initial release
