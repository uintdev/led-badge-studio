# Features

- A three-part window: the messages on the left, the message you're
  editing with its preview and pixels in the middle, and how it's shown
  (effect, speed, Flash, Marquee) with the preview's display options on the
  right. In a narrow window the right-hand part moves into the middle
- **Light and dark** appearance (**Settings**, Ctrl+, (⌘, on macOS) or the
  gear in the toolbar): **System** follows the system's setting on Linux, Windows and
  macOS (on GNOME, its Light setting, which it doesn't state as such, counts
  as light), and is dark when the system doesn't say; or always **Light** or
  **Dark**. Native window frames follow the choice on Windows and macOS.
  The LED preview stays dark in both, like the badge
- A **Get started** card with the first steps, until a badge has been
  connected (or it's closed; Settings can show it again while no badge is
  connected)
- On macOS, the app's menus are in the **menu bar** instead of a **File**
  button in the window: the application menu (About, Settings, Services,
  Hide, Quit), **File** (New, Open, Open Recent, Close Window, Save, Save As,
  Load What Was Last Sent, Check Project File), **Edit** (Undo, Redo, Cut,
  Copy, Paste, Select All), **Window** (Minimize, Zoom and the open windows)
  and **Help** (Keyboard Shortcuts, Licenses). Right-clicking the app's
  Dock icon lists the recent files too, also when the app isn't running
  (macOS keeps a copy of the list, cleared along with it)
- The window's title is the project's file name (with a **•** in front when
  there are unsaved changes). Closing the window, quitting, or opening or
  starting another project with unsaved changes asks whether to save them
  first: **Save**, **Don't Save** or **Cancel**. Nothing is saved without
  you saying so. On macOS this includes quitting from the Dock or by
  logging out
- Closing the window or quitting while a message is being sent asks first
  (**Quit Anyway** or **Cancel**): stopping an upload partway leaves the
  badge ignoring everything until it's unplugged and plugged back in
- On macOS the title is drawn the Mac way: the file's name and icon, with
  **— Edited** after it while there are unsaved changes; clicking the title
  offers **Rename** and **Move To**
- **File → Revert To** goes back to the version last opened or saved, or to
  an earlier save, listed by when it was saved ("Today at 14:15"). Each save
  keeps the file as it was: on macOS in the system's version store, as other
  Mac apps do; on Linux and Windows in the app's data folder, the last 10
  saves of each project file (by its path, so a file that's moved or renamed
  starts a new list). Reverting is an edit: Undo brings the changes back,
  and the file is unchanged until saved
- If another program changed the project's file since it was opened or
  saved here, saving over it asks first: **Keep Mine** replaces those
  changes, **Use Theirs** opens the file as it is now (or, when saving before
  closing, leaves it as it is). Coming back to the window after such a
  change, the status bar says so; the file isn't opened again on its own,
  even with nothing unsaved here
- Unsaved work survives a crash: while there's work that hasn't been saved,
  a copy is kept in the app's data folder (updated every few seconds), and
  if the app closes without it being saved or discarded (a crash, being
  forced to quit, the computer shutting down), a banner offers it back the
  next time: **Recover** opens it, unsaved, and **Discard** throws it away
  (with the app open more than once, each keeps its own copy, and only
  work from one that's no longer running is offered)
- While sending, the status bar says about how long it will take; the app's
  Dock icon (macOS) or taskbar button (Windows, and Linux desktops that
  show it, such as KDE Plasma) also shows a progress bar, to follow a long
  upload from another app
- In the message list: **drag** a message to move it; its **⋯** button or a
  right-click offers **Duplicate** (into the next empty slot), **Copy**,
  **Paste**, **Move Up**, **Move Down** and **Clear**. Copied messages can
  be pasted into another project, or another copy of the app
- **Keyboard shortcuts** (Ctrl+/ (⌘/ on macOS), F1, or the keyboard in the
  toolbar) lists every shortcut
- Eight message slots with per-message scroll mode (left, right, up, down,
  fixed, animation, snow, curtain, laser), speed 1-8, flash and marquee border
- Room on the badge, shown as you type: under the text, how wide the
  message is of the 4096 columns a message can be, and about how many more
  characters fit; under the message list, the **Badge memory** the ticked
  messages use of the 20,480 columns the badge holds. Text or drawing that
  wouldn't fit isn't taken (the status bar says which limit was reached), an
  image is cut to the room left, **Center** stops where the room ends, and
  ticking, duplicating or pasting a message that doesn't fit is refused, as
  is switching to a badge type the messages wouldn't fit in (icons, for
  example, are as wide as the badge is tall);
  removing text, erasing and unticking always work. A project opened over
  the limit keeps working, with the gauge in red: the end is cut when sending
