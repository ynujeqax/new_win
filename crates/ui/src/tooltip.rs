//! A small glass capsule with one line of text under a bar item (spec: M2 §2.1). It never takes
//! focus or the mouse.

use std::cell::RefCell;
use std::rc::Rc;

use newwin_platform::dpi::primary_monitor;
use newwin_platform::window::{Handler, WindowSpec, create_window};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::Result;

use crate::backdrop::Frame;
use crate::panel::{GlassWindow, panel_rect};
use crate::popup::PopupStyle;
use crate::{Backdrop, Gfx, Layer, PointF, RectF, TextCache, TextLayout, TextStyle, fade, paint_glass, theme};

const PAD_X: f32 = 10.0;
const HEIGHT: f32 = 24.0;
const FADE_MS: u32 = 200;

struct State {
    gfx: Rc<Gfx>,
    hwnd: HWND,
    layer: Option<Layer>,
    cache: TextCache,
    style: TextStyle,
    text: Option<TextLayout>,
    size: (f32, f32),
    dpi: u32,
    glass: bool,
}

impl Handler for State {
    fn on_message(&mut self, _: HWND, msg: u32, _: WPARAM, _: LPARAM) -> Option<LRESULT> {
        match msg {
            WM_MOUSEACTIVATE => Some(LRESULT(MA_NOACTIVATE as isize)),
            WM_NCHITTEST => Some(LRESULT(HTTRANSPARENT as isize)),
            _ => None,
        }
    }
}

impl State {
    fn s(&self, v: f32) -> f32 {
        (v * self.dpi as f32 / 96.0).round()
    }

    fn layout(&mut self, text: &str) {
        self.text = self.cache.layout(text, &self.style, 10_000.0).ok();
        let w = self.text.as_ref().map_or(0.0, |t| t.width()).ceil() + 2.0 * self.s(PAD_X);
        self.size = (w, self.s(HEIGHT));
    }

    fn paint(&mut self) {
        let (w, h) = self.size;
        let (glass, text) = (self.glass, &self.text);
        let Some(layer) = self.layer.as_mut() else { return };
        let r = layer.draw(None, |p| {
            paint_glass(p, RectF::new(0.0, 0.0, w, h), h / 2.0, glass);
            if let Some(t) = text {
                let origin = PointF { x: ((w - t.width()) / 2.0).round(), y: ((h - t.height()) / 2.0).round() };
                p.text(t, origin, theme::TEXT);
            }
        });
        if let Err(e) = r.and_then(|_| self.gfx.commit()) {
            log::error!("tooltip paint failed: {e}");
        }
    }
}

pub struct Tooltip {
    state: Rc<RefCell<State>>,
    hwnd: HWND,
}

impl Tooltip {
    /// Shows `text` with the capsule's top centred at `anchor` (screen px); fades in.
    pub fn show(gfx: &Rc<Gfx>, text: &str, anchor: PointF, style: &PopupStyle) -> Result<Tooltip> {
        let size = (theme::TOOLTIP_TEXT * style.dpi as f32 / 96.0).round();
        let mut state = State {
            gfx: gfx.clone(),
            hwnd: HWND::default(),
            layer: None,
            cache: TextCache::new(gfx),
            style: TextStyle { family: style.font_family.clone(), size, weight: theme::WEIGHT_REGULAR },
            text: None,
            size: (1.0, 1.0),
            dpi: style.dpi,
            glass: false,
        };
        state.layout(text);
        let (mon, _) = primary_monitor();
        let r = panel_rect(anchor, state.size, mon);
        let (w, h) = state.size;
        let state = Rc::new(RefCell::new(state));
        let hwnd = create_window(
            &WindowSpec {
                class: "NewWin.Tooltip",
                ex_style: WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_NOREDIRECTIONBITMAP,
                style: WS_POPUP,
                rect: r,
                layered_alpha: None,
            },
            state.clone(),
        )?;
        {
            let mut s = state.borrow_mut();
            s.hwnd = hwnd;
            let mut layer = Layer::new(gfx, hwnd, Backdrop::Glass, w as u32, h as u32)?;
            layer.set_corner_radius(h / 2.0)?;
            if let Ok(v) = layer.visual() {
                let _ = fade(gfx, &v, 0.0, 1.0, FADE_MS);
            }
            s.layer = Some(layer);
            s.paint();
        }
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        Ok(Tooltip { state, hwnd })
    }

    /// New text; the capsule resizes around its centre.
    pub fn set_text(&self, text: &str) {
        let Ok(mut s) = self.state.try_borrow_mut() else { return };
        s.layout(text);
        let (w, h) = s.size;
        let mut wr = RECT::default();
        unsafe {
            let _ = GetWindowRect(self.hwnd, &mut wr);
            let (mon, _) = primary_monitor();
            let r = panel_rect(PointF { x: (wr.left + wr.right) as f32 / 2.0, y: wr.top as f32 }, (w, h), mon);
            let _ = SetWindowPos(self.hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOACTIVATE | SWP_NOZORDER);
        }
        if let Some(layer) = s.layer.as_mut() {
            let _ = layer.resize(w as u32, h as u32);
        }
        s.paint();
    }
}

impl Drop for Tooltip {
    fn drop(&mut self) {
        unsafe {
            if IsWindow(Some(self.hwnd)).as_bool() {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

impl GlassWindow for Tooltip {
    fn windows(&self) -> Vec<(HWND, RECT)> {
        let mut r = RECT::default();
        unsafe {
            let _ = GetWindowRect(self.hwnd, &mut r);
        }
        vec![(self.hwnd, r)]
    }

    fn radius(&self) -> f32 {
        self.state.try_borrow().map_or(0.0, |s| s.size.1 / 2.0)
    }

    fn set_backdrop(&self, hwnd: HWND, frame: &Frame) {
        if hwnd != self.hwnd {
            return;
        }
        let Ok(mut s) = self.state.try_borrow_mut() else { return };
        let mut r = RECT::default();
        unsafe {
            let _ = GetWindowRect(self.hwnd, &mut r);
        }
        let Some(layer) = s.layer.as_mut() else { return };
        if layer.set_backdrop(frame, (r.left, r.top)).is_ok() && !s.glass {
            s.glass = true;
            s.paint();
        }
    }

    fn set_capturable(&self, on: bool) {
        if let Ok(s) = self.state.try_borrow()
            && let Some(layer) = s.layer.as_ref()
        {
            layer.set_capturable(on);
        }
    }

    fn set_glass_unavailable(&self) {
        let Ok(mut s) = self.state.try_borrow_mut() else { return };
        if let Some(layer) = s.layer.as_mut() {
            let _ = layer.clear_backdrop();
        }
        if s.glass {
            s.glass = false;
            s.paint();
        }
    }
}
