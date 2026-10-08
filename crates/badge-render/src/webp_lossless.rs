//! A small lossless WebP (VP8L) encoder for LED pictures (RFC 9649).
//!
//! `image-webp`'s encoder writes literal pixels only, which made stills 10x
//! larger than GIFs. LED grids are mostly repeats of the pixel to the left
//! or the row above, so LZ77 copies plus length-limited Huffman codes do far
//! better; transforms, the color cache and entropy images aren't needed.

/// Encodes an RGB image (3 bytes per pixel) as a VP8L bitstream: the payload
/// of a `VP8L` chunk.
pub fn encode_vp8l(rgb: &[u8], width: u32, height: u32) -> Vec<u8> {
    assert!((1..=16384).contains(&width) && (1..=16384).contains(&height));
    let n = (width * height) as usize;
    assert_eq!(rgb.len(), n * 3);
    let pixels: Vec<u32> = rgb
        .as_chunks::<3>()
        .0
        .iter()
        .map(|p| 0xff00_0000 | u32::from(p[0]) << 16 | u32::from(p[1]) << 8 | u32::from(p[2]))
        .collect();
    let tokens = lz77(&pixels, width as usize);

    // Symbol counts for the five codes: green + length prefixes, red, blue,
    // alpha, distance prefixes.
    let mut counts = [
        vec![0u32; 256 + 24],
        vec![0; 256],
        vec![0; 256],
        vec![0; 256],
        vec![0; 40],
    ];
    for t in &tokens {
        match *t {
            Token::Literal(argb) => {
                counts[0][((argb >> 8) & 0xff) as usize] += 1;
                counts[1][((argb >> 16) & 0xff) as usize] += 1;
                counts[2][(argb & 0xff) as usize] += 1;
                counts[3][(argb >> 24) as usize] += 1;
            }
            Token::Copy { length, code } => {
                counts[0][256 + prefix(length).0 as usize] += 1;
                counts[4][prefix(code).0 as usize] += 1;
            }
        }
    }
    let codes: Vec<Code> = counts.iter().map(|c| Code::new(c, 15)).collect();

    let mut w = BitWriter::default();
    w.put(0x2f, 8); // signature
    w.put(width - 1, 14);
    w.put(height - 1, 14);
    w.put(0, 1); // alpha isn't used
    w.put(0, 3); // version
    w.put(0, 1); // no transforms
    w.put(0, 1); // no color cache
    w.put(0, 1); // no entropy image (one set of codes)
    for code in &codes {
        code.write_header(&mut w);
    }
    for t in &tokens {
        match *t {
            Token::Literal(argb) => {
                codes[0].put(&mut w, ((argb >> 8) & 0xff) as usize);
                codes[1].put(&mut w, ((argb >> 16) & 0xff) as usize);
                codes[2].put(&mut w, (argb & 0xff) as usize);
                codes[3].put(&mut w, (argb >> 24) as usize);
            }
            Token::Copy { length, code } => {
                let (p, bits, extra) = prefix(length);
                codes[0].put(&mut w, 256 + p as usize);
                w.put(extra, bits);
                let (p, bits, extra) = prefix(code);
                codes[4].put(&mut w, p as usize);
                w.put(extra, bits);
            }
        }
    }
    w.finish()
}

/// A complete still WebP file from an RGB image.
pub fn encode_still(rgb: &[u8], width: u32, height: u32) -> Vec<u8> {
    let chunk = riff_chunk(b"VP8L", &encode_vp8l(rgb, width, height));
    let mut out = Vec::with_capacity(chunk.len() + 12);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(chunk.len() as u32 + 4).to_le_bytes());
    out.extend_from_slice(b"WEBP");
    out.extend(chunk);
    out
}

/// A RIFF chunk: id, size, data, padded to an even length.
pub fn riff_chunk(id: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut c = Vec::with_capacity(data.len() + 9);
    c.extend_from_slice(id);
    c.extend_from_slice(&(data.len() as u32).to_le_bytes());
    c.extend_from_slice(data);
    if data.len() % 2 == 1 {
        c.push(0);
    }
    c
}

