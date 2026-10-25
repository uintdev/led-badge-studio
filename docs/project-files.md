# Project files (.badge)

Projects are saved as `.badge` files: readable JSON holding all eight
messages, their settings and pixels, the badge type and brightness, and for
mono badges the LED color picked under Preview (`led_color`, 1 to 7: blue,
green, cyan, red, magenta, yellow, white). The LED color is only for showing
the badge on screen (the preview, the Pixels canvas, Export and
`badgectl preview --export`); it isn't sent to the badge. A file without it
(from before it was saved) opens in the LED color you last picked, which is
also the color of new projects. Color badges don't use it, so it isn't saved
for them and is ignored in their files. The file type is
`application/x-led-badge`.

Double-clicking a `.badge` file opens it in LED Badge Studio once the app is
installed:

- **Linux**: the Flatpak and `install.sh` register the file type, its icon
  and the app as the default for it.
- **Windows**: `build.cmd --install` registers `.badge` for your account,
  with its icon. With
  the downloaded zip, right-click a `.badge` file, choose *Open with → Choose
  another app*, pick `led-badge-studio.exe` and tick *Always*.
- **macOS**: Finder associates `.badge` files with the app once
  `LED Badge Studio.app` is in Applications (or has been opened once).

`.badge` files get their own icon on every system; see
[The .badge file icon](branding.md#the-badge-file-icon) to change it.

## Damaged or hand-edited files

`.badge` files are plain text, so they can be edited by hand or written by
scripts. Both the app and `badgectl` read them the same way:

**Repaired** (the file opens; every change is listed): values of the right
kind but outside what the badge supports.

| In the file | Becomes |
|---|---|
| `speed` outside 1-8 (e.g. `9`, `300`, `2.5`) | The nearest allowed value, rounded (8, 8, 3) |
| `top_offset` outside -8 to 8, `letter_spacing` outside -2 to 10 | The nearest allowed value |
| System font `size_px` outside 4-64 | The nearest allowed value |
| A letter color or `default_color` outside 0-7 | Red |
| `led_color` (mono badges) outside 1-7, including 0 (off) | Red |
| More than 8 messages | The first 8 |
| A drawing taller than the badge, wider than 4096 columns, or with pixel values outside 0-7 | Cut to fit; invalid pixels turned off |
| Text that isn't UTF-8 (for example, saved by another program in Latin-1) | Those characters become � |

**Refused** (the file doesn't open; nothing changes): anything without one
sensible reading. The messages below are as `badgectl` prints them; the app
shows the same information laid out in a dialog.

| In the file | Example of the message |
|---|---|
| A cut-off file (for example, saving was interrupted) | `the file ends too early, at line 10, column 68; it may have been cut off while it was being saved` |
| Broken JSON (a missing comma, not a text file) | `the text isn't valid JSON (expected , or }), at line 1, column 31` |
| An empty file | `the file is empty` |
| A value of the wrong kind | `a setting has an unusable value (invalid type: integer 42, expected a string), at message 2, setting "text"` |
| An unknown mode, badge type or brightness | `a setting has an unusable value (unknown variant Spin, expected one of Left, Right, …), at message 1, setting "mode"` |
| Not a LED Badge Studio project (`format` missing or different) | `this isn't a LED Badge Studio project file; its "format" isn't "led-badge-project"` |
| A newer `version` than this app knows | `it was saved by a newer version of the app (file version 2); update LED Badge Studio to open it` |

Settings that are missing from a file get their normal defaults (for
example speed 4), and settings the app doesn't know are ignored, so files
from older and newer versions of the app open where they can.

**In the app**, opening a file that needs repairs (from File → Open, the
recent files, a double-click or by dropping it on the window) first shows a
**Repair** dialog. It lists every problem in a table, so you can see exactly
what will change compared to the file:

| Where | Setting | In the file now | Will become |
|---|---|---|---|
| Message 1 | Speed | 9 | → 8 (allowed: 1 to 8) |
| Message 2 | Letter colors | 12, 250 | → red (colors are 0 to 7) |

Then you choose:

- **Repair, Save and Open**: writes the repaired values to the file, then
  opens it.
- **Open Repaired**: opens the repaired project but leaves the file as it is.
  The project shows as modified; saving it writes the repairs.
- **Cancel**: nothing is opened and nothing changes.

**File → Check Project File…** does the same for any `.badge` file without
opening it, like `badgectl check`. It reports "No problems", shows the
Repair dialog (where **Repair and Save File** fixes the file and leaves your
current project alone; the file as it was is kept as an earlier version, as
when saving, so **File → Revert To** can bring it back), or explains why the
file can't be read.

A file that can't be opened at all shows a **Couldn't open** dialog: what's
wrong as a sentence, then the details on their own lines, for example:

> **Couldn't open damaged.badge**
> The file ends too early.
> **Where:** Line 10, column 68
> **Likely cause:** It may have been cut off while it was being saved.
> **File:** /home/you/Documents/damaged.badge

(and **What to do** where there's advice, such as for a file from a newer
version). Your current project stays as it was. Unsaved
changes are only asked about once the new file has been read successfully,
so a broken file never costs you your work.

**With `badgectl`**, a project given with `-p` is repaired the same way and
each repair is printed as a warning. To look at files without sending
anything, use `badgectl check` (see [Using the command line](command-line.md)):

```bash
badgectl check messages.badge
```

```bash
badgectl check --fix messages.badge     # also save the repairs to the file
```

If the app's own settings file (`settings.json`) is ever damaged, the app
starts with default settings and says so. The damaged file is kept next to
it as `settings.json.damaged` instead of being overwritten.

The vendor software's `.tls` projects and `.led` files aren't supported:
recreate those messages in the app.
