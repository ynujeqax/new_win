//! Vertical scroll state of the settings pages: a wheel, a thin thumb, no arithmetic in paint.

use newwin_platform::dpi::scale;

use crate::{Color, Painter, RectF};

/// One wheel notch (120) scrolls this many logical px.
const NOTCH_PX: f32 = 48.0;
const THUMB_W: f32 = 4.0;
const THUMB_MIN_H: f32 = 24.0;
const THUMB: Color = Color::rgba(255, 255, 255, 0.28);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Scroll {
    pub offset: f32,
    pub content_h: f32,
    pub view_h: f32,
}

fn finite(v: f32) -> f32 {
    if v.is_finite() { v } else { 0.0 }
}

impl Scroll {
    pub fn new(content_h: f32, view_h: f32) -> Scroll {
        Scroll { offset: 0.0, content_h, view_h }
    }

    /// The largest offset: how far the content overflows the view.
    pub fn max_offset(&self) -> f32 {
        (finite(self.content_h) - finite(self.view_h)).max(0.0)
    }

    pub fn clamp(&mut self) {
        self.offset = finite(self.offset).clamp(0.0, self.max_offset());
    }

    /// A wheel delta (120 per notch, positive = up) at 96 dpi.
    pub fn wheel(&mut self, delta: i16) {
        self.wheel_dpi(delta, 96);
    }

    pub fn wheel_dpi(&mut self, delta: i16, dpi: u32) {
        self.offset = finite(self.offset) - f32::from(delta) / 120.0 * scale(NOTCH_PX, dpi);
        self.clamp();
    }

    /// The thumb inside `view` (the scrolled area, physical px) at 96 dpi; `None` when the
    /// content fits.
    pub fn thumb_rect(&self, view: RectF) -> Option<RectF> {
        self.thumb_rect_dpi(view, 96)
    }

    pub fn thumb_rect_dpi(&self, view: RectF, dpi: u32) -> Option<RectF> {
        let max = self.max_offset();
        if max <= 0.0 || !view.h.is_finite() || view.h <= 0.0 {
            return None;
        }
        let ratio = (finite(self.view_h) / finite(self.content_h)).clamp(0.0, 1.0);
        let h = (view.h * ratio).max(scale(THUMB_MIN_H, dpi)).min(view.h);
        let t = (finite(self.offset) / max).clamp(0.0, 1.0);
        let w = scale(THUMB_W, dpi);
        Some(RectF::new(view.right() - w, view.y + (view.h - h) * t, w, h))
    }

    /// Dragging the thumb: the pointer is at `pointer_y`, `grab` below the thumb's top.
    pub fn drag_thumb(&mut self, view: RectF, pointer_y: f32, grab: f32) {
        self.drag_thumb_dpi(view, pointer_y, grab, 96);
    }

    pub fn drag_thumb_dpi(&mut self, view: RectF, pointer_y: f32, grab: f32, dpi: u32) {
        let Some(thumb) = self.thumb_rect_dpi(view, dpi) else { return };
        let travel = view.h - thumb.h;
        if travel <= 0.0 || !pointer_y.is_finite() {
            return;
        }
        let t = ((pointer_y - finite(grab) - view.y) / travel).clamp(0.0, 1.0);
        self.offset = t * self.max_offset();
        self.clamp();
    }

    pub fn paint(&self, p: &mut Painter, view: RectF, dpi: u32) {
        if let Some(t) = self.thumb_rect_dpi(view, dpi) {
            p.fill_round_rect(t, t.w / 2.0, THUMB);
        }
    }
}
