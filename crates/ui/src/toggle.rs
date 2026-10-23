//! The on/off switch of the settings window: a capsule with a sliding knob (150 ms).

use newwin_core::anim::Tween;
use newwin_platform::dpi::scale;

use crate::{Color, Painter, PointF, RectF, theme};

const TRACK_OFF: Color = Color::rgba(255, 255, 255, 0.18);
const TRACK_ON: Color = Color::rgba(255, 255, 255, 1.0);
const KNOB_OFF: Color = Color::rgba(255, 255, 255, 0.75);
const KNOB_ON: Color = Color::rgba(14, 14, 16, 1.0);
/// Knob diameter and its gap to the track edge, logical px.
const KNOB: f32 = 14.0;
const KNOB_GAP: f32 = 3.0;

pub(crate) fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let ch = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color::rgba(ch(a.r, b.r), ch(a.g, b.g), ch(a.b, b.b), a.a + (b.a - a.a) * t)
}

#[derive(Clone, Copy, Debug)]
pub struct Toggle {
    pub on: bool,
    anim: Tween,
}

impl Toggle {
    pub fn new(on: bool) -> Toggle {
        Toggle { on, anim: Tween::at_rest(if on { 1.0 } else { 0.0 }) }
    }

    /// The switch's rectangle with its top-left at `origin`, in physical px.
    pub fn rect(origin: PointF, dpi: u32) -> RectF {
        RectF::new(origin.x, origin.y, scale(theme::TOGGLE_W, dpi), scale(theme::TOGGLE_H, dpi))
    }

    pub fn flip(&mut self, now_ms: u64) {
        self.set(!self.on, now_ms);
    }

    /// Sets the state, gliding the knob when it changes.
    pub fn set(&mut self, on: bool, now_ms: u64) {
        self.on = on;
        self.anim.to(if on { 1.0 } else { 0.0 }, now_ms, theme::TOGGLE_MS);
    }

    pub fn animating(&self, now_ms: u64) -> bool {
        self.anim.running(now_ms)
    }

    pub fn paint(&self, p: &mut Painter, r: RectF, now_ms: u64) {
        let t = self.anim.value(now_ms);
        p.fill_round_rect(r, r.h / 2.0, mix(TRACK_OFF, TRACK_ON, t));
        let k = r.h * KNOB / theme::TOGGLE_H;
        let gap = r.h * KNOB_GAP / theme::TOGGLE_H;
        let x = r.x + gap + (r.w - k - 2.0 * gap) * t;
        p.fill_round_rect(RectF::new(x, r.y + (r.h - k) / 2.0, k, k), k / 2.0, mix(KNOB_OFF, KNOB_ON, t));
    }
}