enum Token {
    Literal(u32),
    /// `length` pixels copied from `code`'s distance back (`code` is the
    /// distance code: 1-120 for a nearby pixel, else distance + 120).
    Copy {
        length: u32,
        code: u32,
    },
}

/// Longest copy the format allows.
const MAX_LENGTH: usize = 4096;
/// Farthest copy the distance codes reach.
const MAX_DISTANCE: usize = (1 << 20) - 120;
const HASH_BITS: u32 = 15;
const CHAIN_DEPTH: usize = 24;

/// The pixels as literals and copies: a greedy search over a hash chain of
/// pixel pairs, plus the copies an LED grid is made of (left, row above).
fn lz77(px: &[u32], width: usize) -> Vec<Token> {
    let n = px.len();
    let hash = |i: usize| {
        let h = px[i].wrapping_mul(0x9E37_79B1) ^ px[i + 1].wrapping_mul(0x85EB_CA6B).rotate_left(13);
        (h >> (32 - HASH_BITS)) as usize
    };
    let mut head = vec![usize::MAX; 1 << HASH_BITS];
    let mut prev = vec![usize::MAX; n];
    let insert = |i: usize, head: &mut Vec<usize>, prev: &mut Vec<usize>| {
        if i + 1 < n {
            let h = hash(i);
            prev[i] = head[h];
            head[h] = i;
        }
    };
    let match_len = |a: usize, b: usize| {
        let max = (n - b).min(MAX_LENGTH);
        (0..max).take_while(|&k| px[a + k] == px[b + k]).count()
    };
    let plane = plane_codes(width);
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < n {
        let mut best = (0, 0); // (length, distance)
        let mut consider = |len: usize, dist: usize| {
            if len > best.0 || (len == best.0 && dist < best.1) {
                best = (len, dist);
            }
        };
        for dist in [1, width] {
            if dist <= i {
                consider(match_len(i - dist, i), dist);
            }
        }
        if i + 1 < n {
            let mut cand = head[hash(i)];
            for _ in 0..CHAIN_DEPTH {
                if cand == usize::MAX || i - cand > MAX_DISTANCE {
                    break;
                }
                consider(match_len(cand, i), i - cand);
                cand = prev[cand];
            }
        }
        let (len, dist) = best;
        // A copy pays for itself from 2 pixels when it's a near one (short
        // distance code), else from 3.
        let near = plane.contains_key(&dist);
        if len >= 3 || (len == 2 && near) {
            let code = plane.get(&dist).copied().unwrap_or(dist as u32 + 120);
            tokens.push(Token::Copy {
                length: len as u32,
                code,
            });
            for k in i..i + len {
                insert(k, &mut head, &mut prev);
            }
            i += len;
        } else {
            tokens.push(Token::Literal(px[i]));
            insert(i, &mut head, &mut prev);
            i += 1;
        }
    }
    tokens
}

