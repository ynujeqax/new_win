use newwin_core::apps::GroupKey;
use newwin_platform::activate::{close, minimize, new_instance_target, restore};
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

#[test]
fn unregistered_app_id_falls_back_to_the_exe() {
    // Chromium browsers tag windows with a per-profile id that no Start menu entry has.
    let key = GroupKey::AppId("Helium.ABC.UserData.Profile1".into());
    let exe = Some(r"C:\Helium\chrome.exe");
    assert_eq!(new_instance_target(&key, exe, |_| false).as_deref(), Some(r"C:\Helium\chrome.exe"));
    assert_eq!(new_instance_target(&key, exe, |_| true).as_deref(), Some(r"shell:AppsFolder\Helium.ABC.UserData.Profile1"));
    assert_eq!(new_instance_target(&key, None, |_| false), None);
    let key = GroupKey::ExePath(r"c:\a\b.exe".into());
    assert_eq!(new_instance_target(&key, None, |_| false).as_deref(), Some(r"c:\a\b.exe"));
}
