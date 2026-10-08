//! `badgectl calibrate`: measures the badge's timing by tapping along.
//!
//! Each test message lights the whole display once a cycle and the user
//! presses Enter at every flash. A line fitted through the taps, each given
//! its cycle number so missed flashes don't matter, averages out reaction
//! time and gives the cycle length to about 1%.

use std::io::{BufRead, Write};
use std::time::Instant;

use anyhow::Result;
use badge_core::{AddressMode, CardType, Framebuffer, LedColor, Message, Mode, Project, Source};
use badge_render::preview::{self, TICK_US};

/// Aimed-for cycle length: comfortable to tap along to, quick to repeat.
const TARGET_CYCLE_US: u64 = 3_000_000;
pub const DEFAULT_TAPS: usize = 15;
/// Fewer for the pause: its cycles are long and only whole steps matter.
pub const DEFAULT_PAUSE_TAPS: usize = 8;
const MIN_TAPS: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fit {
    /// Seconds per cycle.
    pub period: f64,
    /// Root-mean-square distance of the taps from the fitted line, seconds.
    pub spread: f64,
    /// Cycles between the first and last tap used.
    pub cycles: i64,
    /// Standard error of `period`, seconds.
    pub sigma: f64,
    /// Taps dropped as too far off.
    pub dropped: usize,
}

/// Fits tap times (seconds, ascending) to one tap per cycle. Each tap gets
/// its cycle number from the typical gap; double taps are dropped.
pub fn fit(taps: &[f64]) -> Option<Fit> {
    if taps.len() < 3 {
        return None;
    }
    let mut gaps: Vec<f64> = taps.windows(2).map(|w| w[1] - w[0]).filter(|g| *g > 0.).collect();
    gaps.sort_by(f64::total_cmp);
    let typical = *gaps.get(gaps.len() / 2)?;
    if typical <= 0. {
        return None;
    }
    let mut points: Vec<(f64, f64)> = Vec::new();
    for &t in taps {
        let n = ((t - taps[0]) / typical).round();
        if points.last().is_none_or(|&(last, _)| n > last) {
            points.push((n, t));
        }
    }
    // Drop taps far off the line (over 3x the typical distance and a tenth of
    // a cycle) and fit again.
    let first = line(&points)?;
    let mut off: Vec<f64> = points.iter().map(|p| (p.1 - first.at(p.0)).abs()).collect();
    off.sort_by(f64::total_cmp);
    let limit = (3. * off[off.len() / 2] * 1.4826).max(0.1 * typical);
    let kept: Vec<(f64, f64)> = points
        .iter()
        .copied()
        .filter(|p| (p.1 - first.at(p.0)).abs() <= limit)
        .collect();
    let l = line(&kept)?;
    Some(Fit {
        period: l.slope,
        spread: l.rms,
        cycles: kept.last()?.0 as i64 - kept[0].0 as i64,
        sigma: l.slope_sigma,
        dropped: points.len() - kept.len(),
    })
}

/// A least-squares line through (cycle number, time) points.
struct Line {
    slope: f64,
    mean_n: f64,
    mean_t: f64,
    rms: f64,
    /// Standard error of the slope.
    slope_sigma: f64,
}

impl Line {
    fn at(&self, n: f64) -> f64 {
        self.mean_t + self.slope * (n - self.mean_n)
    }
}

fn line(points: &[(f64, f64)]) -> Option<Line> {
    if points.len() < 3 {
        return None;
    }
    let k = points.len() as f64;
    let (mn, mt) = (
        points.iter().map(|p| p.0).sum::<f64>() / k,
        points.iter().map(|p| p.1).sum::<f64>() / k,
    );
    let sxx: f64 = points.iter().map(|p| (p.0 - mn).powi(2)).sum();
    let sxy: f64 = points.iter().map(|p| (p.0 - mn) * (p.1 - mt)).sum();
    if sxx <= 0. {
        return None;
    }
    let slope = sxy / sxx;
    let sse: f64 = points.iter().map(|p| (p.1 - (mt + slope * (p.0 - mn))).powi(2)).sum();
    Some(Line {
        slope,
        mean_n: mn,
        mean_t: mt,
        rms: (sse / k).sqrt(),
        slope_sigma: (sse / (k - 2.).max(1.) / sxx).sqrt(),
    })
}

