//! Drawing primitives in physical pixels.

use windows::Win32::Graphics::Direct2D::*;
use windows_numerics::Vector2;

use crate::{Color, PointF, RectF, TextLayout};

pub struct Painter<'a> {
    dc: &'a ID2D1DeviceContext,
    clip: RectF,
}

impl<'a> Painter<'a> {
    pub(crate) fn new(dc: &'a ID2D1DeviceContext, clip: RectF) -> Self {
        Self { dc, clip }
    }

    /// The region being redrawn; drawing outside it is discarded.
    pub fn clip(&self) -> RectF {
        self.clip
    }

    fn brush(&self, c: Color) -> Option<ID2D1SolidColorBrush> {
        unsafe { self.dc.CreateSolidColorBrush(&c.d2d(), None).ok() }
    }

    /// Clears the redrawn region to fully transparent.
    pub fn clear(&mut self) {
        unsafe {
            self.dc.PushAxisAlignedClip(&self.clip.d2d(), D2D1_ANTIALIAS_MODE_ALIASED);
            self.dc.Clear(Some(&Color::rgba(0, 0, 0, 0.0).d2d()));
            self.dc.PopAxisAlignedClip();
        }
    }

    pub fn fill_round_rect(&mut self, r: RectF, radius: f32, c: Color) {
        let Some(b) = self.brush(c) else { return };
        let rr = D2D1_ROUNDED_RECT { rect: r.d2d(), radiusX: radius, radiusY: radius };
        unsafe { self.dc.FillRoundedRectangle(&rr, &b) };
    }

    /// Stroke centered inside `r`, so a 1 px border stays crisp on pixel edges.
    pub fn stroke_round_rect(&mut self, r: RectF, radius: f32, c: Color, width: f32) {
        let Some(b) = self.brush(c) else { return };
        let inner = r.inset(width / 2.0);
        let rr = D2D1_ROUNDED_RECT { rect: inner.d2d(), radiusX: (radius - width / 2.0).max(0.0), radiusY: (radius - width / 2.0).max(0.0) };
        unsafe { self.dc.DrawRoundedRectangle(&rr, &b, width, None) };
    }

    pub fn line(&mut self, a: PointF, b: PointF, c: Color, width: f32) {
        let Some(br) = self.brush(c) else { return };
        unsafe { self.dc.DrawLine(Vector2 { X: a.x, Y: a.y }, Vector2 { X: b.x, Y: b.y }, &br, width, None) };
    }

    pub fn text(&mut self, t: &TextLayout, origin: PointF, c: Color) {
        let Some(b) = self.brush(c) else { return };
        unsafe { self.dc.DrawTextLayout(Vector2 { X: origin.x, Y: origin.y }, &t.layout, &b, D2D1_DRAW_TEXT_OPTIONS_NONE) };
    }
}
