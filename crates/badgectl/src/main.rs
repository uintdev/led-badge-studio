//! `badgectl`: send text, images or projects to an LED name badge from the
//! command line.

use std::path::PathBuf;

mod calibrate;

use anyhow::{Context, Result, bail};
use badge_core::{AddressMode, Brightness, CardType, FontChoice, Framebuffer, Mode, Project, Source};
use badge_render::{FontLibrary, ImportOptions, Renderer};
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "badgectl",
    version,
    long_version = concat!(
        env!("CARGO_PKG_VERSION"),
        "\n",
        env!("BRAND_NAME"),
        "\n",
        env!("BRAND_COPYRIGHT"),
        "\n",
        env!("BRAND_HOMEPAGE")
    ),
    about = "Program 0416:5020 LED name badges",
    after_help = "Run `badgectl help <COMMAND>` for a command's options and examples."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List connected badges and serial ports.
    #[command(after_help = "Examples:
  badgectl list
  badgectl list --watch")]
    List {
        /// Keep running and print the list again whenever a device is plugged
        /// in or removed (Ctrl+C to stop).
        #[arg(long)]
        watch: bool,
    },
    /// Send messages to a badge.
    #[command(after_help = "Examples:
  badgectl send -t \"Hello world\"
  badgectl send -t \"Hello world\" --mode left --speed 6 --brightness 50
  badgectl send -t \"Line one\" -t \"Line two ~f\" --flash
  badgectl send -i logo.png --mode fixed
  badgectl send -p messages.badge
  badgectl send -d \"USB badge 2\" -t \"Second badge\" --mode fixed")]
    Send {
        #[command(flatten)]
        content: Content,
        #[command(flatten)]
        target: Target,
    },
    /// Print how messages will look, as text art, or save the animation.
    #[command(after_help = "Examples:
  badgectl preview -t \"Hi ~1\"
  badgectl preview -t \"Hi ~f\" --mode laser --export hi.webp
  badgectl preview -t \"Hi ~f\" --export hi.png --still
  badgectl preview -p messages.badge --export messages.gif --square --no-glow")]
    Preview {
        #[command(flatten)]
        content: Content,
        /// Also save the animation (every ticked message, one full cycle
        /// each, at the badge's speed) as an image: GIF, PNG (animated PNG)
        /// or WebP, from the file's extension (.gif, .png, .webp).
        #[arg(long, value_name = "FILE", alias = "gif")]
        export: Option<PathBuf>,
        /// With --export: save one still frame instead (the one of the
        /// first ticked message showing the most LEDs).
        #[arg(long, requires = "export")]
        still: bool,
        /// LED size in the saved image, in pixels.
        #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u32).range(2..=64))]
        led_size: u32,
        /// In the saved image: square LEDs instead of round ones.
        #[arg(long)]
        square: bool,
        /// In the saved image: no glow around lit LEDs.
        #[arg(long)]
        no_glow: bool,
        /// In the saved image: LEDs dimmed to the brightness setting (as
        /// the app's Show brightness).
        #[arg(long)]
        show_brightness: bool,
    },
    /// Check .badge files for problems, without sending anything.
    ///
    /// Exit status: 0 when every file is fine (or was repaired with --fix),
    /// 2 when a file has values that can be repaired, 1 when a file can't be
    /// opened at all; with --strict, 3 when sending a file would cut messages
    /// short (and nothing else is wrong).
    #[command(after_help = "Examples:
  badgectl check messages.badge
  badgectl check *.badge
  badgectl check --fix messages.badge
  badgectl check --strict *.badge")]
    Check {
        /// The .badge files to check.
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Repair what can be repaired and save it back to the file.
        #[arg(long)]
        fix: bool,
        /// Also fail (exit status 3) when the ticked messages need more than
        /// the badge's memory, so sending would cut them short.
        #[arg(long)]
        strict: bool,
    },
    /// Measure the badge's timing by tapping along.
    ///
    /// For the preview. Sends a test message per speed, which replaces
    /// what's on the badge: press Enter each time the badge flashes.
    #[command(after_help = "Examples:
  badgectl calibrate
  badgectl calibrate --mode animation --speeds 1,8
  badgectl calibrate --mode pause
  badgectl calibrate --mode scroll --taps 30")]
    Calibrate {
        /// What to measure. `all` runs animation, pause and scroll in turn:
        /// the other two are reported in the timer unit animation measures.
        #[arg(long, value_enum, default_value_t = CalibrateMode::All)]
        mode: CalibrateMode,
        /// The speeds to measure, comma-separated.
        #[arg(long, value_delimiter = ',', default_values_t = [1u8, 2, 3, 4, 5, 6, 7, 8],
              value_parser = clap::value_parser!(u8).range(1..=8))]
        speeds: Vec<u8>,
        /// The badge type.
        #[arg(long, value_enum, default_value_t = Card::Mono11)]
        card: Card,
        /// Animation: pictures per cycle (default: about 3 seconds' worth per
        /// speed).
        #[arg(long, value_parser = clap::value_parser!(u16).range(2..=60))]
        frames: Option<u16>,
        /// Taps per speed: more is more precise (30 halves the error of 15).
        /// Default: 15, or 8 for the pause.
        #[arg(long, value_parser = clap::value_parser!(u16).range(6..=200))]
        taps: Option<u16>,
        /// Device id or label from `badgectl list` (default: first USB badge).
        #[arg(long, short)]
        device: Option<String>,
    },
    /// Print the licenses: the app's own, the built-in fonts' and the bundled
    /// libraries'.
    #[command(after_help = "Examples:
  badgectl licenses
  badgectl licenses --fonts")]
    Licenses {
        /// Only the font notices and licenses.
        #[arg(long, conflicts_with = "third_party")]
        fonts: bool,
        /// Only the third-party library notices.
        #[arg(long)]
        third_party: bool,
    },
    /// Manage the Linux udev rule that lets your account write to the badge.
    #[cfg(target_os = "linux")]
    #[command(
        alias = "udev-rule",
        after_help = "Examples:
  badgectl udev status
  badgectl udev install
  badgectl udev remove
  badgectl udev print"
    )]
    Udev {
        #[command(subcommand)]
        action: Option<UdevAction>,
    },
}