/// The short codes for nearby pixels: (x, y) offsets back, for an image
/// `width` wide, by distance (RFC 9649 section 3.6.4, the 120-entry table).
fn plane_codes(width: usize) -> std::collections::HashMap<usize, u32> {
    #[rustfmt::skip]
    const MAP: [(i8, i8); 120] = [
        (0, 1),  (1, 0),  (1, 1),  (-1, 1), (0, 2),  (2, 0),  (1, 2),  (-1, 2),
        (2, 1),  (-2, 1), (2, 2),  (-2, 2), (0, 3),  (3, 0),  (1, 3),  (-1, 3),
        (3, 1),  (-3, 1), (2, 3),  (-2, 3), (3, 2),  (-3, 2), (0, 4),  (4, 0),
        (1, 4),  (-1, 4), (4, 1),  (-4, 1), (3, 3),  (-3, 3), (2, 4),  (-2, 4),
        (4, 2),  (-4, 2), (0, 5),  (3, 4),  (-3, 4), (4, 3),  (-4, 3), (5, 0),
        (1, 5),  (-1, 5), (5, 1),  (-5, 1), (2, 5),  (-2, 5), (5, 2),  (-5, 2),
        (4, 4),  (-4, 4), (3, 5),  (-3, 5), (5, 3),  (-5, 3), (0, 6),  (6, 0),
        (1, 6),  (-1, 6), (6, 1),  (-6, 1), (2, 6),  (-2, 6), (6, 2),  (-6, 2),
        (4, 5),  (-4, 5), (5, 4),  (-5, 4), (3, 6),  (-3, 6), (6, 3),  (-6, 3),
        (0, 7),  (7, 0),  (1, 7),  (-1, 7), (5, 5),  (-5, 5), (7, 1),  (-7, 1),
        (4, 6),  (-4, 6), (6, 4),  (-6, 4), (2, 7),  (-2, 7), (7, 2),  (-7, 2),
        (3, 7),  (-3, 7), (7, 3),  (-7, 3), (5, 6),  (-5, 6), (6, 5),  (-6, 5),
        (8, 0),  (4, 7),  (-4, 7), (7, 4),  (-7, 4), (8, 1),  (8, 2),  (6, 6),
        (-6, 6), (8, 3),  (5, 7),  (-5, 7), (7, 5),  (-7, 5), (8, 4),  (6, 7),
        (-6, 7), (7, 6),  (-7, 6), (8, 5),  (7, 7),  (-7, 7), (8, 6),  (8, 7),
    ];
    let mut out = std::collections::HashMap::new();
    for (i, &(x, y)) in MAP.iter().enumerate() {
        let d = i64::from(x) + i64::from(y) * width as i64;
        // The first (shortest) code for a distance wins; the decoder turns
        // anything below 1 into 1, so those can't be used for other distances.
        if d >= 1 {
            out.entry(d as usize).or_insert(i as u32 + 1);
        }
    }
    out
}

/// A value's prefix code, its number of extra bits and their value
/// (RFC 9649 section 3.6.3; `value` is 1-based).
fn prefix(value: u32) -> (u32, u32, u32) {
    let d = value - 1;
    if d < 4 {
        return (d, 0, 0);
    }
    let high = 31 - d.leading_zeros();
    let second = (d >> (high - 1)) & 1;
    let bits = high - 1;
    (2 * high + second, bits, d & ((1 << bits) - 1))
}

/// A canonical Huffman code for one alphabet.
struct Code {
    lengths: Vec<u8>,
    /// Bit-reversed codes (the stream is read least significant bit first).
    codes: Vec<u16>,
    /// The symbols used, when there are at most two (written as a "simple"
    /// code if they fit).
    used: Vec<usize>,
}

impl Code {
    fn new(counts: &[u32], max_len: u8) -> Self {
        let used: Vec<usize> = (0..counts.len()).filter(|&s| counts[s] > 0).collect();
        let lengths = if used.len() <= 1 {
            // One symbol (or none): coded with no bits at all.
            let mut l = vec![0; counts.len()];
            l[used.first().copied().unwrap_or(0)] = 1;
            l
        } else {
            huffman_lengths(counts, max_len)
        };
        let codes = canonical_codes(&lengths);
        Code { lengths, codes, used }
    }

    /// Bits per symbol: none when only one symbol is in use.
    fn single(&self) -> bool {
        self.used.len() <= 1
    }

    fn put(&self, w: &mut BitWriter, symbol: usize) {
        if !self.single() {
            w.put(u32::from(self.codes[symbol]), u32::from(self.lengths[symbol]));
        }
    }

