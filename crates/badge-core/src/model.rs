use serde::{Deserialize, Serialize};

use crate::Framebuffer;

/// Number of message slots a badge stores.
pub const MESSAGE_COUNT: usize = 8;

/// Badge hardware variant ("card type" in the original software).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CardType {
    /// Monochrome, 11 rows (the common 11x44 badge).
    #[default]
    Mono11,
    /// Monochrome, 12 rows.
    Mono12,
    /// Monochrome, 16 rows.
    Mono16,
    /// Seven-color, 12 rows.
    Rgb12,
    /// Seven-color, 16 rows.
    Rgb16,
}

impl CardType {
    pub const ALL: [CardType; 5] = [
        CardType::Mono11,
        CardType::Mono12,
        CardType::Mono16,
        CardType::Rgb12,
        CardType::Rgb16,
    ];

    pub fn height(self) -> usize {
        match self {
            CardType::Mono11 => 11,
            CardType::Mono12 | CardType::Rgb12 => 12,
            CardType::Mono16 | CardType::Rgb16 => 16,
        }
    }

    pub fn is_color(self) -> bool {
        matches!(self, CardType::Rgb12 | CardType::Rgb16)
    }

    pub fn label(self) -> &'static str {
        match self {
            CardType::Mono11 => "Mono 11 rows",
            CardType::Mono12 => "Mono 12 rows",
            CardType::Mono16 => "Mono 16 rows",
            CardType::Rgb12 => "Color 12 rows",
            CardType::Rgb16 => "Color 16 rows",
        }
    }
}

/// LED color as a 3-bit RGB index (bit 2 = red, bit 1 = green, bit 0 = blue),
/// matching `TxtBmpImageShower.LedColors`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "i64", into = "u8")]
pub struct LedColor(u8);

/// Out-of-range values (e.g. from a hand-edited file, including negative or
/// above 255) wrap into 0-7.
impl From<i64> for LedColor {
    fn from(index: i64) -> Self {
        LedColor(index.rem_euclid(8) as u8)
    }
}

impl From<LedColor> for u8 {
    fn from(c: LedColor) -> Self {
        c.0
    }
}

impl LedColor {
    pub const OFF: LedColor = LedColor(0);
    pub const BLUE: LedColor = LedColor(1);
    pub const GREEN: LedColor = LedColor(2);
    pub const CYAN: LedColor = LedColor(3);
    pub const RED: LedColor = LedColor(4);
    pub const MAGENTA: LedColor = LedColor(5);
    pub const YELLOW: LedColor = LedColor(6);
    pub const WHITE: LedColor = LedColor(7);

    /// The seven lit colors, in palette order.
    pub const PALETTE: [LedColor; 7] = [
        LedColor::RED,
        LedColor::GREEN,
        LedColor::BLUE,
        LedColor::YELLOW,
        LedColor::MAGENTA,
        LedColor::CYAN,
        LedColor::WHITE,
    ];

    pub const fn new(index: u8) -> Self {
        LedColor(index & 7)
    }

    pub const fn index(self) -> u8 {
        self.0
    }

    pub const fn is_on(self) -> bool {
        self.0 != 0
    }

    pub const fn red(self) -> bool {
        self.0 & 4 != 0
    }

    pub const fn green(self) -> bool {
        self.0 & 2 != 0
    }

    pub const fn blue(self) -> bool {
        self.0 & 1 != 0
    }

    /// Thresholds an RGB pixel the same way the original image import did.
    pub fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        LedColor(((r > 127) as u8) << 2 | ((g > 127) as u8) << 1 | (b > 127) as u8)
    }

    pub fn to_rgb(self) -> [u8; 3] {
        [
            if self.red() { 255 } else { 0 },
            if self.green() { 255 } else { 0 },
            if self.blue() { 255 } else { 0 },
        ]
    }

    pub fn name(self) -> &'static str {
        match self.0 {
            0 => "Off",
            1 => "Blue",
            2 => "Green",
            3 => "Cyan",
            4 => "Red",
            5 => "Magenta",
            6 => "Yellow",
            _ => "White",
        }
    }
}

/// How a message is shown on the badge. Values are the on-wire mode numbers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Left = 0,
    Right = 1,
    Up = 2,
    Down = 3,
    Fixed = 4,
    Animation = 5,
    Snow = 6,
    Curtain = 7,
    Laser = 8,
}