#[cfg(target_os = "linux")]
#[derive(Subcommand)]
enum UdevAction {
    /// Print the rule (default).
    Print,
    /// Show whether the rule is installed.
    Status,
    /// Install or reinstall the rule (asks for your password).
    Install,
    /// Remove the rule (asks for your password).
    Remove,
}

#[derive(clap::Args)]
struct Content {
    /// Message text (repeat for up to 8 messages). `~0`..`~f` insert icons.
    #[arg(long, short)]
    text: Vec<String>,
    /// Image file per message (bmp/png/gif/jpg/ico).
    #[arg(long, short)]
    image: Vec<PathBuf>,
    /// Load a .badge project instead. It brings its own badge type, modes,
    /// speeds, brightness and fonts, so --card through --vertical are ignored.
    #[arg(long, short, conflicts_with_all = ["text", "image"])]
    project: Option<PathBuf>,
    /// Badge type: mono or color (rgb), and its number of rows.
    #[arg(long, value_enum, default_value_t = Card::Mono11)]
    card: Card,
    /// How the messages move.
    #[arg(long, value_enum, default_value_t = ModeArg::Left)]
    mode: ModeArg,
    /// 1 (slow) to 8 (fast).
    #[arg(long, default_value_t = 4, value_parser = clap::value_parser!(u8).range(1..=8))]
    speed: u8,
    /// Flash the messages on and off.
    #[arg(long)]
    flash: bool,
    /// Animated border.
    #[arg(long)]
    marquee: bool,
    /// Brightness in percent: 100, 75, 50 or 25.
    #[arg(long, default_value_t = 100)]
    brightness: u8,
    /// Font: a built-in one ("Fusion Pixel Font" or "Unifont"; default:
    /// IBM VGA + Fusion Pixel/Unifont), or a computer font as "Family" or
    /// "Family:size[:bold][:italic]".
    #[arg(long)]
    font: Option<String>,
    /// Use rotated glyphs (built-in font).
    #[arg(long)]
    vertical: bool,
}

#[derive(clap::Args)]
struct Target {
    /// Device id or label from `badgectl list` (default: first USB badge).
    #[arg(long, short)]
    device: Option<String>,
    /// Store this address in the badge (0-999).
    #[arg(long, conflicts_with = "to_address")]
    write_address: Option<u16>,
    /// Only the badge with this address accepts the upload.
    #[arg(long)]
    to_address: Option<u16>,
}

