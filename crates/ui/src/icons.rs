//! Monochrome line icons for the bar (spec: M2 §2.1). Each is centred on `c` inside a box of
//! side `s` (physical px); `w` is the stroke width. Fractional inputs (`bars`, `waves`, `mute`,
//! `turn`) let callers animate them; cross-fades pass a colour with a lower alpha.

use crate::{Color, Painter, PointF, RectF};

fn at(c: PointF, dx: f32, dy: f32) -> PointF {
    PointF { x: c.x + dx, y: c.y + dy }
}

/// Opacity of the `k`-th (1-based) of several arcs when `level` of them are lit.
fn lit(level: f32, k: u8) -> f32 {
    (level - f32::from(k - 1)).clamp(0.0, 1.0)
}

/// The three points of `^`, turned by `turn` × 180° around `c`.
pub fn chevron_points(c: PointF, s: f32, turn: f32) -> [PointF; 3] {
    let (a, h) = (s * 0.32, s * 0.18);
    let (sin, cos) = (std::f32::consts::PI * turn).sin_cos();
    let rot = |dx: f32, dy: f32| at(c, dx * cos - dy * sin, dx * sin + dy * cos);
    [rot(-a, h), rot(0.0, -h), rot(a, h)]
}

/// The tray button: `^`, pointing down when the tray is open (`turn` = 1).
pub fn chevron(p: &mut Painter, c: PointF, s: f32, turn: f32, color: Color, w: f32) {
    p.stroke_polyline(&chevron_points(c, s, turn), color, w);
}

/// An Ethernet port with three pins.
pub fn ethernet(p: &mut Painter, c: PointF, s: f32, color: Color, w: f32) {
    let (hw, hh, tab, notch) = (s * 0.34, s * 0.26, s * 0.16, s * 0.12);
    let (l, r, t, b) = (c.x - hw, c.x + hw, c.y - hh + notch / 2.0, c.y + hh + notch / 2.0);
    let outline = [
        PointF { x: l, y: b },
        PointF { x: l, y: t },
        PointF { x: c.x - tab, y: t },
        PointF { x: c.x - tab, y: t - notch },
        PointF { x: c.x + tab, y: t - notch },
        PointF { x: c.x + tab, y: t },
        PointF { x: r, y: t },
        PointF { x: r, y: b },
    ];
    p.stroke_polygon(&outline, color, w);
    for k in [-1.0, 0.0, 1.0] {
        let x = c.x + k * hw * 0.45;
        p.line(PointF { x, y: b - hh * 0.75 }, PointF { x, y: b - hh * 0.25 }, color, w);
    }
}

/// Wi-Fi: a dot and three arcs; `bars` (0..=3) of them lit.
pub fn wifi(p: &mut Painter, c: PointF, s: f32, bars: f32, color: Color, w: f32) {
    // The fan is centred on its own box (dot bottom to outer arc top), not on the arcs' origin,
    // and the origin sits on a pixel centre so the arcs stay symmetric and crisp.
    let (outer, dot) = (s * 0.64, (s * 0.08).max(w * 0.9));
    let base = PointF { x: c.x.floor() + 0.5, y: (c.y + (outer - dot) / 2.0).floor() + 0.5 };
    p.fill_round_rect(RectF::new(base.x - dot, base.y - dot, 2.0 * dot, 2.0 * dot), dot, color);
    for k in 1..=3u8 {
        let a = lit(bars, k);
        // Unlit arcs stay faintly visible so the shape reads as Wi-Fi at any strength.
        let c2 = color.with_alpha(color.a * (0.25 + 0.75 * a));
        p.stroke_arc(base, outer * f32::from(k) / 3.0 + dot * (1.0 - f32::from(k) / 3.0), -137.0, 94.0, c2, w);
    }
}

/// A diagonal slash drawn from the top-left `progress` (0..=1) of the way down.
pub fn slash(p: &mut Painter, c: PointF, s: f32, progress: f32, color: Color, w: f32) {
    let progress = progress.clamp(0.0, 1.0);
    if progress <= 0.0 {
        return;
    }
    let d = s * 0.4;
    p.line(at(c, -d, -d), at(c, -d + 2.0 * d * progress, -d + 2.0 * d * progress), color, w);
}

/// A speaker with `waves` (0..=3) sound waves; `mute` (0..=1) swaps the waves for a cross.
pub fn speaker(p: &mut Painter, c: PointF, s: f32, waves: f32, mute: f32, color: Color, w: f32) {
    let x0 = c.x - s * 0.42;
    let (box_h, cone_h) = (s * 0.13, s * 0.3);
    let neck = x0 + s * 0.15;
    let mouth = x0 + s * 0.34;
    let body = [
        PointF { x: x0, y: c.y - box_h },
        PointF { x: neck, y: c.y - box_h },
        PointF { x: mouth, y: c.y - cone_h },
        PointF { x: mouth, y: c.y + cone_h },
        PointF { x: neck, y: c.y + box_h },
        PointF { x: x0, y: c.y + box_h },
    ];
    p.stroke_polygon(&body, color, w);
    let mute = mute.clamp(0.0, 1.0);
    let origin = PointF { x: mouth, y: c.y };
    for k in 1..=3u8 {
        let a = lit(waves, k) * (1.0 - mute);
        if a > 0.0 {
            p.stroke_arc(origin, s * (0.17 * f32::from(k) - 0.03), -48.0, 96.0, color.with_alpha(color.a * a), w);
        }
    }
    if mute > 0.0 {
        // The cross grows out of its centre where the waves were.
        let (x, d) = (at(origin, s * 0.27, 0.0), s * 0.13 * mute);
        let cross = color.with_alpha(color.a * mute);
        p.line(at(x, -d, -d), at(x, d, d), cross, w);
        p.line(at(x, -d, d), at(x, d, -d), cross, w);
    }
}

