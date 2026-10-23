//! A segmented control: a translucent track with a pill that glides under the selected label.

use newwin_core::anim::Tween;
use newwin_platform::dpi::scale;

use crate::toggle::mix;
use crate::{Color, Painter, PointF, RectF, TextLayout, theme};

const TRACK: Color = Color::rgba(255, 255, 255, 0.07);
const PILL: Color = Color::rgba(255, 255, 255, 0.16);
const LABEL_DIM: Color = Color::rgba(255, 255, 255, 0.60);
const LABEL_ON: Color = Color::rgba(255, 255, 255, 1.0);
const PILL_MS: u32 = 150;

#[derive(Clone, Debug)]
pub struct Segmented {
    pub labels: Vec<String>,
    pub selected: usize,
    pill: Tween,
}

impl Segmented {
    pub fn new(labels: Vec<String>, selected: usize) -> Segmented {
        let selected = selected.min(labels.len().saturating_sub(1));
        Segmented { labels, selected, pill: Tween::at_rest(selected as f32) }
    }

    /// One rectangle per label, side by side from `origin`; `widths` are the labels' text widths
    /// in physical px (a missing width counts as 0).
    pub fn layout(&self, origin: PointF, widths: &[f32], dpi: u32) -> Vec<RectF> {
        let (pad, h) = (scale(theme::SEGMENT_PAD_X, dpi), scale(theme::SEGMENT_H, dpi));
        let mut x = origin.x;
        (0..self.labels.len())
            .map(|i| {
                let w = widths.get(i).copied().unwrap_or(0.0).max(0.0) + 2.0 * pad;
                let r = RectF::new(x, origin.y, w, h);
                x += w;
                r
            })
            .collect()
    }

    pub fn hit(&self, layout: &[RectF], p: PointF) -> Option<usize> {
        layout.iter().take(self.labels.len()).position(|r| r.contains(p))
    }

    /// Selects `i` (ignored when out of range); the pill glides there.
    pub fn select(&mut self, i: usize, now_ms: u64) {
        if i >= self.labels.len() {
            return;
        }
        self.selected = i;
        self.pill.to(i as f32, now_ms, PILL_MS);
    }

    pub fn animating(&self, now_ms: u64) -> bool {
        self.pill.running(now_ms)
    }

    /// `texts[i]` is the laid-out label `i`; `layout` comes from [`Segmented::layout`].
    pub fn paint(&self, p: &mut Painter, layout: &[RectF], texts: &[TextLayout], now_ms: u64) {
        let (Some(first), Some(last)) = (layout.first(), layout.last()) else { return };
        let track = RectF::new(first.x, first.y, last.right() - first.x, first.h);
        p.fill_round_rect(track, track.h / 2.0, TRACK);
        let pos = self.pill.value(now_ms).clamp(0.0, (layout.len() - 1) as f32);
        let (i0, f) = (pos.floor() as usize, pos.fract());
        if let (Some(a), Some(b)) = (layout.get(i0), layout.get((i0 + 1).min(layout.len() - 1))) {
            let lerp = |u: f32, v: f32| u + (v - u) * f;
            let r = RectF::new(lerp(a.x, b.x), lerp(a.y, b.y), lerp(a.w, b.w), lerp(a.h, b.h));
            p.fill_round_rect(r, r.h / 2.0, PILL);
        }
        for (i, (r, t)) in layout.iter().zip(texts).enumerate() {
            let lit = (1.0 - (pos - i as f32).abs()).clamp(0.0, 1.0);
            let origin = PointF { x: (r.x + (r.w - t.width()) / 2.0).round(), y: (r.y + (r.h - t.height()) / 2.0).round() };
            p.text(t, origin, mix(LABEL_DIM, LABEL_ON, lit));
        }
    }
}