/// What `badgectl calibrate` measures.
#[derive(Clone, Copy, ValueEnum)]
enum CalibrateMode {
    /// Animation, then pause, then scroll.
    All,
    /// Time per Animation picture, and the badge's timer unit.
    Animation,
    /// The pause on each page (Fixed mode; also between the pages of the
    /// other paged modes).
    Pause,
    /// Time per scroll step and steps per pass (Left mode).
    Scroll,
}

#[derive(Clone, Copy, ValueEnum)]
enum Card {
    Mono11,
    Mono12,
    Mono16,
    Rgb12,
    Rgb16,
}

impl From<Card> for CardType {
    fn from(c: Card) -> Self {
        match c {
            Card::Mono11 => CardType::Mono11,
            Card::Mono12 => CardType::Mono12,
            Card::Mono16 => CardType::Mono16,
            Card::Rgb12 => CardType::Rgb12,
            Card::Rgb16 => CardType::Rgb16,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum ModeArg {
    Left,
    Right,
    Up,
    Down,
    Fixed,
    Animation,
    Snow,
    Curtain,
    Laser,
}

impl From<ModeArg> for Mode {
    fn from(m: ModeArg) -> Self {
        Mode::ALL[m as usize]
    }
}

fn print_devices() {
    let devices = badge_io::list_devices();
    if devices.is_empty() {
        println!("No badges found.");
        #[cfg(target_os = "linux")]
        println!("If one is plugged in, run `badgectl udev status` to check access.");
    }
    for d in devices {
        println!("{:<16} {}", d.label, d.id);
    }
}

fn watch_devices() -> Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    let watcher = badge_io::watch_devices(move || {
        let _ = tx.send(());
    });
    let Some(_watcher) = watcher else {
        bail!("device notifications aren't available on this system");
    };
    eprintln!("Watching for devices being plugged in or removed (Ctrl+C to stop)…");
    while rx.recv().is_ok() {
        // One plug-in sends a burst of events; print once it has settled.
        std::thread::sleep(std::time::Duration::from_millis(250));
        while rx.try_recv().is_ok() {}
        println!("\n[{}] devices changed:", chrono::Local::now().format("%H:%M:%S"));
        print_devices();
    }
    Ok(())
}

/// `badgectl check`: returns the exit status.
fn check(files: &[PathBuf], fix: bool, strict: bool) -> i32 {
    let mut status = 0;
    let mut over_memory = false;
    let mut renderer: Option<Renderer> = None;
    for path in files {
        let name = path.display();
        let loaded = badge_formats::open_checked(path);
        match loaded {
            Ok(ref l) if l.fixes.is_empty() => println!("{name}: OK"),
            Ok(ref l) => {
                let n = l.fixes.len();
                let s = if n == 1 { "" } else { "s" };
                if fix {
                    match badge_formats::save_project(&l.project, path) {
                        Ok(()) => println!("{name}: repaired {n} problem{s} and saved:"),
                        Err(e) => {
                            println!("{name}: {n} problem{s}, but saving the repairs failed: {e:#}");
                            status = 1;
                        }
                    }
                } else {
                    println!("{name}: {n} problem{s} that can be repaired (run with --fix to save the repairs):");
                    if status == 0 {
                        status = 2;
                    }
                }
                for f in &l.fixes {
                    println!("  - {f}");
                }
            }
            Err(ref e) => {
                println!("{name}: can't be opened: {e:#}");
                status = 1;
            }
        }
        // Not a problem with the file: it changes the exit status only with
        // --strict, and only when nothing else is wrong (after every file).
        if let Ok(l) = loaded {
            let mut project = l.project;
            // Text is drawn with this computer's fonts, as sending would;
            // they're loaded once, and only for ticked text.
            let card = project.card;
            for m in project
                .messages
                .iter_mut()
                .filter(|m| m.selected && m.source == Source::Text)
            {
                let r = renderer.get_or_insert_with(|| Renderer::new(FontLibrary::discover()));
                m.bitmap = r.render(m, card);
            }
            if let Some(over) = badge_core::wording::memory_overflow(&project) {
                println!("  warning: {over}");
                over_memory = true;
            }
        }
    }
    if strict && over_memory && status == 0 {
        status = 3;
    }
    status
}

fn main() {
    // Exit quietly when the reader of our output goes away (`badgectl … |
    // head`), like other command-line tools, instead of panicking.
    // SAFETY: restores the default handler before any other code runs.
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    if let Err(e) = run(Cli::parse()) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Check { files, fix, strict } => {
            let status = check(&files, fix, strict);
            if status != 0 {
                std::process::exit(status);
            }
        }
        Command::List { watch } => {
            print_devices();
            if watch {
                watch_devices()?;
            }
        }
        Command::Licenses { fonts, third_party } => {
            use badge_render::licenses::{APP_LICENSE, FONT_LICENSES, THIRD_PARTY_NOTICES};
            use std::fmt::Write as _;
            use std::io::Write;
            let mut out = String::new();
            let own = (!fonts && !third_party).then_some(&APP_LICENSE);
            if !third_party {
                for doc in own.into_iter().chain(FONT_LICENSES) {
                    let rule = "=".repeat(doc.title.len());
                    let _ = write!(out, "{}\n{rule}\n\n{}\n\n", doc.title, doc.text.trim_end());
                }
            }
            if !fonts {
                out += THIRD_PARTY_NOTICES;
            }
            // Ignore errors such as a closed pipe (`badgectl licenses | head`).
            let _ = std::io::stdout().lock().write_all(out.as_bytes());
        }
        #[cfg(target_os = "linux")]
        Command::Udev { action } => match action.unwrap_or(UdevAction::Print) {
            UdevAction::Print => {
                print!("{}", badge_io::UDEV_RULE);
                eprintln!(
                    "\n# Install with `badgectl udev install`, or by hand:\n#   badgectl udev | sudo tee {}\n#   sudo udevadm control --reload-rules && sudo udevadm trigger",
                    badge_io::UDEV_RULE_PATH
                );
            }
            UdevAction::Status => {
                let text = match badge_io::udev_rule_state() {
                    badge_io::UdevRuleState::Installed => format!("installed ({})", badge_io::UDEV_RULE_PATH),
                    badge_io::UdevRuleState::Outdated => {
                        "outdated (from an older version); run `badgectl udev install` to replace it".into()
                    }
                    badge_io::UdevRuleState::NotInstalled => "not installed; run `badgectl udev install`".into(),
                    badge_io::UdevRuleState::NotNeeded => "not needed on this system".into(),
                    badge_io::UdevRuleState::Unknown => {
                        "can't be checked: this Flatpak has no access to the host system".into()
                    }
                };
                println!("udev rule: {text}");
                for d in badge_io::list_usb() {
                    let ok = if badge_io::can_write(&d) {
                        "writable"
                    } else {
                        "NOT writable"
                    };
                    println!("{:<16} {}  ({ok})", d.label, d.id);
                }
            }
            UdevAction::Install => {
                badge_io::install_udev_rule()?;
                println!(
                    "Installed {}. Badges plugged in now should work right away.",
                    badge_io::UDEV_RULE_PATH
                );
            }
            UdevAction::Remove => {
                badge_io::remove_udev_rule()?;
                println!("Removed. A badge plugged in now may keep access until it is unplugged.");
            }
        },
        Command::Preview {
            content,
            export,
            still,
            led_size,
            square,
            no_glow,
            show_brightness,
        } => {
            let project = build_project(&content)?;
            if let Some(path) = &export {
                use badge_render::preview_export::{Format, Look, Part, busiest_frame, save_animation, save_still};
                let Some(format) = Format::from_path(path) else {
                    bail!("{}: use a .gif, .png or .webp file name", path.display());
                };
                let width = badge_render::preview::display_width(project.card);
                let parts: Vec<Part> = project
                    .messages
                    .iter()
                    .filter(|m| m.has_content())
                    .map(|m| Part {
                        bitmap: &m.bitmap,
                        params: badge_render::preview::PreviewParams {
                            mode: m.mode,
                            flash: m.flash,
                            marquee: m.marquee,
                            width,
                            speed: m.speed,
                        },
                    })
                    .collect();
                if parts.is_empty() {
                    bail!("nothing to animate: no ticked message has content");
                }
                // As the app's preview draws it by default (round LEDs with
                // their glow), unless asked otherwise.
                let look = Look {
                    cell: led_size as f32,
                    round: !square,
                    color_card: project.card.is_color(),
                    // The project's LED color (mono badges), else red.
                    tint: project.led_color.unwrap_or(badge_core::LedColor::RED),
                    glow: !no_glow,
                    level: project.brightness.fraction(),
                    dim: show_brightness,
                };
                if still {
                    save_still(&busiest_frame(&parts[0]), width, &look, format, path)?;
                } else {
                    save_animation(&parts, &look, format, path)?;
                }
                eprintln!("Saved {}", path.display());
            }
            for (i, m) in project.messages.iter().enumerate().filter(|(_, m)| m.selected) {
                println!("Message {} ({}, speed {}):", i + 1, m.mode.label(), m.speed);
                print_art(&m.bitmap);
            }
        }
        Command::Calibrate {
            mode,
            speeds,
            card,
            frames,
            taps,
            device,
        } => {
            let device = pick_device(device.as_deref())?;
            use calibrate::Kind;
            let kinds: &[Kind] = match mode {
                CalibrateMode::All => &[Kind::Animation, Kind::Pause, Kind::Scroll],
                CalibrateMode::Animation => &[Kind::Animation],
                CalibrateMode::Pause => &[Kind::Pause],
                CalibrateMode::Scroll => &[Kind::Scroll],
            };
            for (i, &kind) in kinds.iter().enumerate() {
                if kinds.len() > 1 {
                    println!("\n#### Part {} of {}", i + 1, kinds.len());
                }
                let taps = taps.map_or(
                    if kind == Kind::Pause {
                        calibrate::DEFAULT_PAUSE_TAPS
                    } else {
                        calibrate::DEFAULT_TAPS
                    },
                    usize::from,
                );
                let frames = if kind == Kind::Animation {
                    frames.map(usize::from)
                } else {
                    None
                };
                if calibrate::run(&device, card.into(), kind, &speeds, frames, taps)? == calibrate::Done::Quit {
                    break;
                }
            }
        }
        Command::Send { content, target } => {
            let mut last = 0;
            let mut progress = |done: usize, total: usize| {
                let pct = done * 100 / total.max(1);
                if pct / 10 != last / 10 || done == total {
                    eprint!("\rSending… {pct:3}%");
                    last = pct;
                }
                true
            };
            let project = build_project(&content)?;
            if let Some(over) = badge_core::wording::memory_overflow(&project) {
                eprintln!("warning: {over}");
            }
            let device = pick_device(target.device.as_deref())?;
            let address = match (target.write_address, target.to_address) {
                (Some(a), _) => AddressMode::Write(a.min(badge_core::MAX_ADDRESS)),
                (_, Some(a)) => AddressMode::SendTo(a.min(badge_core::MAX_ADDRESS)),
                _ => AddressMode::Ignore,
            };
            badge_io::send(&project, address, &device, &mut progress)?;
            badge_formats::last_sent::LastSent::record(&badge_core::protocol::as_sent(&project));
            eprintln!("\rSent to {}.        ", device.label);
        }
    }
    Ok(())
}

fn pick_device(want: Option<&str>) -> Result<badge_io::DeviceInfo> {
    let devices = badge_io::list_devices();
    match want {
        Some(w) => devices
            .into_iter()
            .find(|d| d.id == w || d.label.eq_ignore_ascii_case(w))
            .with_context(|| format!("device {w:?} not found (see `badgectl list`)")),
        None => devices
            .into_iter()
            .find(|d| d.kind == badge_io::DeviceKind::Usb)
            .context("no USB badge found; plug one in or pass --device"),
    }
}

fn render_all(project: &mut Project) {
    let mut r = Renderer::new(FontLibrary::discover());
    let card = project.card;
    let mut warned: Vec<&str> = Vec::new();
    for (i, m) in project.messages.iter().enumerate() {
        if let FontChoice::System { family, .. } = &m.font
            && m.selected
            && m.source == Source::Text
            && !warned.contains(&family.as_str())
            && !r.system.is_installed(family)
        {
            eprintln!(
                "warning: font {family:?} (message {}) isn't installed; a fallback font is used",
                i + 1
            );
            warned.push(family);
        }
    }
    for m in &mut project.messages {
        m.bitmap = r.render(m, card);
    }
}

fn build_project(c: &Content) -> Result<Project> {
    let card: CardType = c.card.into();
    let mut project = if let Some(path) = &c.project {
        let loaded = badge_formats::open_checked(path)?;
        for f in &loaded.fixes {
            eprintln!("warning: {}: {f}", path.display());
        }
        if !loaded.fixes.is_empty() {
            eprintln!(
                "(those values were adjusted for this run; `badgectl check --fix {}` saves the repairs)",
                path.display()
            );
        }
        loaded.project
    } else {
        if c.text.is_empty() && c.image.is_empty() {
            bail!("give --text, --image or --project");
        }
        if c.text.len() + c.image.len() > badge_core::MESSAGE_COUNT {
            bail!("at most {} messages", badge_core::MESSAGE_COUNT);
        }
        let mut p = Project::new(card);
        p.brightness = match c.brightness {
            75 => Brightness::ThreeQuarters,
            50 => Brightness::Half,
            25 => Brightness::Quarter,
            100 => Brightness::Full,
            b => bail!("brightness must be 100, 75, 50 or 25 (got {b})"),
        };
        let font = match &c.font {
            None => FontChoice::Builtin,
            // The other built-in pixel fonts, by name.
            Some(spec)
                if let Some(font) = badge_core::PixelFont::ALL.into_iter().find(|f| {
                    let name = f.label().to_lowercase();
                    let spec = spec.to_lowercase();
                    spec == name || name.contains(&spec) && spec.len() >= 5
                }) =>
            {
                FontChoice::Pixel(font)
            }
            Some(spec) => {
                let mut parts = spec.split(':');
                let family = parts.next().unwrap_or_default().to_string();
                let mut size_px = card.height() as u32;
                let (mut bold, mut italic) = (false, false);
                for p in parts {
                    match p {
                        "bold" => bold = true,
                        "italic" => italic = true,
                        n => size_px = n.parse().with_context(|| format!("bad font size {n:?}"))?,
                    }
                }
                FontChoice::System {
                    family,
                    size_px,
                    bold,
                    italic,
                }
            }
        };
        let mut slots = p.messages.iter_mut();
        for t in &c.text {
            let m = slots.next().unwrap();
            m.text.clone_from(t);
            m.font = font.clone();
            m.vertical = c.vertical;
        }
        for path in &c.image {
            let m = slots.next().unwrap();
            m.text.clear();
            m.source = Source::Bitmap;
            m.bitmap = badge_render::import_image(path, card, &ImportOptions::default())?;
        }
        for (i, m) in p.messages.iter_mut().enumerate() {
            m.selected = i < c.text.len() + c.image.len();
            m.mode = c.mode.into();
            m.speed = c.speed;
            m.flash = c.flash;
            m.marquee = c.marquee;
        }
        p
    };
    // Bring options such as the font size into range.
    project.normalize();
    render_all(&mut project);
    Ok(project)
}

fn print_art(fb: &Framebuffer) {
    let w = fb.used_width();
    for y in 0..fb.height() {
        let line: String = (0..w).map(|x| if fb.is_on(x, y) { '█' } else { '·' }).collect();
        println!("  {line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use badge_core::LedColor;

    /// A folder of its own for the test's files, removed when it ends.
    struct TempDir(PathBuf);

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn strict_check_puts_repairs_first_whatever_the_order() {
        let dir = TempDir(std::env::temp_dir().join(format!("badgectl-check-{}", std::process::id())));
        let _ = std::fs::remove_dir_all(&dir.0);
        std::fs::create_dir_all(&dir.0).unwrap();
        let fine = dir.0.join("fine.badge");
        badge_formats::save_project(&Project::new(CardType::Mono11), &fine).unwrap();
        // Every message ticked, as wide as a message can be.
        let mut big = Project::new(CardType::Mono11);
        for m in &mut big.messages {
            m.selected = true;
            m.source = Source::Bitmap;
            m.bitmap.set(badge_core::MAX_WIDTH - 1, 0, LedColor::RED);
        }
        let over = dir.0.join("over.badge");
        badge_formats::save_project(&big, &over).unwrap();
        // A speed out of range, which can be repaired.
        let repairable = dir.0.join("repairable.badge");
        let text = std::fs::read_to_string(&fine)
            .unwrap()
            .replacen("\"speed\": ", "\"speed\": 9", 1);
        std::fs::write(&repairable, text).unwrap();

        assert_eq!(check(std::slice::from_ref(&fine), false, true), 0);
        assert_eq!(check(std::slice::from_ref(&over), false, false), 0, "only a warning");
        assert_eq!(check(std::slice::from_ref(&over), false, true), 3);
        assert_eq!(check(&[over.clone(), repairable.clone()], false, true), 2);
        assert_eq!(check(&[repairable, over], false, true), 2);
    }
}
