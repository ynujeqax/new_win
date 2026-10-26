//! Glass popovers with custom content (spec: M2 §3): the tray list, the volume panel.
//!
//! Results are posted like [`crate::popup`]'s: `lParam` = [`PICKED`] with the content's action
//! id in `wParam`, then `lParam` = [`CLOSED`] with the panel's HWND once the closing animation
//! has run. The owner drops the [`Panel`] on `CLOSED`. `lParam` = [`REGIONS`] (same `wParam`)
//! when the panel changed size: the owner re-sends the glass regions.

use std::cell::RefCell;
use std::rc::Rc;

use newwin_platform::clock::now_ms;
use newwin_platform::dpi::primary_monitor;
use newwin_platform::window::{Handler, WindowSpec, create_window};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::Result;

use crate::backdrop::Frame;
use crate::popup::{CLOSED, PICKED, REGIONS};
use crate::win_loop::{Frames, Mouse, REVEAL_TIMER, end_clear, start_clear};
use crate::shadow::Shadow;
use crate::{Backdrop, Gfx, Layer, Painter, PointF, RectF, fade, paint_glass, slide_y_both, theme};

const CLOSE_TIMER: usize = 2;

/// A glass window whose backdrop the shell's capture feeds (bar menus, panels, tooltips).
pub trait GlassWindow {
    /// The window and any child glass windows, with screen rectangles.
    fn windows(&self) -> Vec<(HWND, RECT)>;
    /// Corner radius, physical px.
    fn radius(&self) -> f32;
    fn set_backdrop(&self, hwnd: HWND, frame: &Frame);
    fn set_capturable(&self, on: bool);
    fn set_glass_unavailable(&self);
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PanelInput {
    Move(PointF),
    Leave,
    Press(PointF),
    Release(PointF),
    RightRelease(PointF),
    Wheel(i16),
    Key(u16),
    /// A key pressed while Ctrl is held (instead of `Key`).
    CtrlKey(u16),
    /// `WM_CONTEXTMENU`: the menu key or `Shift+F10` (`None`), or a right click at a client point.
    ContextMenu(Option<PointF>),
    Char(char),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PanelReply {
    pub repaint: bool,
    pub resize: bool,
    pub action: Option<u32>,
    pub close: bool,
}

/// What a panel shows; sizes in physical px.
pub trait PanelContent {
    fn size(&self) -> (f32, f32);
    fn paint(&mut self, p: &mut Painter, now_ms: u64);
    fn input(&mut self, ev: PanelInput, now_ms: u64) -> PanelReply;
    /// True while a drawn animation runs; the panel repaints every frame until it ends.
    fn animating(&self, now_ms: u64) -> bool;
    /// When the content next changes on its own (a caret blink), if ever: the panel repaints then.
    fn wake_at(&self, _now_ms: u64) -> Option<u64> {
        None
    }
}

/// The panel's screen rectangle: centred under `anchor`, kept inside `monitor`.
pub fn panel_rect(anchor: PointF, size: (f32, f32), monitor: RECT) -> RECT {
    let (w, h) = (size.0.round() as i32, size.1.round() as i32);
    let left = ((anchor.x - size.0 / 2.0).round() as i32).min(monitor.right - w).max(monitor.left);
    let top = anchor.y.round() as i32;
    RECT { left, top, right: left + w, bottom: top + h }
}

/// Capsule when short, else the panel radius.
pub fn radius_for(h: f32, dpi: u32) -> f32 {
    let s = dpi as f32 / 96.0;
    if h <= 44.0 * s { h / 2.0 } else { (theme::PANEL_RADIUS * s).round() }
}

struct State<C> {
    gfx: Rc<Gfx>,
    hwnd: HWND,
    layer: Option<Layer>,
    content: C,
    size: (f32, f32),
    dpi: u32,
    notify: HWND,
    msg: u32,
    mouse: Mouse,
    closing: bool,
    closed: bool,
    glass: bool,
    frames: Frames,
    /// Stays open while it loses focus once (an app's own menu opened from it).
    held: bool,
    shown: std::time::Instant,
    /// Shown but clear until its first glass frame (`show_after_glass`).
    waiting: bool,
    shadow: Option<Shadow>,
}

pub struct Panel<C: PanelContent + 'static> {
    state: Rc<RefCell<State<C>>>,
    hwnd: HWND,
}

impl<C: PanelContent + 'static> Panel<C> {
    /// Shows `content` with its top centred at `anchor` (screen px) and takes the foreground.
    pub fn show(gfx: &Rc<Gfx>, content: C, anchor: PointF, dpi: u32, notify: HWND, msg: u32) -> Result<Panel<C>> {
        Self::create(gfx, content, anchor, dpi, notify, msg, false)
    }

