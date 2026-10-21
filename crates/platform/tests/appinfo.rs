use newwin_core::apps::GroupKey;
use newwin_platform::appinfo::*;
use newwin_platform::windows_watch::{AppNames, describe};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{DestroyWindow, WINDOW_EX_STYLE, WS_OVERLAPPEDWINDOW, WS_VISIBLE};

#[test]
fn file_description_reads_version_resource() {
    assert_eq!(file_description(r"C:\Windows\System32\notepad.exe").as_deref(), Some("Notepad"));
    assert_eq!(file_description(r"C:\definitely\missing.exe"), None);
}

#[test]
fn own_process_image_path() {
    let path = process_image_path(std::process::id()).unwrap().to_lowercase();
    assert!(path.ends_with(".exe"), "{path}");
    assert!(path.contains("appinfo"), "{path}");
}

#[test]
fn appsfolder_names_come_from_the_shell() {
    let _com = ComGuard::init();
    assert_eq!(appsfolder_display_name("Microsoft.Windows.Explorer").as_deref(), Some("File Explorer"));
    assert_eq!(appsfolder_display_name("No.Such.App!Nope"), None);
}

#[test]
fn placeholder_names() {
    // Exe keys are lowercased; the original-case path, when known, gives the nicer stem.
    assert_eq!(placeholder_name(&GroupKey::exe(r"C:\Apps\Foo Bar.exe"), None), "foo bar");
    assert_eq!(placeholder_name(&GroupKey::exe(r"C:\Apps\Foo Bar.exe"), Some(r"C:\Apps\Foo Bar.exe")), "Foo Bar");
    assert_eq!(placeholder_name(&GroupKey::AppId("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App".into()), None), "App");
    assert_eq!(
        placeholder_name(&GroupKey::AppId("Chrome".into()), Some(r"C:\Program Files\Google\Chrome\chrome.exe")),
        "chrome"
    );
}

#[test]
fn resolve_name_prefers_shell_then_file_description_then_stem() {
    let _com = ComGuard::init();
    assert_eq!(resolve_name(&GroupKey::AppId("Microsoft.Windows.Explorer".into()), None), "File Explorer");
    assert_eq!(resolve_name(&GroupKey::exe(r"C:\Windows\System32\notepad.exe"), None), "Notepad");
    assert_eq!(resolve_name(&GroupKey::AppId("Unknown!X".into()), Some(r"C:\Windows\System32\notepad.exe")), "Notepad");
    assert_eq!(resolve_name(&GroupKey::exe(r"C:\nowhere\tool.exe"), None), "tool");
}

#[test]
fn explorer_gets_its_implicit_app_id() {
    assert_eq!(implicit_app_id(r"C:\Windows\explorer.exe"), Some("Microsoft.Windows.Explorer"));
    assert_eq!(implicit_app_id(r"c:\windows\EXPLORER.EXE"), Some("Microsoft.Windows.Explorer"));
    assert_eq!(implicit_app_id(r"C:\Tools\explorer.exe"), None);
    assert_eq!(implicit_app_id(r"C:\Windows\System32\notepad.exe"), None);
}

#[test]
fn exe_paths_map_to_start_menu_items() {
    let _com = ComGuard::init();
    assert_eq!(
        known_folder_relative(r"C:\Windows\System32\notepad.exe").as_deref(),
        Some(r"{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\notepad.exe")
    );
    assert_eq!(known_folder_relative(r"D:\portable\tool.exe"), None);
}

#[test]
fn describe_own_window() {
    let hwnd = newwin_platform::window::create_test_window(
        WINDOW_EX_STYLE(0),
        WS_OVERLAPPEDWINDOW | WS_VISIBLE,
        RECT { left: -32000, top: -32000, right: -31800, bottom: -31900 },
        "NewWin describe test",
    )
    .unwrap();
    let mut names = AppNames::without_worker();
    let w = describe(hwnd.0 as u64, &mut names).unwrap();
    assert!(w.own_process);
    assert!(w.visible);
    assert!(!w.has_owner && !w.tool_window);
    assert_eq!(w.title, "NewWin describe test");
    assert!(matches!(w.group_key, GroupKey::ExePath(_)));
    unsafe { DestroyWindow(hwnd).unwrap() };
    assert!(describe(hwnd.0 as u64, &mut names).is_none());
    let _ = HWND::default();
}
