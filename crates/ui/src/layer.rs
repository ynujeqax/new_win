//! A window's DirectComposition content: a root visual with one drawn surface, and for glass
//! windows a blurred backdrop under it (spec: liquid glass §3).
//!
//! ```text
//! root ─ glass (rounded clip) ─ shift (counter-offset while sliding) ─ backdrop (tiny image, scaled up)
//!      └ content (drawn surface)
//! ```

use std::rc::Rc;

use newwin_core::glass::{Image, PxRect, SCALE};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Direct2D::Common::{D2D_RECT_F, D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT};
use windows::Win32::Graphics::Direct2D::{
    D2D1_BITMAP_OPTIONS_NONE, D2D1_BITMAP_PROPERTIES1, D2D1_INTERPOLATION_MODE_NEAREST_NEIGHBOR, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE,
    ID2D1DeviceContext,
};
use windows::Win32::Graphics::DirectComposition::{
    DCOMPOSITION_BITMAP_INTERPOLATION_MODE_LINEAR, IDCompositionRectangleClip, IDCompositionSurface, IDCompositionTarget,
    IDCompositionVirtualSurface, IDCompositionVisual2,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM};
use windows::Win32::UI::WindowsAndMessaging::{SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE};
use windows::core::{IUnknown, Result};
use windows_numerics::Matrix3x2;

use crate::backdrop::Frame;
use crate::gfx::{Gfx, is_device_lost};
use crate::{Painter, RectF};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backdrop {
    /// Liquid glass: a blurred image of the screen under the window, set with [`Layer::set_backdrop`].
    /// The window is excluded from screen capture so it never blurs itself.
    Glass,
    /// Fully transparent window; all color comes from what we draw.
    Tint,
}

struct GlassParts {
    clip: IDCompositionRectangleClip,
    shift: IDCompositionVisual2,
    backdrop: IDCompositionVisual2,
    image: Option<(IDCompositionSurface, (u32, u32))>,
}

struct Parts {
    _target: IDCompositionTarget,
    root: IDCompositionVisual2,
    surface: IDCompositionVirtualSurface,
    glass: Option<GlassParts>,
}

/// The last backdrop, kept so a device loss can re-upload it.
struct LastBackdrop {
    image: Image,
    area: PxRect,
    origin: (i32, i32),
}

pub struct Layer {
    gfx: Rc<Gfx>,
    hwnd: HWND,
    kind: Backdrop,
    width: u32,
    height: u32,
    corner_radius: f32,
    generation: u32,
    parts: Option<Parts>,
    last: Option<LastBackdrop>,
}