/// A music note: what is playing.
pub fn music_note(p: &mut Painter, c: PointF, s: f32, color: Color, w: f32) {
    let r = s * 0.15;
    let head = at(c, -s * 0.12, s * 0.24);
    p.fill_round_rect(RectF::new(head.x - r, head.y - r, 2.0 * r, 2.0 * r), r, color);
    let stem_x = head.x + r - w / 2.0;
    let top = PointF { x: stem_x, y: c.y - s * 0.36 };
    p.line(PointF { x: stem_x, y: head.y }, top, color, w);
    p.stroke_polyline(&[top, at(top, s * 0.26, s * 0.12), at(top, s * 0.26, s * 0.3)], color, w);
}

/// ▶: play, filled, with softened corners.
pub fn play(p: &mut Painter, c: PointF, s: f32, color: Color) {
    let (h, l, r) = (s * 0.36, c.x - s * 0.26, c.x + s * 0.34);
    let tri = [PointF { x: l, y: c.y - h }, PointF { x: r, y: c.y }, PointF { x: l, y: c.y + h }];
    p.fill_polygon(&tri, color);
    p.stroke_polygon(&tri, color, s * 0.08);
}

/// ❚❚: pause.
pub fn pause(p: &mut Painter, c: PointF, s: f32, color: Color) {
    let (bw, h, gap) = (s * 0.2, s * 0.68, s * 0.14);
    for x in [c.x - gap / 2.0 - bw, c.x + gap / 2.0] {
        p.fill_round_rect(RectF::new(x, c.y - h / 2.0, bw, h), bw * 0.3, color);
    }
}

/// ⏭ (or ⏮ when `back`): skip to the next or previous track.
pub fn skip(p: &mut Painter, c: PointF, s: f32, back: bool, color: Color) {
    let d = if back { -1.0 } else { 1.0 };
    let h = s * 0.3;
    for k in [0.0, 1.0] {
        let base = c.x + d * (-s * 0.36 + k * s * 0.32);
        let tri = [PointF { x: base, y: c.y - h }, PointF { x: base + d * s * 0.32, y: c.y }, PointF { x: base, y: c.y + h }];
        p.fill_polygon(&tri, color);
        p.stroke_polygon(&tri, color, s * 0.06);
    }
    let bar = c.x + d * s * 0.3;
    p.fill_round_rect(RectF::new(bar - s * 0.05, c.y - h, s * 0.1, 2.0 * h), s * 0.03, color);
}

/// A check mark: the chosen item in a list.
pub fn check(p: &mut Painter, c: PointF, s: f32, color: Color, w: f32) {
    p.stroke_polyline(&[at(c, -s * 0.4, 0.0), at(c, -s * 0.12, s * 0.3), at(c, s * 0.42, -s * 0.32)], color, w);
}

/// Where the magnifier's parts go: the lens (centre, radius) and the handle (from, to).
pub struct MagnifierGeometry {
    pub lens: PointF,
    pub radius: f32,
    pub handle: (PointF, PointF),
}

pub fn magnifier_geometry(c: PointF, s: f32) -> MagnifierGeometry {
    let lens = at(c, -0.1 * s, -0.1 * s);
    let radius = 0.31 * s;
    // The handle starts on the lens rim, at 45° down right.
    let rim = radius * std::f32::consts::FRAC_1_SQRT_2;
    MagnifierGeometry { lens, radius, handle: (at(lens, rim, rim), at(c, 0.38 * s, 0.38 * s)) }
}

/// Spotlight's button on the bar, and the icon in its field.
pub fn magnifier(p: &mut Painter, c: PointF, s: f32, color: Color, w: f32) {
    let m = magnifier_geometry(c, s);
    let d = 2.0 * m.radius;
    p.stroke_round_rect(RectF::new(m.lens.x - m.radius, m.lens.y - m.radius, d, d), m.radius, color, w);
    p.line(m.handle.0, m.handle.1, color, w);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PointF;

    #[test]
    fn chevron_turns_upside_down() {
        let c = PointF { x: 10.0, y: 10.0 };
        let up = chevron_points(c, 8.0, 0.0);
        assert!(up[1].y < up[0].y && up[1].y < up[2].y, "^ points up: {up:?}");
        assert!(up[0].x < up[2].x);
        let down = chevron_points(c, 8.0, 1.0);
        assert!(down[1].y > down[0].y && down[1].y > down[2].y, "turned: {down:?}");
        assert!(down[0].x > down[2].x, "ends mirrored");
        assert!((down[1].x - c.x).abs() < 1e-4);
    }
}
