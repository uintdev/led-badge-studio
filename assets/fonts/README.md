# Fonts

These are the pixel fonts behind the default font, listed under *Built-in*
in the **Font** list as "IBM VGA + Fusion Pixel" (11- and 12-row badges) or
"IBM VGA + Unifont" (16-row badges), after the sources it's made of: the IBM
VGA files supply the plain letters, digits and punctuation, and Fusion Pixel
or Unifont everything else. All of them are compiled into the app (`crates/badge-render/src/bitmap_font.rs`), so nothing
needs to be installed. "Computer font" messages use fonts installed on the
system instead.

| File | Used for | Source | License |
|---|---|---|---|
| `ascii-8x16.bin` | ASCII, 16-row badges | IBM PC VGA 8x16 ROM font, as shipped with the vendor's BMP Badge software | See [ASCII fonts](#ascii-fonts) |
| `ascii-8x12.bin` | ASCII, 12-row badges | IBM PC VGA/EGA 8x14 ROM font, trimmed to 12 rows by the vendor | See [ASCII fonts](#ascii-fonts) |
| `ascii-8x11.bin` | ASCII, 11-row badges | IBM PC VGA/EGA 8x14 ROM font, trimmed to 11 rows by the vendor | See [ASCII fonts](#ascii-fonts) |
| `cjk-10-*.lbg` | Chinese, Japanese, Korean on 11-row badges | [Fusion Pixel Font](https://github.com/TakWolf/fusion-pixel-font) 10 px monospaced, v2026.09.25 | SIL Open Font License 1.1 |
| `cjk-12-*.lbg` | Chinese, Japanese, Korean on 12-row badges | Fusion Pixel Font 12 px monospaced, v2026.09.25 | SIL Open Font License 1.1 |
| `cjk-16-*.lbg` | Chinese, Japanese, Korean on 16-row badges | [GNU Unifont](https://unifoundry.com/unifont/) 18.0.01 (`unifont_jp` glyphs for Japanese) | SIL Open Font License 1.1, or GPL-2.0-or-later with the GNU font embedding exception |

The full license texts are in [`licenses/`](licenses/):

- `fusion-pixel-font-OFL.txt`: Fusion Pixel Font, Copyright (c) 2022, TakWolf.
- `fusion-pixel-font-*-OFL.txt` / `-LICENSE.txt`: the fonts Fusion Pixel Font
  is built from (Ark Pixel Font, Cubic 11, Galmuri, Boutique Bitmap 9x9), all
  under the SIL Open Font License 1.1.
- `unifont-OFL-1.1.txt` and `unifont-COPYING-GPL-2.0.txt`: GNU Unifont, by
  Roman Czyborra, Paul Hardy and contributors. Its `jiskan16` Japanese glyphs
  are public domain.

## The fonts in the Font list

| Listed as | Made of |
|---|---|
| **IBM VGA + Fusion Pixel** / **IBM VGA + Unifont** (default) | `ascii-8x*.bin` for plain letters, digits and punctuation; `cjk-*.lbg` for everything else (Fusion Pixel at 11 and 12 rows, Unifont at 16) |
| **Fusion Pixel Font** | `ascii-fusion-10.lbg` / `ascii-fusion-12.lbg` (its own letters) and `cjk-10-*` / `cjk-12-*` (12 px, centered, on 16-row badges) |
| **GNU Unifont** (16-row badges) | `ascii-unifont-16.lbg` (its own letters) and `cjk-16-*` |

The `cjk-*` tables come in four regional styles (below); the right one is
picked for each character, so they aren't separate fonts. Fusion Pixel Font
is itself built from Ark Pixel Font, Cubic 11, Galmuri and Boutique Bitmap
9x9 (their licenses are in `licenses/`); they're part of it, not separate
fonts. Characters a font doesn't have come from the default font.

## Regular fonts that ship with the app

The **Font** list shows a *Built-in* group (the default font above, and any
regular fonts that ship with the app) and then the computer's own fonts.
None ship yet. To add one, put its TrueType or OpenType file in `app/`, list
it in `APP_FONTS` in `crates/badge-render/src/system_font.rs`, and add its
license here and in `crates/badge-render/src/licenses.rs`. It then appears
under *Built-in* on every computer.

## Regional styles

Chinese characters look slightly different in Simplified Chinese, Traditional
Chinese, Japanese and Korean. As in the original software, each character is
drawn in the style of the first encoding that contains it (GBK, then Big5,
Shift-JIS, EUC-KR); kana always use the Japanese style and Hangul the Korean
one. The `zh-hans` table holds the full font; the `zh-hant`, `ja` and `ko`
tables only hold the glyphs that differ from it. Unifont has no separate
Traditional Chinese or Korean variants, so those tables are empty at 16 px.

The vertical ("Fall") glyphs are the normal glyphs rotated 90 degrees
counterclockwise, which is exactly what the vendor's Simplified Chinese
vertical libraries contain.

## Rebuilding

```bash
python3 packaging/fonts/build_fonts.py
```

This downloads the source fonts into `target/fonts-src/`, regenerates the
`cjk-*.lbg` tables (zlib-compressed; the format is described at the top of the
script) and copies the license files. Update the version numbers in the script
to move to newer releases.

## ASCII fonts

The ASCII fonts are the ones from the vendor software. Comparing them glyph by
glyph with the Linux console fonts identifies them: `ascii-8x16.bin` matches the
IBM VGA 8x16 font in 91 of 94 printable characters, and the 11- and 12-row
fonts are the IBM VGA/EGA 8x14 font with rows trimmed (53 of 94 characters
unchanged, the rest shortened).

IBM never released these ROM fonts under a license. In the United States,
bitmap fonts are not protected by copyright: typefaces are excluded by
[37 CFR 202.1(e)](https://www.ecfr.gov/current/title-37/chapter-II/subchapter-A/part-202/section-202.1),
and the Copyright Office treats a bitmap font as a representation of a typeface
(see the [comp.fonts FAQ](https://tug.ctan.org/help/comp-fonts-FAQ/comp.fonts_FAQ:_General_Info_(2_6))).
That is why these glyphs are widely redistributed, for example as the VGA
console fonts of the Linux kernel and the `kbd` package. Other countries can
treat font designs differently; if that matters for your use, swap them for an
openly licensed 8-pixel font.

## Vendor fonts that are not included

The vendor software also ships Chinese, Japanese and Korean bitmap libraries
(`HZK11`…`HZK16V`, `TWK*`, `JPK*`, `KRK*`). They are not part of this project:

- They have no license or author information; the files are bare glyph data
  with no header (`file` reports only "data").
- They cover the complete GBK, Big5, Shift-JIS and EUC-KR grids (about 22,000
  glyphs each), far more than the classic DOS-era `HZK16`. They don't match the
  X11 bitmap fonts (`gb16st`, `gb16fs`, `jiskan16`, `hanglg16`, `hanglm16`) or
  the classic UCDOS `HZK16`/`HZK12` files.
- Their styles (Song for Simplified Chinese, matching the classic HZK design;
  Ming for Traditional Chinese; Mincho for Japanese; a sans Gothic for Korean),
  and the original software's default font name 宋体 (SimSun), point to them
  being renders of Windows system fonts such as SimSun, MingLiU, MS Mincho and
  Gulim.
- Microsoft's [font redistribution FAQ](https://learn.microsoft.com/typography/fonts/font-faq)
  says Windows fonts may not be converted to bitmap formats and shipped with
  an app. The classic HZK-style data is likewise described by Chinese
  font-project maintainers as not public domain
  ([discussion](https://timothyqiu.itch.io/vonwaon-bitmap/comments)).

If you own a copy of the vendor software, you can still use its libraries for
the exact original look: set `LED_BADGE_STUDIO_FONTLIB` to the folder containing them
(for example the `fontlib` folder of an installed BMP Badge), and the app uses
them in place of the built-in CJK fonts. Step-by-step instructions are in the
[fonts guide](../../docs/fonts.md#using-the-vendor-fonts-led_badge_studio_fontlib). Don't
redistribute them.
