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
