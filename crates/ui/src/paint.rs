//! Drawing primitives in physical pixels.

use windows::Win32::Graphics::Direct2D::Common::{D2D_RECT_F, D2D_SIZE_F, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_CLOSED, D2D1_FIGURE_END_OPEN, D2D1_GRADIENT_STOP};
use windows::Win32::Graphics::Direct2D::*;
use windows::core::Interface;
use windows_numerics::Vector2;

use std::rc::Rc;

use newwin_core::app_icon::IconImage;

use crate::icon_bitmaps::IconBitmaps;
use crate::{Color, PointF, RectF, TextLayout};

pub struct Painter<'a> {
    dc: &'a ID2D1DeviceContext,
    clip: RectF,
    /// Round caps and joins for line icons, created on first use.
    round: Option<ID2D1StrokeStyle>,
}

fn v(p: PointF) -> Vector2 {
    Vector2 { X: p.x, Y: p.y }
}

impl<'a> Painter<'a> {
    pub(crate) fn new(dc: &'a ID2D1DeviceContext, clip: RectF) -> Self {
        Self { dc, clip, round: None }
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

    /// Drawing until the matching [`Self::pop_clip`] stays inside `r` (a scrolled list).
    pub fn push_clip(&mut self, r: RectF) {
        unsafe { self.dc.PushAxisAlignedClip(&r.d2d(), D2D1_ANTIALIAS_MODE_ALIASED) };
    }

    pub fn pop_clip(&mut self) {
        unsafe { self.dc.PopAxisAlignedClip() };
    }

    pub fn fill_round_rect(&mut self, r: RectF, radius: f32, c: Color) {
        let Some(b) = self.brush(c) else { return };
        let rr = D2D1_ROUNDED_RECT { rect: r.d2d(), radiusX: radius, radiusY: radius };
        unsafe { self.dc.FillRoundedRectangle(&rr, &b) };
    }

    /// Like [`Self::fill_round_rect`], shading from `top` at the top edge to `bottom` at the bottom.
    pub fn fill_round_rect_v(&mut self, r: RectF, radius: f32, top: Color, bottom: Color) {
        let stops = [D2D1_GRADIENT_STOP { position: 0.0, color: top.d2d() }, D2D1_GRADIENT_STOP { position: 1.0, color: bottom.d2d() }];
        let brush = unsafe {
            // The two-argument form lives on the plain render target.
            let Ok(rt) = self.dc.cast::<ID2D1RenderTarget>() else { return };
            let Ok(coll) = rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP) else { return };
            let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: Vector2 { X: 0.0, Y: r.y }, endPoint: Vector2 { X: 0.0, Y: r.y + r.h } };
            self.dc.CreateLinearGradientBrush(&props, None, &coll)
        };
        let Ok(b) = brush else { return };
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

    fn round_style(&mut self) -> Option<ID2D1StrokeStyle> {
        if self.round.is_none() {
            let props = D2D1_STROKE_STYLE_PROPERTIES {
                startCap: D2D1_CAP_STYLE_ROUND,
                endCap: D2D1_CAP_STYLE_ROUND,
                dashCap: D2D1_CAP_STYLE_ROUND,
                lineJoin: D2D1_LINE_JOIN_ROUND,
                miterLimit: 10.0,
                dashStyle: D2D1_DASH_STYLE_SOLID,
                dashOffset: 0.0,
            };
            self.round = unsafe { self.dc.GetFactory().ok()?.CreateStrokeStyle(&props, None).ok() };
        }
        self.round.clone()
    }

    /// One figure through `pts`, open or closed, as a path geometry.
    fn figure(&self, pts: &[PointF], closed: bool, filled: bool) -> Option<ID2D1PathGeometry> {
        let (first, rest) = pts.split_first()?;
        unsafe {
            let geo = self.dc.GetFactory().ok()?.CreatePathGeometry().ok()?;
            let sink = geo.Open().ok()?;
            sink.BeginFigure(v(*first), if filled { D2D1_FIGURE_BEGIN_FILLED } else { D2D1_FIGURE_BEGIN_HOLLOW });
            let rest: Vec<Vector2> = rest.iter().map(|p| v(*p)).collect();
            sink.AddLines(&rest);
            sink.EndFigure(if closed { D2D1_FIGURE_END_CLOSED } else { D2D1_FIGURE_END_OPEN });
            sink.Close().ok()?;
            Some(geo)
        }
    }

