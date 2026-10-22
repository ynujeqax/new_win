//! Slider geometry (spec: M2 §3.2): track position ↔ level 0..=100.

use crate::RectF;

/// The level at pointer x on `track`, rounded and clamped.
pub fn level_at(x: f32, track: RectF) -> u8 {
    if track.w <= 0.0 {
        return 0;
    }
    ((x - track.x) / track.w * 100.0).round().clamp(0.0, 100.0) as u8
}

/// Where the thumb's centre sits for `level`.
pub fn thumb_x(level: u8, track: RectF) -> f32 {
    track.x + track.w * f32::from(level.min(100)) / 100.0
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