impl Mode {
    pub const ALL: [Mode; 9] = [
        Mode::Left,
        Mode::Right,
        Mode::Up,
        Mode::Down,
        Mode::Fixed,
        Mode::Animation,
        Mode::Snow,
        Mode::Curtain,
        Mode::Laser,
    ];

    pub fn wire(self) -> u8 {
        self as u8
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Left => "Scroll left",
            Mode::Right => "Scroll right",
            Mode::Up => "Scroll up",
            Mode::Down => "Scroll down",
            Mode::Fixed => "Fixed",
            Mode::Animation => "Animation",
            Mode::Snow => "Snow (piling)",
            Mode::Curtain => "Curtain (split)",
            Mode::Laser => "Laser",
        }
    }
}

/// Global display brightness. Values are the on-wire levels; the badge has
/// exactly these four (higher values wrap around).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Brightness {
    #[default]
    Full = 0,
    ThreeQuarters = 1,
    Half = 2,
    Quarter = 3,
}

impl Brightness {
    pub const ALL: [Brightness; 4] = [
        Brightness::Full,
        Brightness::ThreeQuarters,
        Brightness::Half,
        Brightness::Quarter,
    ];

    pub fn wire(self) -> u8 {
        self as u8
    }

    /// The brightness as a fraction of full: 1, 0.75, 0.5 or 0.25.
    pub fn fraction(self) -> f32 {
        match self {
            Brightness::Full => 1.,
            Brightness::ThreeQuarters => 0.75,
            Brightness::Half => 0.5,
            Brightness::Quarter => 0.25,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Brightness::Full => "100%",
            Brightness::ThreeQuarters => "75%",
            Brightness::Half => "50%",
            Brightness::Quarter => "25%",
        }
    }
}

/// Multi-badge addressing (the "Send" menu in the original software).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AddressMode {
    /// Every badge accepts the data.
    #[default]
    Ignore,
    /// Store this address in the badge.
    Write(u16),
    /// Only the badge with this address accepts the data.
    SendTo(u16),
}

/// Upper bound accepted by the address picker in the original software.
pub const MAX_ADDRESS: u16 = 999;

/// Valid range of [`Message::top_offset`] (rows).
pub const TOP_OFFSET_RANGE: (i32, i32) = (-8, 8);
/// Valid range of [`Message::letter_spacing`] (columns).
pub const LETTER_SPACING_RANGE: (i32, i32) = (-2, 10);
/// Valid range of a system font's size (pixels).
pub const FONT_SIZE_RANGE: (u32, u32) = (4, 64);

/// The pixel fonts built into the app besides the default one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PixelFont {
    /// Fusion Pixel Font: 10 px on 11-row badges, 12 px on 12- and 16-row
    /// ones (it has no 16 px design).
    FusionPixel,
    /// GNU Unifont: 16 px, so 16-row badges only.
    Unifont,
}

impl PixelFont {
    pub const ALL: [PixelFont; 2] = [PixelFont::FusionPixel, PixelFont::Unifont];

    pub fn label(self) -> &'static str {
        match self {
            PixelFont::FusionPixel => "Fusion Pixel Font",
            PixelFont::Unifont => "GNU Unifont",
        }
    }

    /// Whether it fits a badge `height` rows tall.
    pub fn fits(self, height: usize) -> bool {
        match self {
            PixelFont::FusionPixel => true,
            PixelFont::Unifont => height >= 16,
        }
    }
}

/// Font used to turn message text into pixels.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FontChoice {
    /// The default built-in pixel font: IBM VGA letters, Chinese, Japanese
    /// and Korean from Fusion Pixel Font or GNU Unifont ("Auto" in the
    /// original).
    #[default]
    Builtin,
    /// One of the other pixel fonts built into the app, used for every
    /// character.
    Pixel(PixelFont),
    /// A system font rasterized without anti-aliasing ("Manual" in the original).
    System {
        family: String,
        size_px: u32,
        bold: bool,
        italic: bool,
    },
}

/// Where a message's pixels came from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    /// Rendered from `text` + `font`; re-rendered whenever those change.
    #[default]
    Text,
    /// Imported image or hand-drawn pixels; `text` is ignored.
    Bitmap,
}

