//! A soft shadow under a glass window (panels, menus). It is its own window, just below the
//! owner in z-order and larger than it, so the shadow can fall outside the owner's rectangle; it
//! never takes the pointer (layered + transparent) and follows the owner's show and hide motion.

use std::cell::RefCell;
use std::rc::Rc;

use newwin_platform::window::{Handler, WindowSpec, create_window};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::Result;

use crate::{Backdrop, Color, Gfx, Layer, RectF, fade, set_offset_opacity, slide_y};

/// How far the shadow spreads past the owner's edge, and how far it falls below it (96 DPI).
const BLUR: f32 = 26.0;
const FALL: f32 = 8.0;
/// Darkness right at the owner's edge.
const PEAK: f32 = 0.5;
const STEPS: usize = 16;

struct Inert;

impl Handler for Inert {
    fn on_message(&mut self, _: HWND, msg: u32, _: WPARAM, _: LPARAM) -> Option<LRESULT> {
        // Never activated, never hit: clicks go to whatever is under the shadow.
        match msg {
            WM_MOUSEACTIVATE => Some(LRESULT(MA_NOACTIVATE as isize)),
            WM_NCHITTEST => Some(LRESULT(HTTRANSPARENT as isize)),
            _ => None,
        }
    }
}

pub struct Shadow {
    gfx: Rc<Gfx>,
    hwnd: HWND,
    owner: HWND,
    layer: Layer,
    margin: i32,
    fall: f32,
    blur: f32,
    radius: f32,
}

/// The rectangle of the shadow window for an owner at `r`.
pub fn shadow_rect(r: RECT, margin: i32) -> RECT {
    RECT { left: r.left - margin, top: r.top - margin, right: r.right + margin, bottom: r.bottom + margin }
}

impl Shadow {
    /// A shadow under `owner`, whose glass covers `rect` (screen px) with corners of `radius`.
    /// It starts invisible: run [`Self::animate`] (or [`Self::set_offset_opacity`]) with the owner.
    pub fn new(gfx: &Rc<Gfx>, owner: HWND, rect: RECT, radius: f32, dpi: u32) -> Result<Shadow> {
        let s = dpi as f32 / 96.0;
        let (blur, fall) = ((BLUR * s).round(), (FALL * s).round());
        let margin = (blur + fall) as i32;
        let r = shadow_rect(rect, margin);
        let hwnd = create_window(
            &WindowSpec {
                class: "NewWin.Shadow",
                ex_style: WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_NOREDIRECTIONBITMAP,
                style: WS_POPUP,
                rect: r,
                layered_alpha: Some(255),
            },
            Rc::new(RefCell::new(Inert)),
        )?;
        let layer = Layer::new(gfx, hwnd, Backdrop::Tint, (r.right - r.left) as u32, (r.bottom - r.top) as u32)?;
        // Like the glass: kept out of the screen capture, or the glass would blur (and darken by)
        // the shadows under it.
        unsafe { SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)? };
        let mut shadow = Shadow { gfx: gfx.clone(), hwnd, owner, layer, margin, fall, blur, radius };
        shadow.set_offset_opacity(0.0, 0.0);
        shadow.paint();
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        shadow.stay_under();
        Ok(shadow)
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    /// Shown in screen captures (while a screenshot is taken, as the glass is).
    pub fn set_capturable(&self, on: bool) {
        let affinity = if on { WDA_NONE } else { WDA_EXCLUDEFROMCAPTURE };
        unsafe {
            let _ = SetWindowDisplayAffinity(self.hwnd, affinity);
        }
    }

    /// Right below the owner, so the owner's glass covers the shadow's middle.
    pub fn stay_under(&self) {
        unsafe {
            let _ = SetWindowPos(self.hwnd, Some(self.owner), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }

    /// The owner moved or changed size.
    pub fn follow(&mut self, rect: RECT, radius: f32) {
        let r = shadow_rect(rect, self.margin);
        let (w, h) = ((r.right - r.left) as u32, (r.bottom - r.top) as u32);
        unsafe {
            let _ = SetWindowPos(self.hwnd, Some(self.owner), r.left, r.top, w as i32, h as i32, SWP_NOACTIVATE);
        }
        self.radius = radius;
        if self.layer.size() != (w, h) {
            let _ = self.layer.resize(w, h);
        }
        self.paint();
    }

    fn paint(&mut self) {
        let (w, h) = self.layer.size();
        let m = self.margin as f32;
        let owner = RectF::new(m, m + self.fall, w as f32 - 2.0 * m, h as f32 - 2.0 * m);
        let (blur, radius) = (self.blur, self.radius);
        let r = self.layer.draw(None, |p| {
            p.clear();
            // Stacked rounded rectangles, widest and faintest first: where more of them overlap
            // the shadow is darker, falling off smoothly towards the outer edge.
            let a = PEAK / STEPS as f32;
            for k in 0..STEPS {
                let t = k as f32 / STEPS as f32;
                let grow = blur * (1.0 - t).powf(1.6);
                p.fill_round_rect(owner.inset(-grow), radius + grow, Color::rgba(0, 0, 0, a));
            }
        });
        if let Err(e) = r.and_then(|_| self.gfx.commit()) {
            log::warn!("shadow paint failed: {e}");
        }
    }

    /// Jumps to an offset and opacity (as the owner does before its show animation).
    pub fn set_offset_opacity(&mut self, y: f32, opacity: f32) {
        if let Ok(v) = self.layer.visual() {
            let _ = set_offset_opacity(&self.gfx, &v, None, y, opacity);
        }
    }

    /// The owner's scale about its top centre, as in [`crate::scale`].
    pub fn scale(&mut self, from: f32, to: f32, ms: u32, spring: bool) {
        let (w, _) = self.layer.size();
        let center = (w as f32 / 2.0, self.margin as f32);
        if let Ok(v) = self.layer.visual()
            && let Err(e) = crate::scale(&self.gfx, &v, from, to, ms, center, spring)
        {
            log::warn!("shadow scale failed: {e}");
        }
    }

    /// The owner's slide and fade, on the same curve.
    pub fn animate(&mut self, from_y: f32, to_y: f32, from_o: f32, to_o: f32, ms: u32) {
        if let Ok(v) = self.layer.visual() {
            let r = slide_y(&self.gfx, &v, from_y, to_y, ms).and_then(|_| fade(&self.gfx, &v, from_o, to_o, ms));
            if let Err(e) = r {
                log::warn!("shadow animation failed: {e}");
            }
        }
    }
}

impl Drop for Shadow {
    fn drop(&mut self) {
        unsafe {
            if IsWindow(Some(self.hwnd)).as_bool() {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}
