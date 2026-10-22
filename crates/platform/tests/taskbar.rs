use std::path::Path;

use newwin_platform::taskbar::{NativeTaskbar, explorer_tray, native_taskbar_visible, restore_from_state_file};
use newwin_platform::window::{WindowSpec, create_window};
use std::cell::RefCell;
use std::rc::Rc;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::{DestroyWindow, GetWindowThreadProcessId, WS_EX_TOOLWINDOW, WS_POPUP};

#[test]
fn restore_from_missing_state_file_is_noop() {
    assert!(!restore_from_state_file(Path::new("Z:/definitely/missing-taskbar-state.toml")).unwrap());
}

/// Touches the real taskbar; run with `--include-ignored`.
#[test]
#[ignore]
fn hide_then_restore_roundtrip() {
    let state = std::env::temp_dir().join(format!("newwin-taskbar-test-{}.toml", std::process::id()));
    let _ = std::fs::remove_file(&state);
    assert!(native_taskbar_visible());

    let tb = NativeTaskbar::hide(&state).unwrap();
    assert!(!native_taskbar_visible());
    assert!(state.exists());

    tb.restore().unwrap();
    assert!(native_taskbar_visible());
    assert!(!state.exists());
}

/// A hide that was never restored (crash) is healed by the state file.
#[test]
#[ignore]
fn state_file_heals_an_unrestored_hide() {
    let state = std::env::temp_dir().join(format!("newwin-taskbar-heal-{}.toml", std::process::id()));
    let _ = std::fs::remove_file(&state);

    let tb = NativeTaskbar::hide(&state).unwrap();
    std::mem::forget(tb);
    assert!(!native_taskbar_visible());

    assert!(restore_from_state_file(&state).unwrap());
    assert!(native_taskbar_visible());
    assert!(!state.exists());
}

struct Noop;

impl newwin_platform::window::Handler for Noop {
    fn on_message(&mut self, _: HWND, _: u32, _: WPARAM, _: LPARAM) -> Option<LRESULT> {
        None
    }
}

/// Our own tray window (M2) has the same class as explorer's; the native taskbar is explorer's.
#[test]
fn explorer_tray_skips_windows_of_this_process() {
    let ours = create_window(
        &WindowSpec { class: "Shell_TrayWnd", ex_style: WS_EX_TOOLWINDOW, style: WS_POPUP, rect: RECT::default(), layered_alpha: None },
        Rc::new(RefCell::new(Noop)),
    )
    .unwrap();
    let found = explorer_tray().expect("explorer's taskbar exists");
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(found, Some(&mut pid)) };
    unsafe {
        let _ = DestroyWindow(ours);
    }
    assert_ne!(found, ours);
    assert_ne!(pid, unsafe { GetCurrentProcessId() });
}
