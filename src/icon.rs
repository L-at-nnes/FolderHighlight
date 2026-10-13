//! Procedural folder icon: renders a shaded folder in any color, encoded as a multi-size PNG .ico.

pub const SIZES: [usize; 7] = [16, 24, 32, 48, 64, 128, 256];
const SS: usize = 4; // supersampling per axis

type Rgb = [f32; 3];

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn in_rrect(x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> bool {
    if x < x0 || x > x1 || y < y0 || y > y1 {
        return false;
    }
    let cx = x.clamp(x0 + r, x1 - r);
    let cy = y.clamp(y0 + r, y1 - r);
    (x - cx).powi(2) + (y - cy).powi(2) <= r * r
}

/// Color at a point of the 256x256 canvas, or None if transparent.
fn shade(x: f32, y: f32, base: Rgb) -> Option<Rgb> {
    const WHITE: Rgb = [1.0, 1.0, 1.0];
    const BLACK: Rgb = [0.0, 0.0, 0.0];
    if in_rrect(x, y, 12.0, 88.0, 244.0, 224.0, 16.0) {
        let top = mix(base, WHITE, 0.28);
        let c = mix(top, base, ((y - 88.0) / 136.0).clamp(0.0, 1.0));
        return Some(if y < 94.0 { mix(c, WHITE, 0.45) } else { c });
    }
    if in_rrect(x, y, 30.0, 66.0, 226.0, 130.0, 6.0) {
        return Some([0.96, 0.96, 0.97]);
    }
    if in_rrect(x, y, 12.0, 50.0, 244.0, 214.0, 16.0) || in_rrect(x, y, 12.0, 30.0, 112.0, 80.0, 14.0) {
        return Some(mix(base, BLACK, 0.28));
    }
    None
}

fn render(size: usize, base: Rgb) -> Vec<u8> {
    let mut out = Vec::with_capacity(size * size * 4);
    let scale = 256.0 / size as f32;
    for py in 0..size {
        for px in 0..size {
            let (mut r, mut g, mut b, mut n) = (0.0, 0.0, 0.0, 0.0);
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = (px as f32 + (sx as f32 + 0.5) / SS as f32) * scale;
                    let y = (py as f32 + (sy as f32 + 0.5) / SS as f32) * scale;
                    if let Some(c) = shade(x, y, base) {
                        r += c[0];
                        g += c[1];
                        b += c[2];
                        n += 1.0;
                    }
                }
            }
            if n == 0.0 {
                out.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                let a = n / (SS * SS) as f32;
                out.extend_from_slice(&[
                    (r / n * 255.0 + 0.5) as u8,
                    (g / n * 255.0 + 0.5) as u8,
                    (b / n * 255.0 + 0.5) as u8,
                    (a * 255.0 + 0.5) as u8,
                ]);
            }
        }
    }
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
        }
    }
    !c
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn png(size: usize, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity((size * 4 + 1) * size);
    for row in rgba.chunks(size * 4) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(size as u32).to_be_bytes());
    ihdr.extend_from_slice(&(size as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &miniz_oxide::deflate::compress_to_vec_zlib(&raw, 9));
    chunk(&mut out, b"IEND", &[]);
    out
}

/// Builds the bytes of a .ico file for a folder of the given 0xRRGGBB color.
pub fn folder_ico(rgb: u32, sizes: &[usize]) -> Vec<u8> {
    let base = [
        (rgb >> 16 & 0xFF) as f32 / 255.0,
        (rgb >> 8 & 0xFF) as f32 / 255.0,
        (rgb & 0xFF) as f32 / 255.0,
    ];
    let images: Vec<Vec<u8>> = sizes.iter().map(|&s| png(s, &render(s, base))).collect();
    let mut out = vec![0, 0, 1, 0];
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len();
    for (&s, img) in sizes.iter().zip(&images) {
        let d = if s >= 256 { 0 } else { s as u8 };
        out.extend_from_slice(&[d, d, 0, 0, 1, 0, 32, 0]);
        out.extend_from_slice(&(img.len() as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += img.len();
    }
    for img in images {
        out.extend_from_slice(&img);
    }
    out
}
