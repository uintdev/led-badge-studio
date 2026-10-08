#!/usr/bin/env python3
"""Builds the CJK glyph tables embedded in LED Badge Studio from open-licensed fonts.

  python3 packaging/fonts/build_fonts.py [--cache DIR]

Sources (downloaded into the cache directory, default target/fonts-src/):
  * Fusion Pixel Font 10px and 12px, monospaced BDF (SIL OFL 1.1)
    https://github.com/TakWolf/fusion-pixel-font
  * GNU Unifont 16px, standard and Japanese variants (SIL OFL 1.1, or
    GPL-2.0+ with the GNU font embedding exception)
    https://unifoundry.com/unifont/

Outputs assets/fonts/cjk-<size>-<script>.lbg (zlib-compressed glyph tables)
and copies each font's license into assets/fonts/licenses/.

Table format (before zlib compression, all integers little-endian):
  b"LBGF", u8 version=1, u8 cell (glyph box is cell x cell pixels),
  u16 reserved, u32 count,
  count x u32 code points (ascending),
  count x u8 advance,
  count x cell rows of u16 (bit 15 = leftmost pixel).
"""

import argparse
import gzip
import io
import os
import struct
import sys
import urllib.request
import zipfile
import zlib

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
OUT = os.path.join(ROOT, "assets", "fonts")

FUSION_VERSION = "2026.09.25"
UNIFONT_VERSION = "18.0.01"

# Script codes used by the app, and the matching Fusion Pixel variants.
SCRIPTS = {"zh-hans": "zh_hans", "zh-hant": "zh_hant", "ja": "ja", "ko": "ko"}


def fetch(url, dest):
    if not os.path.exists(dest) or os.path.getsize(dest) == 0:
        print("downloading", url)
        with urllib.request.urlopen(url) as r, open(dest + ".part", "wb") as f:
            f.write(r.read())
        os.replace(dest + ".part", dest)
    return dest


def keep(cp):
    """Code points worth embedding: everything except ASCII (the app has
    its own ASCII fonts) and control/private-use ranges."""
    return cp >= 0x80 and not (0xE000 <= cp <= 0xF8FF) and cp <= 0xFFFF


def ascii_only(cp):
    """Printable ASCII: each font's own letters, digits and punctuation, so
    Fusion Pixel and Unifont can also be used on their own."""
    return 0x20 <= cp <= 0x7E


def parse_bdf(text, cell, wanted=keep):
    """Returns {code point: (advance, rows)} with glyphs placed in a
    cell x cell box (top row = font ascent line)."""
    ascent = descent = None
    glyphs = {}
    lines = iter(text.splitlines())
    for line in lines:
        if line.startswith("FONT_ASCENT "):
            ascent = int(line.split()[1])
        elif line.startswith("FONT_DESCENT "):
            descent = int(line.split()[1])
        elif line.startswith("STARTCHAR"):
            enc = dw = None
            bbx = None
            for line in lines:
                if line.startswith("ENCODING "):
                    enc = int(line.split()[1])
                elif line.startswith("DWIDTH "):
                    dw = int(line.split()[1])
                elif line.startswith("BBX "):
                    bbx = tuple(int(x) for x in line.split()[1:5])
                elif line == "BITMAP":
                    w, h, xo, yo = bbx
                    rows = []
                    for _ in range(h):
                        hexrow = next(lines).strip()
                        bits = int(hexrow, 16) if hexrow else 0
                        rows.append((bits, len(hexrow) * 4))
                    next(lines)  # ENDCHAR
                    if enc is None or enc < 0 or not wanted(enc):
                        break
                    box = [0] * cell
                    # Glyph top relative to the cell top.
                    top = ascent - (yo + h)
                    for i, (bits, nbits) in enumerate(rows):
                        y = top + i
                        if 0 <= y < cell:
                            v = bits >> (nbits - w) if nbits >= w else bits << (w - nbits)
                            v = (v << (16 - w - xo)) if 16 - w - xo >= 0 else v >> (w + xo - 16)
                            box[y] = v & 0xFFFF
                    # Blank glyphs only matter for ASCII (the space).
                    if any(box) or wanted is ascii_only:
                        glyphs[enc] = (min(dw or cell, cell), box)
                    break
    assert ascent is not None and descent is not None
    return glyphs


def parse_hex(data, wanted=keep):
    glyphs = {}
    for line in data.decode().splitlines():
        cp_s, bits = line.split(":")
        cp = int(cp_s, 16)
        if not wanted(cp):
            continue
        if len(bits) == 64:  # 16x16
            rows = [int(bits[i:i + 4], 16) for i in range(0, 64, 4)]
            adv = 16
        else:  # 8x16
            rows = [int(bits[i:i + 2], 16) << 8 for i in range(0, 32, 2)]
            adv = 8
        if any(rows) or wanted is ascii_only:
            glyphs[cp] = (adv, rows)
    return glyphs


