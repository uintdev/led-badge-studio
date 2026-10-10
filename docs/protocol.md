# The badge protocol

How LED Badge Studio talks to `0416:5020` badges: what is sent, in what
order, how fast, and what was learned (often the hard way) about doing it
safely. Read this before changing anything in
[`badge-core/src/protocol.rs`](../crates/badge-core/src/protocol.rs) (the
bytes) or [`badge-io/src/lib.rs`](../crates/badge-io/src/lib.rs) (sending
them).

Where it comes from:

- **The byte layout** was taken from the vendor's .NET app, `BmpBadge.exe`
  (V9.18): `assembleUSBMonoArray`, `assembleUSBColorArray`,
  `assembleCOMMonoArray` and `assembleCOMColorArray`. The encoders are
  byte-for-byte ports; the tests in `protocol.rs` pin the layout down.
- **The timing and safety rules** (pacing, the padding report, holding the
  device open, the Windows notes) come from testing on a real badge: the
  monochrome 11 × 44 badge with a WCH CH546 controller. The vendor app gets
  away without most of them only because it is slow.

Other badges with the same USB ID (see the table in the
[README](../README.md#supported-badges)) are expected to speak the same
protocol, but haven't been tested. The serial (COM port) protocol is
implemented from the vendor code and hasn't been tested on hardware.

## Overview

- **One-way.** The app only ever writes. The badge never answers with
  anything useful: no acknowledgement, no status, no way to read back what
  it holds. That's why the app keeps its own record of what it last sent
  (the "last sent" banner) instead of asking the badge.
- **One upload replaces everything.** Every upload carries all eight message
  slots and the settings, and the badge shows the new messages once it has
  all of them. (It erases its memory after the header: the serial protocol
  pauses there for it.)
- **An upload is a 64-byte header followed by pixels.** The header says how
  long each message is; the badge reads exactly that much pixel data, and
  ignores anything after it.
- **No checksums, no retransmission.** A byte lost on the way leaves the
  badge waiting for data that never comes (see
  [What goes wrong](#what-goes-wrong)).

## The upload stream (USB, and color serial badges)

The stream is the 4 bytes `wang`, a 60-byte header body, then the pixel
payload. Multi-byte numbers are big-endian. Offsets below count from the
start of the stream.

### Header (bytes 0–63)

| Offset | Size | Contents |
|---|---|---|
| 0 | 4 | `77 61 6E 67`: the ASCII magic `wang` |
| 4 | 1 | Always 0 |
| 5 | 1 | Brightness in the high nibble: `0x00` 100%, `0x10` 75%, `0x20` 50%, `0x30` 25% |
| 6 | 1 | Flash: bit *i* set = message *i* flashes on and off |
| 7 | 1 | Marquee: bit *i* set = message *i* has the animated border |
| 8–15 | 8 × 1 | Per message (slot 0 first): mode in the low nibble, speed − 1 in the high nibble |
| 16–31 | 8 × 2 | Per message: its length in **byte-columns** (8 pixels wide each), big-endian; 0 for a message that isn't sent |
| 32–33 | 2 | 0 |
| 34 | 1 | Address mode: 0 none, 1 store an address, 2 send only to an address (see [Addressing](#addressing)) |
| 35–36 | 2 | The address, big-endian (0–999 in the vendor app) |
| 37 | 1 | 0 |
| 38 | 1 | The year **divided by 100** (20 for 2026). The vendor app does this; kept as it is |
| 39–43 | 5 | Month, day, hour, minute, second of the upload |
| 44 | 1 | The vendor's "security code": 0 (see [development notes](development.md#how-the-protocol-was-worked-out)) |
| 45–63 | 19 | 0 |

**Brightness**: the badge only reads the low two bits of the nibble (tested:
values 4–15 repeat 0–3), so the four levels are all there is.

**Modes** (the low nibble of bytes 8–15):

| Value | Mode | Value | Mode |
|---|---|---|---|
| 0 | Scroll left | 5 | Animation |
| 1 | Scroll right | 6 | Snow (piling) |
| 2 | Scroll up | 7 | Curtain (split) |
| 3 | Scroll down | 8 | Laser |
| 4 | Fixed | | |

**Speed**: 1 (slowest) to 8 (fastest) in the app is sent as 0–7. What each
speed means in milliseconds is in [Preview timing](preview-timing.md).

**Which messages are sent**: unticked messages, and empty ones, get length
0; the badge skips a message of length 0. The flash, marquee, mode and speed
bytes are filled in for all eight slots either way.

### Pixel payload (from byte 64)

The selected messages follow one another in slot order, each exactly as long
as the header says. Inside a message the pixels go **column by column**:

```
for each message (slot 0 to 7, with length > 0):
    for each byte-column (0 to length - 1):
        for each row (top to bottom):
            monochrome: 1 byte
            color:      3 bytes: blue, green, red
```

Each byte is **8 horizontal pixels** of one row, the leftmost in the most
significant bit: `0x80` is the first of the eight lit, `0x01` the last. A
message is padded with unlit pixels to a whole byte-column on the right.

On a monochrome badge a bit is 1 for a lit LED. A color badge gets one such
byte per color plane: an LED's color is the mix of the planes it's lit in,
which gives the 7 colors (red, green, blue, yellow, magenta, cyan, white).

So one byte-column takes as many bytes as the badge has rows (11, 12 or 16)
on a monochrome badge, and three times that on a color one.

### Memory limit

The badge holds **2560 byte-columns** (20,480 pixels wide) across all
messages together, whatever its height. The app enforces this the way the
vendor app does: messages are cut in slot order, so later messages are
shortened or dropped first, and the header lengths match what's actually
sent. The editor shows the total against this limit ("Badge memory").

## USB (HID)

The badge is a USB HID device, vendor `0x0416`, product `0x5020`. The app
finds it by that ID alone (that's all the vendor app checks).

- **Reports**: the stream is cut into 64-byte pieces, the last one padded
  with zeros. Each goes out as one HID **output report** with report ID 0,
  so every write is 65 bytes: `00` and then 64 bytes of the stream. The
  zeros padding the last report are harmless: the badge stops reading at
  the lengths in the header.
- **One extra report of zeros** follows the data. The badge only finishes
  storing a report when the next one arrives; without it, the last report
  was sometimes lost.
- **Nothing is read** from the device.

Sizes, as an example: a 40-pixel-wide message (about "Hello" in the default
font) on an 11-row badge is 5 byte-columns, so 64 + 5 × 11 = 119 bytes:
2 reports, plus the padding report. A full badge
(2560 byte-columns) is 441 reports on a monochrome 11-row badge, and 1,921 on
a color 16-row one.

### Pacing: how fast to send

The badge **accepts each report immediately**, but then takes **about
200–300 ms to store it**, and can only queue **about 7** reports. A report
that arrives while the queue is full is corrupted or dropped, and nothing
tells the sender. So the app paces the upload by time:

| Setting | Default | What it's for |
|---|---|---|
| Fast limit | 6 reports | An upload this short (padding included) fits in the queue, so it goes out quickly |
| Fast gap | 20 ms | Pause between reports of a short upload |
| Gap | 325 ms | Pause between reports of a longer upload |
| Padding report | on | The extra report of zeros after the data |
| Tail hold | 1200 ms | How long the device stays open after the last report |

Measured with an 82-report upload: 250 ms gaps fail, 300 ms and 350 ms work;
325 ms leaves some margin. A long upload therefore takes about a third of a
second per report: around 2.4 minutes for a full monochrome 11-row badge,
and around 10 minutes for a full color 16-row one. The app's estimate
("about N s") comes from the same numbers (`Pacing::estimate`).

**Keep the device open until the badge is done.** Closing it right after the
last write lost the last reports: the operating system stops talking to the
device, and what was still queued never got stored. The tail hold gives the
badge time to work through its queue.

For experiments, each setting can be overridden with an environment
variable: `LED_BADGE_STUDIO_FAST_LIMIT`,
`LED_BADGE_STUDIO_REPORT_GAP_MS`, `LED_BADGE_STUDIO_TAIL_HOLD_MS` and
`LED_BADGE_STUDIO_TRAILING_PAD` (0 turns the padding report off). For
example, to try a shorter gap from the command line:

```bash
LED_BADGE_STUDIO_REPORT_GAP_MS=300 badgectl send -p messages.badge
```

### Each system

The app uses [hidapi](https://github.com/ruabmbua/hidapi-rs) on every
system, with a different backend on each, and each has its own quirks.

**Linux** (hidraw, through udev):
- The badge is a `/dev/hidraw*` node, which ordinary users can't write to by
  default. The [udev rule](linux-badge-access.md) gives the logged-in user
  access. The app checks it can open the node for writing before sending,
  so it can say what's missing instead of failing.
- In the Flatpak, the app needs the `--device=all` permission to see the
  node at all.

**macOS** (IOKit):
- hidapi ties its device manager to the run loop of the thread that first
  lists devices. If that thread ends, the next listing crashes the app. All
  listing and opening therefore happens on one thread that lives as long as
  the app (`on_hid_thread`); writing to an open device can happen on any
  thread.

**Windows** (hidapi's own Rust backend, `windows-native`):
- **A write that finished at once reports 0 bytes.** The backend only counts
  bytes for writes that completed in the background. The app treats 0 as a
  complete write, not a short one.
- **Each write waits at most 1 second** for the badge. Paced uploads never
  get near that unless something else goes wrong (next point).
- **Listing HID devices interrupts an upload.** On Windows, listing asks
  every HID device for its name and serial number, which means talking to
  the badge. A badge asked that in the middle of an upload drops the upload:
  the current write times out (`WaitTimedOut`), Windows resets the badge,
  and for the next half second or so opening it fails with "A device
  attached to the system is not functioning" (os error 31) or "The system
  cannot find the file specified" (os error 2). (macOS and Linux list
  devices from what the system already knows, without asking the badge.)
  Two things guard against this:
  - The app doesn't list devices while it's sending (a lock in `badge-io`
    makes a listing wait for the send to end). Without it, plugging in any
    USB device during a send would interrupt it.
  - Other programs can still list HID devices at any time. An upload that
    fails with one of the errors above is started again from the beginning
    after a 1-second pause, up to 3 times. Starting again is safe: the
    reset leaves the badge ready for a new upload, unlike an upload that was
    cut short without one (see below).

### What goes wrong

- **A lost or corrupted report leaves the badge stuck.** It waits forever
  for the rest of the upload, keeps showing its old messages, and ignores
  every later upload until it's **power-cycled** (unplugged and plugged back
  in). Causes: sending too fast, closing the device too early, or unplugging
  mid-upload. The app tells you to unplug and replug if the badge keeps
  showing its old messages after a send.
- **Nothing reports success.** "Sent" means every report was written and the
  device was held open for the tail; whether the badge stored it all can
  only be seen on the badge. When an upload completes, the badge blanks and
  restarts its display straight away.
- **Interrupted uploads on Windows**: above.

### Safe-sending checklist

If you change the sending code, keep all of these:

1. Send the whole upload in one go, with a fresh header: there's no way to
   resume or patch part of one.
2. Pace reports: never more than the badge can queue (short uploads only),
   otherwise one per 325 ms.
3. Send the extra report of zeros after the data.
4. Keep the device open for the tail hold after the last write.
5. On Windows, don't list HID devices while sending, and only retry after
   an error that means the badge was reset.
6. Make the header lengths match the payload exactly (the encoder does
   this; don't patch the stream by hand).
7. Test long uploads (80 reports or more) on a real badge: short ones fit in
   the queue and hide pacing problems.

## Addressing

The header can carry an address (0–999), for setups with several badges:

- **Mode 0** (the default): every badge accepts the upload.
- **Mode 1, store an address**: the badge accepts the upload and remembers
  the address.
- **Mode 2, send to an address**: only a badge that has stored that address
  accepts the upload.

The app (the **All badges** menu in the toolbar) and `badgectl send
--write-address` / `--to-address` send
these the way the vendor app does. Whether a badge that doesn't match ignores
the upload cleanly hasn't been tested.

## Serial badges (COM port)

Older badges connect through a USB serial adapter and use the serial
protocol. The app sends it on any serial port it lists (`ttyUSB*`,
`ttyACM*`, `COM*`), with 8 data bits, no parity, and the DTR line on. The
packets go out one at a time, with a pause before each one and an extra pause
after the second (the header), while the badge erases its memory. After each
packet the app waits until it has left the port, for at most twice its time
on the wire plus a second: a device that doesn't take the data (one that
isn't a badge) ends the send with an error instead of keeping it waiting.

### Color serial badges: the `wang` stream

The same stream as over USB, at **115,200 baud with 2 stop bits**:

1. `wang` (4 bytes)
2. The 60-byte header body
3. The payload in 64-byte pieces (the last one not padded)

10 ms pause before each packet, and 1.4 s after the header.

### Monochrome serial badges: the older `hello` protocol

A different layout, at **1200 baud with 1 stop bit**, 30 ms before each
packet and 1 s after the header:

1. **`hello` and a brightness byte** (6 bytes): the brightness as 0–3, not
   shifted into a nibble.
2. **A 47-byte header**:

   | Offset | Contents |
   |---|---|
   | 1–8 | Per message: speed − 1 in bits 4–6, flash `0x08`, marquee `0x80`, and the mode in bits 0–2 when it's below 8 |
   | 9 and 43 | Address mode: `0x55` none, `0x33` store an address, `0xAA` send to an address |
   | 10 + 4*i* | Message *i*: where its pixels start (in byte-columns from the start of each row), big-endian, 2 bytes |
   | 12 + 4*i* | Message *i*: its length in byte-columns, big-endian, 2 bytes |
   | 42 | Bit *i* set = message *i* uses mode 8 (laser), which doesn't fit in 3 bits |
   | 44–45 | The address, big-endian |
   | others | 0 |

3. **One packet per row**, top to bottom, each holding that row of every
   message one after another (8 pixels per byte, leftmost in the top bit).
   So this protocol goes **row by row**, unlike the `wang` stream.
4. **A single 0 byte** to end the upload.

There's no pacing beyond the fixed pauses; the vendor app sends it the same
way, and none of it has been tried on a real serial badge yet.

## Testing changes

- `cargo test -p badge-core` checks the byte layout against hand-checked
  examples (the header of a sample project, the color plane order, the
  memory limit, report padding and both serial layouts). Change the
  expected bytes only when the protocol change is deliberate, and say so.
- For timing, send a long project to a real badge several times
  (`badgectl send -p …`), including on Windows. When reporting results, say
  which badge (see the README's table) and which system you used.
