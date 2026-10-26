//! Byte-exact encoders for the badge upload protocols.
//!
//! Ported from `assembleUSBMonoArray`, `assembleUSBColorArray`,
//! `assembleCOMMonoArray` and `assembleCOMColorArray` in BmpBadge V9.18.

use crate::framebuffer::Plane;
use crate::{AddressMode, MESSAGE_COUNT, Project, Timestamp};

pub const USB_VENDOR_ID: u16 = 0x0416;
pub const USB_PRODUCT_ID: u16 = 0x5020;

/// HID report payload size; each write is prefixed with report id 0.
pub const REPORT_SIZE: usize = 64;

/// Maximum number of 8-pixel byte-columns the badge memory holds across
/// all messages.
pub const MAX_TOTAL_COLUMNS: usize = 2560;

const MAGIC: &[u8; 4] = b"wang";

/// Byte-columns each message contributes, after applying the 2560-column cap
/// in message order (later messages are truncated first).
pub fn message_columns(project: &Project) -> [usize; MESSAGE_COUNT] {
    let mut out = [0usize; MESSAGE_COUNT];
    let mut total = 0usize;
    for (i, m) in project.messages.iter().take(MESSAGE_COUNT).enumerate() {
        let want = if m.selected { m.bitmap.used_width() / 8 } else { 0 };
        let cols = if total + want > MAX_TOTAL_COLUMNS {
            MAX_TOTAL_COLUMNS - total
        } else {
            want
        };
        out[i] = cols;
        total += cols;
    }
    out
}

/// When the ticked messages need more than the badge's memory: how much
/// they need, and what sending them does about it (which messages are cut
/// short and which left out, by `message_columns`).
pub fn memory_overflow(project: &Project) -> Option<String> {
    let used = project.used_columns();
    if used <= MAX_TOTAL_COLUMNS {
        return None;
    }
    let cols = message_columns(project);
    let (mut short, mut left) = (Vec::new(), Vec::new());
    for (i, m) in project.messages.iter().take(MESSAGE_COUNT).enumerate() {
        if m.selected && cols[i] < m.bitmap.used_width() / 8 {
            if cols[i] > 0 { &mut short } else { &mut left }.push(i + 1);
        }
    }
    let cut = match (numbered(&short), numbered(&left)) {
        (Some(s), Some(l)) => format!("cuts {s} short and leaves out {l}"),
        (Some(s), None) => format!("cuts {s} short"),
        (None, Some(l)) => format!("leaves out {l}"),
        (None, None) => return None,
    };
    Some(format!(
        "The ticked messages need {} columns, more than the {} the badge holds: sending {cut}.",
        used * 8,
        MAX_TOTAL_COLUMNS * 8
    ))
}

/// "message 6", "messages 6 and 7", "messages 6, 7 and 8"; `None` for none.
fn numbered(numbers: &[usize]) -> Option<String> {
    let (last, rest) = numbers.split_last()?;
    if rest.is_empty() {
        return Some(format!("message {last}"));
    }
    let rest: Vec<String> = rest.iter().map(usize::to_string).collect();
    Some(format!("messages {} and {last}", rest.join(", ")))
}

/// Returns true when nothing would be sent.
pub fn is_empty(project: &Project) -> bool {
    message_columns(project).iter().all(|&c| c == 0)
}

fn bitmask(project: &Project, f: impl Fn(&crate::Message) -> bool) -> u8 {
    project
        .messages
        .iter()
        .take(MESSAGE_COUNT)
        .enumerate()
        .fold(0u8, |acc, (i, m)| if f(m) { acc | (1 << i) } else { acc })
}

