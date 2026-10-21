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
    let target = |key, exe, reg: bool| new_instance_target(key, exe, None, |_| reg);
    assert_eq!(target(&key, exe, false), Some((r"C:\Helium\chrome.exe".into(), String::new())));
    assert_eq!(target(&key, exe, true), Some((r"shell:AppsFolder\Helium.ABC.UserData.Profile1".into(), String::new())));
    assert_eq!(target(&key, None, false), None);
    let key = GroupKey::ExePath(r"c:\a\b.exe".into());
    assert_eq!(target(&key, None, false), Some((r"c:\a\b.exe".into(), String::new())));
}

#[test]
fn unregistered_app_id_prefers_the_windows_relaunch_command() {
    // Steam windows belong to steamwebhelper.exe; the window says to relaunch Steam.exe.
    let key = GroupKey::AppId("Valve.Steam.Client".into());
    let helper = Some(r"C:\Steam\bin\cef\steamwebhelper.exe");
    let target = |relaunch, reg: bool| new_instance_target(&key, helper, relaunch, |_| reg);
    let pair = |f: &str, a: &str| Some((f.to_string(), a.to_string()));
    assert_eq!(target(Some(r"C:\Programs\Steam\Steam.exe"), false), pair(r"C:\Programs\Steam\Steam.exe", ""));
    assert_eq!(target(Some(r#""C:\Program Files\X\x.exe" --new-window"#), false), pair(r"C:\Program Files\X\x.exe", "--new-window"));
    assert_eq!(target(Some(r"C:\x.exe -a b"), false), pair(r"C:\x.exe", "-a b"));
    assert_eq!(target(Some("   "), false), pair(r"C:\Steam\bin\cef\steamwebhelper.exe", ""));
    assert_eq!(target(Some(r"C:\Programs\Steam\Steam.exe"), true), pair(r"shell:AppsFolder\Valve.Steam.Client", ""));
}
