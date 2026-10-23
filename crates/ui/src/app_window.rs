//! A normal top-level glass window (spec: M4 §1, §5 `window.rs`): on the bar and in Alt+Tab,
//! dragged by its own caption ("NewWin", "–", "×"), fixed size. The settings app puts its content
//! in it.
//!
//! The window owns its [`Backdrops`] (its own glass thread): the region is the window rect, re-sent
//! when the window moves (`WM_WINDOWPOSCHANGED`) or is activated; none while minimized. Frames come
//! back to the window itself as [`GLASS_MSG`].
//!
//! Content is painted over the whole window in physical px, under the caption it draws nothing it
//! needs to see: the top [`CAPTION_H`] logical px are non-client (`WM_NCHITTEST`), so mouse input
//! there never reaches the content. `PanelReply::action` has no owner to go to and is ignored;
//! `resize` repaints (the size is fixed); `close` closes the window.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use newwin_core::glass::PxRect;
use newwin_platform::clock::now_ms;
use newwin_platform::dpi::{primary_monitor, scale};
use newwin_platform::window::{Handler, WindowSpec, create_window};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT, TrackMouseEvent};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{HSTRING, Result};

use crate::backdrop::{Backdrops, FRAME, Region, UNAVAILABLE, take_frame};
use crate::panel::{PanelInput, PanelReply};
use crate::win_loop::{Frames, Mouse, REVEAL_TIMER, end_clear, point, start_clear};
use crate::{
    Backdrop, Color, Gfx, Layer, Painter, PointF, RectF, TextCache, TextLayout, TextStyle, ease_out_progress, fade, paint_glass,
    theme,
};

/// Height of the caption strip, logical px.
pub const CAPTION_H: f32 = 40.0;
/// Width of each caption button ("–", "×"), logical px; they sit at the right, close outermost.
pub const BUTTON_W: f32 = 46.0;
/// Corner radius of the window, logical px (the panels' radius).
pub const RADIUS: f32 = theme::PANEL_RADIUS;
/// Glass frames from the window's own [`Backdrops`] (`wParam` = `FRAME` / `UNAVAILABLE`).
pub const GLASS_MSG: u32 = WM_APP + 1;

/// Not exported by the `windows` crate.
const WM_NCMOUSELEAVE: u32 = 0x02A2;
/// Caption button hover fade.
const HOVER_MS: f32 = 120.0;
const TITLE_SIZE: f32 = 12.0;
const TITLE_PAD_X: f32 = 16.0;
const GLYPH: f32 = 10.0;
const FONT: &str = "Segoe UI Variable Text";

/// What an app window shows; painted in physical px over the whole window.
pub trait AppContent {
    fn paint(&mut self, p: &mut Painter, now_ms: u64);
    fn input(&mut self, ev: PanelInput, now_ms: u64) -> PanelReply;
    /// True while a drawn animation runs; the window repaints every frame until it ends.
    fn animating(&self, now_ms: u64) -> bool;
    /// When the content next changes on its own (a caret blink), if ever: the window repaints then.
    fn wake_at(&self, _now_ms: u64) -> Option<u64> {
        None
    }
    /// The window's DPI: called once before the first paint and whenever the window moves to a
    /// monitor with another DPI. Sizes are physical px, so content scales its layout here.
    fn dpi_changed(&mut self, _dpi: u32) {}
}

/// What a point in the window is: `p` in physical client px, `size` the window size in logical px.
/// The top [`CAPTION_H`] px drag the window (`HTCAPTION`) except the two [`BUTTON_W`]-wide boxes at
/// the right: minimize (`HTMINBUTTON`), then close (`HTCLOSE`). The rest is `HTCLIENT`.
pub fn caption_hit(p: PointF, size: (f32, f32), dpi: u32) -> u32 {
    let w = scale(size.0, dpi);
    let bw = scale(BUTTON_W, dpi);
    if p.y < 0.0 || p.y >= scale(CAPTION_H, dpi) || p.x < 0.0 || p.x >= w {
        HTCLIENT
    } else if p.x >= w - bw {
        HTCLOSE
    } else if p.x >= w - 2.0 * bw {
        HTMINBUTTON
    } else {
        HTCAPTION
    }
}

