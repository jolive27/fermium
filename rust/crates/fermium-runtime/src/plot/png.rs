//! PNG encoding (RGB, 8 bits) with our own zlib/deflate: LZ77 with hash chains and the fixed
//! Huffman code (RFC 1951 §3.2.6), per-row PNG filters chosen by the minimum-sum heuristic.

use super::raster::Image;

pub fn crc32(data: &[u8]) -> u32 {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let t = TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        for n in 0..256u32 {
            let mut c = n;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 };
            }
            t[n as usize] = c;
        }
        t
    });
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = t[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for &x in chunk {
            a += x as u32;
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    (b << 16) | a
}

struct Bits {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl Bits {
    fn put(&mut self, v: u32, len: u32) {
        self.acc |= (v as u64) << self.n;
        self.n += len;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }
    /// a Huffman code, most significant bit first
    fn code(&mut self, code: u32, len: u32) {
        let mut r = 0;
        for i in 0..len {
            r |= ((code >> i) & 1) << (len - 1 - i);
        }
        self.put(r, len);
    }
    fn flush(&mut self) {
        if self.n > 0 {
            self.out.push(self.acc as u8);
        }
        self.acc = 0;
        self.n = 0;
    }
}

fn lit(b: &mut Bits, v: u32) {
    match v {
        0..=143 => b.code(0x30 + v, 8),
        144..=255 => b.code(0x190 + (v - 144), 9),
        256..=279 => b.code(v - 256, 7),
        _ => b.code(0xC0 + (v - 280), 8),
    }
}

const LBASE: [u32; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEXT: [u32; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DBASE: [u32; 30] = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DEXT: [u32; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

/// zlib stream (deflate, one fixed-Huffman block) of `data`.
pub fn zlib(data: &[u8]) -> Vec<u8> {
    let mut b = Bits { out: vec![0x78, 0x01], acc: 0, n: 0 };
    b.put(1, 1); // final block
    b.put(1, 2); // fixed Huffman
    const HBITS: usize = 15;
    const WINDOW: usize = 32768;
    let mut head = vec![usize::MAX; 1 << HBITS];
    let mut prev = vec![usize::MAX; data.len().max(1)];
    let hash = |i: usize| -> usize {
        ((data[i] as usize) << 10 ^ (data[i + 1] as usize) << 5 ^ data[i + 2] as usize) & ((1 << HBITS) - 1)
    };
    let mut i = 0;
    let n = data.len();
    let insert = |i: usize, head: &mut Vec<usize>, prev: &mut Vec<usize>| {
        if i + 2 < n {
            let h = hash(i);
            prev[i] = head[h];
            head[h] = i;
        }
    };
    while i < n {
        let mut best_len = 0;
        let mut best_dist = 0;
        if i + 2 < n {
            let mut cand = head[hash(i)];
            let mut chain = 0;
            while cand != usize::MAX && i - cand <= WINDOW && chain < 64 {
                let maxl = (n - i).min(258);
                let mut l = 0;
                while l < maxl && data[cand + l] == data[i + l] {
                    l += 1;
                }
                if l > best_len {
                    best_len = l;
                    best_dist = i - cand;
                    if l == maxl {
                        break;
                    }
                }
                cand = prev[cand];
                chain += 1;
            }
        }
        if best_len >= 3 {
            let li = LBASE.iter().rposition(|&x| x <= best_len as u32).unwrap();
            lit(&mut b, 257 + li as u32);
            b.put(best_len as u32 - LBASE[li], LEXT[li]);
            let di = DBASE.iter().rposition(|&x| x <= best_dist as u32).unwrap();
            b.code(di as u32, 5);
            b.put(best_dist as u32 - DBASE[di], DEXT[di]);
            for k in 0..best_len {
                insert(i + k, &mut head, &mut prev);
            }
            i += best_len;
        } else {
            lit(&mut b, data[i] as u32);
            insert(i, &mut head, &mut prev);
            i += 1;
        }
    }
    lit(&mut b, 256);
    b.flush();
    let mut out = b.out;
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

/// A tiny inflater for tests (stored and fixed-Huffman blocks only): the inverse of [`zlib`].
#[cfg(test)]
pub fn inflate_fixed(z: &[u8]) -> Vec<u8> {
    let d = &z[2..z.len() - 4];
    let mut pos = 0usize;
    let bit = |pos: &mut usize| -> u32 {
        let v = (d[*pos / 8] >> (*pos % 8)) & 1;
        *pos += 1;
        v as u32
    };
    let bits = |pos: &mut usize, n: u32| -> u32 {
        let mut v = 0;
        for i in 0..n {
            v |= bit(pos) << i;
        }
        v
    };
    let mut out: Vec<u8> = Vec::new();
    let _final = bits(&mut pos, 1);
    let _ty = bits(&mut pos, 2);
    loop {
        // decode a fixed literal/length code
        let mut code = 0u32;
        let mut len = 0;
        let sym;
        loop {
            code = (code << 1) | bit(&mut pos);
            len += 1;
            if len == 7 && code <= 0x17 {
                sym = code + 256;
                break;
            }
            if len == 8 && (0x30..=0xBF).contains(&code) {
                sym = code - 0x30;
                break;
            }
            if len == 8 && (0xC0..=0xC7).contains(&code) {
                sym = code - 0xC0 + 280;
                break;
            }
            if len == 9 && (0x190..=0x1FF).contains(&code) {
                sym = code - 0x190 + 144;
                break;
            }
        }
        if sym < 256 {
            out.push(sym as u8);
        } else if sym == 256 {
            break;
        } else {
            let li = (sym - 257) as usize;
            let l = LBASE[li] + bits(&mut pos, LEXT[li]);
            let mut dc = 0;
            for _ in 0..5 {
                dc = (dc << 1) | bit(&mut pos);
            }
            let dist = DBASE[dc as usize] + bits(&mut pos, DEXT[dc as usize]);
            for _ in 0..l {
                let v = out[out.len() - dist as usize];
                out.push(v);
            }
        }
    }
    out
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut c = kind.to_vec();
    c.extend_from_slice(data);
    out.extend_from_slice(&c);
    out.extend_from_slice(&crc32(&c).to_be_bytes());
}

/// PNG bytes of an RGB image.
pub fn encode_rgb(img: &Image) -> Vec<u8> {
    let (w, h) = (img.w, img.h);
    let row = w * 3;
    let mut raw = Vec::with_capacity((row + 1) * h);
    let zero = vec![0u8; row];
    for y in 0..h {
        let cur = &img.rgb[y * row..(y + 1) * row];
        let up = if y > 0 { &img.rgb[(y - 1) * row..y * row] } else { &zero[..] };
        // candidates: None, Sub, Up; keep the one with the smallest sum of |signed bytes|
        let mut best: (u64, u8, Vec<u8>) = (u64::MAX, 0, Vec::new());
        for f in 0..3u8 {
            let v: Vec<u8> = (0..row)
                .map(|i| match f {
                    0 => cur[i],
                    1 => cur[i].wrapping_sub(if i >= 3 { cur[i - 3] } else { 0 }),
                    _ => cur[i].wrapping_sub(up[i]),
                })
                .collect();
            let score: u64 = v.iter().map(|&b| (b as i8).unsigned_abs() as u64).sum();
            if score < best.0 {
                best = (score, f, v);
            }
        }
        raw.push(best.1);
        raw.extend_from_slice(&best.2);
    }
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &zlib(&raw));
    chunk(&mut out, b"IEND", &[]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deflate_round_trip_and_checksums() {
        assert_eq!(crc32(b"123456789"), 0xCBF43926);
        assert_eq!(adler32(b"Wikipedia"), 0x11E60398);
        let mut data: Vec<u8> = (0..5000u32).map(|i| ((i * 7) % 13) as u8).collect();
        data.extend(std::iter::repeat(255u8).take(3000));
        data.extend((0..300u32).map(|i| (i * i % 251) as u8));
        let z = zlib(&data);
        assert!(z.len() < data.len() / 4);
        assert_eq!(inflate_fixed(&z), data);
    }
}