/// What is measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Time per Animation picture, and the timer unit.
    Animation,
    /// The pause on each page (Fixed mode, and between pages of the other
    /// paged modes).
    Pause,
    /// Time per scroll step and steps per pass (Left mode).
    Scroll,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Animation => "Animation",
            Kind::Pause => "Pause",
            Kind::Scroll => "Scroll",
        }
    }
}

fn step_us(speed: u8) -> u64 {
    preview::units_per_step(speed) * TICK_US
}

/// Pictures per cycle (Animation) or message width (Scroll) for about 3 s a
/// cycle.
fn size_for(kind: Kind, card: CardType, speed: u8) -> usize {
    match kind {
        Kind::Animation => {
            let est = preview::animation_frame_us(speed).max(1);
            ((TARGET_CYCLE_US as f64 / est as f64).round() as usize).clamp(2, 60)
        }
        Kind::Scroll => {
            let steps = (TARGET_CYCLE_US / step_us(speed)) as usize;
            steps.saturating_sub(preview::display_width(card)).max(8)
        }
        Kind::Pause => 2,
    }
}

fn test_project(kind: Kind, card: CardType, speed: u8, size: usize) -> Project {
    let width = preview::display_width(card);
    let h = card.height();
    let mut fb = Framebuffer::new(h);
    let light = |fb: &mut Framebuffer, cols: std::ops::Range<usize>| {
        for x in cols {
            for y in 0..h {
                fb.set(x, y, LedColor::RED);
            }
        }
    };
    let mode = match kind {
        Kind::Animation => {
            // One LED in every other picture, so the badge plays them all.
            let fw = preview::animation_frame_width(width);
            light(&mut fb, 0..width);
            for k in 1..size {
                fb.set(k * fw + (k % width), h - 1, LedColor::RED);
            }
            Mode::Animation
        }
        Kind::Pause => {
            // The last LED makes the message exactly two pages.
            light(&mut fb, 0..width);
            fb.set(2 * width - 1, h - 1, LedColor::RED);
            Mode::Fixed
        }
        Kind::Scroll => {
            // A bar, then dark up to one LED marking the end.
            light(&mut fb, 0..4);
            fb.set(size - 1, h - 1, LedColor::RED);
            Mode::Left
        }
    };
    let mut project = Project::new(card);
    for m in &mut project.messages {
        m.selected = false;
    }
    project.messages[0] = Message {
        selected: true,
        text: String::new(),
        source: Source::Bitmap,
        bitmap: fb,
        mode,
        speed,
        flash: false,
        marquee: false,
        ..project.messages[0].clone()
    };
    project
}

/// Steps per cycle of the test message, as the preview models it.
fn model_steps(project: &Project) -> usize {
    let m = &project.messages[0];
    let params = preview::PreviewParams {
        mode: m.mode,
        flash: m.flash,
        marquee: m.marquee,
        width: preview::display_width(project.card),
        speed: m.speed,
    };
    preview::cycle_len(&m.bitmap, &params)
}

struct Measured {
    speed: u8,
    cycle_us: f64,
    fit: Fit,
    taps: usize,
    /// Pictures per cycle (Animation).
    frames: usize,
    model_steps: usize,
}

impl Measured {
    /// Measured cycle length in preview steps.
    fn steps(&self) -> f64 {
        self.cycle_us / step_us(self.speed) as f64
    }

    /// Standard error of `steps`.
    fn steps_sigma(&self) -> f64 {
        self.fit.sigma * 1e6 / step_us(self.speed) as f64
    }
}