/// One of the eight message slots.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Message {
    /// Included when sending ("Select" column).
    pub selected: bool,
    pub text: String,
    /// Per-character color for color badges. Shorter than `text` means the
    /// remaining characters use `default_color`.
    pub colors: Vec<LedColor>,
    pub default_color: LedColor,
    pub font: FontChoice,
    /// Rotated glyphs ("Fall" column); built-in font only.
    pub vertical: bool,
    /// Vertical shift in rows applied to rendered text (positive moves up).
    pub top_offset: i32,
    /// Extra blank columns between characters (negative = tighter).
    pub letter_spacing: i32,
    /// Speed 1 (slowest) to 8.
    pub speed: u8,
    pub mode: Mode,
    pub flash: bool,
    /// Marquee border ("Lamp" column).
    pub marquee: bool,
    pub source: Source,
    pub bitmap: Framebuffer,
}

impl Default for Message {
    fn default() -> Self {
        Message {
            selected: false,
            text: String::new(),
            colors: Vec::new(),
            default_color: LedColor::RED,
            font: FontChoice::Builtin,
            vertical: false,
            top_offset: 0,
            letter_spacing: 0,
            speed: 4,
            mode: Mode::Left,
            flash: false,
            marquee: false,
            source: Source::Text,
            bitmap: Framebuffer::new(CardType::default().height()),
        }
    }
}

impl Message {
    pub fn color_at(&self, char_index: usize) -> LedColor {
        self.colors.get(char_index).copied().unwrap_or(self.default_color)
    }

    /// Speed nibble as sent on the wire (0..=7).
    pub fn wire_speed(&self) -> u8 {
        self.speed.clamp(1, 8) - 1
    }

    /// True when the slot will contribute data when sent.
    /// Neither text nor pixels.
    pub fn is_blank(&self) -> bool {
        self.text.is_empty() && self.bitmap.used_width() == 0
    }

    pub fn has_content(&self) -> bool {
        self.selected && self.bitmap.used_width() > 0
    }
}

/// A complete badge configuration: everything that is sent in one go.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub card: CardType,
    pub brightness: Brightness,
    pub messages: Vec<Message>,
    /// The LED color of a mono badge (they come in red, blue, white...),
    /// for showing it on screen; not sent to the badge. Only mono badge
    /// types use it. `None`: not chosen (the app uses its default).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub led_color: Option<LedColor>,
}

impl Default for Project {
    fn default() -> Self {
        Project::new(CardType::default())
    }
}

impl Project {
    pub fn new(card: CardType) -> Self {
        let mut messages: Vec<Message> = (0..MESSAGE_COUNT)
            .map(|_| Message {
                bitmap: Framebuffer::new(card.height()),
                ..Message::default()
            })
            .collect();
        messages[0].selected = true;
        messages[0].text = "Hello".into();
        Project {
            card,
            brightness: Brightness::Full,
            messages,
            led_color: None,
        }
    }

    /// Ensures there are exactly eight messages with framebuffers of the
    /// right height and settings in their valid ranges (e.g. after loading a
    /// file or changing card type).
    pub fn normalize(&mut self) {
        self.messages.resize_with(MESSAGE_COUNT, Message::default);
        // Off isn't an LED color.
        if self.led_color == Some(LedColor::OFF) {
            self.led_color = None;
        }
        let h = self.card.height();
        for m in &mut self.messages {
            if m.bitmap.height() != h {
                m.bitmap.set_height(h);
            }
            m.speed = m.speed.clamp(1, 8);
            m.top_offset = m.top_offset.clamp(TOP_OFFSET_RANGE.0, TOP_OFFSET_RANGE.1);
            m.letter_spacing = m.letter_spacing.clamp(LETTER_SPACING_RANGE.0, LETTER_SPACING_RANGE.1);
            if let FontChoice::System { size_px, .. } = &mut m.font {
                *size_px = (*size_px).clamp(FONT_SIZE_RANGE.0, FONT_SIZE_RANGE.1);
            }
        }
    }

    pub fn set_card(&mut self, card: CardType) {
        self.card = card;
        self.normalize();
    }

    /// Byte-columns used by all selected messages, before the 2560 cap.
    pub fn used_columns(&self) -> usize {
        self.messages
            .iter()
            .filter(|m| m.selected)
            .map(|m| m.bitmap.used_width() / 8)
            .sum()
    }
}

/// Wall-clock time stamped into the header. Kept separate from any time
/// library so the encoder stays pure and testable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Timestamp {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}
