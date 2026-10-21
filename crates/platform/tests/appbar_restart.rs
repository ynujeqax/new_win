use newwin_platform::appbar::AppBar;
use newwin_platform::window::create_test_window;
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::WindowsAndMessaging::{DestroyWindow, WINDOW_EX_STYLE, WM_APP, WS_POPUP};

/// AppBar registrations live in explorer and die with it; after TaskbarCreated the bar must
/// register again (explorer may or may not still know the old registration).
#[test]
fn an_appbar_can_register_again_after_explorer_restarts() {
    let hwnd = create_test_window(WINDOW_EX_STYLE(0), WS_POPUP, RECT { left: 0, top: 0, right: 10, bottom: 10 }, "appbar").unwrap();
    let mut bar = AppBar::register(hwnd, WM_APP + 30).unwrap();
    bar.reregister().unwrap();
    bar.reregister().unwrap();
    let mon = RECT { left: 0, top: 0, right: 800, bottom: 600 };
    assert_eq!(bar.set_top(mon, 0).top, 0);
    drop(bar);
    unsafe { DestroyWindow(hwnd).unwrap() };
}
