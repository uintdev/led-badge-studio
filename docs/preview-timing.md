# Preview timing

The preview plays each message at the badge's own pace. The badge's firmware
can't be read, so the timing is a model fitted to measurements of a real
badge (the monochrome 11 x 44 one). The model lives in
[`crates/badge-render/src/preview.rs`](../crates/badge-render/src/preview.rs);
this page explains it and how to check it against another badge.

## The model

Everything the badge does moves in **steps**, and every step is a whole
number of one **timer unit** of 11.087 ms (`TICK_US` in `preview.rs`). The
speed setting picks how many units a step takes:

| Speed | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 |
|---|---|---|---|---|---|---|---|---|
| Units per step | 18 | 12 | 9 | 7 | 5 | 3 | 2 | 1 |
| Step (ms) | 199.6 | 133.0 | 99.8 | 77.6 | 55.4 | 33.3 | 22.2 | 11.1 |

What a step is depends on the mode:

| Mode | One step | Notes |
|---|---|---|
| Left, Right | The text moves one column | A pass is the message's width plus the display's |
| Up, Down | The page moves one row | Each page comes in, pauses, then leaves |
| Fixed | - | Each page shows for one pause |
| Animation | A quarter of a picture | Each picture is 4 steps. Pictures are the display width rounded up to whole bytes: 48 columns on the 44-wide badge, so columns 44-47 of every picture never show |
| Snow | One row falls | Rows fall one at a time, bottom row first, 2 steps apart; after the last page the display falls away the same way |
| Curtain | The opening widens one column on each side | After the last page, the display clears from the center outward at the same pace |
| Laser | One column is drawn | Each lit pixel of the column being drawn beams to the right edge; after the last page the columns are erased from the left, beaming back |

The **pause** on each page of the paged modes (Up, Down, Fixed, Snow,
Curtain, Laser) is 32 steps, so it gets shorter at higher speeds: 6.4 s at
speed 1, 1.77 s at speed 5, 0.35 s at speed 8. **Flash** turns the display
off and on every 8 steps, during pauses and while scrolling.

## What has been measured

- **The timer unit and the steps per speed**: `badgectl calibrate` in
  Animation mode at all 8 speeds, 30 taps each: 11086.9 µs ±1.5, with every
  speed within 0.15% of a whole number of units. It also agrees with a video
  of the laser mode at speed 5 (55.42 ms a step).
- **Animation pictures 48 columns wide**: checked on the badge with text
  running across the picture boundaries.
- **Curtain**: compared with a video at speed 7, including the closing after
  the last page.
- **The pause**: measured at speeds 5 and 7 only.
- **Scrolling**: not measured yet; assumed to use the same steps.

The frame rates published for other badges with the same USB ID (for
example in [pagong/led-name-badge](https://github.com/pagong/led-name-badge))
don't match this badge.

## Measuring a badge

```bash
badgectl calibrate
```

This runs all three measurements in turn: Animation (the timer unit), then
the pause, then scrolling. Each sends a test message, and you press Enter
every time the badge flashes; see
[Using the command line](command-line.md) for the details and options. The
results say where the badge and the preview differ. To correct the
preview, change `TICK_US`, the units per speed (`TICKS`) or the pause
(`HOLD_STEPS`) in `preview.rs` accordingly; its tests check the measured
values.

The preview redraws only when the picture changes, and advances by the real
time that has passed rather than by redraws, so it keeps the badge's pace
even when a redraw is slow.