    /// Connected line segments with round caps and joins.
    pub fn stroke_polyline(&mut self, pts: &[PointF], c: Color, width: f32) {
        self.stroke_figure(pts, false, c, width);
    }

    /// Like [`Self::stroke_polyline`], back to the first point.
    pub fn stroke_polygon(&mut self, pts: &[PointF], c: Color, width: f32) {
        self.stroke_figure(pts, true, c, width);
    }

    fn stroke_figure(&mut self, pts: &[PointF], closed: bool, c: Color, width: f32) {
        let style = self.round_style();
        let (Some(geo), Some(b)) = (self.figure(pts, closed, false), self.brush(c)) else { return };
        unsafe { self.dc.DrawGeometry(&geo, &b, width, style.as_ref()) };
    }

    pub fn fill_polygon(&mut self, pts: &[PointF], c: Color) {
        let (Some(geo), Some(b)) = (self.figure(pts, true, true), self.brush(c)) else { return };
        unsafe { self.dc.FillGeometry(&geo, &b, None) };
    }

    /// Circular arc; angles in degrees, 0 = to the right, positive = clockwise on screen.
    pub fn stroke_arc(&mut self, center: PointF, radius: f32, start_deg: f32, sweep_deg: f32, c: Color, width: f32) {
        let at = |deg: f32| {
            let r = deg.to_radians();
            Vector2 { X: center.x + radius * r.cos(), Y: center.y + radius * r.sin() }
        };
        let style = self.round_style();
        let Some(b) = self.brush(c) else { return };
        unsafe {
            let Ok(f) = self.dc.GetFactory() else { return };
            let Ok(geo) = f.CreatePathGeometry() else { return };
            let Ok(sink) = geo.Open() else { return };
            sink.BeginFigure(at(start_deg), D2D1_FIGURE_BEGIN_HOLLOW);
            sink.AddArc(&D2D1_ARC_SEGMENT {
                point: at(start_deg + sweep_deg),
                size: D2D_SIZE_F { width: radius, height: radius },
                rotationAngle: 0.0,
                sweepDirection: if sweep_deg >= 0.0 { D2D1_SWEEP_DIRECTION_CLOCKWISE } else { D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE },
                arcSize: if sweep_deg.abs() > 180.0 { D2D1_ARC_SIZE_LARGE } else { D2D1_ARC_SIZE_SMALL },
            });
            sink.EndFigure(D2D1_FIGURE_END_OPEN);
            if sink.Close().is_ok() {
                self.dc.DrawGeometry(&geo, &b, width, style.as_ref());
            }
        }
    }

    pub fn text(&mut self, t: &TextLayout, origin: PointF, c: Color) {
        let Some(b) = self.brush(c) else { return };
        unsafe { self.dc.DrawTextLayout(Vector2 { X: origin.x, Y: origin.y }, &t.layout, &b, D2D1_DRAW_TEXT_OPTIONS_NONE) };
    }

    /// Text in its font's own colours (emoji), at `opacity`; any monochrome part is white.
    pub fn text_color(&mut self, t: &TextLayout, origin: PointF, opacity: f32) {
        if opacity <= 0.0 {
            return;
        }
        let Some(b) = self.brush(Color::rgba(255, 255, 255, opacity.min(1.0))) else { return };
        unsafe { self.dc.DrawTextLayout(Vector2 { X: origin.x, Y: origin.y }, &t.layout, &b, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT) };
    }

    /// An app icon in `dst` (physical px), at `opacity`; its bitmap comes from `cache`.
    pub fn icon(&mut self, cache: &mut IconBitmaps, img: &Rc<IconImage>, dst: RectF, opacity: f32) {
        if opacity <= 0.0 {
            return;
        }
        let Some(bitmap) = cache.bitmap(self.dc, img) else { return };
        let d = D2D_RECT_F { left: dst.x.round(), top: dst.y.round(), right: (dst.x + dst.w).round(), bottom: (dst.y + dst.h).round() };
        unsafe { self.dc.DrawBitmap(&bitmap, Some(&d), opacity.min(1.0), D2D1_INTERPOLATION_MODE_LINEAR, None, None) };
    }
}
