# Changelog

## 0.2.6 (2026-11-25)

### All platforms

- A question still open is cancelled when the project is replaced.
- Repair checks the file again if it changed meanwhile.
- Save As adds .badge to any name without it.
- Projects opened by a relative path reopen from Open Recent.
- Keys that change nothing no longer tick a message.
- Use Default Font and paste keep messages within 4096 columns.
- Send asks first when a message is too wide.
- The badge picked in the menu is chosen again when back.
- Sends to one badge address aren't remembered as last sent.
- Tabs show as spaces; other control characters show nothing.
- Missing characters come from a font that really draws them.
- Spaces no font has show as spaces instead of vanishing.
- Characters no font can draw are named when typed.
- badgectl finishes the upload when killed or its terminal closes.
- badgectl warns about characters no font can draw.

### Linux

- In the Flatpak, Ctrl+C also waits for badgectl's upload.
- badgectl pauses on Ctrl+Z only after the upload.

### macOS

- badgectl pauses on Ctrl+Z only after the upload.

### Windows

- Badges show up even if device notifications fail.

## 0.2.5 (2026-11-24)

### All platforms

- Reopening the open file after saving shows your saved edits.
- Saves that change a file keep it under Revert To.
- Save As and exports ask before replacing a file.
- Exports with another extension get their format's added.
- Recovered work asks before saving over later changes.
- Use Theirs keeps the "doesn't fit the badge" warning.
- Unreadable settings are kept aside, not reset, and earlier ones too.
- Files opened in another running copy stay in Open Recent.
- Undo waits while a question is open.
- Undo no longer merges typing in two messages.
- Typing, fonts or spacing can't push a message past its limits.
- Edits tick a message only when the badge has room.
- A badge plugged back in wins over a serial port.
- Clear and Revert To drop an image still loading.
- Switching messages clears the text selection.
- A double-click keeps the whole text selected.
- Pasted messages are repaired like project files.
- Erasing a drawing back to before isn't a change.
- Short uploads wait longer for the badge to store them.
- Project files with decimals like `4.0` open normally.
- Long exports use far less memory and say if cut.
- Spaces a system font lacks show blank, not as boxes.
- Phone photos import the right way up.
- Large images import with far less memory.
- A failed save in Repair, Save and Open says so.
- Center's messages show when it's used from the keyboard.
- The preview and Pop Out use less CPU while playing.
- badgectl warns about cut text; `check --strict` fails on it.
- badgectl warns when a font is too tall.
- badgectl keeps the order of `-t` and `-i`.
- badgectl exits with 64 when the command line is wrong.
- badgectl finishes the upload when you press Ctrl+C.
- badgectl calibrate records what it sent and reports errors cleanly.

### Linux

- Ctrl+Q in Pop Out asks before quitting.
- Badge access setup shows its real error when it fails.
- Badge Access no longer opens over another question.
- The maximize button updates as soon as a window is maximized.

### macOS

- A badge held by another app says so.
- Badges show up even if device notifications fail.

### Windows

- Ctrl+Q in Pop Out asks before quitting.
- "Access is denied" suggests another program has the badge.
- Windows use less graphics memory.
- Pasting several lines puts one space between them.
- Moving a window to another display is more reliable.

## 0.2.4 (2026-11-13)

### All platforms

- Dragging a message no longer shows a second preview in Pop Out.

### Windows

- Pop Out keeps playing while you work in the main window.

## 0.2.3 (2026-11-11)

### All platforms

- Saving no longer deletes a file named like its temporary one.
- A day-old time reads "1 day ago", not "1 days ago".
- badgectl refuses addresses over 999 instead of sending 999.
- badgectl shows a failed send's error on its own line.

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
