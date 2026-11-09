//! The CatalyX mark: an X woven from two rounded strokes. The rising "/" (the boost) is in the accent
//! and passes over the "\" with a thin gap where they cross. One geometry, three uses: the tile in the
//! title bar (`paint`), the window and taskbar icon (`icon_rgba`), and the program's own icon
//! (`ico_bytes`, written to assets/catalyx.ico by `catalyx --write-icon`).

use egui::{Color32, Painter, Pos2, Stroke};

// Unit square, y down. A = the rising stroke, B = the one it crosses.
const A: ((f32, f32), (f32, f32)) = ((0.275, 0.725), (0.725, 0.275));
const B: ((f32, f32), (f32, f32)) = ((0.275, 0.275), (0.725, 0.725));
const A_W: f32 = 0.150;
const B_W: f32 = 0.110;
/// Extra width of the gap cut into B around A.
const GAP: f32 = 0.090;

/// Draws the mark centred on `c`, `size` px wide. `tile` is the flat colour behind it (the gap is cut
/// in that colour), `under` colours the "\" stroke, `over` the "/" stroke.
pub fn paint(p: &Painter, c: Pos2, size: f32, tile: Color32, under: Color32, over: Color32) {
    let at = |(u, v): (f32, f32)| Pos2::new(c.x + (u - 0.5) * size, c.y + (v - 0.5) * size);
    let capsule = |seg: ((f32, f32), (f32, f32)), w: f32, col: Color32| {
        let (a, b) = (at(seg.0), at(seg.1));
        let w = w * size;
        p.line_segment([a, b], Stroke::new(w, col));
        p.circle_filled(a, w / 2.0, col);
        p.circle_filled(b, w / 2.0, col);
    };
    capsule(B, B_W, under);
    capsule(A, A_W + GAP, tile);
    capsule(A, A_W, over);
}

fn dist_to_segment(x: f32, y: f32, s: ((f32, f32), (f32, f32))) -> f32 {
    let ((ax, ay), (bx, by)) = s;
    let (dx, dy) = (bx - ax, by - ay);
    let t = (((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    let (px, py) = (ax + t * dx, ay + t * dy);
    ((x - px) * (x - px) + (y - py) * (y - py)).sqrt()
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// The icon as RGBA, `n` x `n` pixels: a dark squircle, the "\" in soft white, the "/" in an ember
/// gradient. Anti-aliased by `ss` x `ss` supersampling.
pub fn rgba(n: usize, ss: usize) -> Vec<u8> {
    let (top, bottom) = ([36.0f32, 36.0, 43.0], [19.0f32, 19.0, 24.0]);
    let (a_top, a_bottom) = ([255.0f32, 176.0, 116.0], [233.0f32, 98.0, 42.0]);
    let b_col = [228.0f32, 228.0, 238.0];
    let in_squircle = |u: f32, v: f32| {
        let r = 0.22f32;
        let (dx, dy) = ((u - 0.5).abs() - (0.5 - r), (v - 0.5).abs() - (0.5 - r));
        dx <= 0.0 || dy <= 0.0 || dx * dx + dy * dy <= r * r
    };
    let mut out = vec![0u8; n * n * 4];
    for py in 0..n {
        for px in 0..n {
            let (mut covered, mut sum) = (0u32, [0.0f32; 3]);
            for sy in 0..ss {
                for sx in 0..ss {
                    let u = (px as f32 + (sx as f32 + 0.5) / ss as f32) / n as f32;
                    let v = (py as f32 + (sy as f32 + 0.5) / ss as f32) / n as f32;
                    if !in_squircle(u, v) {
                        continue;
                    }
                    covered += 1;
                    let mut col = mix(top, bottom, v);
                    let in_b = dist_to_segment(u, v, B) <= B_W / 2.0;
                    let in_gap = dist_to_segment(u, v, A) <= (A_W + GAP) / 2.0;
                    let in_a = dist_to_segment(u, v, A) <= A_W / 2.0;
                    if in_b && !in_gap {
                        col = mix(col, b_col, 0.92);
                    }
                    if in_a {
                        // The gradient runs along the stroke, from its upper end to its lower end.
                        col = mix(a_top, a_bottom, ((v - A.1 .1) / (A.0 .1 - A.1 .1)).clamp(0.0, 1.0));
                    }
                    for k in 0..3 {
                        sum[k] += col[k];
                    }
                }
            }
            let i = (py * n + px) * 4;
            if covered > 0 {
                for k in 0..3 {
                    out[i + k] = (sum[k] / covered as f32).round() as u8;
                }
            }
            out[i + 3] = (covered as f32 / (ss * ss) as f32 * 255.0).round() as u8;
        }
    }
    out
}

/// The window and taskbar icon.
pub fn icon_data() -> egui::IconData {
    egui::IconData { rgba: rgba(64, 3), width: 64, height: 64 }
}

fn png(n: usize, ss: usize) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut buf, n as u32, n as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        if let Ok(mut w) = enc.write_header() {
            let _ = w.write_image_data(&rgba(n, ss));
        }
    }
    buf
}

/// A complete .ico file (PNG-compressed images, 16 to 256 px) for the program's own icon.
pub fn ico_bytes() -> Vec<u8> {
    let sizes: [(usize, usize); 5] = [(16, 8), (24, 6), (32, 6), (48, 4), (256, 3)];
    let images: Vec<Vec<u8>> = sizes.iter().map(|&(n, ss)| png(n, ss)).collect();
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (&(n, _), img) in sizes.iter().zip(&images) {
        let dim = if n >= 256 { 0u8 } else { n as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes()); // planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&(img.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += img.len() as u32;
    }
    for img in &images {
        out.extend_from_slice(img);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_are_transparent_and_the_centre_is_opaque() {
        let n = 64;
        let px = rgba(n, 2);
        let alpha = |x: usize, y: usize| px[(y * n + x) * 4 + 3];
        assert_eq!(alpha(0, 0), 0, "the squircle's corner is cut");
        assert_eq!(alpha(n / 2, n / 2), 255);
        assert_eq!(px.len(), n * n * 4);
    }

    #[test]
    fn the_rising_stroke_is_ember_where_the_strokes_do_not_meet() {
        let n = 64;
        let px = rgba(n, 2);
        // A point on the "/" stroke, away from the crossing: warm (red well above blue).
        let (x, y) = ((0.66 * n as f32) as usize, (0.34 * n as f32) as usize);
        let i = (y * n + x) * 4;
        assert!(px[i] > 200 && px[i + 2] < 140, "got rgb {:?}", &px[i..i + 3]);
    }

    #[test]
    fn the_icon_file_is_a_valid_ico_with_five_sizes() {
        let ico = ico_bytes();
        assert_eq!(ico[0..4], [0, 0, 1, 0]);
        assert_eq!(u16::from_le_bytes([ico[4], ico[5]]), 5);
        // Every entry points inside the file and at a PNG.
        for i in 0..5 {
            let e = &ico[6 + i * 16..6 + (i + 1) * 16];
            let size = u32::from_le_bytes([e[8], e[9], e[10], e[11]]) as usize;
            let off = u32::from_le_bytes([e[12], e[13], e[14], e[15]]) as usize;
            assert!(off + size <= ico.len());
            assert_eq!(ico[off..off + 4], [0x89, b'P', b'N', b'G']);
        }
    }
}
