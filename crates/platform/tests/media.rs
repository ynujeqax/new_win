use std::time::{Duration, Instant};

use newwin_platform::media::{MediaWatch, take_media};
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::WindowsAndMessaging::*;

const WM_MEDIA: u32 = WM_APP + 77;

/// Reads what plays on this machine: the watch reports once at start, playing or not.
#[test]
fn the_watch_reports_the_current_session() {
    let hwnd = newwin_platform::window::create_test_window(WS_EX_TOOLWINDOW, WS_POPUP, RECT::default(), "media").unwrap();
    let watch = MediaWatch::start(hwnd, WM_MEDIA);
    let start = Instant::now();
    let mut got = None;
    while got.is_none() && start.elapsed() < Duration::from_secs(5) {
        let mut msg = MSG::default();
        unsafe {
            if PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_MEDIA {
                    got = Some(*take_media(msg.lParam));
                } else {
                    DispatchMessageW(&msg);
                }
            } else {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
    drop(watch);
    let info = got.expect("a first report within 5 s");
    eprintln!("now playing: {info:?}");
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}