pub fn run(
    device: &badge_io::DeviceInfo,
    card: CardType,
    kind: Kind,
    speeds: &[u8],
    size: Option<usize>,
    taps_wanted: usize,
) -> Result<Done> {
    println!(
        "Calibrating the {} timing of {} ({}), with badgectl {} (preview timer unit {TICK_US} µs).",
        kind.label(),
        device.label,
        card.label(),
        env!("CARGO_PKG_VERSION")
    );
    match kind {
        Kind::Animation => println!("For each speed: the badge gets a test animation with one fully lit picture,"),
        Kind::Pause => println!("For each speed: the badge gets a Fixed message of two pages, one fully lit,"),
        Kind::Scroll => {
            println!("For each speed: the badge gets a message with a lit bar scrolling in from the right,")
        }
    }
    match kind {
        Kind::Scroll => println!("the flash: press Enter the moment it appears at the right edge."),
        _ => println!("the flash. Start it (press the badge's button, or unplug it), then press"),
    }
    println!("Enter every time it flashes. A missed flash doesn't matter.");
    println!("After {taps_wanted} taps it moves on. Instead of tapping, type d (done, needs {MIN_TAPS}),");
    println!("r (send again), s (skip this speed) or q (quit), then Enter.");
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    let mut results: Vec<Measured> = Vec::new();
    let mut done = Done::Finished;
    'speeds: for &speed in speeds {
        let n = match kind {
            Kind::Pause => 2,
            _ => size.unwrap_or_else(|| size_for(kind, card, speed)),
        };
        let project = test_project(kind, card, speed, n);
        let steps = model_steps(&project);
        loop {
            println!();
            let what = match kind {
                Kind::Animation => format!("{n} pictures a cycle"),
                Kind::Pause => "two pages".to_string(),
                Kind::Scroll => format!("{steps} steps a pass"),
            };
            println!(
                "== {}, speed {speed}: {what}, about {:.1} s a cycle",
                kind.label(),
                (steps as u64 * step_us(speed)) as f64 / 1e6
            );
            let mut done_pct = 0;
            badge_io::send(&project, AddressMode::Ignore, device, &mut |done, total| {
                let pct = done * 100 / total.max(1);
                if pct / 25 != done_pct / 25 {
                    eprint!("\rSending… {pct:3}%");
                    done_pct = pct;
                }
                true
            })?;
            eprintln!("\rSent.          ");
            println!("Start the badge, then press Enter at every flash.");
            let mut taps: Vec<Instant> = Vec::new();
            let action = loop {
                print!("  tap {:2}> ", taps.len() + 1);
                std::io::stdout().flush().ok();
                let Some(line) = lines.next() else { break "q" };
                let now = Instant::now();
                match line?.trim() {
                    "" => {
                        taps.push(now);
                        if taps.len() >= taps_wanted {
                            break "d";
                        }
                    }
                    "d" if taps.len() >= MIN_TAPS => break "d",
                    "d" => println!("  At least {MIN_TAPS} taps first."),
                    "r" => break "r",
                    "s" => break "s",
                    "q" => break "q",
                    other => println!("  Unknown input {other:?}: Enter, d, r, s or q."),
                }
            };
            match action {
                "r" => continue,
                "s" => continue 'speeds,
                "q" if taps.len() < MIN_TAPS => {
                    done = Done::Quit;
                    break 'speeds;
                }
                _ => {}
            }
            let start = taps[0];
            let secs: Vec<f64> = taps.iter().map(|t| t.duration_since(start).as_secs_f64()).collect();
            match fit(&secs) {
                Some(f) => {
                    let r = Measured {
                        speed,
                        cycle_us: f.period * 1e6,
                        fit: f,
                        taps: secs.len(),
                        frames: n,
                        model_steps: steps,
                    };
                    let detail = match kind {
                        Kind::Animation => format!(
                            "{:.2} ms a picture (±{:.2}), {:.2} fps",
                            r.cycle_us / n as f64 / 1000.,
                            f.sigma * 1e3 / n as f64,
                            n as f64 * 1e6 / r.cycle_us
                        ),
                        Kind::Pause => format!(
                            "{:.2} s a page (±{:.2}), {:.1} steps (the preview: {})",
                            r.cycle_us / 2e6,
                            f.sigma / 2.,
                            r.steps() / 2.,
                            steps / 2
                        ),
                        Kind::Scroll => format!(
                            "{:.2} s a pass (±{:.2}), {:.1} steps (the preview: {steps})",
                            r.cycle_us / 1e6,
                            f.sigma,
                            r.steps()
                        ),
                    };
                    println!(
                        "  Speed {speed}: {detail}, from {} taps over {} cycles ({} dropped); taps within ±{:.0} ms",
                        secs.len(),
                        f.cycles,
                        f.dropped,
                        f.spread * 1000.
                    );
                    results.push(r);
                }
                None => println!("  Not enough taps to measure speed {speed}; skipped."),
            }
            if action == "q" {
                done = Done::Quit;
                break 'speeds;
            }
            break;
        }
    }
    if results.is_empty() {
        println!("\nNothing measured.");
    } else {
        match kind {
            Kind::Animation => report_animation(&results),
            Kind::Pause | Kind::Scroll => report_steps(kind, &results),
        }
    }
    Ok(done)
}