    fn write_header(&self, w: &mut BitWriter) {
        // A "simple" code holds one or two symbols of up to 8 bits.
        let simple_fits = match self.used.as_slice() {
            [] => true,
            [a] => *a < 256,
            [_, b] => *b < 256,
            _ => false,
        };
        if simple_fits {
            let first = self.used.first().copied().unwrap_or(0);
            w.put(1, 1); // simple code
            w.put(self.used.len().max(1) as u32 - 1, 1);
            if first < 2 {
                w.put(0, 1);
                w.put(first as u32, 1);
            } else {
                w.put(1, 1);
                w.put(first as u32, 8);
            }
            if let [_, second] = self.used.as_slice() {
                w.put(*second as u32, 8);
            }
            return;
        }
        // Normal code: the code lengths, run-length coded (16: repeat the
        // previous length 3-6 times; 17: 3-10 zeros; 18: 11-138 zeros), with
        // a Huffman code of their own.
        let mut runs: Vec<(u8, u32, u32)> = Vec::new(); // (symbol, extra bits, extra value)
        let l = &self.lengths;
        let mut i = 0;
        while i < l.len() {
            let v = l[i];
            let run = l[i..].iter().take_while(|&&x| x == v).count();
            if v == 0 && run >= 3 {
                let r = run.min(138);
                if r >= 11 {
                    runs.push((18, 7, r as u32 - 11));
                } else {
                    runs.push((17, 3, r as u32 - 3));
                }
                i += r;
            } else if v != 0 && run >= 4 {
                runs.push((v, 0, 0));
                let mut left = run - 1;
                while left >= 3 {
                    let r = left.min(6);
                    runs.push((16, 2, r as u32 - 3));
                    left -= r;
                }
                for _ in 0..left {
                    runs.push((v, 0, 0));
                }
                i += run;
            } else {
                runs.push((v, 0, 0));
                i += 1;
            }
        }
        let mut counts = [0u32; 19];
        for &(s, _, _) in &runs {
            counts[s as usize] += 1;
        }
        let cl = Code::new(&counts, 7);
        const ORDER: [usize; 19] = [17, 18, 0, 1, 2, 3, 4, 5, 16, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
        let count = ORDER
            .iter()
            .rposition(|&s| cl.lengths[s] != 0)
            .map_or(4, |p| (p + 1).max(4));
        w.put(0, 1); // normal code
        w.put(count as u32 - 4, 4);
        for &s in &ORDER[..count] {
            w.put(u32::from(cl.lengths[s]), 3);
        }
        w.put(0, 1); // all symbols' lengths follow
        for &(s, bits, extra) in &runs {
            cl.put(w, s as usize);
            w.put(extra, bits);
        }
    }
}

/// Huffman code lengths for `counts`, none longer than `max_len`: built
/// the usual way, and if too deep, built again from flattened counts.
fn huffman_lengths(counts: &[u32], max_len: u8) -> Vec<u8> {
    let mut weights: Vec<u64> = counts.iter().map(|&c| u64::from(c)).collect();
    loop {
        let lengths = huffman_once(&weights);
        if lengths.iter().all(|&l| l <= max_len) {
            return lengths;
        }
        for w in weights.iter_mut().filter(|w| **w > 0) {
            *w = (*w).div_ceil(2);
        }
    }
}

fn huffman_once(weights: &[u64]) -> Vec<u8> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;
    // Nodes: leaves first, then internal ones; parent links give depths.
    let mut parent: Vec<usize> = Vec::new();
    let mut heap = BinaryHeap::new();
    let mut leaf_node = vec![usize::MAX; weights.len()];
    for (s, &w) in weights.iter().enumerate().filter(|&(_, &w)| w > 0) {
        leaf_node[s] = parent.len();
        heap.push(Reverse((w, parent.len())));
        parent.push(usize::MAX);
    }
    while heap.len() > 1 {
        let Reverse((wa, a)) = heap.pop().unwrap();
        let Reverse((wb, b)) = heap.pop().unwrap();
        let node = parent.len();
        parent.push(usize::MAX);
        parent[a] = node;
        parent[b] = node;
        heap.push(Reverse((wa + wb, node)));
    }
    leaf_node
        .iter()
        .map(|&n| {
            if n == usize::MAX {
                return 0;
            }
            let (mut depth, mut at) = (0u8, n);
            while parent[at] != usize::MAX {
                at = parent[at];
                depth += 1;
            }
            depth
        })
        .collect()
}

