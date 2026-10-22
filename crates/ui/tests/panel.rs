use std::time::{Duration, Instant};

use newwin_ui::panel::{Panel, PanelContent, PanelInput, PanelReply};
use newwin_ui::popup::{CLOSED, PICKED};
use newwin_ui::{Gfx, Painter, PointF};
use windows::Win32::Foundation::{LPARAM, RECT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;

const MSG: u32 = WM_APP + 11;

struct Clicky;

impl PanelContent for Clicky {
    fn size(&self) -> (f32, f32) {
        (200.0, 100.0)
    }
    fn paint(&mut self, _: &mut Painter, _: u64) {}
    fn input(&mut self, ev: PanelInput, _: u64) -> PanelReply {
        match ev {
            PanelInput::Release(_) => PanelReply { action: Some(7), close: true, ..Default::default() },
            _ => PanelReply::default(),
        }
    }
    fn animating(&self, _: u64) -> bool {
        false
    }
}

#[test]
fn panel_posts_action_then_closed() {
    let gfx = Gfx::new().unwrap();
    let notify = newwin_platform::window::create_test_window(WS_EX_TOOLWINDOW, WS_POPUP, RECT::default(), "notify").unwrap();
    let panel = Panel::show(&gfx, Clicky, PointF { x: 300.0, y: 60.0 }, 96, notify, MSG).unwrap();
    unsafe { SendMessageW(panel.hwnd(), WM_LBUTTONUP, Some(WPARAM(0)), Some(LPARAM((20 << 16) | 20))) };
    let mut got = Vec::new();
    let end = Instant::now() + Duration::from_secs(2);
    let mut m = MSG::default();
    while Instant::now() < end && got.len() < 2 {
        unsafe {
            while PeekMessageW(&mut m, None, 0, 0, PM_REMOVE).as_bool() {
                if m.message == MSG {
                    got.push((m.wParam.0, m.lParam.0));
                } else {
                    DispatchMessageW(&m);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(got, vec![(7, PICKED), (panel.hwnd().0 as usize, CLOSED)]);
}

#[derive(Default)]
struct Seen {
    inputs: Vec<PanelInput>,
    paints: u32,
}

/// Records what reaches it; wakes once at `wake` (ms on the panel clock), then never.
struct Recorder {
    seen: std::rc::Rc<std::cell::RefCell<Seen>>,
    wake: Option<u64>,
}

impl PanelContent for Recorder {
    fn size(&self) -> (f32, f32) {
        (200.0, 100.0)
    }
    fn paint(&mut self, _: &mut Painter, _: u64) {
        self.seen.borrow_mut().paints += 1;
    }
    fn input(&mut self, ev: PanelInput, _: u64) -> PanelReply {
        self.seen.borrow_mut().inputs.push(ev);
        PanelReply::default()
    }
    fn animating(&self, _: u64) -> bool {
        false
    }
    fn wake_at(&self, now: u64) -> Option<u64> {
        self.wake.filter(|&w| w > now)
    }
}

fn pump(ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    let mut m = MSG::default();
    while Instant::now() < end {
        unsafe {
            while PeekMessageW(&mut m, None, 0, 0, PM_REMOVE).as_bool() {
                DispatchMessageW(&m);
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn recorder_panel(wake: Option<u64>) -> (Panel<Recorder>, std::rc::Rc<std::cell::RefCell<Seen>>) {
    let gfx = Gfx::new().unwrap();
    let notify = newwin_platform::window::create_test_window(WS_EX_TOOLWINDOW, WS_POPUP, RECT::default(), "notify-rec").unwrap();
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Seen::default()));
    let panel = Panel::show(&gfx, Recorder { seen: seen.clone(), wake }, PointF { x: 300.0, y: 60.0 }, 96, notify, MSG).unwrap();
    (panel, seen)
}

#[test]
fn ctrl_key_and_context_menu_reach_content() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardState, SetKeyboardState};
    let (panel, seen) = recorder_panel(None);
    let h = panel.hwnd();
    unsafe {
        SendMessageW(h, WM_CONTEXTMENU, Some(WPARAM(h.0 as usize)), Some(LPARAM(-1)));
        let mut r = RECT::default();
        GetWindowRect(h, &mut r).unwrap();
        let screen = ((r.top + 30) << 16) | ((r.left + 20) & 0xFFFF);
        SendMessageW(h, WM_CONTEXTMENU, Some(WPARAM(h.0 as usize)), Some(LPARAM(screen as isize)));
        let mut keys = [0u8; 256];
        GetKeyboardState(&mut keys).unwrap();
        let saved = keys;
        keys[0x11] = 0x80; // VK_CONTROL down, for this thread only
        SetKeyboardState(&keys).unwrap();
        SendMessageW(h, WM_KEYDOWN, Some(WPARAM(0x41)), Some(LPARAM(0)));
        SetKeyboardState(&saved).unwrap();
        SendMessageW(h, WM_KEYDOWN, Some(WPARAM(0x41)), Some(LPARAM(0)));
    }
    let got = &seen.borrow().inputs;
    assert_eq!(got[0], PanelInput::ContextMenu(None));
    assert_eq!(got[1], PanelInput::ContextMenu(Some(PointF { x: 20.0, y: 30.0 })));
    assert_eq!(got[2], PanelInput::CtrlKey(0x41));
    assert_eq!(got[3], PanelInput::Key(0x41));
}

#[test]
fn wake_at_repaints_once() {
    let (panel, seen) = recorder_panel(Some(newwin_platform::clock::now_ms() + 60));
    let first = seen.borrow().paints;
    pump(300);
    assert_eq!(seen.borrow().paints, first + 1, "one repaint at the wake time, none after");
    drop(panel);
}

#[test]
fn read_returns_content_state() {
    let (panel, _seen) = recorder_panel(Some(42));
    assert_eq!(panel.read(|c| c.wake), Some(Some(42)));
}

fn waiting_panel() -> Panel<Recorder> {
    let gfx = Gfx::new().unwrap();
    let notify = newwin_platform::window::create_test_window(WS_EX_TOOLWINDOW, WS_POPUP, RECT::default(), "notify-wait").unwrap();
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Seen::default()));
    Panel::show_after_glass(&gfx, Recorder { seen, wake: None }, PointF { x: 300.0, y: 60.0 }, 96, notify, MSG).unwrap()
}

/// Spotlight must never show its fallback tint: it is on screen (and has focus, so no typed
/// letter is lost) but clear until its glass arrives.
#[test]
fn a_glass_waiting_panel_stays_clear_until_its_glass() {
    let p = waiting_panel();
    assert!(unsafe { IsWindowVisible(p.hwnd()) }.as_bool(), "shown at once, to take the keyboard");
    assert!(!p.revealed());
    newwin_ui::GlassWindow::set_glass_unavailable(&p);
    assert!(p.revealed(), "no glass coming: show the tint");
}

#[test]
fn a_glass_waiting_panel_shows_anyway_after_a_while() {
    let p = waiting_panel();
    pump(400);
    assert!(p.revealed());
}
