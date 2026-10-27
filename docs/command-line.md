# Using the command line

`badgectl` is the command-line tool that comes with every build of LED Badge
Studio. It does what the app does, from a terminal or a script: find badges,
send text, images or projects, preview and export animations, check project
files, and on Linux set up badge access.

## Running it

The examples below are written as `badgectl …`. How you run it depends on how
you installed the app.

### Linux (Flatpak)

`badgectl` is inside the Flatpak, so it runs through `flatpak run`:

```bash
flatpak run --command=badgectl dev.uint.LedBadgeStudio list
```

To type just `badgectl`, add this alias to your `~/.bashrc` (or your shell's
equivalent), then open a new terminal:

```bash
alias badgectl='flatpak run --command=badgectl dev.uint.LedBadgeStudio'
```

Files you name on the command line (images, projects, exports) must be in
your home folder, which is all the Flatpak can see.

### Linux (without Flatpak)

`./build.sh --native` installs it to `~/.local/bin`. If that folder is on
your `PATH` (it is on most distributions), run it as it is:

```bash
badgectl list
```

### Windows

`badgectl.exe` is next to `led-badge-studio.exe`: in the folder you unzipped
the download to, or, after `build.cmd --install`, in
`%LOCALAPPDATA%\Programs\LED Badge Studio` (named after the app). It isn't
added to `PATH`, so open a terminal in that folder (in Explorer, right-click
the folder background → *Open in Terminal*) and run it as `.\badgectl.exe`
in PowerShell:

```powershell
.\badgectl.exe list
```

To run it from anywhere, add that folder to your `PATH` (*Settings → System
→ About → Advanced system settings → Environment Variables*). The `udev`
commands are Linux-only; Windows needs no setup to reach the badge.

### macOS

`badgectl` is inside the app, in `Contents/MacOS`, so it runs from there.
In Terminal:

```bash
"/Applications/LED Badge Studio.app/Contents/MacOS/badgectl" list
```

(After `./build.sh --install` the app is in `~/Applications` instead: put `~`
in front of the path, outside the quotes.)

To type just `badgectl`, add this alias to your `~/.zshrc` (or your shell's
equivalent), then open a new Terminal window:

```bash
alias badgectl='"/Applications/LED Badge Studio.app/Contents/MacOS/badgectl"'
```

