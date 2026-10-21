use newwin_platform::activate::{close, minimize, restore};
use newwin_platform::window::create_test_window;
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::WindowsAndMessaging::*;

fn pump() {
    let mut msg = MSG::default();
    for _ in 0..50 {
        unsafe {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn minimize_restore_close_a_window() {
    let w = create_test_window(
        WINDOW_EX_STYLE(0),
        WS_OVERLAPPEDWINDOW | WS_VISIBLE,
        RECT { left: 100, top: 100, right: 400, bottom: 300 },
        "NewWin activate test",
    )
    .unwrap();
    let h = w.0 as usize as u64;
    pump();
    minimize(h);
    pump();
    assert!(unsafe { IsIconic(w).as_bool() });
    restore(h);
    pump();
    assert!(!unsafe { IsIconic(w).as_bool() });
    close(h);
    pump();
    assert!(!unsafe { IsWindow(Some(w)).as_bool() });
}