/// The 60 header bytes that follow `"wang"` (shared by USB and color serial).
fn header_body(project: &Project, address: AddressMode, now: Timestamp) -> [u8; 60] {
    let mut h = [0u8; 60];
    // h[0] (= packet byte 4) is always 0.
    // Brightness 0-3 in the high nibble. The badge only reads the low two bits
    // of it (tested on hardware: 4-15 repeat 0-3), so these four levels are
    // all there is.
    h[1] = (project.brightness.wire() << 4) & 0xF0;
    h[2] = bitmask(project, |m| m.flash);
    h[3] = bitmask(project, |m| m.marquee);
    for (i, m) in project.messages.iter().take(MESSAGE_COUNT).enumerate() {
        h[4 + i] = m.mode.wire() | (m.wire_speed() << 4);
    }
    for (i, cols) in message_columns(project).iter().enumerate() {
        h[12 + 2 * i] = (cols >> 8) as u8;
        h[13 + 2 * i] = *cols as u8;
    }
    // h[28..30] = 0
    let (kind, addr) = match address {
        AddressMode::Ignore => (0u8, 0u16),
        AddressMode::Write(a) => (1, a),
        AddressMode::SendTo(a) => (2, a),
    };
    h[30] = kind;
    h[31] = (addr >> 8) as u8;
    h[32] = addr as u8;
    // h[33] = 0
    // The original writes Year / 100 (i.e. the century); kept for fidelity.
    h[34] = (now.year / 100) as u8;
    h[35] = now.month;
    h[36] = now.day;
    h[37] = now.hour;
    h[38] = now.minute;
    h[39] = now.second;
    // h[40] = the vendor's "SecurityCode": 0 unless a program on localhost
    // port 1234 sets it (purpose unknown). Left 0, as the vendor app sends
    // it normally; the badge works with 0.
    h
}

/// Column-major pixel payload: for each message, for each byte-column, for
/// each row, one byte (mono) or three bytes B, G, R (color).
fn payload(project: &Project) -> Vec<u8> {
    let cols = message_columns(project);
    let height = project.card.height();
    let color = project.card.is_color();
    let mut out = Vec::new();
    for (i, m) in project.messages.iter().take(MESSAGE_COUNT).enumerate() {
        for col in 0..cols[i] {
            for row in 0..height {
                if color {
                    out.push(m.bitmap.column_byte(Plane::Blue, col, row));
                    out.push(m.bitmap.column_byte(Plane::Green, col, row));
                    out.push(m.bitmap.column_byte(Plane::Red, col, row));
                } else {
                    out.push(m.bitmap.column_byte(Plane::Mono, col, row));
                }
            }
        }
    }
    out
}

/// Full USB upload stream.
pub fn encode_usb(project: &Project, address: AddressMode, now: Timestamp) -> Vec<u8> {
    let mut out = Vec::with_capacity(64 + project.used_columns() * 48);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&header_body(project, address, now));
    out.extend_from_slice(&payload(project));
    out
}

/// Splits a USB stream into 65-byte HID output reports (report id 0 +
/// 64 data bytes, last one zero-padded).
pub fn usb_reports(stream: &[u8]) -> Vec<[u8; REPORT_SIZE + 1]> {
    stream
        .chunks(REPORT_SIZE)
        .map(|chunk| {
            let mut r = [0u8; REPORT_SIZE + 1];
            r[1..1 + chunk.len()].copy_from_slice(chunk);
            r
        })
        .collect()
}

/// A serial upload: port settings plus the packets and pacing the original
/// software used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SerialPlan {
    pub baud_rate: u32,
    pub two_stop_bits: bool,
    pub packets: Vec<Vec<u8>>,
    /// Pause before every packet.
    pub gap_ms: u64,
    /// Extra pause after the second packet (the header) while the badge
    /// erases its memory.
    pub settle_ms: u64,
}