/// A `size` (physical px) rectangle centred in `work`.
pub fn centered(work: RECT, size: (i32, i32)) -> RECT {
    let left = work.left + (work.right - work.left - size.0) / 2;
    let top = work.top + (work.bottom - work.top - size.1) / 2;
    RECT { left, top, right: left + size.0, bottom: top + size.1 }
}

fn work_area() -> RECT {
    let mut work = RECT::default();
    let ok = unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut work as *mut RECT as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)) };
    if ok.is_err() || work.right <= work.left || work.bottom <= work.top { primary_monitor().0 } else { work }
}

/// A 0→1 fade with ease-out, retargetable mid-way.
#[derive(Clone, Copy, Default)]
struct Fade {
    from: f32,
    to: f32,
    start: u64,
}

impl Fade {
    fn value(&self, now: u64) -> f32 {
        let t = now.saturating_sub(self.start) as f32 / HOVER_MS;
        self.from + (self.to - self.from) * ease_out_progress(t)
    }

    fn running(&self, now: u64) -> bool {
        (now.saturating_sub(self.start) as f32) < HOVER_MS && self.from != self.to
    }

    fn set(&mut self, to: f32, now: u64) {
        if to != self.to {
            *self = Fade { from: self.value(now), to, start: now };
        }
    }
}

struct State<C> {
    gfx: Rc<Gfx>,
    hwnd: HWND,
    layer: Option<Layer>,
    backdrops: Option<Backdrops>,
    sent: Vec<Region>,
    content: C,
    /// Logical px.
    size: (f32, f32),
    /// Shared with [`AppWindow::dpi`], readable while the window is busy.
    dpi: Rc<Cell<u32>>,
    title: String,
    text: TextCache,
    title_layout: Option<TextLayout>,
    active: bool,
    glass: bool,
    waiting: bool,
    frames: Frames,
    mouse: Mouse,
    nc_tracking: bool,
    /// The caption button under the mouse (`HTMINBUTTON` / `HTCLOSE`), or 0.
    hover: u32,
    /// The caption button the left button went down on, or 0.
    pressed: u32,
    min_fade: Fade,
    close_fade: Fade,
}

pub struct AppWindow<C: AppContent + 'static> {
    state: Rc<RefCell<State<C>>>,
    hwnd: HWND,
    dpi: Rc<Cell<u32>>,
}