/// Whether a measurement ran to the end or the user quit.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Done {
    Finished,
    Quit,
}

fn report_animation(results: &[Measured]) {
    println!("\nResults, Animation mode (time per picture):");
    println!("  speed      ms      ±ms     fps   preview ms  taps  spread");
    for r in results {
        let ours = preview::animation_frame_us(r.speed) as f64 / 1000.;
        let frame_us = r.cycle_us / r.frames as f64;
        println!(
            "  {:>5} {:>8.2} {:>8.2} {:>7.2} {:>11.2} {:>5} {:>5.0} ms",
            r.speed,
            frame_us / 1000.,
            r.fit.sigma * 1e3 / r.frames as f64,
            1e6 / frame_us,
            ours,
            r.taps,
            r.fit.spread * 1000.
        );
    }
    // Every picture is a whole number of timer units, so all speeds measure
    // the same unit; weight each by its precision.
    let (mut sum_w, mut sum_wu) = (0., 0.);
    for r in results {
        let units = (preview::ANIMATION_STEPS_PER_PICTURE as u64 * preview::units_per_step(r.speed)) as f64;
        let unit = r.cycle_us / r.frames as f64 / units;
        let sigma = (r.fit.sigma * 1e6 / r.frames as f64 / units).max(0.01);
        let w = 1. / (sigma * sigma);
        sum_w += w;
        sum_wu += w * unit;
    }
    let unit = sum_wu / sum_w;
    let unit_sigma = (1. / sum_w).sqrt();
    println!(
        "\nThe badge's timer unit, from all speeds: {unit:.1} µs (±{unit_sigma:.1}); the preview uses {TICK_US} µs ({:+.2}%).",
        (unit / TICK_US as f64 - 1.) * 100.
    );
    println!("For the preview (crates/badge-render/src/preview.rs):");
    println!("pub const TICK_US: u64 = {};", unit.round() as u64);
}