    /// Like [`Self::show`], but stays clear (while already taking the keyboard) until its first
    /// glass frame, so its fallback tint never flashes; at most 250 ms.
    pub fn show_after_glass(gfx: &Rc<Gfx>, content: C, anchor: PointF, dpi: u32, notify: HWND, msg: u32) -> Result<Panel<C>> {
        Self::create(gfx, content, anchor, dpi, notify, msg, true)
    }

    fn create(gfx: &Rc<Gfx>, content: C, anchor: PointF, dpi: u32, notify: HWND, msg: u32, wait_glass: bool) -> Result<Panel<C>> {
        let started = std::time::Instant::now();
        let size = content.size();
        let (mon, _) = primary_monitor();
        let r = panel_rect(anchor, size, mon);
        let state = Rc::new(RefCell::new(State {
            gfx: gfx.clone(),
            hwnd: HWND::default(),
            layer: None,
            content,
            size,
            dpi,
            notify,
            msg,
            mouse: Mouse::default(),
            closing: false,
            closed: false,
            glass: false,
            frames: Frames::default(),
            held: false,
            shown: started,
            waiting: wait_glass,
            shadow: None,
        }));
        let hwnd = create_window(
            &WindowSpec {
                class: "NewWin.Panel",
                ex_style: WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOREDIRECTIONBITMAP,
                style: WS_POPUP,
                rect: r,
                layered_alpha: None,
            },
            state.clone(),
        )?;
        {
            let mut s = state.borrow_mut();
            s.hwnd = hwnd;
            let mut layer = Layer::new(gfx, hwnd, Backdrop::Glass, size.0 as u32, size.1 as u32)?;
            layer.set_corner_radius(radius_for(size.1, dpi))?;
            s.layer = Some(layer);
            s.shadow = Shadow::new(gfx, hwnd, r, radius_for(size.1, dpi), dpi).map_err(|e| log::warn!("panel shadow: {e}")).ok();
            s.paint();
            if wait_glass {
                s.hide_until_glass();
            } else {
                s.animate(true);
            }
        }
        log::info!("panel first-frame-ms={:.1}", started.elapsed().as_secs_f64() * 1000.0);
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
        }
        if let Some(sh) = &state.borrow().shadow {
            sh.stay_under();
        }
        Ok(Panel { state, hwnd })
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    /// Changes the content, then resizes and repaints.
    pub fn update(&self, f: impl FnOnce(&mut C)) {
        let Ok(mut s) = self.state.try_borrow_mut() else { return };
        f(&mut s.content);
        s.after(PanelReply { repaint: true, resize: true, ..Default::default() });
    }

    /// Starts the closing animation (a second click on the panel's button).
    pub fn close(&self) {
        if let Ok(mut s) = self.state.try_borrow_mut() {
            s.begin_close();
        }
    }

    /// Keeps the panel open when it loses focus, until it is activated again or closed.
    pub fn hold(&self) {
        if let Ok(mut s) = self.state.try_borrow_mut() {
            s.held = true;
        }
    }

    /// Reads the content (`None` while the panel is busy with it).
    pub fn read<R>(&self, f: impl FnOnce(&C) -> R) -> Option<R> {
        self.state.try_borrow().ok().map(|s| f(&s.content))
    }

    /// False while a `show_after_glass` panel still waits for its glass.
    pub fn revealed(&self) -> bool {
        self.state.try_borrow().is_ok_and(|s| !s.waiting)
    }

    pub fn is_closing(&self) -> bool {
        self.state.try_borrow().map_or(true, |s| s.closing)
    }
}

