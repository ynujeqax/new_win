use newwin_ui::Gfx;
use newwin_ui::PointF;
use newwin_ui::popup::{MenuItem, Popup, PopupStyle};
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::WindowsAndMessaging::*;

fn pump(ms: u64) {
    let end = std::time::Instant::now() + std::time::Duration::from_millis(ms);
    let mut m = MSG::default();
    while std::time::Instant::now() < end {
        unsafe {
            while PeekMessageW(&mut m, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&m);
                DispatchMessageW(&m);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn waiting_popup() -> Popup {
    let gfx = Gfx::new().unwrap();
    let notify = newwin_platform::window::create_test_window(WS_EX_TOOLWINDOW, WS_POPUP, RECT::default(), "notify-popup").unwrap();
    let style = PopupStyle { font_family: "Segoe UI Variable Text".into(), font_size_px: 13.0, dpi: 96 };
    let items = vec![MenuItem::new(1, "Task Manager"), MenuItem::new(2, "Settings")];
    Popup::show_after_glass(&gfx, items, PointF { x: 200.0, y: 40.0 }, &style, notify, WM_APP + 30).unwrap()
}

/// The bar's menu must never flash its dark fallback before the glass: on screen (it has the
/// keyboard) but clear until the first glass frame.
#[test]
fn a_glass_waiting_menu_stays_clear_until_its_glass() {
    let p = waiting_popup();
    assert!(unsafe { IsWindowVisible(p.hwnd()) }.as_bool(), "shown at once, to take the keyboard");
    assert!(!p.revealed());
    p.set_glass_unavailable();
    assert!(p.revealed(), "no glass coming: show the fallback");
}

#[test]
fn a_glass_waiting_menu_shows_anyway_after_a_while() {
    let p = waiting_popup();
    pump(400);
    assert!(p.revealed());
}

#[test]
fn a_plain_menu_is_revealed_at_once() {
    let gfx = Gfx::new().unwrap();
    let notify = newwin_platform::window::create_test_window(WS_EX_TOOLWINDOW, WS_POPUP, RECT::default(), "notify-popup2").unwrap();
    let style = PopupStyle { font_family: "Segoe UI Variable Text".into(), font_size_px: 13.0, dpi: 96 };
    let p = Popup::show(&gfx, vec![MenuItem::new(1, "Run")], PointF { x: 200.0, y: 40.0 }, &style, notify, WM_APP + 30).unwrap();
    assert!(p.revealed());
}
