use newwin_platform::input_hook::InputHook;
use newwin_platform::keyboard_layout::current_layout;
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::WindowsAndMessaging::{WM_APP, WS_EX_TOOLWINDOW, WS_POPUP};

#[test]
fn current_layout_has_a_three_letter_code() {
    let l = current_layout().expect("a layout");
    eprintln!("{l:?}");
    assert_eq!(l.code.chars().count(), 3, "{l:?}");
    assert!(!l.name.is_empty());
}

#[test]
fn hook_starts_and_stops() {
    let notify = newwin_platform::window::create_test_window(WS_EX_TOOLWINDOW, WS_POPUP, RECT::default(), "kbd").unwrap();
    let start = std::time::Instant::now();
    let hook = InputHook::start(notify, WM_APP + 10).unwrap();
    drop(hook);
    assert!(start.elapsed() < std::time::Duration::from_secs(1), "the hook thread stops promptly");
}
