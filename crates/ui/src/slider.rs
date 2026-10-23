//! Slider geometry (spec: M2 §3.2): track position ↔ level 0..=100.

use crate::RectF;

/// A slider's value range; `step` snaps values (0 is treated as 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    pub min: i32,
    pub max: i32,
    pub step: i32,
}

impl Range {
    const PERCENT: Range = Range { min: 0, max: 100, step: 1 };

    fn span(self) -> (i64, i64) {
        let lo = i64::from(self.min);
        (lo, i64::from(self.max).max(lo))
    }
}

/// The value at pointer x on `track`, snapped to `r.step` and clamped to the range.
pub fn value_at(x: f32, track: RectF, r: Range) -> i32 {
    let (lo, hi) = r.span();
    if track.w <= 0.0 || !x.is_finite() {
        return r.min;
    }
    let frac = f64::from(((x - track.x) / track.w).clamp(0.0, 1.0));
    let step = i64::from(r.step.max(1));
    let raw = frac * (hi - lo) as f64;
    let snapped = lo + (raw / step as f64).round() as i64 * step;
    snapped.clamp(lo, hi) as i32
}

/// Where the thumb's centre sits for value `v`.
pub fn thumb_x_for(v: i32, track: RectF, r: Range) -> f32 {
    let (lo, hi) = r.span();
    if hi == lo {
        return track.x;
    }
    let frac = (i64::from(v).clamp(lo, hi) - lo) as f64 / (hi - lo) as f64;
    track.x + track.w * frac as f32
}

/// The level at pointer x on `track`, rounded and clamped.
pub fn level_at(x: f32, track: RectF) -> u8 {
    value_at(x, track, Range::PERCENT) as u8
}

/// Where the thumb's centre sits for `level`.
pub fn thumb_x(level: u8, track: RectF) -> f32 {
    thumb_x_for(i32::from(level), track, Range::PERCENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_maps_both_ways() {
        let track = RectF::new(100.0, 10.0, 200.0, 4.0);
        assert_eq!(level_at(100.0, track), 0);
        assert_eq!(level_at(200.0, track), 50);
        assert_eq!(level_at(999.0, track), 100);
        assert_eq!(level_at(-5.0, track), 0);
        assert_eq!(thumb_x(50, track), 200.0);
        assert_eq!(thumb_x(100, track), 300.0);
    }
}
