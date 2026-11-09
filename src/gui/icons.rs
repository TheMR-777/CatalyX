//! Vector icons drawn with the painter in a unit box: crisp at any scale and DPI, no icon font
//! or image files to load (which keeps the program small and its memory use low).

use egui::{pos2, vec2, Color32, Painter, Pos2, Rect, Shape, Stroke};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ico {
    Bolt,
    Gamepad,
    Apps,
    Layers,
    Sliders,
    Gauge,
    Gear,
    Check,
    Lock,
    Trash,
    Refresh,
    Info,
    Moon,
    Sun,
    Restore,
    Power,
}

pub fn paint(p: &Painter, icon: Ico, rect: Rect, color: Color32) {
    let sw = (rect.width() / 11.0).clamp(1.3, 2.2);
    let stroke = Stroke::new(sw, color);
    let inset = rect.size().min_elem() * 0.1;
    let b = rect.shrink(inset);
    let at = |x: f32, y: f32| -> Pos2 { pos2(b.left() + x * b.width(), b.top() + y * b.height()) };
    let line = |a: Pos2, c: Pos2| {
        p.line_segment([a, c], stroke);
    };
    let path = |pts: &[Pos2]| {
        p.add(Shape::line(pts.to_vec(), stroke));
    };
    let closed = |pts: &[Pos2]| {
        p.add(Shape::closed_line(pts.to_vec(), stroke));
    };
    let r = |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(at(x, y), vec2(w * b.width(), h * b.height()));
    let round = |rc: Rect, rad: f32| {
        p.rect_stroke(rc, rad, stroke, egui::StrokeKind::Middle);
    };
    let c = b.center();
    let u = b.width();
    match icon {
        Ico::Bolt => {
            p.add(Shape::convex_polygon(
                vec![at(0.58, 0.04), at(0.18, 0.56), at(0.46, 0.56), at(0.38, 0.96), at(0.82, 0.4), at(0.54, 0.4)],
                color,
                Stroke::NONE,
            ));
        }
        Ico::Gamepad => {
            round(r(0.06, 0.26, 0.88, 0.5), u * 0.22);
            line(at(0.28, 0.42), at(0.28, 0.6));
            line(at(0.19, 0.51), at(0.37, 0.51));
            p.circle_filled(at(0.68, 0.47), u * 0.05, color);
            p.circle_filled(at(0.78, 0.57), u * 0.05, color);
        }
        Ico::Apps => {
            let s = 0.36;
            round(r(0.1, 0.1, s, s), u * 0.06);
            round(r(0.54, 0.1, s, s), u * 0.06);
            round(r(0.1, 0.54, s, s), u * 0.06);
            round(r(0.54, 0.54, s, s), u * 0.06);
        }
        Ico::Layers => {
            closed(&[at(0.5, 0.1), at(0.92, 0.32), at(0.5, 0.54), at(0.08, 0.32)]);
            path(&[at(0.08, 0.5), at(0.5, 0.72), at(0.92, 0.5)]);
            path(&[at(0.08, 0.68), at(0.5, 0.9), at(0.92, 0.68)]);
        }
        Ico::Sliders => {
            for (y, k) in [(0.22, 0.65), (0.5, 0.3), (0.78, 0.72)] {
                line(at(0.08, y), at(0.92, y));
                p.circle_filled(at(k, y), u * 0.095, color);
            }
        }
        Ico::Gauge => {
            let steps = 24;
            let pts: Vec<Pos2> = (0..=steps)
                .map(|i| {
                    let a = std::f32::consts::PI * (0.85 + 1.3 * i as f32 / steps as f32);
                    c + vec2(a.cos(), a.sin()) * u * 0.44
                })
                .collect();
            path(&pts);
            line(c, at(0.7, 0.34));
            p.circle_filled(c, u * 0.06, color);
        }
        Ico::Gear => {
            p.circle_stroke(c, u * 0.16, stroke);
            p.circle_stroke(c, u * 0.34, stroke);
            for k in 0..8 {
                let a = std::f32::consts::TAU * k as f32 / 8.0;
                let d = vec2(a.cos(), a.sin());
                line(c + d * u * 0.34, c + d * u * 0.47);
            }
        }
        Ico::Check => path(&[at(0.14, 0.54), at(0.4, 0.78), at(0.86, 0.24)]),
        Ico::Lock => {
            round(r(0.2, 0.44, 0.6, 0.44), u * 0.08);
            path(&[at(0.32, 0.44), at(0.32, 0.3), at(0.4, 0.14), at(0.5, 0.1), at(0.6, 0.14), at(0.68, 0.3), at(0.68, 0.44)]);
        }
        Ico::Trash => {
            line(at(0.14, 0.26), at(0.86, 0.26));
            path(&[at(0.38, 0.26), at(0.4, 0.12), at(0.6, 0.12), at(0.62, 0.26)]);
            path(&[at(0.24, 0.26), at(0.3, 0.9), at(0.7, 0.9), at(0.76, 0.26)]);
            line(at(0.5, 0.42), at(0.5, 0.74));
        }
        Ico::Refresh => {
            let steps = 20;
            let pts: Vec<Pos2> = (0..=steps)
                .map(|i| {
                    let a = std::f32::consts::PI * (-0.35 + 1.7 * i as f32 / steps as f32);
                    c + vec2(a.cos(), a.sin()) * u * 0.38
                })
                .collect();
            path(&pts);
            let tip = *pts.last().unwrap();
            path(&[tip + vec2(-u * 0.02, -u * 0.2), tip, tip + vec2(u * 0.2, -u * 0.04)]);
        }
        Ico::Info => {
            p.circle_stroke(c, u * 0.42, stroke);
            p.circle_filled(at(0.5, 0.31), sw * 0.8, color);
            line(at(0.5, 0.46), at(0.5, 0.72));
        }
        Ico::Moon => {
            // A crescent: the part of the big circle that lies outside an offset smaller circle.
            let (big, small) = (u * 0.42, u * 0.34);
            let c2 = c + vec2(0.2 * u, -0.16 * u);
            let n = 64usize;
            let on = |centre: Pos2, r: f32, k: usize| {
                let a = std::f32::consts::TAU * k as f32 / n as f32;
                centre + vec2(a.cos(), a.sin()) * r
            };
            // Outer edge: big-circle points that are not inside the small circle, starting just after a gap.
            let outside: Vec<bool> = (0..n).map(|k| on(c, big, k).distance(c2) >= small).collect();
            let start = (0..n).find(|k| outside[*k] && !outside[(k + n - 1) % n]).unwrap_or(0);
            let outer: Vec<Pos2> = (0..n).map(|k| (start + k) % n).take_while(|k| outside[*k]).map(|k| on(c, big, k)).collect();
            // Inner edge: small-circle points that are inside the big circle.
            let inside: Vec<bool> = (0..n).map(|k| on(c2, small, k).distance(c) < big).collect();
            let istart = (0..n).find(|k| inside[*k] && !inside[(k + n - 1) % n]).unwrap_or(0);
            let mut inner: Vec<Pos2> = (0..n).map(|k| (istart + k) % n).take_while(|k| inside[*k]).map(|k| on(c2, small, k)).collect();
            if let (Some(end), Some(first)) = (outer.last(), inner.first()) {
                if end.distance(*first) > end.distance(*inner.last().unwrap_or(first)) {
                    inner.reverse();
                }
            }
            let mut pts = outer;
            pts.extend(inner);
            if pts.len() > 3 {
                closed(&pts);
            } else {
                p.circle_stroke(c, big, stroke);
            }
        }
        Ico::Sun => {
            p.circle_stroke(c, u * 0.19, stroke);
            for k in 0..8 {
                let a = std::f32::consts::TAU * k as f32 / 8.0;
                let d = vec2(a.cos(), a.sin());
                line(c + d * u * 0.3, c + d * u * 0.44);
            }
        }
        Ico::Restore => {
            let steps = 20;
            let pts: Vec<Pos2> = (0..=steps)
                .map(|i| {
                    let a = std::f32::consts::PI * (1.0 + 1.5 * i as f32 / steps as f32);
                    c + vec2(a.cos(), a.sin()) * u * 0.36
                })
                .collect();
            path(&pts);
            let tip = pts[0];
            path(&[tip + vec2(-u * 0.02, u * 0.2), tip, tip + vec2(-u * 0.2, u * 0.0)]);
        }
        Ico::Power => {
            let steps = 20;
            let pts: Vec<Pos2> = (0..=steps)
                .map(|i| {
                    // A ring open at the top: from just right of 12 o'clock all the way round.
                    let a = -std::f32::consts::FRAC_PI_2 + 0.55 + (std::f32::consts::TAU - 1.1) * i as f32 / steps as f32;
                    c + vec2(a.cos(), a.sin()) * u * 0.38
                })
                .collect();
            path(&pts);
            line(at(0.5, 0.08), at(0.5, 0.5));
        }
    }
}

/// Paint an icon centred at `centre` with the given size.
pub fn paint_at(p: &Painter, icon: Ico, centre: Pos2, size: f32, color: Color32) {
    paint(p, icon, Rect::from_center_size(centre, vec2(size, size)), color);
}