The download isn't code-signed. If macOS blocks `badgectl` as coming from an
unidentified developer, allow the app first (see
[Installing](installing.md#windows-and-macos)), or clear the download flag
on the whole app once:

```bash
xattr -dr com.apple.quarantine "/Applications/LED Badge Studio.app"
```

The `udev` commands are Linux-only; macOS needs no setup to reach the badge.

## Getting help

`badgectl` lists its commands, and each command lists its options (with
defaults and allowed values) and a few examples:

```bash
badgectl --help
```

```bash
badgectl help send
```

`badgectl send --help` is the same as `badgectl help send`, and `-h` gives a
shorter version. The examples below show more ways to combine the options,
and [All commands and options](#all-commands-and-options) has them all in
tables. `badgectl --version` prints the version.

## Examples

**Find your badge:**

```bash
badgectl list            # e.g. "USB badge 1      /dev/hidraw3"
```

```bash
badgectl list --watch    # keeps running, printing the list as devices come and go
```

**Send text** (to the first badge found, unless you pick one with `-d`):

```bash
badgectl send -t "Hello world"
```

```bash
badgectl send -t "Hello world" --mode left --speed 6 --brightness 50
```

```bash
badgectl send -d "USB badge 2" -t "Second badge" --mode fixed
```

```bash
badgectl send -t "Line one" -t "Line two ~f" --flash    # two messages; ~f is the heart icon
```

**Images, projects and previews:**

```bash
badgectl send -i logo.png --mode fixed                  # an image
badgectl send -p messages.badge                         # a project saved by the app
badgectl preview -t "Hi ~1"                             # text-art preview, no badge needed
badgectl preview -t "Hi ~f" --mode laser --export hi.webp   # ...and save the animation (.gif, .png, .webp)
badgectl preview -t "Hi ~f" --export hi.png --still          # ...or one still frame
```

**Measure the badge's timing** (so the preview can match it):

```bash
badgectl calibrate                       # everything: animation, then pause, then scroll
badgectl calibrate --mode animation      # only the Animation pictures
badgectl calibrate --mode pause          # only the pause on each page
badgectl calibrate --mode scroll         # only scrolling
badgectl calibrate --speeds 1,8          # only some speeds
badgectl calibrate --taps 30             # more precise (twice the taps)
```

For each speed it sends a test message (which replaces what's on the badge)
that lights the whole display once a cycle: the flash. Start the badge,
press Enter at every flash, and after 15 taps (8 for the pause) it moves on;
a missed flash doesn't matter, and taps far off the others are dropped.
What it measures:

| Mode | Test message | Result |
|---|---|---|
| `animation` | An animation whose first picture is fully lit | The time per picture at each speed, and the badge's timer unit from all speeds together, with the line to put in `crates/badge-render/src/preview.rs` |
| `pause` | A Fixed message of two pages, the first fully lit | How long each page stays, the pause the badge makes between pages, in the preview's steps |
| `scroll` | A lit bar scrolling left; tap when it appears at the right edge | How long a pass takes, in the preview's steps |

For the pause and the scroll, it says at which speeds (if any) the preview
differs from the badge, so the preview can be corrected. The pause takes a
while at the slowest speeds: a cycle is about 13 seconds at speed 1.

`badgectl calibrate` on its own (`--mode all`, the default) is a complete
calibration: `animation`, then `pause`, then `scroll`, in that order because
the last two are reported in steps of the timer unit the first measures.
Typing q at any point ends the run, with the results so far. Run it for a
badge model that hasn't been measured, or when the preview drifts away from
the badge. To redo one part: Animation pictures out of step points to
`animation`, page changes too early or late to `pause`, scrolling too fast or
slow to `scroll`.

**Check project files** (for hand-edited or script-generated files):

```bash
badgectl check messages.badge          # "OK", or the list of problems
```

```bash
badgectl check *.badge                 # several files at once
```

```bash
badgectl check --fix messages.badge    # repair what can be repaired and save it
```

```bash
badgectl check --strict *.badge        # also fail when sending would cut messages short
```

It prints one line per file: `OK`, the problems that can be repaired (with
what each value becomes), or why the file can't be opened. A warning follows
when the ticked messages need more than the badge's memory, saying which
messages sending would cut short or leave out. The exit status
makes it usable in scripts:

| Exit status | Meaning |
|---|---|
| 0 | Every file is fine (or, with `--fix`, was repaired and saved) |
| 1 | A file can't be opened (broken JSON, wrong kind of value, …) |
| 2 | A file has values that can be repaired (run with `--fix`) |
| 3 | With `--strict`: the ticked messages need more than the badge's memory, so sending would cut them short |

See [Damaged or hand-edited files](project-files.md#damaged-or-hand-edited-files) for what is
repaired and what is refused.

**Badge access (Linux):**

```bash
badgectl udev status     # is the rule installed, and can you write to the badge?
```

```bash
badgectl udev install    # install or reinstall the rule (asks for your password)
```

```bash
badgectl udev remove     # uninstall the rule
```

```bash
badgectl udev print      # just print the rule
```

**Licenses:**

```bash
badgectl licenses        # every license text (--fonts or --third-party for one part)
```

## All commands and options

| Command | What it does |
|---|---|
| `list [--watch]` | List connected badges and serial ports; `--watch` keeps running and prints the list again when a device is plugged in or removed |
| `send` | Send messages to a badge (content and badge options below) |
| `preview` | Print how the messages will look, as text art, and optionally save the animation as an image |
| `check FILE… [--fix]` | Check `.badge` files without sending anything; `--fix` saves the repairs |
| `calibrate` | Measure the badge's timing (Animation, pause and scroll) by tapping along |
| `licenses [--fonts \| --third-party]` | Print the license texts |
| `udev [print \| status \| install \| remove]` | Linux only: the badge access rule (`print` is the default) |
| `help [COMMAND]` | Help for a command; `--version` prints the version |

**What to show** (`send` and `preview`):

| Option | Meaning | Default |
|---|---|---|
| `-t`, `--text TEXT` | A message's text; repeat for up to 8 messages. `~0` … `~f` insert the built-in symbols | |
| `-i`, `--image FILE` | A message's image (BMP, PNG, GIF, JPG, ICO); repeat for more | |
| `-p`, `--project FILE` | A `.badge` project instead of `-t`/`-i` (its own settings are used) | |
| `--card TYPE` | Badge type: `mono11`, `mono12`, `mono16`, `rgb12`, `rgb16` | `mono11` |
| `--mode MODE` | `left`, `right`, `up`, `down`, `fixed`, `animation`, `snow`, `curtain`, `laser` | `left` |
| `--speed N` | 1 (slow) to 8 (fast) | 4 |
| `--flash` | Flash the messages | off |
| `--marquee` | Animated border | off |
| `--brightness N` | 100, 75, 50 or 25 (percent) | 100 |
| `--font FONT` | `"Fusion Pixel Font"` or `"Unifont"` (built-in), or a computer font as `"Family"` or `"Family:size[:bold][:italic]"` | the default pixel font |
| `--vertical` | Rotated glyphs (built-in fonts) | off |

**Which badge** (`send`):

| Option | Meaning |
|---|---|
| `-d`, `--device ID` | A device id or label from `badgectl list` (default: the first USB badge) |
| `--write-address N` | Store this address (0-999) in the badge |
| `--to-address N` | Only the badge with this address accepts the upload |

**Saving the preview** (`preview`):

| Option | Meaning | Default |
|---|---|---|
| `--export FILE` | Save the animation; the format comes from the extension (`.gif`, `.png`, `.webp`) | |
| `--still` | With `--export`: save one still frame instead | |
| `--led-size N` | LED size in pixels (2-64) | 10 |
| `--square` | Square LEDs instead of round ones | |
| `--no-glow` | No glow around lit LEDs | |
| `--show-brightness` | LEDs dimmed to the brightness setting | |

**Timing** (`calibrate`):

| Option | Meaning | Default |
|---|---|---|
| `--mode MODE` | `all`, `animation`, `pause` or `scroll` | `all` |
| `--speeds LIST` | The speeds to measure, comma-separated | `1,2,3,4,5,6,7,8` |
| `--taps N` | Taps per speed (6-200); more is more precise | 15, or 8 for `pause` |
| `--frames N` | Animation: pictures per cycle (2-60) | about 3 seconds' worth |
| `--card TYPE` | Badge type, as above | `mono11` |
| `-d`, `--device ID` | As above | the first USB badge |

With `-p`, a project brings its own badge type, modes, speeds, brightness
and fonts, so `--card` through `--vertical` are ignored.
