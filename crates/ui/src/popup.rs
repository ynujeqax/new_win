//! Glass popup menus: text items, separators, one level of submenu.
//!
//! Results are posted to a notify window as `msg`: `lParam` = [`PICKED`] with `wParam` = item id,
//! or `lParam` = [`CLOSED`] with `wParam` = the root popup's HWND (so a late close of a replaced
//! menu can be told apart). A popup never destroys itself: the owner drops the [`Popup`] on
//! `CLOSED` (which always follows a `PICKED`). Submenus never take activation, so the root
//! keeps keyboard focus and a click anywhere else deactivates it.

use std::cell::RefCell;
use std::rc::Rc;

use newwin_platform::dpi::primary_monitor;
use newwin_platform::window::{Handler, WindowSpec, create_window};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_RETURN, VK_RIGHT, VK_UP};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::Result;

use crate::{Backdrop, Color, Gfx, Layer, PointF, RectF, TextCache, TextLayout, TextStyle, theme};

pub const PICKED: isize = 1;
pub const CLOSED: isize = 0;
/// Not exported by the `windows` crate.
const WM_MOUSELEAVE: u32 = 0x02A3;

#[derive(Clone, Debug, PartialEq)]
pub struct MenuItem {
    pub id: u32,
    pub label: String,
    pub enabled: bool,
    pub bold: bool,
    pub separator_before: bool,
    pub submenu: Option<Vec<MenuItem>>,
}

impl MenuItem {
    pub fn new(id: u32, label: impl Into<String>) -> Self {
        Self { id, label: label.into(), enabled: true, bold: false, separator_before: false, submenu: None }
    }

    pub fn enabled(mut self, on: bool) -> Self {
        self.enabled = on;
        self
    }

    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    pub fn separator(mut self) -> Self {
        self.separator_before = true;
        self
    }

    pub fn submenu(mut self, items: Vec<MenuItem>) -> Self {
        self.submenu = Some(items);
        self
    }
}

/// Physical-pixel sizes of a popup.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupMetrics {
    pub pad: f32,
    pub item_h: f32,
    pub sep_h: f32,
    pub pad_x: f32,
    pub arrow_w: f32,
    pub min_w: f32,
}

impl PopupMetrics {
    pub fn for_dpi(dpi: u32) -> Self {
        let s = |v: f32| (v * dpi as f32 / 96.0).round();
        Self {
            pad: s(theme::POPUP_PAD),
            item_h: s(theme::POPUP_ITEM_HEIGHT),
            sep_h: s(9.0),
            pad_x: s(theme::POPUP_ITEM_PAD_X),
            arrow_w: s(16.0),
            min_w: s(160.0),
        }
    }
}

/// Popup size and item rectangles (popup-local) from the items' text widths.
pub fn layout_items(widths: &[f32], separators: &[bool], has_sub: &[bool], m: &PopupMetrics) -> (f32, f32, Vec<RectF>) {
    let arrow = if has_sub.iter().any(|&s| s) { m.arrow_w } else { 0.0 };
    let text = widths.iter().copied().fold(0.0, f32::max);
    let w = (text + 2.0 * m.pad_x + arrow + 2.0 * m.pad).max(m.min_w);
    let mut y = m.pad;
    let rects = widths
        .iter()
        .enumerate()
        .map(|(i, _)| {
            if i > 0 && separators[i] {
                y += m.sep_h;
            }
            let r = RectF::new(m.pad, y, w - 2.0 * m.pad, m.item_h);
            y += m.item_h;
            r
        })
        .collect();
    (w, y + m.pad, rects)
}

pub fn item_at(rects: &[RectF], p: PointF) -> Option<usize> {
    rects.iter().position(|r| r.contains(p))
}

pub struct PopupStyle {
    pub font_family: String,
    pub font_size_px: f32,
    pub dpi: u32,
}

