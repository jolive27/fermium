//! Animated GIF encoding (GIF89a, looping): one global 256-colour palette from the most frequent
//! colours of all frames (plots use few colours; antialiasing blends go to the nearest entry), LZW.

use super::raster::Image;
use std::collections::HashMap;

fn palette(frames: &[Image]) -> Vec<[u8; 3]> {
    let mut count: HashMap<[u8; 3], u64> = HashMap::new();
    for f in frames {
        for px in f.rgb.chunks(3) {
            *count.entry([px[0], px[1], px[2]]).or_insert(0) += 1;
        }
    }
    let mut v: Vec<([u8; 3], u64)> = count.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut pal: Vec<[u8; 3]> = v.iter().take(256).map(|e| e.0).collect();
    while pal.len() < 256 {
        pal.push([0, 0, 0]);
    }
    pal
}

fn nearest(pal: &[[u8; 3]], c: [u8; 3]) -> u8 {
    let mut best = (u32::MAX, 0u8);
    for (i, p) in pal.iter().enumerate() {
        let d: u32 = (0..3).map(|k| (p[k] as i32 - c[k] as i32).pow(2) as u32).sum();
        if d < best.0 {
            best = (d, i as u8);
            if d == 0 {
                break;
            }
        }
    }
    best.1
}

fn lzw(indices: &[u8]) -> Vec<u8> {
    let min_code = 8u32;
    let clear = 1u32 << min_code;
    let eoi = clear + 1;
    let mut out = Vec::new();
    let (mut acc, mut nbits) = (0u64, 0u32);
    let mut emit = |code: u32, width: u32, out: &mut Vec<u8>| {
        acc |= (code as u64) << nbits;
        nbits += width;
        while nbits >= 8 {
            out.push(acc as u8);
            acc >>= 8;
            nbits -= 8;
        }
    };
    let mut dict: HashMap<(u32, u8), u32> = HashMap::new();
    let mut next = eoi + 1;
    let mut width = min_code + 1;
    emit(clear, width, &mut out);
    let mut cur: Option<u32> = None;
    for &b in indices {
        match cur {
            None => cur = Some(b as u32),
            Some(p) => {
                if let Some(&c) = dict.get(&(p, b)) {
                    cur = Some(c);
                } else {
                    emit(p, width, &mut out);
                    if next < 4096 {
                        dict.insert((p, b), next);
                        next += 1;
                        if next > (1 << width) && width < 12 {
                            width += 1;
                        }
                    } else {
                        emit(clear, width, &mut out);
                        dict.clear();
                        next = eoi + 1;
                        width = min_code + 1;
                    }
                    cur = Some(b as u32);
                }
            }
        }
    }
    if let Some(p) = cur {
        emit(p, width, &mut out);
    }
    emit(eoi, width, &mut out);
    if nbits > 0 {
        out.push(acc as u8);
    }
    out
}

/// A looping animated GIF of the frames (all the same size) at `fps` frames per second.
pub fn encode_animation(frames: &[Image], fps: u32) -> Vec<u8> {
    let (w, h) = (frames[0].w as u16, frames[0].h as u16);
    let pal = palette(frames);
    let mut out = b"GIF89a".to_vec();
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&h.to_le_bytes());
    out.extend_from_slice(&[0xF7, 0, 0]); // global colour table of 256 entries
    for c in &pal {
        out.extend_from_slice(c);
    }
    out.extend_from_slice(&[0x21, 0xFF, 0x0B]);
    out.extend_from_slice(b"NETSCAPE2.0");
    out.extend_from_slice(&[3, 1, 0, 0, 0]); // loop forever
    let delay = (100 / fps.max(1)) as u16;
    let mut cache: HashMap<[u8; 3], u8> = HashMap::new();
    for f in frames {
        out.extend_from_slice(&[0x21, 0xF9, 4, 0]);
        out.extend_from_slice(&delay.to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        out.push(0x2C);
        out.extend_from_slice(&[0, 0, 0, 0]);
        out.extend_from_slice(&w.to_le_bytes());
        out.extend_from_slice(&h.to_le_bytes());
        out.push(0);
        let idx: Vec<u8> = f
            .rgb
            .chunks(3)
            .map(|p| {
                let c = [p[0], p[1], p[2]];
                *cache.entry(c).or_insert_with(|| nearest(&pal, c))
            })
            .collect();
        out.push(8);
        for block in lzw(&idx).chunks(255) {
            out.push(block.len() as u8);
            out.extend_from_slice(block);
        }
        out.push(0);
    }
    out.push(0x3B);
    out
}

/// LZW decoder for tests: the inverse of [`lzw`].
#[cfg(test)]
pub fn unlzw(data: &[u8]) -> Vec<u8> {
    let clear = 256u32;
    let eoi = 257u32;
    let mut pos = 0usize;
    let mut width = 9u32;
    let read = |pos: &mut usize, width: u32| -> u32 {
        let mut v = 0;
        for i in 0..width {
            let bit = (data[*pos / 8] >> (*pos % 8)) & 1;
            v |= (bit as u32) << i;
            *pos += 1;
        }
        v
    };
    let mut table: Vec<Vec<u8>> = (0..258).map(|i| if i < 256 { vec![i as u8] } else { vec![] }).collect();
    let mut out = Vec::new();
    let mut prev: Option<Vec<u8>> = None;
    loop {
        let code = read(&mut pos, width);
        if code == clear {
            table.truncate(258);
            width = 9;
            prev = None;
            continue;
        }
        if code == eoi {
            break;
        }
        let entry = if (code as usize) < table.len() {
            table[code as usize].clone()
        } else {
            let p = prev.clone().unwrap();
            let mut e = p.clone();
            e.push(p[0]);
            e
        };
        out.extend_from_slice(&entry);
        if let Some(p) = prev {
            if table.len() < 4096 {
                let mut e = p.clone();
                e.push(entry[0]);
                table.push(e);
                if table.len() == (1 << width) && width < 12 {
                    width += 1;
                }
            }
        }
        prev = Some(entry);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lzw_round_trip() {
        let mut data: Vec<u8> = (0..20000u32).map(|i| ((i / 7) % 5) as u8).collect();
        data.extend((0..9000u32).map(|i| (i.wrapping_mul(2654435761) >> 24) as u8));
        assert_eq!(unlzw(&lzw(&data)), data);
    }
}