/// Pause or scroll results in preview steps (whose length the Animation
/// measurement established).
fn report_steps(kind: Kind, results: &[Measured]) {
    let (title, per) = match kind {
        Kind::Pause => ("pause mode (the pause on each page)", 2.),
        _ => ("scroll mode (one pass of a Left scroll)", 1.),
    };
    println!("\nResults, {title}, in the preview's steps (units per step x {TICK_US} µs):");
    println!("  speed   seconds   steps     ±   preview  taps  spread");
    let mut differ = Vec::new();
    for r in results {
        let steps = r.steps() / per;
        let sigma = r.steps_sigma() / per;
        let model = r.model_steps as f64 / per;
        println!(
            "  {:>5} {:>9.3} {:>7.1} {:>5.1} {:>9.0} {:>5} {:>5.0} ms",
            r.speed,
            r.cycle_us / per / 1e6,
            steps,
            sigma,
            model,
            r.taps,
            r.fit.spread * 1000.
        );
        if (steps - model).abs() >= 0.5 && (steps - model).abs() > 3. * sigma {
            differ.push(r.speed);
        }
    }
    if differ.is_empty() {
        println!("\nThe preview matches the badge at every speed measured.");
    } else {
        let list: Vec<String> = differ.iter().map(u8::to_string).collect();
        println!("\nThe preview differs from the badge at speed {}.", list.join(", "));
        println!("Paste these results into an issue (or the chat) so the preview can be corrected.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fit_finds_the_cycle_through_jitter_and_missed_taps() {
        // 2.8 s cycles, taps up to 60 ms off, cycles 3 and 7 missed, and a
        // double tap in cycle 5.
        let jitter = [0.02, -0.05, 0.06, 0.0, -0.03, 0.04, -0.06, 0.01, 0.03, -0.02, 0.05, 0.0];
        let mut taps: Vec<f64> = (0..12)
            .filter(|n| *n != 3 && *n != 7)
            .map(|n| 1.0 + n as f64 * 2.8 + jitter[n])
            .collect();
        taps.insert(5, taps[4] + 0.15);
        let f = fit(&taps).unwrap();
        assert!((f.period - 2.8).abs() < 0.01, "{f:?}");
        assert_eq!(f.cycles, 11);
        assert!(f.spread < 0.06);
        assert!(fit(&[0., 1.]).is_none());
        // A tap half a second late is dropped, and doesn't bend the line.
        let mut late = taps.clone();
        late[6] += 0.5;
        let g = fit(&late).unwrap();
        assert_eq!(g.dropped, 1);
        assert!((g.period - 2.8).abs() < 0.01, "{g:?}");
    }

    #[test]
    fn the_test_animation_has_every_picture() {
        let p = test_project(Kind::Animation, CardType::Mono11, 3, 7);
        let m = &p.messages[0];
        assert_eq!((m.mode, m.speed, m.selected), (Mode::Animation, 3, true));
        assert!(p.messages[1..].iter().all(|m| !m.selected));
        // The last picture has its LED, so the badge plays all 7.
        assert_eq!(m.bitmap.content_width().div_ceil(48), 7);
        // The flash lights the whole display.
        assert!((0..44).all(|x| (0..11).all(|y| m.bitmap.get(x, y).is_on())));
        // About 3 s a cycle at every speed.
        for sp in 1..=8 {
            let cycle = size_for(Kind::Animation, CardType::Mono11, sp) as f64 * preview::animation_frame_us(sp) as f64;
            assert!((2_000_000. ..4_000_000.).contains(&cycle), "speed {sp}: {cycle}");
        }
    }

    #[test]
    fn the_pause_test_is_two_pages_of_one_pause_each() {
        for card in [CardType::Mono11, CardType::Mono16, CardType::Rgb12] {
            let p = test_project(Kind::Pause, card, 1, 2);
            let m = &p.messages[0];
            let w = preview::display_width(card);
            assert_eq!(m.mode, Mode::Fixed);
            assert_eq!(m.bitmap.content_width(), 2 * w);
            assert!((0..w).all(|x| m.bitmap.get(x, 0).is_on()), "page 1 is the flash");
            assert!((w..2 * w - 1).all(|x| !m.bitmap.get(x, 0).is_on()));
            // The preview's cycle is two pauses of 32 steps.
            assert_eq!(model_steps(&p), 64);
        }
    }

    #[test]
    fn the_scroll_test_takes_about_3_seconds_a_pass_where_it_can() {
        for sp in 1..=8 {
            let n = size_for(Kind::Scroll, CardType::Mono11, sp);
            let p = test_project(Kind::Scroll, CardType::Mono11, sp, n);
            let m = &p.messages[0];
            assert_eq!((m.mode, m.bitmap.content_width()), (Mode::Left, n));
            // A pass is the message plus the display width.
            assert_eq!(model_steps(&p), n + 44);
            // At the slowest speeds, crossing the display alone takes longer.
            let shortest = ((44 + 8) * step_us(sp)) as f64;
            let cycle = (model_steps(&p) as u64 * step_us(sp)) as f64;
            assert!(
                (1_500_000. ..4_500_000_f64.max(shortest + 1.)).contains(&cycle),
                "speed {sp}: {cycle}"
            );
        }
    }
}