struct State {
    gfx: Rc<Gfx>,
    hwnd: HWND,
    layer: Option<Layer>,
    items: Vec<MenuItem>,
    texts: Vec<TextLayout>,
    arrow: Option<TextLayout>,
    rects: Vec<RectF>,
    size: (f32, f32),
    metrics: PopupMetrics,
    hover: Option<usize>,
    tracking: bool,
    notify: HWND,
    msg: u32,
    is_child: bool,
    /// The root popup's window; reported with CLOSED.
    root: HWND,
    closed: bool,
    child: Option<(usize, Popup)>,
    font_family: String,
    font_size: f32,
    dpi: u32,
}

pub struct Popup {
    state: Rc<RefCell<State>>,
    hwnd: HWND,
}

impl Popup {
    /// Shows a menu with its top-left at `anchor` (screen px), kept inside the monitor.
    pub fn show(gfx: &Rc<Gfx>, items: Vec<MenuItem>, anchor: PointF, style: &PopupStyle, notify: HWND, msg: u32) -> Result<Popup> {
        Self::create(gfx, items, anchor, style, notify, msg, None)
    }

    /// `root` is `Some` for a submenu.
    fn create(gfx: &Rc<Gfx>, items: Vec<MenuItem>, anchor: PointF, style: &PopupStyle, notify: HWND, msg: u32, root: Option<HWND>) -> Result<Popup> {
        let is_child = root.is_some();
        let metrics = PopupMetrics::for_dpi(style.dpi);
        let mut cache = TextCache::new(gfx);
        let mut texts = Vec::with_capacity(items.len());
        for it in &items {
            let weight = if it.bold { theme::WEIGHT_SEMIBOLD } else { theme::WEIGHT_REGULAR };
            let ts = TextStyle { family: style.font_family.clone(), size: style.font_size_px, weight };
            texts.push(cache.layout(&it.label, &ts, 10_000.0)?);
        }
        let arrow_style = TextStyle { family: style.font_family.clone(), size: style.font_size_px, weight: theme::WEIGHT_REGULAR };
        let arrow = items.iter().any(|i| i.submenu.is_some()).then(|| cache.layout("›", &arrow_style, 1000.0)).transpose()?;
        let widths: Vec<f32> = texts.iter().map(TextLayout::width).collect();
        let seps: Vec<bool> = items.iter().map(|i| i.separator_before).collect();
        let subs: Vec<bool> = items.iter().map(|i| i.submenu.is_some()).collect();
        let (w, h, rects) = layout_items(&widths, &seps, &subs, &metrics);

        let (mon, _) = primary_monitor();
        let x = (anchor.x as i32).min(mon.right - w as i32).max(mon.left);
        let y = (anchor.y as i32).min(mon.bottom - h as i32).max(mon.top);
        let mut ex = WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOREDIRECTIONBITMAP;
        if is_child {
            ex |= WS_EX_NOACTIVATE;
        }
        let state = Rc::new(RefCell::new(State {
            gfx: gfx.clone(),
            hwnd: HWND::default(),
            layer: None,
            items,
            texts,
            arrow,
            rects,
            size: (w, h),
            metrics,
            hover: None,
            tracking: false,
            notify,
            msg,
            is_child,
            root: root.unwrap_or_default(),
            closed: false,
            child: None,
            font_family: style.font_family.clone(),
            font_size: style.font_size_px,
            dpi: style.dpi,
        }));
        let hwnd = create_window(
            &WindowSpec {
                class: "NewWin.Popup",
                ex_style: ex,
                style: WS_POPUP,
                rect: RECT { left: x, top: y, right: x + w as i32, bottom: y + h as i32 },
                layered_alpha: None,
            },
            state.clone(),
        )?;
        {
            let mut s = state.borrow_mut();
            s.hwnd = hwnd;
            if !is_child {
                s.root = hwnd;
            }
            s.layer = Some(Layer::new(gfx, hwnd, Backdrop::Tint, w as u32, h as u32)?);
            s.paint();
        }
        unsafe {
            if is_child {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            } else {
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = SetForegroundWindow(hwnd);
            }
        }
        Ok(Popup { state, hwnd })
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    pub fn close(self) {}
}

impl Drop for Popup {
    fn drop(&mut self) {
        if let Ok(mut s) = self.state.try_borrow_mut() {
            s.closed = true;
            s.child = None;
        }
        unsafe {
            if IsWindow(Some(self.hwnd)).as_bool() {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

impl State {
    fn paint(&mut self) {
        let (w, h) = self.size;
        let m = self.metrics;
        let r = (theme::POPUP_RADIUS * self.dpi as f32 / 96.0).round();
        let item_r = (theme::POPUP_ITEM_RADIUS * self.dpi as f32 / 96.0).round();
        let (items, texts, rects, hover, arrow) = (&self.items, &self.texts, &self.rects, self.hover, &self.arrow);
        let Some(layer) = self.layer.as_mut() else { return };
        let res = layer.draw(None, |p| {
            let full = RectF::new(0.0, 0.0, w, h);
            p.fill_round_rect(full, r, theme::TINT);
            p.fill_round_rect(full, r, theme::POPUP_BG);
            p.stroke_round_rect(full, r, theme::BORDER, 1.0);
            for (i, it) in items.iter().enumerate() {
                let rect = rects[i];
                if it.separator_before && i > 0 {
                    let y = (rect.y - m.sep_h / 2.0).round() + 0.5;
                    p.line(PointF { x: rect.x + m.pad_x, y }, PointF { x: rect.right() - m.pad_x, y }, theme::SEPARATOR, 1.0);
                }
                if hover == Some(i) && it.enabled {
                    p.fill_round_rect(rect, item_r, theme::POPUP_HOVER);
                }
                let color: Color = if !it.enabled {
                    theme::TEXT_DISABLED
                } else if hover == Some(i) {
                    theme::TEXT_STRONG
                } else {
                    theme::TEXT
                };
                let t = &texts[i];
                p.text(t, PointF { x: rect.x + m.pad_x, y: (rect.y + (rect.h - t.height()) / 2.0).round() }, color);
                if it.submenu.is_some()
                    && let Some(a) = arrow
                {
                    p.text(a, PointF { x: rect.right() - m.pad_x - a.width(), y: (rect.y + (rect.h - a.height()) / 2.0).round() }, color);
                }
            }
        });
        if let Err(e) = res.and_then(|_| self.gfx.commit()) {
            log::error!("popup paint failed: {e}");
        }
    }

    fn post(&self, wp: usize, what: isize) {
        unsafe {
            let _ = PostMessageW(Some(self.notify), self.msg, WPARAM(wp), LPARAM(what));
        }
    }

    /// Reports the end of the menu once; the owner then drops the root popup.
    fn finish(&mut self, picked: Option<u32>) {
        if self.closed {
            return;
        }
        self.closed = true;
        if let Some(id) = picked {
            self.post(id as usize, PICKED);
        }
        self.post(self.root.0 as usize, CLOSED);
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    fn set_hover(&mut self, hover: Option<usize>) {
        if self.hover != hover {
            self.hover = hover;
            self.paint();
        }
        // Submenus follow the hovered item.
        let wants = hover.filter(|&i| self.items[i].enabled && self.items[i].submenu.is_some());
        if self.child.as_ref().map(|(i, _)| *i) != wants {
            self.child = None;
            if let Some(i) = wants {
                self.open_child(i);
            }
        }
    }

    fn open_child(&mut self, i: usize) {
        let Some(sub) = self.items[i].submenu.clone() else { return };
        let mut wr = RECT::default();
        unsafe {
            let _ = GetWindowRect(self.hwnd, &mut wr);
        }
        let anchor = PointF { x: wr.right as f32 - self.metrics.pad, y: wr.top as f32 + self.rects[i].y - self.metrics.pad };
        let style = PopupStyle { font_family: self.font_family.clone(), font_size_px: self.font_size, dpi: self.dpi };
        match Popup::create(&self.gfx, sub, anchor, &style, self.notify, self.msg, Some(self.root)) {
            Ok(p) => self.child = Some((i, p)),
            Err(e) => log::error!("submenu failed: {e}"),
        }
    }

    fn activate_item(&mut self, i: usize) {
        let it = &self.items[i];
        if !it.enabled {
            return;
        }
        if it.submenu.is_some() {
            self.set_hover(Some(i));
        } else {
            let id = it.id;
            self.finish(Some(id));
        }
    }

    fn step(&mut self, down: bool) {
        let n = self.items.len() as isize;
        let mut i = self.hover.map_or(if down { -1 } else { n }, |h| h as isize);
        for _ in 0..n {
            i = (i + if down { 1 } else { -1 }).rem_euclid(n);
            if self.items[i as usize].enabled {
                self.set_hover(Some(i as usize));
                return;
            }
        }
    }
}

fn point(lp: LPARAM) -> PointF {
    PointF { x: (lp.0 & 0xFFFF) as i16 as f32, y: ((lp.0 >> 16) & 0xFFFF) as i16 as f32 }
}

impl Handler for State {
    fn on_message(&mut self, _hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
        match msg {
            WM_MOUSEACTIVATE if self.is_child => Some(LRESULT(MA_NOACTIVATE as isize)),
            WM_MOUSEMOVE => {
                if !self.tracking {
                    let mut tme = TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: self.hwnd, dwHoverTime: 0 };
                    self.tracking = unsafe { TrackMouseEvent(&mut tme) }.is_ok();
                }
                let hit = item_at(&self.rects, point(lp));
                self.set_hover(hit);
                Some(LRESULT(0))
            }
            WM_MOUSELEAVE => {
                self.tracking = false;
                // Keep the highlight on the item whose submenu the pointer moved into.
                if self.child.is_none() {
                    self.set_hover(None);
                }
                Some(LRESULT(0))
            }
            WM_LBUTTONUP => {
                if let Some(i) = item_at(&self.rects, point(lp)) {
                    self.activate_item(i);
                }
                Some(LRESULT(0))
            }
            WM_KEYDOWN => {
                let vk = wp.0 as u16;
                if vk == VK_ESCAPE.0 {
                    self.finish(None);
                } else if vk == VK_DOWN.0 {
                    self.step(true);
                } else if vk == VK_UP.0 {
                    self.step(false);
                } else if vk == VK_RETURN.0 || vk == VK_RIGHT.0 {
                    if let Some(i) = self.hover {
                        self.activate_item(i);
                    }
                } else if vk == VK_LEFT.0 {
                    self.child = None;
                }
                Some(LRESULT(0))
            }
            WM_ACTIVATE if !self.is_child && (wp.0 & 0xFFFF) as u32 == WA_INACTIVE => {
                self.finish(None);
                Some(LRESULT(0))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod popup_point_tests {
    use super::*;

    #[test]
    fn lparam_points_are_signed() {
        let lp = LPARAM(((0xFFFEu32 << 16) | 0x0010) as isize);
        assert_eq!(point(lp), PointF { x: 16.0, y: -2.0 });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics() -> PopupMetrics {
        PopupMetrics { pad: 6.0, item_h: 30.0, sep_h: 9.0, pad_x: 10.0, arrow_w: 16.0, min_w: 120.0 }
    }

    #[test]
    fn layout_with_separator_and_submenu() {
        let (w, h, rects) = layout_items(&[50.0, 100.0, 70.0], &[false, false, true], &[false, true, false], &metrics());
        assert_eq!((w, h), (148.0, 111.0));
        assert_eq!(rects[0], RectF::new(6.0, 6.0, 136.0, 30.0));
        assert_eq!(rects[1], RectF::new(6.0, 36.0, 136.0, 30.0));
        assert_eq!(rects[2], RectF::new(6.0, 75.0, 136.0, 30.0));
    }

    #[test]
    fn minimum_width() {
        let (w, _, rects) = layout_items(&[20.0], &[false], &[false], &metrics());
        assert_eq!(w, 120.0);
        assert_eq!(rects[0].w, 108.0);
    }

    #[test]
    fn hit_testing() {
        let (_, _, rects) = layout_items(&[50.0, 100.0, 70.0], &[false, false, true], &[false, true, false], &metrics());
        assert_eq!(item_at(&rects, PointF { x: 10.0, y: 40.0 }), Some(1));
        assert_eq!(item_at(&rects, PointF { x: 10.0, y: 70.0 }), None);
        assert_eq!(item_at(&rects, PointF { x: 10.0, y: 2.0 }), None);
    }
}