impl<C: AppContent + 'static> AppWindow<C> {
    /// Creates, shows and activates the window, `size_logical` big, centred on the primary
    /// monitor's work area. `title` is the caption text and the name on the bar and in Alt+Tab.
    pub fn create(gfx: &Rc<Gfx>, mut content: C, class: &'static str, title: &str, size_logical: (f32, f32)) -> Result<AppWindow<C>> {
        let started = std::time::Instant::now();
        let (_, dpi) = primary_monitor();
        let px = phys(size_logical, dpi);
        content.dpi_changed(dpi);
        let dpi_cell = Rc::new(Cell::new(dpi));
        let state = Rc::new(RefCell::new(State {
            gfx: gfx.clone(),
            hwnd: HWND::default(),
            layer: None,
            backdrops: None,
            sent: Vec::new(),
            content,
            size: size_logical,
            dpi: dpi_cell.clone(),
            title: title.to_owned(),
            text: TextCache::new(gfx),
            title_layout: None,
            active: true,
            glass: false,
            waiting: true,
            frames: Frames::default(),
            mouse: Mouse::default(),
            nc_tracking: false,
            hover: 0,
            pressed: 0,
            min_fade: Fade::default(),
            close_fade: Fade::default(),
        }));
        let hwnd = create_window(
            &WindowSpec {
                class,
                ex_style: WS_EX_APPWINDOW | WS_EX_NOREDIRECTIONBITMAP,
                style: WS_POPUP | WS_MINIMIZEBOX | WS_SYSMENU,
                rect: centered(work_area(), px),
                layered_alpha: None,
            },
            state.clone(),
        )?;
        // From here on the window must not leak if a later step fails.
        let win = AppWindow { state, hwnd, dpi: dpi_cell };
        unsafe { SetWindowTextW(hwnd, &HSTRING::from(title))? };
        {
            let mut s = win.state.borrow_mut();
            s.hwnd = hwnd;
            let mut layer = Layer::new(gfx, hwnd, Backdrop::Glass, px.0 as u32, px.1 as u32)?;
            layer.set_corner_radius(scale(RADIUS, dpi).round())?;
            s.layer = Some(layer);
            s.backdrops = Some(Backdrops::spawn(hwnd, GLASS_MSG));
            s.layout_title();
            s.paint();
            s.hide_until_glass();
        }
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
        }
        if let Ok(mut s) = win.state.try_borrow_mut() {
            s.sync_glass();
        }
        log::info!("app window first-frame-ms={:.1}", started.elapsed().as_secs_f64() * 1000.0);
        Ok(win)
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    /// The window's current DPI (it follows the monitor it is on).
    pub fn dpi(&self) -> u32 {
        self.dpi.get()
    }

    /// Changes the content, then repaints.
    pub fn update(&self, f: impl FnOnce(&mut C)) {
        let Ok(mut s) = self.state.try_borrow_mut() else { return };
        f(&mut s.content);
        s.paint();
    }

    /// Reads the content (`None` while the window is busy with it).
    pub fn read<R>(&self, f: impl FnOnce(&C) -> R) -> Option<R> {
        self.state.try_borrow().ok().map(|s| f(&s.content))
    }

    /// The glass regions last sent to the window's `Backdrops` (for tests).
    #[doc(hidden)]
    pub fn last_regions(&self) -> Vec<Region> {
        self.state.try_borrow().map(|s| s.sent.clone()).unwrap_or_default()
    }
}