impl<C: PanelContent + 'static> Drop for Panel<C> {
    fn drop(&mut self) {
        if let Ok(mut s) = self.state.try_borrow_mut() {
            s.closed = true;
            s.shadow = None;
        }
        unsafe {
            if IsWindow(Some(self.hwnd)).as_bool() {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

impl<C: PanelContent + 'static> GlassWindow for Panel<C> {
    fn windows(&self) -> Vec<(HWND, RECT)> {
        let mut r = RECT::default();
        unsafe {
            let _ = GetWindowRect(self.hwnd, &mut r);
        }
        vec![(self.hwnd, r)]
    }

    fn radius(&self) -> f32 {
        self.state.try_borrow().map_or(0.0, |s| radius_for(s.size.1, s.dpi))
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
        if let Err(e) = layer.set_backdrop(frame, (r.left, r.top)) {
            log::error!("panel backdrop failed: {e}");
            return;
        }
        if !s.glass {
            log::debug!("panel glass after {} ms", s.shown.elapsed().as_millis());
            s.glass = true;
            s.paint();
            s.reveal();
        }
    }

    fn set_capturable(&self, on: bool) {
        let Ok(s) = self.state.try_borrow() else { return };
        if let Some(layer) = s.layer.as_ref() {
            layer.set_capturable(on);
        }
        if let Some(sh) = &s.shadow {
            sh.set_capturable(on);
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
        s.reveal();
    }
}

impl<C: PanelContent> State<C> {
    fn paint(&mut self) {
        let (w, h) = self.size;
        let r = radius_for(h, self.dpi);
        let now = now_ms();
        let glass = self.glass;
        let content = &mut self.content;
        let Some(layer) = self.layer.as_mut() else { return };
        let res = layer.draw(None, |p| {
            paint_glass(p, RectF::new(0.0, 0.0, w, h), r, glass);
            content.paint(p, now);
        });
        if let Err(e) = res.and_then(|_| self.gfx.commit()) {
            log::error!("panel paint failed: {e}");
        }
        self.keep_animating(now);
    }

    /// Slides in from just under the bar (or back up when closing) and fades.
    /// Clear, where the show animation starts, until [`Self::reveal`].
    fn hide_until_glass(&mut self) {
        let drop = -(theme::PANEL_DROP_PX * self.dpi as f32 / 96.0).round();
        if let Some(layer) = self.layer.as_mut()
            && let Err(e) = start_clear(&self.gfx, layer, self.hwnd, drop)
        {
            log::warn!("panel cannot start clear: {e}");
        }
    }

    /// The glass came (or will not): play the show animation.
    fn reveal(&mut self) {
        if !self.waiting {
            return;
        }
        self.waiting = false;
        end_clear(self.hwnd);
        if !self.closing {
            self.animate(true);
        }
    }

    fn animate(&mut self, show: bool) {
        let drop = -(theme::PANEL_DROP_PX * self.dpi as f32 / 96.0).round();
        let Some(layer) = self.layer.as_mut() else { return };
        let (Ok(root), Ok(inner)) = (layer.visual(), layer.backdrop_visual()) else { return };
        let (from, to, o0, o1, ms) = if show { (drop, 0.0, 0.0, 1.0, theme::PANEL_SHOW_MS) } else { (0.0, drop, 1.0, 0.0, theme::PANEL_HIDE_MS) };
        let r = slide_y_both(&self.gfx, &root, inner.as_ref(), from, to, ms).and_then(|_| fade(&self.gfx, &root, o0, o1, ms));
        // Opening springs up from a little smaller; closing shrinks a touch as it fades.
        let (s0, s1) = if show { (theme::OPEN_SCALE, 1.0) } else { (1.0, theme::CLOSE_SCALE) };
        if let Err(e) = crate::scale(&self.gfx, &root, s0, s1, ms, (self.size.0 / 2.0, 0.0), show) {
            log::warn!("panel scale failed: {e}");
        }
        if let Some(sh) = self.shadow.as_mut() {
            sh.animate(from, to, o0, o1, ms);
            sh.scale(s0, s1, ms, show);
        }
        if let Err(e) = r {
            log::warn!("panel animation failed: {e}");
        }
    }

    fn keep_animating(&mut self, now: u64) {
        let want = !self.closing && self.content.animating(now);
        let wake = self.content.wake_at(now).filter(|_| !self.closing);
        self.frames.schedule(self.hwnd, want, wake, now);
    }

    fn post(&self, wp: usize, what: isize) {
        unsafe {
            let _ = PostMessageW(Some(self.notify), self.msg, WPARAM(wp), LPARAM(what));
        }
    }

    fn begin_close(&mut self) {
        if self.closing {
            return;
        }
        self.closing = true;
        self.mouse.release();
        self.animate(false);
        unsafe { SetTimer(Some(self.hwnd), CLOSE_TIMER, theme::PANEL_HIDE_MS, None) };
    }

    fn resize(&mut self) {
        let size = self.content.size();
        if size == self.size {
            return;
        }
        self.size = size;
        let mut wr = RECT::default();
        unsafe {
            let _ = GetWindowRect(self.hwnd, &mut wr);
            // Keep the top centre where it was.
            let cx = (wr.left + wr.right) as f32 / 2.0;
            let (mon, _) = primary_monitor();
            let r = panel_rect(PointF { x: cx, y: wr.top as f32 }, size, mon);
            let _ = SetWindowPos(self.hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOACTIVATE | SWP_NOZORDER);
            if let Some(sh) = self.shadow.as_mut() {
                sh.follow(r, radius_for(size.1, self.dpi));
            }
        }
        if let Some(layer) = self.layer.as_mut() {
            let _ = layer.resize(size.0 as u32, size.1 as u32);
            let _ = layer.set_corner_radius(radius_for(size.1, self.dpi));
        }
        // The glass thread reads the screen for the old rect until the owner re-sends it.
        self.post(self.hwnd.0 as usize, REGIONS);
    }

    fn after(&mut self, reply: PanelReply) {
        if let Some(id) = reply.action {
            self.post(id as usize, PICKED);
        }
        if reply.close {
            self.begin_close();
            return;
        }
        if reply.resize {
            self.resize();
        }
        if reply.repaint || reply.resize {
            self.paint();
        }
    }

    fn feed(&mut self, ev: PanelInput) {
        if self.closing {
            return;
        }
        let reply = self.content.input(ev, now_ms());
        self.after(reply);
    }
}

impl<C: PanelContent> Handler for State<C> {
    fn on_message(&mut self, _hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
        match msg {
            WM_KEYDOWN if wp.0 as u16 == VK_ESCAPE.0 => self.begin_close(),
            WM_TIMER if wp.0 == REVEAL_TIMER => self.reveal(),
            WM_TIMER if wp.0 == CLOSE_TIMER => {
                unsafe {
                    let _ = KillTimer(Some(self.hwnd), CLOSE_TIMER);
                    let _ = ShowWindow(self.hwnd, SW_HIDE);
                }
                if !self.closed {
                    self.closed = true;
                    self.post(self.hwnd.0 as usize, CLOSED);
                }
            }
            WM_ACTIVATE if (wp.0 & 0xFFFF) as u32 == WA_INACTIVE => {
                if !self.held {
                    self.begin_close();
                }
            }
            WM_ACTIVATE => self.held = false,
            WM_TIMER => {
                if !Frames::on_timer(self.hwnd, wp.0) {
                    return None;
                }
                self.paint();
            }
            _ => {
                if let Some(ev) = self.mouse.translate(self.hwnd, msg, wp, lp)? {
                    self.feed(ev);
                }
            }
        }
        Some(LRESULT(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_rect_centres_and_clamps() {
        let mon = RECT { left: 0, top: 0, right: 1920, bottom: 1080 };
        assert_eq!(panel_rect(PointF { x: 500.0, y: 50.0 }, (260.0, 120.0), mon), RECT { left: 370, top: 50, right: 630, bottom: 170 });
        assert_eq!(panel_rect(PointF { x: 1900.0, y: 50.0 }, (260.0, 120.0), mon), RECT { left: 1660, top: 50, right: 1920, bottom: 170 });
        assert_eq!(panel_rect(PointF { x: 10.0, y: 50.0 }, (260.0, 120.0), mon).left, 0);
    }
}