- Text in the built-in pixel fonts (including Chinese, Japanese and Korean,
  and rotated "vertical" glyphs) or any font installed on the computer, all
  in one **Font** list. Under *Built-in*: the default pixel font, named
  after its sources (**IBM VGA + Fusion Pixel** on 11- and 12-row badges,
  **IBM VGA + Unifont** on 16-row ones), then **Fusion Pixel Font** and
  **GNU Unifont** (16-row badges) on their own, letters included. Then the
  computer's fonts. The list refreshes by itself when fonts are installed or
  removed, and a project whose font isn't installed says so instead of
  silently changing the look
- When a message's font can't be used (a computer font that isn't installed
  here, or GNU Unifont on a badge under 16 rows), a banner says which
  messages are affected; they're shown in a stand-in font meanwhile, and
  **Use Default Font** switches them to the default font (Undo reverses it)
- Right-click in the message box for **Cut**, **Copy**, **Paste** and
  **Select All**
- Built-in symbols (`~0` … `~f`), per-character colors on color badges,
  adjustable letter spacing
- Pixel editor: draw and erase by hand, import images (BMP, PNG, GIF with
  frames laid out for the Animation mode, JPG, ICO; or drop them on the
  window), export as PNG. Typing over a drawing asks first, so it isn't lost
  by accident
- Live animated preview of each display mode, for the message you're editing
  or every ticked message in turn (like the badge plays them). In the
  Animation mode each picture is 48 columns wide (the display width rounded
  up to whole bytes, as on the badge: on the 44-wide badge, columns 44-47 of
  each screen are never shown, so text skips a few columns between screens),
  and each picture shows for 4 steps. The steps follow the badge's own
  timer, measured on the badge at all 8 speeds (`badgectl calibrate`): from
  200 ms at speed 1 to 11 ms at speed 8, so a picture lasts 0.8 s at speed
  1 and 44 ms at speed 8
- **Export…** in the Preview card saves the preview as an image, exactly
  as the preview shows it (LED color and shape, Glow, Show brightness):
  - **Format**: GIF, PNG (animated PNG) or WebP (lossless). WebP is usually
    the smallest by far; GIF works everywhere. Typing a file name ending in
    `.gif`, `.png` or `.webp` picks that format.
  - **Animation**: one full cycle at the badge's own speed, looping (every
    ticked message in turn with **All ticked**). **Current frame**: the
    frame shown when you clicked Export, as a still (pause the preview to
    pick one).

  `badgectl preview --export FILE` does the same from the command line, the
  format chosen by the extension (`.gif`, `.png`, `.webp`); `--still` saves
  one frame (the one showing the most) and `--led-size` sets the LED size in
  pixels. It draws round LEDs with their glow at the project's brightness,
  like the app's default preview; `--square`, `--no-glow` and
  `--show-brightness` change that. All the encoders are pure Rust
- The preview redraws only when its picture changes, so pauses between
  pages and Fixed messages use almost no CPU
- **Display width** (16 to 132 LEDs, the range of the original app) shows
  the preview as if the badge were wider or narrower; **Auto** goes back to
  the badge type's own width. A wide display gets smaller LEDs so the whole
  of it stays in view, in the main window and in Pop Out
- **Pop Out** shows the preview in a window of its own, as large as the
  window allows; it follows every change made in the main window. **F11**
  (**⌃⌘F** on macOS) or a double-click toggles full screen (the title bar appears when you move
  the pointer to the top). **Esc** leaves full screen, or closes the window
  when it isn't full screen; its close button closes it too, and it closes
  with the main window. Only one preview plays at a time: while the pop-out
  is open, the main window says so and offers **Show Window** and **Close
  Pop-Out**, and Play, Pause and the other preview settings still control it
- LEDs drawn to look like the real thing: on mono badges, lit LEDs have a
  hot, nearly white core and unlit ones show the color of their packaging
  (dark red on a red badge), following the LED color you pick (saved with
  the project); color badges show clean pixel colors on neutral LEDs
- Optional LED glow: a tight halo around each lit LED and a faint wash over
  its neighbors, in the LED's color. It follows the badge's brightness
  setting: dimmer, with the halo reaching about 3 px at 25% and 4 px at
  100% (in the preview; scaled in the larger editor). Toggle it with
  **Glow** under Preview display (on by default); it applies to the preview
  and the Pixels canvas