impl Layer {
    pub fn new(gfx: &Rc<Gfx>, hwnd: HWND, kind: Backdrop, width: u32, height: u32) -> Result<Self> {
        if kind == Backdrop::Glass {
            unsafe { SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)? };
        }
        let mut layer = Layer {
            gfx: gfx.clone(),
            hwnd,
            kind,
            width: width.max(1),
            height: height.max(1),
            corner_radius: 0.0,
            generation: gfx.generation(),
            parts: None,
            last: None,
        };
        layer.ensure()?;
        Ok(layer)
    }

    fn ensure(&mut self) -> Result<&mut Parts> {
        if self.generation != self.gfx.generation() {
            self.parts = None;
            self.generation = self.gfx.generation();
        }
        if self.parts.is_none() {
            log::debug!("creating composition visuals for {:?}", self.hwnd);
            let dcomp = self.gfx.devices()?.dcomp;
            unsafe {
                let target = dcomp.CreateTargetForHwnd(self.hwnd, true)?;
                let root = dcomp.CreateVisual()?;
                let content = dcomp.CreateVisual()?;
                let surface =
                    dcomp.CreateVirtualSurface(self.width, self.height, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_ALPHA_MODE_PREMULTIPLIED)?;
                content.SetContent(&surface)?;
                let glass = if self.kind == Backdrop::Glass {
                    let clip = dcomp.CreateRectangleClip()?;
                    let container = dcomp.CreateVisual()?;
                    let shift = dcomp.CreateVisual()?;
                    let backdrop = dcomp.CreateVisual()?;
                    backdrop.SetBitmapInterpolationMode(DCOMPOSITION_BITMAP_INTERPOLATION_MODE_LINEAR)?;
                    container.SetClip(&clip)?;
                    shift.AddVisual(&backdrop, false, None)?;
                    container.AddVisual(&shift, false, None)?;
                    root.AddVisual(&container, false, None)?;
                    // Content above the glass, as verified in the own_blur_probe spike.
                    root.AddVisual(&content, true, &container)?;
                    Some(GlassParts { clip, shift, backdrop, image: None })
                } else {
                    root.AddVisual(&content, false, None)?;
                    None
                };
                target.SetRoot(&root)?;
                self.parts = Some(Parts { _target: target, root, surface, glass });
            }
            self.update_clip()?;
            if self.last.is_some() {
                self.upload()?;
            }
        }
        Ok(self.parts.as_mut().expect("parts just created"))
    }

    fn update_clip(&mut self) -> Result<()> {
        let (w, h, r) = (self.width as f32, self.height as f32, self.corner_radius);
        let Some(g) = self.parts.as_ref().and_then(|p| p.glass.as_ref()) else { return Ok(()) };
        unsafe {
            g.clip.SetLeft2(0.0)?;
            g.clip.SetTop2(0.0)?;
            g.clip.SetRight2(w)?;
            g.clip.SetBottom2(h)?;
            g.clip.SetTopLeftRadiusX2(r)?;
            g.clip.SetTopLeftRadiusY2(r)?;
            g.clip.SetTopRightRadiusX2(r)?;
            g.clip.SetTopRightRadiusY2(r)?;
            g.clip.SetBottomLeftRadiusX2(r)?;
            g.clip.SetBottomLeftRadiusY2(r)?;
            g.clip.SetBottomRightRadiusX2(r)?;
            g.clip.SetBottomRightRadiusY2(r)?;
        }
        Ok(())
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn resize(&mut self, width: u32, height: u32) -> Result<()> {
        self.width = width.max(1);
        self.height = height.max(1);
        if let Some(p) = &self.parts {
            unsafe { p.surface.Resize(self.width, self.height)? };
        }
        self.update_clip()
    }

    /// Corner radius of the glass clip, physical px.
    pub fn set_corner_radius(&mut self, r: f32) -> Result<()> {
        self.corner_radius = r;
        self.update_clip()?;
        self.gfx.commit()
    }

    /// Shows `frame` under the content. `window_origin` is the window's top-left in desktop px,
    /// so the image lands exactly over the screen area it was captured from.
    pub fn set_backdrop(&mut self, frame: &Frame, window_origin: (i32, i32)) -> Result<()> {
        if self.kind != Backdrop::Glass {
            return Ok(());
        }
        self.last = Some(LastBackdrop { image: frame.image.clone(), area: frame.area, origin: window_origin });
        self.ensure()?;
        self.upload()?;
        self.gfx.commit()
    }

    fn upload(&mut self) -> Result<()> {
        let dcomp = self.gfx.devices()?.dcomp;
        let Some(last) = self.last.as_ref() else { return Ok(()) };
        let Some(g) = self.parts.as_mut().and_then(|p| p.glass.as_mut()) else { return Ok(()) };
        let (w, h) = (last.image.width as u32, last.image.height as u32);
        if w == 0 || h == 0 {
            return Ok(());
        }
        unsafe {
            if g.image.as_ref().is_none_or(|(_, size)| *size != (w, h)) {
                let s = dcomp.CreateSurface(w, h, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_ALPHA_MODE_PREMULTIPLIED)?;
                g.backdrop.SetContent(&s)?;
                g.image = Some((s, (w, h)));
            }
            let (surface, _) = g.image.as_ref().expect("image surface just created");
            let mut offset = POINT::default();
            let dc: ID2D1DeviceContext = surface.BeginDraw(None, &mut offset)?;
            let props = D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                dpiX: 96.0,
                dpiY: 96.0,
                bitmapOptions: D2D1_BITMAP_OPTIONS_NONE,
                ..Default::default()
            };
            let drawn = dc
                .CreateBitmap(D2D_SIZE_U { width: w, height: h }, Some(last.image.bgra.as_ptr() as _), w * 4, &props)
                .map(|bmp| {
                    let dst = D2D_RECT_F { left: offset.x as f32, top: offset.y as f32, right: (offset.x as u32 + w) as f32, bottom: (offset.y as u32 + h) as f32 };
                    dc.DrawBitmap(&bmp, Some(&dst), 1.0, D2D1_INTERPOLATION_MODE_NEAREST_NEIGHBOR, None, None);
                });
            surface.EndDraw()?;
            drawn?;
            let s = SCALE as f32;
            let to = Matrix3x2 { M11: s, M12: 0.0, M21: 0.0, M22: s, M31: 0.0, M32: 0.0 }
                * Matrix3x2::translation((last.area.left - last.origin.0) as f32, (last.area.top - last.origin.1) as f32);
            g.backdrop.SetTransform2(&to)?;
        }
        Ok(())
    }

    /// Removes the backdrop (capture unavailable or glass turned off).
    pub fn clear_backdrop(&mut self) -> Result<()> {
        self.last = None;
        if let Some(g) = self.parts.as_mut().and_then(|p| p.glass.as_mut()) {
            unsafe { g.backdrop.SetContent(None::<&IUnknown>)? };
            g.image = None;
        }
        self.gfx.commit()
    }

    pub fn has_backdrop(&self) -> bool {
        self.last.is_some()
    }

    /// The visual that keeps the backdrop still on screen while the root slides; `None` for
    /// tint-only layers.
    pub fn backdrop_visual(&mut self) -> Result<Option<IDCompositionVisual2>> {
        Ok(self.ensure()?.glass.as_ref().map(|g| g.shift.clone()))
    }

    /// Redraws `dirty` (or everything). After a device loss the devices are rebuilt and the
    /// whole surface is redrawn once.
    pub fn draw(&mut self, dirty: Option<RectF>, mut f: impl FnMut(&mut Painter)) -> Result<()> {
        let mut dirty = dirty;
        for attempt in 0..2 {
            match self.try_draw(dirty, &mut f) {
                Err(e) if attempt == 0 && is_device_lost(e.code()) => {
                    self.gfx.device_lost();
                    dirty = None;
                }
                r => return r,
            }
        }
        Ok(())
    }

    fn try_draw(&mut self, dirty: Option<RectF>, f: &mut impl FnMut(&mut Painter)) -> Result<()> {
        let (w, h) = (self.width as i32, self.height as i32);
        let rect = match dirty {
            Some(d) => RECT {
                left: (d.x.floor() as i32).clamp(0, w),
                top: (d.y.floor() as i32).clamp(0, h),
                right: (d.right().ceil() as i32).clamp(0, w),
                bottom: (d.bottom().ceil() as i32).clamp(0, h),
            },
            None => RECT { left: 0, top: 0, right: w, bottom: h },
        };
        if rect.right <= rect.left || rect.bottom <= rect.top {
            return Ok(());
        }
        let surface = self.ensure()?.surface.clone();
        unsafe {
            let mut offset = POINT::default();
            let dc: ID2D1DeviceContext = surface.BeginDraw(Some(&rect), &mut offset)?;
            // `offset` is where `rect`'s top-left landed in the atlas.
            dc.SetTransform(&Matrix3x2::translation((offset.x - rect.left) as f32, (offset.y - rect.top) as f32));
            dc.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let clip = RectF::new(rect.left as f32, rect.top as f32, (rect.right - rect.left) as f32, (rect.bottom - rect.top) as f32);
            {
                let mut p = Painter::new(&dc, clip);
                p.clear();
                f(&mut p);
            }
            surface.EndDraw()
        }
    }

    /// The root visual (for slide/fade animations); valid until the next device loss.
    pub fn visual(&mut self) -> Result<IDCompositionVisual2> {
        Ok(self.ensure()?.root.clone())
    }
}

