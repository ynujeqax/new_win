//! The settings window's side navigation: a column of labels with a pill that glides to the
//! selected one, like the bar's active-app pill.

use newwin_core::anim::Tween;
use newwin_platform::dpi::scale;

use crate::toggle::mix;
use crate::{Color, Painter, PointF, RectF, TextLayout, theme};

const LABEL_DIM: Color = Color::rgba(255, 255, 255, 0.60);
const LABEL_ON: Color = Color::rgba(255, 255, 255, 1.0);

#[derive(Clone, Debug)]
pub struct Nav {
    pub items: Vec<String>,
    pub selected: usize,
    pill: Tween,
}

impl Nav {
    pub fn new(items: Vec<String>, selected: usize) -> Nav {
        let selected = selected.min(items.len().saturating_sub(1));
        Nav { items, selected, pill: Tween::at_rest(selected as f32) }
    }

    /// One full-width row per item, stacked down from `origin`.
    pub fn layout(&self, origin: PointF, width: f32, dpi: u32) -> Vec<RectF> {
        let h = scale(theme::NAV_ROW_H, dpi);
        (0..self.items.len()).map(|i| RectF::new(origin.x, origin.y + h * i as f32, width.max(0.0), h)).collect()
    }

    pub fn hit(&self, layout: &[RectF], p: PointF) -> Option<usize> {
        layout.iter().take(self.items.len()).position(|r| r.contains(p))
    }

    /// Selects `i` (ignored when out of range); the pill glides there.
    pub fn select(&mut self, i: usize, now_ms: u64) {
        if i >= self.items.len() {
            return;
        }
        self.selected = i;
        self.pill.to(i as f32, now_ms, theme::NAV_MS);
    }

    pub fn animating(&self, now_ms: u64) -> bool {
        self.pill.running(now_ms)
    }

    /// `texts[i]` is the laid-out label `i`; `layout` comes from [`Nav::layout`].
    pub fn paint(&self, p: &mut Painter, layout: &[RectF], texts: &[TextLayout], now_ms: u64, dpi: u32) {
        let Some(last) = layout.len().checked_sub(1) else { return };
        let pos = self.pill.value(now_ms).clamp(0.0, last as f32);
        let (i0, f) = (pos.floor() as usize, pos.fract());
        if let (Some(a), Some(b)) = (layout.get(i0), layout.get((i0 + 1).min(last))) {
            let r = RectF::new(a.x + (b.x - a.x) * f, a.y + (b.y - a.y) * f, a.w + (b.w - a.w) * f, a.h);
            p.fill_round_rect(r, scale(theme::POPUP_ITEM_RADIUS, dpi), theme::ACTIVE_PILL);
        }
        let pad = scale(theme::NAV_PAD_X, dpi);
        for (i, (r, t)) in layout.iter().zip(texts).enumerate() {
            let lit = (1.0 - (pos - i as f32).abs()).clamp(0.0, 1.0);
            p.text(t, PointF { x: (r.x + pad).round(), y: (r.y + (r.h - t.height()) / 2.0).round() }, mix(LABEL_DIM, LABEL_ON, lit));
        }
    }
}
