//! A window's DirectComposition content: one visual with one virtual surface.

use std::rc::Rc;

use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Direct2D::{D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, ID2D1DeviceContext};
use windows::Win32::Graphics::DirectComposition::{IDCompositionTarget, IDCompositionVirtualSurface, IDCompositionVisual2};
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM};
use windows::Win32::UI::Controls::MARGINS;
use windows::core::Result;
use windows_numerics::Matrix3x2;

use crate::gfx::{Gfx, is_device_lost};
use crate::{Painter, RectF};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backdrop {
    /// DWM Acrylic behind the window (probe: renders as a light gray slab; unused by the bar).
    SystemAcrylic,
    /// Fully transparent window; all color comes from what we draw.
    Tint,
}

struct Parts {
    _target: IDCompositionTarget,
    visual: IDCompositionVisual2,
    surface: IDCompositionVirtualSurface,
}

pub struct Layer {
    gfx: Rc<Gfx>,
    hwnd: HWND,
    width: u32,
    height: u32,
    generation: u32,
    parts: Option<Parts>,
}

impl Layer {
    pub fn new(gfx: &Rc<Gfx>, hwnd: HWND, backdrop: Backdrop, width: u32, height: u32) -> Result<Self> {
        if backdrop == Backdrop::SystemAcrylic {
            unsafe {
                let dark: i32 = 1;
                DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const _ as _, 4)?;
                let b = DWMSBT_TRANSIENTWINDOW;
                DwmSetWindowAttribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, &b as *const _ as _, 4)?;
                DwmExtendFrameIntoClientArea(hwnd, &MARGINS { cxLeftWidth: -1, cxRightWidth: -1, cyTopHeight: -1, cyBottomHeight: -1 })?;
            }
        }
        let mut layer =
            Layer { gfx: gfx.clone(), hwnd, width: width.max(1), height: height.max(1), generation: gfx.generation(), parts: None };
        layer.ensure()?;
        Ok(layer)
    }

    fn ensure(&mut self) -> Result<&Parts> {
        if self.generation != self.gfx.generation() {
            self.parts = None;
            self.generation = self.gfx.generation();
        }
        if self.parts.is_none() {
            log::debug!("creating composition visual for {:?}", self.hwnd);
            let dcomp = self.gfx.devices()?.dcomp;
            unsafe {
                let target = dcomp.CreateTargetForHwnd(self.hwnd, true)?;
                let visual = dcomp.CreateVisual()?;
                let surface =
                    dcomp.CreateVirtualSurface(self.width, self.height, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_ALPHA_MODE_PREMULTIPLIED)?;
                visual.SetContent(&surface)?;
                target.SetRoot(&visual)?;
                self.parts = Some(Parts { _target: target, visual, surface });
            }
        }
        Ok(self.parts.as_ref().expect("parts just created"))
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
        Ok(())
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
        Ok(self.ensure()?.visual.clone())
    }
}