def write_table(path, cell, glyphs):
    cps = sorted(glyphs)
    body = io.BytesIO()
    body.write(b"LBGF" + struct.pack("<BBHI", 1, cell, 0, len(cps)))
    body.write(struct.pack(f"<{len(cps)}I", *cps))
    body.write(bytes(glyphs[c][0] for c in cps))
    for c in cps:
        body.write(struct.pack(f"<{cell}H", *glyphs[c][1]))
    data = zlib.compress(body.getvalue(), 9)
    with open(path, "wb") as f:
        f.write(data)
    return len(cps), len(data)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cache", default=os.path.join(ROOT, "target", "fonts-src"))
    args = ap.parse_args()
    os.makedirs(args.cache, exist_ok=True)
    os.makedirs(os.path.join(OUT, "licenses"), exist_ok=True)

    # Fusion Pixel Font: 10px (11-row badges) and 12px (12-row badges).
    for size in (10, 12):
        name = f"fusion-pixel-font-{size}px-monospaced-bdf-v{FUSION_VERSION}.zip"
        z = zipfile.ZipFile(fetch(
            f"https://github.com/TakWolf/fusion-pixel-font/releases/download/{FUSION_VERSION}/{name}",
            os.path.join(args.cache, name)))
        base = None
        for script, variant in SCRIPTS.items():
            glyphs = parse_bdf(z.read(f"fusion-pixel-{size}px-monospaced-{variant}.bdf").decode("utf-8"), size)
            if base is None:
                base = glyphs
                table = glyphs
            else:
                # Store only glyphs that differ from the Simplified Chinese table.
                table = {c: g for c, g in glyphs.items() if base.get(c) != g}
            n, b = write_table(os.path.join(OUT, f"cjk-{size}-{script}.lbg"), size, table)
            print(f"cjk-{size}-{script}: {n} glyphs, {b // 1024} KiB")
        # Fusion Pixel's own letters, digits and punctuation (the default
        # font uses the IBM VGA ones instead).
        latin = parse_bdf(z.read(f"fusion-pixel-{size}px-monospaced-zh_hans.bdf").decode("utf-8"), size, ascii_only)
        n, b = write_table(os.path.join(OUT, f"ascii-fusion-{size}.lbg"), size, latin)
        print(f"ascii-fusion-{size}: {n} glyphs, {b} B")
        if size == 12:
            for member in z.namelist():
                if member.endswith((".txt",)) and ("OFL" in member or "LICENSE" in member):
                    dest = "fusion-pixel-font-OFL.txt" if member == "OFL.txt" else \
                        "fusion-pixel-font-" + member.split("/")[1] + "-" + os.path.basename(member)
                    with open(os.path.join(OUT, "licenses", dest), "wb") as f:
                        f.write(z.read(member))
        if size == 10:
            for member in z.namelist():
                if member.startswith("LICENSES/") and member.endswith(".txt"):
                    dest = "fusion-pixel-font-" + member.split("/")[1] + "-" + os.path.basename(member)
                    with open(os.path.join(OUT, "licenses", dest), "wb") as f:
                        f.write(z.read(member))

    # GNU Unifont: 16px (16-row badges). Japanese uses unifont_jp glyphs.
    uni = parse_hex(gzip.decompress(open(fetch(
        f"https://unifoundry.com/pub/unifont/unifont-{UNIFONT_VERSION}/font-builds/unifont-{UNIFONT_VERSION}.hex.gz",
        os.path.join(args.cache, f"unifont-{UNIFONT_VERSION}.hex.gz")), "rb").read()))
    uni_jp = parse_hex(gzip.decompress(open(fetch(
        f"https://unifoundry.com/pub/unifont/unifont-{UNIFONT_VERSION}/font-builds/unifont_jp-{UNIFONT_VERSION}.hex.gz",
        os.path.join(args.cache, f"unifont_jp-{UNIFONT_VERSION}.hex.gz")), "rb").read()))
    uni_ascii = parse_hex(gzip.decompress(open(os.path.join(args.cache, f"unifont-{UNIFONT_VERSION}.hex.gz"), "rb").read()),
                          ascii_only)
    n, b = write_table(os.path.join(OUT, "ascii-unifont-16.lbg"), 16, uni_ascii)
    print(f"ascii-unifont-16: {n} glyphs, {b} B")
    n, b = write_table(os.path.join(OUT, "cjk-16-zh-hans.lbg"), 16, uni)
    print(f"cjk-16-zh-hans (Unifont): {n} glyphs, {b // 1024} KiB")
    for script in ("zh-hant", "ko"):
        write_table(os.path.join(OUT, f"cjk-16-{script}.lbg"), 16, {})
    n, b = write_table(os.path.join(OUT, "cjk-16-ja.lbg"), 16,
                       {c: g for c, g in uni_jp.items() if uni.get(c) != g})
    print(f"cjk-16-ja (Unifont JP differences): {n} glyphs, {b // 1024} KiB")
    tarball = fetch(f"https://unifoundry.com/pub/unifont/unifont-{UNIFONT_VERSION}/unifont-{UNIFONT_VERSION}.tar.gz",
                    os.path.join(args.cache, f"unifont-{UNIFONT_VERSION}.tar.gz"))
    import tarfile
    with tarfile.open(tarball) as t:
        for member, dest in (("OFL-1.1.txt", "unifont-OFL-1.1.txt"), ("COPYING", "unifont-COPYING-GPL-2.0.txt")):
            data = t.extractfile(f"unifont-{UNIFONT_VERSION}/{member}").read()
            with open(os.path.join(OUT, "licenses", dest), "wb") as f:
                f.write(data)
    print("done")


if __name__ == "__main__":
    sys.exit(main())