/// Serial (COM port) upload. Monochrome badges use the older `"hello"`
/// protocol at 1200 baud, color badges the `"wang"` protocol at 115200.
pub fn encode_serial(project: &Project, address: AddressMode, now: Timestamp) -> SerialPlan {
    if project.card.is_color() {
        let stream = encode_usb(project, address, now);
        let mut packets = vec![stream[..4].to_vec(), stream[4..64].to_vec()];
        packets.extend(stream[64..].chunks(REPORT_SIZE).map(<[u8]>::to_vec));
        return SerialPlan {
            baud_rate: 115_200,
            two_stop_bits: true,
            packets,
            gap_ms: 10,
            settle_ms: 1400,
        };
    }

    let cols = message_columns(project);
    let mut packets = Vec::new();
    packets.push(vec![b'h', b'e', b'l', b'l', b'o', project.brightness.wire()]);

    let mut h = vec![0u8; 47];
    let mut offset = 0usize;
    for (i, m) in project.messages.iter().take(MESSAGE_COUNT).enumerate() {
        let mut b = m.wire_speed() << 4;
        if m.flash {
            b |= 0x08;
        }
        if m.marquee {
            b |= 0x80;
        }
        let mode = m.mode.wire();
        if mode < 8 {
            b |= mode;
        } else {
            h[42] |= 1 << i;
        }
        h[i + 1] = b;
        h[(i << 2) + 10] = (offset >> 8) as u8;
        h[(i << 2) + 11] = offset as u8;
        h[(i << 2) + 12] = (cols[i] >> 8) as u8;
        h[(i << 2) + 13] = cols[i] as u8;
        offset += cols[i];
    }
    let (flag, addr) = match address {
        AddressMode::Write(a) => (0x33u8, Some(a)),
        AddressMode::Ignore => (0x55, None),
        AddressMode::SendTo(a) => (0xAA, Some(a)),
    };
    h[9] = flag;
    h[43] = flag;
    if let Some(a) = addr {
        h[44] = (a >> 8) as u8;
        h[45] = a as u8;
    }
    packets.push(h);

    // Row-major: one packet per row containing every message's columns.
    for row in 0..project.card.height() {
        let mut p = Vec::with_capacity(offset);
        for (i, m) in project.messages.iter().take(MESSAGE_COUNT).enumerate() {
            for col in 0..cols[i] {
                p.push(m.bitmap.column_byte(Plane::Mono, col, row));
            }
        }
        packets.push(p);
    }
    packets.push(vec![0]);

    SerialPlan {
        baud_rate: 1200,
        two_stop_bits: false,
        packets,
        gap_ms: 30,
        settle_ms: 1000,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Brightness, CardType, LedColor, Mode};

    fn now() -> Timestamp {
        Timestamp {
            year: 2026,
            month: 10,
            day: 1,
            hour: 13,
            minute: 37,
            second: 5,
        }
    }

    fn sample(card: CardType) -> Project {
        let mut p = Project::new(card);
        for m in &mut p.messages {
            m.selected = false;
        }
        let m = &mut p.messages[0];
        m.selected = true;
        m.speed = 5;
        m.mode = Mode::Snow;
        m.flash = true;
        // An "L" shape: left column lit, bottom row lit across 10 pixels.
        for y in 0..card.height() {
            m.bitmap.set(0, y, LedColor::RED);
        }
        for x in 0..10 {
            m.bitmap.set(x, card.height() - 1, LedColor::GREEN);
        }
        let m2 = &mut p.messages[2];
        m2.selected = true;
        m2.marquee = true;
        m2.mode = Mode::Laser;
        m2.speed = 8;
        m2.bitmap.set(3, 0, LedColor::WHITE);
        p.brightness = Brightness::Half;
        p
    }

    #[test]
    fn usb_header_layout() {
        let p = sample(CardType::Mono11);
        let s = encode_usb(&p, AddressMode::SendTo(0x0123), now());
        let expected_header: [u8; 64] = [
            b'w', b'a', b'n', b'g', 0x00, 0x20, 0x01, 0x04, //
            0x46, 0x30, 0x78, 0x30, 0x30, 0x30, 0x30, 0x30, // mode|speed<<4 x8
            0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, // lengths
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
            0x00, 0x00, 0x02, 0x01, 0x23, 0x00, 20, 10, //
            1, 13, 37, 5, 0x00, 0x00, 0x00, 0x00, //
            0, 0, 0, 0, 0, 0, 0, 0, //
            0, 0, 0, 0, 0, 0, 0, 0,
        ];
        assert_eq!(&s[..64], &expected_header[..]);
        // 3 columns x 11 rows of payload.
        assert_eq!(s.len(), 64 + 3 * 11);
        // Message 0, column 0: rows 0..9 = 0x80, row 10 = 0xFF.
        assert_eq!(
            &s[64..75],
            &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0xFF]
        );
        // Message 0, column 1: only row 10 has pixels 8 and 9.
        assert_eq!(s[64 + 11 + 10], 0xC0);
        // Message 2, column 0, row 0: pixel 3.
        assert_eq!(s[64 + 22], 0x10);
    }

    #[test]
    fn usb_color_plane_order() {
        let mut p = Project::new(CardType::Rgb12);
        p.messages[0].bitmap.clear();
        p.messages[0].bitmap.set(0, 0, LedColor::RED);
        p.messages[0].bitmap.set(1, 0, LedColor::BLUE);
        let s = encode_usb(&p, AddressMode::Ignore, now());
        assert_eq!(s.len(), 64 + 12 * 3);
        // Row 0: B, G, R.
        assert_eq!(&s[64..67], &[0x40, 0x00, 0x80]);
    }

    #[test]
    fn column_cap() {
        let mut p = Project::new(CardType::Mono11);
        for m in &mut p.messages {
            m.selected = true;
            m.bitmap.set(4095, 0, LedColor::RED);
        }
        let c = message_columns(&p);
        assert_eq!(c[..5], [512, 512, 512, 512, 512]);
        assert_eq!(c[5..], [0, 0, 0]);
        assert_eq!(
            memory_overflow(&p).as_deref(),
            Some(
                "The ticked messages need 32768 columns, more than the 20480 the badge holds: \
                 sending leaves out messages 6, 7 and 8."
            )
        );
    }

    #[test]
    fn memory_overflow_names_what_is_cut() {
        let mut p = Project::new(CardType::Mono11);
        assert_eq!(memory_overflow(&p), None);
        // 500 byte-columns each: five fit, the sixth gets 60, the rest none.
        for m in &mut p.messages {
            m.selected = true;
            m.bitmap.set(3999, 0, LedColor::RED);
        }
        p.messages[7].selected = false;
        assert_eq!(
            memory_overflow(&p).as_deref(),
            Some(
                "The ticked messages need 28000 columns, more than the 20480 the badge holds: \
                 sending cuts message 6 short and leaves out message 7."
            )
        );
        // Exactly full is fine.
        for m in &mut p.messages[5..] {
            m.selected = false;
        }
        p.messages[5].bitmap.clear();
        p.messages[5].bitmap.set(479, 0, LedColor::RED);
        p.messages[5].selected = true;
        assert_eq!(p.used_columns(), MAX_TOTAL_COLUMNS);
        assert_eq!(memory_overflow(&p), None);
    }

    #[test]
    fn reports_are_padded() {
        let r = usb_reports(&[1u8; 70]);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0][0], 0);
        assert_eq!(r[1][1..7], [1; 6]);
        assert_eq!(r[1][7], 0);
    }

    #[test]
    fn serial_mono_layout() {
        let p = sample(CardType::Mono11);
        let plan = encode_serial(&p, AddressMode::Ignore, now());
        assert_eq!(plan.baud_rate, 1200);
        assert_eq!(plan.packets[0], b"hello\x02");
        let h = &plan.packets[1];
        assert_eq!(h.len(), 47);
        assert_eq!(h[1], (4 << 4) | 0x08 | 6);
        assert_eq!(h[3], 0x80 | (7 << 4)); // laser goes to byte 42
        assert_eq!(h[42], 1 << 2);
        assert_eq!((h[9], h[43]), (0x55, 0x55));
        // offsets/lengths: msg0 at 0 len 2, msg2 at 2 len 1
        assert_eq!(&h[10..14], &[0, 0, 0, 2]);
        assert_eq!(&h[18..22], &[0, 2, 0, 1]);
        assert_eq!(plan.packets.len(), 2 + 11 + 1);
        assert_eq!(plan.packets[2], vec![0x80, 0x00, 0x10]);
        assert_eq!(plan.packets[12], vec![0xFF, 0xC0, 0x00]);
    }

    #[test]
    fn serial_color_matches_usb_stream() {
        let p = sample(CardType::Rgb16);
        let plan = encode_serial(&p, AddressMode::Ignore, now());
        let usb = encode_usb(&p, AddressMode::Ignore, now());
        assert_eq!(plan.packets.concat(), usb);
        assert_eq!(plan.packets[0], b"wang");
        assert_eq!(plan.packets[1].len(), 60);
    }
}
