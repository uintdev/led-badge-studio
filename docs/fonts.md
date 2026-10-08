# Fonts and licenses

The badge font is compiled into the app; nothing else needs installing.

- **ASCII**: the IBM PC VGA ROM fonts (8x16, and 8x14 trimmed to 11 and 12
  rows), as shipped with the vendor software. Bitmap fonts aren't covered by
  copyright in the United States, which is why these glyphs are freely
  redistributed (for example as the Linux console fonts).
- **Chinese, Japanese and Korean**: [Fusion Pixel Font](https://github.com/TakWolf/fusion-pixel-font)
  (10 px for 11-row badges, 12 px for 12-row badges) and
  [GNU Unifont](https://unifoundry.com/unifont/) (16-row badges), both under
  the SIL Open Font License 1.1.

The vendor software's own CJK libraries (`HZK*`, `TWK*`, `JPK*`, `KRK*`) are
**not** included: they appear to be bitmap renders of Windows system fonts
(SimSun and others), which Microsoft doesn't allow to be redistributed. If you
own the vendor software you can still use them; see below.

## Using the vendor fonts (`LED_BADGE_STUDIO_FONTLIB`)

If you prefer the exact look of the original software for Chinese, Japanese
and Korean text, point the `LED_BADGE_STUDIO_FONTLIB` environment variable at a
folder containing the vendor's font files. The app then uses those instead of
the built-in CJK fonts. ASCII text looks the same either way.

**Where to find them.** In an installed copy of BMP Badge, the files are in its
`fontlib` folder (by default `C:\Program Files (x86)\BMPBadge\fontlib`). Without
Windows, you can extract them from the vendor's installer instead:

```bash
msiextract BMPBadgeSetup.msi    # from the msitools package; creates a fontlib/ folder
```

The folder must contain the files under their original names: `HZK11`, `HZK11V`,
`HZK12`, `HZK12V`, `HZK16`, `HZK16V` and the same set for `TWK`, `JPK` and
`KRK` (upper or lower case doesn't matter). You can copy the whole folder
anywhere, for example to `~/.local/share/led-badge-studio/vendor-fontlib`. Keep it
for your own use; don't redistribute it.

**Start the app with it** (Linux and macOS):

```bash
LED_BADGE_STUDIO_FONTLIB=~/.local/share/led-badge-studio/vendor-fontlib led-badge-studio
```

The same works for the command-line tool:

```bash
LED_BADGE_STUDIO_FONTLIB=~/.local/share/led-badge-studio/vendor-fontlib badgectl send -t "中文"
```

With the Flatpak, pass it with `--env` (the folder must be inside your home
folder, which the Flatpak can read):

```bash
flatpak run --env=LED_BADGE_STUDIO_FONTLIB=$HOME/.local/share/led-badge-studio/vendor-fontlib dev.uint.LedBadgeStudio
```

On Windows (PowerShell):

```powershell
$env:LED_BADGE_STUDIO_FONTLIB = "C:\Program Files (x86)\BMPBadge\fontlib"; .\led-badge-studio.exe
```

**Make it permanent**, so the app picks it up when started from the
application menu too:

- **Linux** (GNOME, KDE and most desktops, including SteamOS): create
  `~/.config/environment.d/led-badge-studio.conf` containing the line below, then log
  out and back in:

  ```
  LED_BADGE_STUDIO_FONTLIB=${HOME}/.local/share/led-badge-studio/vendor-fontlib
  ```

  Alternatively, edit the `Exec=` line of
  `~/.local/share/applications/dev.uint.LedBadgeStudio.desktop` to
  `Exec=env LED_BADGE_STUDIO_FONTLIB=/home/you/.local/share/led-badge-studio/vendor-fontlib led-badge-studio %f`.
- **Flatpak**: `flatpak override --user --env=LED_BADGE_STUDIO_FONTLIB=$HOME/.local/share/led-badge-studio/vendor-fontlib dev.uint.LedBadgeStudio`
  (undo with `flatpak override --user --reset dev.uint.LedBadgeStudio`).
- **Windows**: `setx LED_BADGE_STUDIO_FONTLIB "C:\Program Files (x86)\BMPBadge\fontlib"`,
  then start the app again.
- **macOS**: `launchctl setenv LED_BADGE_STUDIO_FONTLIB ~/vendor-fontlib` (lasts until
  you log out).

**Check that it's in use**: *File → About LED Badge Studio* (on macOS, *LED
Badge Studio → About LED Badge Studio* in the menu bar) shows "Built-in font: vendor
libraries from …" when the folder was found, or "built in" otherwise. If the
folder doesn't contain the files (at least `HZK12`), the app quietly uses the
built-in fonts. To go back to the built-in fonts, unset the variable. While
the vendor files are in use, the **Font** list names the default font
**IBM VGA + vendor fonts**.

## License texts in the app

*File → About LED Badge Studio → Licenses…* (on macOS, *Help → Licenses* in
the menu bar, or the same button in About) shows LED Badge Studio's own
license (MIT, the project's `LICENSE` file), the font attribution, the full
license of every font, and the licenses of all Rust libraries the app is built
from. The same texts are available as `badgectl licenses` (add `--fonts` or
`--third-party` to narrow it down), in
[THIRD_PARTY_NOTICES.txt](../THIRD_PARTY_NOTICES.txt), and in
`share/doc/led-badge-studio/` after installing with `install.sh` or inside the
Flatpak. Because the built-in CJK fonts are converted to the app's own format,
they are Modified Versions under the SIL Open Font License and don't use the
original fonts' Reserved Font Names.

How each font was identified, the regional-style handling and the full license
texts: [assets/fonts/README.md](../assets/fonts/README.md) and
[assets/fonts/licenses/](../assets/fonts/licenses/). To rebuild the CJK tables:
`python3 packaging/fonts/build_fonts.py`.