- **Show brightness** under Preview display (on by default) dims the LEDs
  in the preview and the Pixels canvas to the brightness you've chosen
  (100%, 75%, 50% or 25%), as the badge will show them
- Keyboard use: nothing is focused until you press **Tab** (or click
  something), and a focus ring only shows while you use the keyboard; a
  click hides it. **Tab** and **Shift+Tab** go through every button,
  checkbox, field and slider; **Enter** or **Space** presses the focused
  one. Clicking a button doesn't take the keyboard away from the message
  box. In the message list, **↑ ↓** (and **Home**, **End**) change the
  selected message, **Alt+↑ ↓** (**⌥↑ ↓** on macOS) move it, **Ctrl+D**,
  **Ctrl+C** and **Ctrl+V** (**⌘D**, **⌘C** and **⌘V** on macOS) duplicate,
  copy and paste it, and **Shift+F10** (or the Menu
  key) opens its actions menu; **← →** move a focused slider (with
  **Shift**, by 10). **Tab** goes through the window in reading order: the
  toolbar, the message list, the middle column, then the right-hand panel.
  In menus, **↑ ↓** move, **Enter** or **Space** chooses, **→ ←** open and
  leave Open Recent, **Esc** closes; a menu opened from the keyboard starts
  on its first item. In dialogs, **Enter** does the main action (or
  **Cancel** where the main action would throw something away), **Esc**
  cancels, and **Tab** moves between the dialog's buttons only. When a menu
  or dialog closes, the keyboard goes back to where it was
- Undo/redo for everything (Ctrl+Z, Ctrl+Shift+Z / Ctrl+Y; on macOS ⌘Z,
  ⇧⌘Z / ⌘Y); typing bursts and brush strokes undo as one step
- Image import options next to **Load image…**: **Fit to height** (scale tall
  images down to the badge), **Invert** (for dark artwork on white) and a
  **Threshold** slider (which pixels light up). They re-apply live to an image
  you just loaded; the threshold snaps to its default (marked on the slider),
  and **Reset** sets it back. Each option explains itself in its tooltip.
  Images are read in the background, so a large one (a long animated GIF,
  say) doesn't hold up the window: the status bar says **Loading image…**
  until it's in, and again while Fit to height is applied to it
- **Center** in the Pixels section moves the lit pixels sideways so the badge
  shows them centered: to the start of the message in the modes that center
  a short message themselves (Fixed, Snow, Curtain, Laser), and to the middle
  of the display in the others (when narrower than it)
- **File → Open Recent** lists your last 10 projects with the folder each is
  in (showing more of the path where two files share a name, and shortening
  very long names in the middle; hover for the full path), and **Clear
  Recent Files** empties the list. Files that have
  been moved or deleted drop off the list when you try to open them. On
  macOS it's in the menu bar's File menu, which names the folder only where
  two files share a name, and **Clear Menu** empties it. On Windows the
  recent files are also in the taskbar button's Jump List (right-click it)
  once the app is installed (`build.cmd --install`, which registers `.badge`
  files with it)
- Reopens your last project on startup (can be turned off in Settings)
- Brightness and multi-badge addressing (set or target a badge address)
- Saves projects as `.badge` files, which open in the app when double-clicked
  and have their own file icon (see [Project files](project-files.md))
- Safe with damaged or hand-edited project files: values outside what the
  badge supports can be repaired, with a table of exactly what will change
  before anything does (in the app, or with `badgectl check --fix`); files
  that can't be read are refused with the reason and where in the file it
  is; and your current work is never discarded for a file that won't open
  ([details](project-files.md#damaged-or-hand-edited-files))
- Notices badges as soon as they're plugged in or removed, from the
  system's device notifications (a check every 2 seconds where those aren't
  available, such as a Flatpak without network access)
- A USB badge is picked over a serial port, which may be a computer's own
  (the Steam Deck has one), unless you choose the port under **Badge** in the toolbar
- A status light on the badge button in the toolbar: green when the badge
  is ready, amber when it needs [badge access](linux-badge-access.md) (Linux),
  red when none is connected; hover it for what it means
- One-click badge access setup on Linux, with copyable commands for doing it
  by hand
- Remembers what was last sent and offers to load it when the badge is
  plugged in (the badges can't be read back)
- `badgectl`, a command-line tool for scripting: list badges, send text,
  images or projects, preview as text art, check and repair `.badge` files,
  manage badge access
- Your own name, copyright and icons from one file
  ([Branding](branding.md)), and one command to build
  and install on Linux (Flatpak), Windows and macOS
