use newwin_platform::input_hook::{InputHook, send_tagged};
use newwin_platform::keyboard_layout::current_layout;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::Input::KeyboardAndMouse::{INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VK_LWIN};
use windows::Win32::UI::WindowsAndMessaging::{MSG, PM_REMOVE, PeekMessageW, WM_APP, WS_EX_TOOLWINDOW, WS_POPUP};

/// The hook keeps its target in process-wide statics: one hook per test at a time.
static ONE_HOOK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn current_layout_has_a_three_letter_code() {
    let l = current_layout().expect("a layout");
    eprintln!("{l:?}");
    assert_eq!(l.code.chars().count(), 3, "{l:?}");
    assert!(!l.name.is_empty());
}

#[test]
fn hook_starts_and_stops() {
    let _one = ONE_HOOK.lock();
    let notify = newwin_platform::window::create_test_window(WS_EX_TOOLWINDOW, WS_POPUP, RECT::default(), "kbd").unwrap();
    let start = std::time::Instant::now();
    let hook = InputHook::start(notify, WM_APP + 10, WM_APP + 14, true).unwrap();
    drop(hook);
    assert!(start.elapsed() < std::time::Duration::from_secs(1), "the hook thread stops promptly");
}

/// Spotlight messages that reach `notify` within `ms`.
fn spotlight_posts(notify: HWND, ms: u64) -> usize {
    let end = std::time::Instant::now() + std::time::Duration::from_millis(ms);
    let mut n = 0;
    let mut m = MSG::default();
    while std::time::Instant::now() < end {
        while unsafe { PeekMessageW(&mut m, Some(notify), WM_APP + 14, WM_APP + 14, PM_REMOVE) }.as_bool() {
            n += 1;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    n
}

/// One test, because both halves send real Win keys and must not overlap. Start stays closed:
/// the untagged tap is masked by the hook, and the tagged press carries its own mask.
#[test]
fn win_taps_post_spotlight_but_tagged_ones_do_not() {
    let _one = ONE_HOOK.lock();
    let notify = newwin_platform::window::create_test_window(WS_EX_TOOLWINDOW, WS_POPUP, RECT::default(), "kbd-tap").unwrap();
    let hook = InputHook::start(notify, WM_APP + 10, WM_APP + 14, true).unwrap();
    let key = |up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VK_LWIN, dwFlags: if up { KEYEVENTF_KEYUP } else { Default::default() }, ..Default::default() } },
    };
    // Someone else's injected Win tap (AutoHotkey, remote tools) counts like a real one.
    unsafe { SendInput(&[key(false), key(true)], size_of::<INPUT>() as i32) };
    assert_eq!(spotlight_posts(notify, 500), 1);
    // Our own tagged keys never do: a masked Win press, as the hook itself replays it.
    send_tagged(&[(0x5B, false), (0xFC, false), (0xFC, true), (0x5B, true)]);
    assert_eq!(spotlight_posts(notify, 500), 0);
    drop(hook);
}