/// Canonical codes for `lengths` (shorter first, then by symbol), bit
/// reversed.
fn canonical_codes(lengths: &[u8]) -> Vec<u16> {
    let max = lengths.iter().copied().max().unwrap_or(0) as usize;
    let mut count = vec![0u16; max + 1];
    for &l in lengths.iter().filter(|&&l| l > 0) {
        count[l as usize] += 1;
    }
    let mut next = vec![0u16; max + 2];
    let mut code = 0u16;
    for len in 1..=max {
        code = (code + count[len - 1]) << 1;
        next[len] = code;
    }
    lengths
        .iter()
        .map(|&l| {
            if l == 0 {
                return 0;
            }
            let c = next[l as usize];
            next[l as usize] += 1;
            c.reverse_bits() >> (16 - l)
        })
        .collect()
}

#[derive(Default)]
struct BitWriter {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl BitWriter {
    fn put(&mut self, value: u32, bits: u32) {
        if bits == 0 {
            return;
        }
        self.acc |= u64::from(value & ((1u64 << bits) - 1) as u32) << self.n;
        self.n += bits;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out.push(self.acc as u8);
        }
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(file: &[u8]) -> (u32, u32, Vec<u8>) {
        let mut d = image_webp::WebPDecoder::new(std::io::Cursor::new(file)).unwrap();
        let (w, h) = d.dimensions();
        let mut buf = vec![0; d.output_buffer_size().unwrap()];
        d.read_image(&mut buf).unwrap();
        let rgb = if d.has_alpha() {
            buf.chunks(4).flat_map(|p| [p[0], p[1], p[2]]).collect()
        } else {
            buf
        };
        (w, h, rgb)
    }

    fn round_trip(rgb: &[u8], w: u32, h: u32) -> usize {
        let file = encode_still(rgb, w, h);
        let (dw, dh, back) = decode(&file);
        assert_eq!((dw, dh), (w, h));
        assert!(back == rgb, "pixels differ for a {w}x{h} image");
        file.len()
    }

    #[test]
    fn prefixes_match_the_specification() {
        // value → (prefix, extra bits, extra) per RFC 9649 section 3.6.3.
        assert_eq!(prefix(1), (0, 0, 0));
        assert_eq!(prefix(4), (3, 0, 0));
        assert_eq!(prefix(5), (4, 1, 0));
        assert_eq!(prefix(6), (4, 1, 1));
        assert_eq!(prefix(7), (5, 1, 0));
        assert_eq!(prefix(9), (6, 2, 0));
        assert_eq!(prefix(4096), (23, 10, 1023));
        for v in 1..5000u32 {
            let (p, bits, extra) = prefix(v);
            let back = if p < 4 {
                p + 1
            } else {
                ((2 + (p & 1)) << ((p - 2) >> 1)) + extra + 1
            };
            assert_eq!((back, bits), (v, if p < 4 { 0 } else { (p - 2) >> 1 }));
        }
    }

    #[test]
    fn images_decode_to_the_same_pixels() {
        // One color, two colors, noise, and an LED-like grid, at odd sizes.
        round_trip(&[7, 8, 9], 1, 1);
        round_trip(&[200; 3 * 37 * 5], 37, 5);
        let two: Vec<u8> = (0..64 * 3).map(|i| if (i / 3) % 2 == 0 { 0 } else { 255 }).collect();
        round_trip(&two, 8, 8);
        let mut seed = 0x1234_5678u32;
        let noise: Vec<u8> = (0..97 * 31 * 3)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                seed as u8
            })
            .collect();
        round_trip(&noise, 97, 31);
        let (w, h) = (440u32, 110u32);
        let grid: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                let (x, y) = (i % w, i / w);
                let lit = (x / 10 + y / 10) % 3 == 0 && x % 10 > 1 && y % 10 > 1;
                if lit { [230, 40, 30] } else { [11, 12, 14] }
            })
            .collect();
        let size = round_trip(&grid, w, h);
        // Repeats make it small: far below the 145 KB of raw pixels.
        assert!(size < 3000, "{size} bytes");
    }
}