impl<C: AppContent + 'static> Drop for AppWindow<C> {
    fn drop(&mut self) {
        unsafe {
            if IsWindow(Some(self.hwnd)).as_bool() {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

fn phys(size: (f32, f32), dpi: u32) -> (i32, i32) {
    (scale(size.0, dpi).round() as i32, scale(size.1, dpi).round() as i32)
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color { a: c.a * a, ..c }
}

impl<C: AppContent> State<C> {
    fn window_rect(&self) -> RECT {
        let mut r = RECT::default();
        unsafe {
            let _ = GetWindowRect(self.hwnd, &mut r);
        }
        r
    }

    /// Sends the window's glass region (none while minimized or hidden).
    fn sync_glass(&mut self) {
        let Some(backdrops) = self.backdrops.as_ref() else { return };
        let shown = unsafe { IsWindowVisible(self.hwnd).as_bool() && !IsIconic(self.hwnd).as_bool() };
        let regions = if shown {
            let r = self.window_rect();
            vec![Region {
                id: self.hwnd.0 as usize as u64,
                window: PxRect { left: r.left, top: r.top, right: r.right, bottom: r.bottom },
                radius: scale(RADIUS, self.dpi.get()).round(),
            }]
        } else {
            Vec::new()
        };
        // Re-sending the same set is cheap: the glass thread keeps its slots and posts nothing new.
        backdrops.set_regions(regions.clone());
        self.sent = regions;
    }

    fn on_glass(&mut self, wp: WPARAM, lp: LPARAM) {
        match wp.0 {
            FRAME => {
                // SAFETY: posted by our own `Backdrops` with wParam = FRAME, taken once here.
                let frame = unsafe { take_frame(lp) };
                if !self.sent.iter().any(|r| r.id == frame.id) {
                    return;
                }
                let r = self.window_rect();
                let Some(layer) = self.layer.as_mut() else { return };
                if let Err(e) = layer.set_backdrop(&frame, (r.left, r.top)) {
                    log::error!("app window backdrop failed: {e}");
                    return;
                }
                if !self.glass {
                    self.glass = true;
                    self.paint();
                }
                self.reveal();
            }
            UNAVAILABLE => {
                log::warn!("app window: screen capture unavailable, using the plain tint");
                if let Some(layer) = self.layer.as_mut() {
                    let _ = layer.clear_backdrop();
                }
                if self.glass {
                    self.glass = false;
                    self.paint();
                }
                self.reveal();
            }
            _ => {}
        }
    }

    fn layout_title(&mut self) {
        let style = TextStyle { family: FONT.into(), size: scale(TITLE_SIZE, self.dpi.get()), weight: theme::WEIGHT_SEMIBOLD };
        let max = scale(self.size.0 - 2.0 * BUTTON_W - TITLE_PAD_X, self.dpi.get());
        self.title_layout = self.text.layout(&self.title, &style, max).ok();
    }

    fn paint(&mut self) {
        let now = now_ms();
        let dpi = self.dpi.get();
        let (w, h) = (scale(self.size.0, dpi).round(), scale(self.size.1, dpi).round());
        let s = dpi as f32 / 96.0;
        let radius = (RADIUS * s).round();
        let glass = self.glass;
        let text_color = if self.active { theme::TEXT } else { theme::TEXT_INACTIVE };
        let (min_level, close_level) = (self.min_fade.value(now), self.close_fade.value(now));
        let pressed = self.pressed;
        let title = self.title_layout.as_ref();
        let content = &mut self.content;
        let Some(layer) = self.layer.as_mut() else { return };
        let res = layer.draw(None, |p| {
            p.clear();
            paint_glass(p, RectF::new(0.0, 0.0, w, h), radius, glass);
            content.paint(p, now);
            paint_caption(p, w, s, title, text_color, pressed, (min_level, close_level));
        });
        if let Err(e) = res.and_then(|_| self.gfx.commit()) {
            log::error!("app window paint failed: {e}");
        }
        self.keep_animating(now);
    }

    fn keep_animating(&mut self, now: u64) {
        let want = self.content.animating(now) || self.min_fade.running(now) || self.close_fade.running(now);
        let wake = self.content.wake_at(now);
        self.frames.schedule(self.hwnd, want, wake, now);
    }

    /// Clear until the first glass frame (at most 250 ms), so the solid fallback tint never
    /// flashes before the glass.
    fn hide_until_glass(&mut self) {
        if let Some(layer) = self.layer.as_mut()
            && let Err(e) = start_clear(&self.gfx, layer, self.hwnd, 0.0)
        {
            log::warn!("app window cannot start clear: {e}");
        }
    }

    fn reveal(&mut self) {
        if !self.waiting {
            return;
        }
        self.waiting = false;
        end_clear(self.hwnd);
        let Some(layer) = self.layer.as_mut() else { return };
        let r = layer.visual().and_then(|root| fade(&self.gfx, &root, 0.0, 1.0, theme::REVEAL_MS));
        if let Err(e) = r {
            log::warn!("app window reveal failed: {e}");
        }
    }

    fn set_hover(&mut self, hit: u32) {
        let hover = if hit == HTMINBUTTON || hit == HTCLOSE { hit } else { 0 };
        if hover == self.hover {
            return;
        }
        self.hover = hover;
        let now = now_ms();
        self.min_fade.set(if hover == HTMINBUTTON { 1.0 } else { 0.0 }, now);
        self.close_fade.set(if hover == HTCLOSE { 1.0 } else { 0.0 }, now);
        self.paint();
    }

    fn after(&mut self, reply: PanelReply) {
        if reply.close {
            unsafe {
                let _ = PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            return;
        }
        if reply.repaint || reply.resize {
            self.paint();
        }
    }

    fn feed(&mut self, ev: PanelInput) {
        let reply = self.content.input(ev, now_ms());
        self.after(reply);
    }

    /// Client mouse and keyboard messages for the content; `None` for anything else.
    fn input(&mut self, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
        let ev = self.mouse.translate(self.hwnd, msg, wp, lp)?;
        if let Some(ev) = ev {
            self.feed(ev);
        }
        Some(LRESULT(0))
    }

    fn hit(&self, lp: LPARAM) -> u32 {
        let r = self.window_rect();
        let p = point(lp);
        caption_hit(PointF { x: p.x - r.left as f32, y: p.y - r.top as f32 }, self.size, self.dpi.get())
    }

    /// Moves to `dpi`: the window takes `suggested` (WM_DPICHANGED's rect) or, without one, keeps
    /// its centre; the content hears of it before the repaint.
    fn on_dpi(&mut self, dpi: u32, suggested: Option<RECT>) {
        if dpi == self.dpi.get() || dpi == 0 {
            return;
        }
        self.dpi.set(dpi);
        let (w, h) = phys(self.size, dpi);
        let r = suggested.filter(|r| r.right > r.left && r.bottom > r.top).unwrap_or_else(|| {
            let r = self.window_rect();
            let (cx, cy) = ((r.left + r.right) / 2, (r.top + r.bottom) / 2);
            RECT { left: cx - w / 2, top: cy - h / 2, right: cx - w / 2 + w, bottom: cy - h / 2 + h }
        });
        // Inside the handler: the messages SetWindowPos sends are queued by the platform (answered
        // 0, which is what a frameless popup wants) and WM_WINDOWPOSCHANGED re-sends the glass.
        unsafe {
            let _ = SetWindowPos(self.hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
        }
        if let Some(layer) = self.layer.as_mut() {
            let _ = layer.resize((r.right - r.left) as u32, (r.bottom - r.top) as u32);
            let _ = layer.set_corner_radius(scale(RADIUS, dpi).round());
        }
        self.content.dpi_changed(dpi);
        self.layout_title();
        self.paint();
        self.sync_glass();
    }
}

/// Title at the left; "–" and "×" at the right with a hover highlight (`levels` 0..=1).
fn paint_caption(p: &mut Painter, w: f32, s: f32, title: Option<&TextLayout>, color: Color, pressed: u32, levels: (f32, f32)) {
    let cap_h = CAPTION_H * s;
    if let Some(t) = title {
        p.text(t, PointF { x: (TITLE_PAD_X * s).round(), y: ((cap_h - t.height()) / 2.0).round() }, color);
    }
    let bw = BUTTON_W * s;
    let stroke = theme::ICON_STROKE * s;
    let g = GLYPH * s / 2.0;
    for (code, x, level) in [(HTMINBUTTON, w - 2.0 * bw, levels.0), (HTCLOSE, w - bw, levels.1)] {
        let box_r = RectF::new(x, 0.0, bw, cap_h).inset(6.0 * s);
        let r = theme::POPUP_ITEM_RADIUS * s;
        if pressed == code {
            p.fill_round_rect(box_r, r, theme::ACTIVE_PILL);
        } else if level > 0.0 {
            p.fill_round_rect(box_r, r, with_alpha(theme::POPUP_HOVER, level));
        }
        let (cx, cy) = (x + bw / 2.0, cap_h / 2.0);
        if code == HTMINBUTTON {
            p.stroke_polyline(&[PointF { x: cx - g, y: cy }, PointF { x: cx + g, y: cy }], color, stroke);
        } else {
            p.stroke_polyline(&[PointF { x: cx - g, y: cy - g }, PointF { x: cx + g, y: cy + g }], color, stroke);
            p.stroke_polyline(&[PointF { x: cx + g, y: cy - g }, PointF { x: cx - g, y: cy + g }], color, stroke);
        }
    }
}

impl<C: AppContent> Handler for State<C> {
    fn on_message(&mut self, _hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
        match msg {
            WM_NCHITTEST => return Some(LRESULT(self.hit(lp) as isize)),
            WM_NCMOUSEMOVE => {
                if !self.nc_tracking {
                    let mut tme =
                        TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE | TME_NONCLIENT, hwndTrack: self.hwnd, dwHoverTime: 0 };
                    self.nc_tracking = unsafe { TrackMouseEvent(&mut tme) }.is_ok();
                }
                self.set_hover(wp.0 as u32);
                return None;
            }
            WM_NCMOUSELEAVE => {
                self.nc_tracking = false;
                self.pressed = 0;
                self.set_hover(0);
            }
            // Our own buttons: DefWindowProc would draw classic ones over them.
            WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK if wp.0 as u32 == HTMINBUTTON || wp.0 as u32 == HTCLOSE => {
                self.pressed = wp.0 as u32;
                self.paint();
            }
            WM_NCLBUTTONUP if wp.0 as u32 == HTMINBUTTON || wp.0 as u32 == HTCLOSE => {
                let hit = wp.0 as u32;
                let was = std::mem::take(&mut self.pressed);
                self.paint();
                if was == hit {
                    if hit == HTMINBUTTON {
                        // Posted: minimizing sends messages (WM_NCACTIVATE) that need this handler free.
                        unsafe {
                            let _ = PostMessageW(Some(self.hwnd), WM_SYSCOMMAND, WPARAM(SC_MINIMIZE as usize), LPARAM(0));
                        }
                    } else {
                        unsafe {
                            let _ = PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                        }
                    }
                }
            }
            WM_NCLBUTTONUP => {
                if self.pressed != 0 {
                    self.pressed = 0;
                    self.paint();
                }
                return None;
            }
            WM_MOUSEMOVE if self.hover != 0 => {
                // Off the caption buttons into the client area.
                self.set_hover(0);
                return self.input(msg, wp, lp);
            }
            WM_CONTEXTMENU if lp.0 != -1 && self.hit(lp) != HTCLIENT => return None, // the system menu
            WM_TIMER if wp.0 == REVEAL_TIMER => self.reveal(),
            WM_TIMER => {
                if !Frames::on_timer(self.hwnd, wp.0) {
                    return None;
                }
                self.paint();
            }
            m if m == GLASS_MSG => self.on_glass(wp, lp),
            WM_WINDOWPOSCHANGED => {
                self.sync_glass();
                return None;
            }
            WM_ACTIVATE => {
                let active = (wp.0 & 0xFFFF) as u32 != WA_INACTIVE;
                if active != self.active {
                    self.active = active;
                    self.paint();
                }
                self.sync_glass();
                return None;
            }
            WM_DPICHANGED => {
                // The platform zeroes lParam when it had to queue the message (the RECT is gone).
                let suggested = (lp.0 != 0).then(|| unsafe { *(lp.0 as *const RECT) });
                self.on_dpi((wp.0 & 0xFFFF) as u32, suggested);
            }
            WM_DISPLAYCHANGE => {
                if let Some(b) = self.backdrops.as_ref() {
                    b.refresh();
                }
                self.sync_glass();
                return None;
            }
            // DefWindowProc destroys the window, after this handler has returned: destroying it
            // from in here would free the handler slot while it is borrowed.
            WM_CLOSE => return None,
            WM_DESTROY => {
                if let Some(b) = self.backdrops.as_ref() {
                    b.set_regions(Vec::new());
                }
                self.sent.clear();
                unsafe { PostQuitMessage(0) };
            }
            _ => return self.input(msg, wp, lp),
        }
        Some(LRESULT(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centred_in_the_work_area() {
        let work = RECT { left: 0, top: 0, right: 1920, bottom: 1032 };
        assert_eq!(centered(work, (760, 560)), RECT { left: 580, top: 236, right: 1340, bottom: 796 });
        let right = RECT { left: 1920, top: 40, right: 3840, bottom: 1080 };
        assert_eq!(centered(right, (760, 560)).left, 2500);
    }

    #[test]
    fn hover_fade_eases_to_its_target() {
        let mut f = Fade::default();
        f.set(1.0, 1000);
        assert_eq!(f.value(1000), 0.0);
        assert!(f.running(1060));
        assert_eq!(f.value(1200), 1.0);
        assert!(!f.running(1200));
        f.set(0.0, 1200);
        assert!(f.value(1260) < 1.0);
    }
}
