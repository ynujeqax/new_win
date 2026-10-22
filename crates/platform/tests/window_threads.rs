use newwin_platform::window::create_test_window;
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::WindowsAndMessaging::{DestroyWindow, WINDOW_EX_STYLE, WS_POPUP};

/// Window classes are per process, but our registry of them was per thread: a second thread
/// failed with "Class already exists".
#[test]
fn windows_can_be_created_from_several_threads() {
    let make = || {
        let h = create_test_window(WINDOW_EX_STYLE(0), WS_POPUP, RECT { left: 0, top: 0, right: 10, bottom: 10 }, "threads").unwrap();
        unsafe { DestroyWindow(h).unwrap() };
    };
    make();
    std::thread::spawn(make).join().unwrap();
}
