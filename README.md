# LED Badge Studio

Design messages and send them to USB LED name badges (USB ID 0416:5020).

An open replacement for the vendor's Windows-only "BMP Badge LED V9.18"
software that works on Linux, Windows and macOS, written in Rust with a
[GPUI](https://www.gpui.rs/) interface. It's based on reverse-engineering
the vendor's "LED Badge 9.18 (English Version)" package.

Most of its code and documentation was written by AI, as an experiment in
AI-driven development, with a human directing the work, writing parts of it
and reviewing all of it. See
[How this was made](#how-this-was-made-ai-disclosure) for how, why, and the
author's stance on AI.

## Contents

- [Supported badges](#supported-badges)
- [Features](#features)
- [Installing](#installing)
- [Building from source](#building-from-source)
- [Documentation](#documentation)
- [Contributing](#contributing)
- [How this was made (AI disclosure)](#how-this-was-made-ai-disclosure)
- [License](#license)

## Supported badges

Every USB badge for the vendor software identifies itself with the same USB
ID, **`0416:5020`** (vendor `0416` is registered to Winbond Electronics Corp.).
That is the only ID the original software looks for, and the only one this app
talks to. Several physically different badges share it:

| Badge                                | Controller (as reported over USB) | USB VID:PID                       | Interface                           | Status                                           |
| ------------------------------------ | --------------------------------- | --------------------------------- | ----------------------------------- | ------------------------------------------------ |
| Monochrome 11 × 44 name badge        | WCH CH546 (`wch.cn` / `CH546`)    | `0416:5020`                       | USB HID                             | **Tested**                                       |
| Monochrome 11 × 44 "LS32" name badge | Winbond / Nuvoton                 | `0416:5020`                       | USB HID                             | Same protocol, not yet tested                    |
| Monochrome 12-row badge (48 px wide) | -                                 | `0416:5020`                       | USB HID                             | Same protocol, not yet tested                    |
| Monochrome 16-row badge              | -                                 | `0416:5020`                       | USB HID                             | Same protocol, not yet tested                    |
| 7-color 12-row badge                 | -                                 | `0416:5020`                       | USB HID                             | Same protocol, not yet tested                    |
| 7-color 16-row badge                 | -                                 | `0416:5020`                       | USB HID                             | Same protocol, not yet tested                    |
| Older serial (COM port) badges       | -                                 | depends on the USB-serial adapter | Serial (`ttyUSB*`/`ttyACM*`/`COM*`) | Implemented from the vendor code, not yet tested |

Pick the matching type under **Badge** in the toolbar. To check what's
connected:

```bash
badgectl list
```

On Linux, `lsusb | grep 0416:5020` shows the badge too. On Windows, Device
Manager lists it under _Human Interface Devices_ as "USB Input Device", with
`HID\VID_0416&PID_5020` among its _Hardware Ids_ (Properties → Details). On
macOS, _System Information_ → _USB_ shows Vendor ID `0x0416` and Product ID
`0x5020`.

Not supported: Bluetooth badges (such as those used with the _Badge Magic_
app), which use a different protocol.

If you have one of the untested variants, reports (good or bad) are very
welcome.

## Features

- Eight message slots, each with its own mode (scrolling, fixed, animation,
  snow, curtain, laser), speed, flash and marquee border
- Text in built-in pixel fonts (including Chinese, Japanese and Korean) or
  any font on your computer, with per-character colors on color badges
- A pixel editor, and image import (including GIFs as animations)
- A live preview that plays each message like the badge does, timed against
  a real badge, in the main window or a full-screen pop-out
- LEDs drawn like the real thing, with optional glow and brightness
- Export the preview as an animated GIF, PNG or WebP, or as a still image
- A light and a dark look, following the system's setting if you like
- Undo and redo for everything, full keyboard use, and drag-and-drop to
  reorder messages
- `.badge` project files that open with a double-click, with safe repairs
  for damaged or hand-edited ones
- Notices badges being plugged in, and sets up badge access on Linux in one
  click
- `badgectl`, a command-line tool for scripting
- One command to build and install on Linux (Flatpak), Windows and macOS,
  under your own name and icons if you like

Every feature in detail: [docs/features.md](docs/features.md).

## Installing

Download the file for your system from the project's **Releases** page on
GitHub:

| System                                      | File                                                                                                                | How                                                                                                                                                                   |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Linux (any distribution, including SteamOS) | `LED_Badge_Studio-linux-x86_64.flatpak` (Intel or AMD) or `LED_Badge_Studio-linux-arm64.flatpak` (ARM)              | `flatpak install --user LED_Badge_Studio-linux-x86_64.flatpak` (or the arm64 one), then allow badge access from the app ([details](docs/installing.md#linux-flatpak)) |
| Windows 10 or 11                            | `LED_Badge_Studio-windows-x64.zip` (Intel or AMD) or `LED_Badge_Studio-windows-arm64.zip` (ARM, such as Snapdragon) | Unzip it anywhere and run `led-badge-studio.exe` ([details](docs/installing.md#windows-and-macos))                                                                    |
| macOS 11 or later, Apple silicon            | `LED_Badge_Studio-macos-arm64.dmg`                                                                                  | Open it and drag `LED Badge Studio.app` to Applications ([details](docs/installing.md#windows-and-macos))                                                             |

### System requirements

| System  | Version                                                                                                          | Processor                                                | Graphics                                                                              |
| ------- | ---------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| Linux   | Any distribution with [Flatpak](https://flatpak.org/setup/) (the Flatpak runtime it needs is downloaded with it) | x86_64 or ARM64                                          | Vulkan or OpenGL; without a usable GPU it falls back to software rendering on its own |
| Windows | Windows 10 (version 1803 or later) or 11                                                                         | x64 or ARM64                                             | Direct3D 11 (feature level 10.1 or above), or Windows' basic display driver           |
| macOS   | macOS 11 Big Sur or later                                                                                        | Apple silicon (M1 or later); Intel Macs aren't supported | Any (Metal)                                                                           |

You also need one of the [supported badges](#supported-badges) and a USB
port for it. On Linux, writing to the badge needs a one-time permission rule
the app can set up for you ([details](docs/linux-badge-access.md)).

The downloads aren't code-signed, so Windows and macOS warn on the first
launch; [Installing](docs/installing.md) explains how to get past that, and
how to update and uninstall. No release for your system, or want the latest
code? Build it yourself with one command, `./build.sh --install`
(`build.cmd --install` on Windows); see
[Building from source](#building-from-source).

## Building from source

1. **Get the source**: `git clone` the repository, or use GitHub's _Code →
   Download ZIP_ and unzip it.
2. **Build and install**, from the project folder:

   **Linux and macOS** (in a terminal):

   ```bash
   ./build.sh --install
   ```

   **Windows**: open a terminal in the project folder and run
   `build.cmd --install`, or double-click `build.cmd` to only build.

The script works out which system it's running on, installs whatever the
build needs that is missing (asking for your password or permission where
needed), builds, and installs: a Flatpak on Linux, the app in
`~/Applications` on macOS, and in `%LOCALAPPDATA%\Programs` with a Start menu
shortcut on Windows. Leave out `--install` to only build; the result is in
`dist/`.

All options, what the script does on each system, and how to build by hand:
[docs/building.md](docs/building.md).

## Documentation

| Document                                            | What's in it                                                                                                                                           |
| --------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| [Features](docs/features.md)                        | Every feature in detail                                                                                                                                |
| [Installing](docs/installing.md)                    | Step by step for each system, the Flatpak's permissions, updating and uninstalling                                                                     |
| [Troubleshooting](docs/troubleshooting.md)          | A badge stuck on its old messages, slow uploads, damaged files, blank windows on virtual machines                                                      |
| [Building from source](docs/building.md)            | Build options, what the build does on each system, building by hand, automatic builds on GitHub, releasing a new version, regenerating generated files |
| [Badge access on Linux](docs/linux-badge-access.md) | The udev rule that lets you write to the badge, from the app, `badgectl` or by hand                                                                    |
| [Project files (.badge)](docs/project-files.md)     | The file format, opening by double-click, and what happens with damaged or hand-edited files                                                           |
| [Using the command line](docs/command-line.md)      | `badgectl`: running it on each system, examples, and every command and option                                                                          |
| [Fonts and licenses](docs/fonts.md)                 | The built-in fonts, using the vendor's own fonts, license texts                                                                                        |
| [Branding](docs/branding.md)                        | Building the app under your own name, copyright and icons; package names and IDs                                                                       |
| [The badge protocol](docs/protocol.md)              | What's sent to the badge byte by byte, over USB and serial, how fast, and how to send safely on each system                                            |
| [Preview timing](docs/preview-timing.md)            | How the preview matches the badge's timing, what was measured, and how to measure a badge                                                              |
| [Development notes](docs/development.md)            | Project layout, how LEDs are drawn, and how the protocol was worked out                                                                                |

## Contributing

Bug reports and fixes are welcome; see [CONTRIBUTING.md](CONTRIBUTING.md). AI
agents follow [AGENTS.md](AGENTS.md) too.

## How this was made (AI disclosure)

Most of the code and documentation was written by AI (specifically the Claude
Opus 5.5 model), under a human's direction, including:

- The trivial reverse-engineering of the vendor software (C# on .NET
  Framework 4, plus an older Visual Basic 6 program)
- The Rust port
- The interface

A human directed the work and contributed to it directly:

- Chose the technologies it's built on (Rust and the GPUI interface framework
  among them), and often the technical approach for individual parts
- Planned what it should do and how it should look and behave, and refined it
  down to the small details
- Wrote some of the code, and specified other changes down to the exact edit
- Reviewed the code and the results
- Tested it extensively against a real badge on Linux, macOS and Windows, and
  tracked down the bugs that came up
- Wrote some of the documentation
- Made the app icon

### Why

This was mainly an experiment in what the model can do, and the badge made a
good opportunity for one.

The vendor's software is Windows-only, dated, split across separate
installers, and awkward to use. A proper replacement has a lot of moving
parts:

- The badge's upload protocol and its timing quirks, for monochrome, color
  and older serial badges
- Rendering text and images for the LEDs, in pixel fonts that cover Chinese,
  Japanese and Korean too
- An editor with a live preview that plays each message like the badge does
- Fitting into Linux, Windows and macOS, each with its own way of reaching
  the badge and its own packaging

That came to over 20,000 lines of Rust, about 1,400 lines of shell,
PowerShell, Python and YAML for building and packaging it, and about 2,600
lines of documentation. For someone who only reprograms their badge now and
then, writing all of that by hand would have taken far more time than it's
worth; with AI it became much more practical.

### Stance on AI

Vibe coding is software development where you describe what you want in plain
language and an AI writes the code. It may involve accepting that code without
thoroughly reviewing it, relying on the results and follow-up prompts to guide
changes. This project largely used the first part, but not the second: a human
reviewed the code and every detail of the resulting program, and asked for
changes until both were right. The review covered the technical details of each
change, the code itself where it was shown, and checks that each result did
what was intended; the code was also read through afterwards.

The author's current stance is that:

- AI within software development should mainly be used for code review, bug
  fixes and automated tests (where unit tests alone may not be enough)
- The humans involved should have at least some understanding of the code
- Other uses of AI can still lead to discoveries and solutions that are a
  net benefit, within software and beyond

Programming is an art form: the work should show care and human creativity,
not be slop.

This project is a deliberate exception to the first point: it was made with
AI as an experiment, not as a model for how software should usually be made.
It still meets the second: its code was understood, and the decisions behind
it and the care over its details were human.

For changes made with AI agents, [AGENTS.md](AGENTS.md) puts the second point
into practice. An agent has to show a diff of everything it changes or
proposes, check its code changes for bugs and test them, and explain what
each one does, how it works and what it affects. The person operating it
then has to explain each code change back in their own words before it
counts as done; until they can, the change is held back, and if their
explanations keep missing the point, it's set aside until they understand
it. Pull requests and comments made with an agent say so, and questions in
a review are answered by the person, not the agent. AI can still help write
the code, but nothing goes in that a human hasn't seen and understood.

## License

LED Badge Studio is released under the [MIT License](LICENSE). The built-in
fonts and the Rust libraries it uses have licenses of their own; see
[Fonts and licenses](docs/fonts.md), or _About → Licenses_ in the app.
