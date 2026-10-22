use newwin_core::apps::*;

fn key(s: &str) -> GroupKey {
    GroupKey::AppId(s.into())
}

fn win(hwnd: Hwnd, k: &str, name: &str) -> WindowInfo {
    WindowInfo {
        hwnd,
        has_owner: false,
        visible: true,
        cloaked: false,
        tool_window: false,
        app_window: false,
        no_activate: false,
        shell_ui: false,
        own_process: false,
        minimized: false,
        title: format!("window {hwnd}"),
        group_key: key(k),
        app_name: name.into(),
    }
}

#[test]
fn filter_rules() {
    assert!(is_taskbar_window(&win(1, "a", "A")));
    assert!(!is_taskbar_window(&WindowInfo { visible: false, ..win(1, "a", "A") }));
    assert!(!is_taskbar_window(&WindowInfo { cloaked: true, ..win(1, "a", "A") }));
    assert!(!is_taskbar_window(&WindowInfo { own_process: true, ..win(1, "a", "A") }));
    assert!(!is_taskbar_window(&WindowInfo { tool_window: true, ..win(1, "a", "A") }));
    assert!(!is_taskbar_window(&WindowInfo { has_owner: true, ..win(1, "a", "A") }));
    assert!(is_taskbar_window(&WindowInfo { has_owner: true, app_window: true, ..win(1, "a", "A") }));
    assert!(is_taskbar_window(&WindowInfo { tool_window: true, app_window: true, ..win(1, "a", "A") }));
}

#[test]
fn system_ui_windows_are_not_apps() {
    // Screen Snipping, Search, notification center: bare top-level CoreWindows.
    assert!(!is_taskbar_window(&WindowInfo { shell_ui: true, ..win(1, "a", "A") }));
    // Explorer's PopupHost and other never-activated helper windows.
    assert!(!is_taskbar_window(&WindowInfo { no_activate: true, ..win(1, "a", "A") }));
    assert!(is_taskbar_window(&WindowInfo { no_activate: true, app_window: true, ..win(1, "a", "A") }));
}

#[test]
fn groups_by_key_in_first_seen_order() {
    let mut m = AppModel::default();
    m.upsert(win(1, "chrome", "Chrome"));
    m.upsert(win(2, "code", "VS Code"));
    m.upsert(win(3, "chrome", "Chrome"));
    let g = m.groups();
    assert_eq!(g.len(), 2);
    assert_eq!((g[0].name.as_str(), g[0].windows.clone()), ("Chrome", vec![1, 3]));
    assert_eq!(g[1].name, "VS Code");
}

#[test]
fn exe_key_is_case_insensitive() {
    assert_eq!(GroupKey::exe(r"C:\A\B.EXE"), GroupKey::exe(r"c:\a\b.exe"));
}

#[test]
fn removing_last_window_drops_group_and_upsert_of_filtered_window_removes_it() {
    let mut m = AppModel::default();
    m.upsert(win(1, "a", "A"));
    m.upsert(win(2, "b", "B"));
    assert!(m.remove(1));
    assert_eq!(m.groups().len(), 1);
    assert!(m.upsert(WindowInfo { cloaked: true, ..win(2, "b", "B") }));
    assert!(m.groups().is_empty());
    assert!(!m.remove(99));
}

#[test]
fn active_group_tracks_active_window() {
    let mut m = AppModel::default();
    m.upsert(win(1, "a", "A"));
    m.upsert(win(2, "b", "B"));
    m.set_active(Some(2));
    assert_eq!(m.active_group(), Some(1));
    m.set_active(Some(77));
    assert_eq!(m.active_group(), None);
}

#[test]
fn rename_group() {
    let mut m = AppModel::default();
    m.upsert(win(1, "a", "a.exe"));
    assert!(m.set_group_name(&key("a"), "Alpha".into()));
    assert_eq!(m.groups()[0].name, "Alpha");
}

#[test]
fn left_click_rules() {
    let mut m = AppModel::default();
    m.upsert(win(1, "a", "A"));
    assert_eq!(left_click(&m, 0), ClickAction::Activate(1));
    m.set_active(Some(1));
    assert_eq!(left_click(&m, 0), ClickAction::Minimize(1));
    m.upsert(WindowInfo { minimized: true, ..win(1, "a", "A") });
    assert_eq!(left_click(&m, 0), ClickAction::Restore(1));
    m.upsert(win(2, "a", "A"));
    assert_eq!(left_click(&m, 0), ClickAction::ShowWindowList(0));
}

#[test]
fn middle_click_rules() {
    let mut m = AppModel::default();
    m.upsert(win(1, "a", "A"));
    assert_eq!(middle_click(&m, 0), ClickAction::Close(1));
    m.upsert(win(2, "a", "A"));
    assert_eq!(middle_click(&m, 0), ClickAction::ShowWindowList(0));
    assert_eq!(middle_click(&m, 5), ClickAction::Nothing);
}

#[test]
fn window_moving_to_another_group_leaves_the_old_one() {
    let mut m = AppModel::default();
    m.upsert(win(1, "a", "A"));
    m.upsert(win(1, "b", "B"));
    assert_eq!(m.groups().len(), 1);
    assert_eq!(m.groups()[0].key, key("b"));
}

const SNIP: &str = "MicrosoftWindows.Client.Core_cw5n1h2txyewy!ScreenClipping";

#[test]
fn screen_snipping_windows_are_recognized_even_while_cloaked() {
    // The overlay is created cloaked and shown ~200 ms later; the screenshot is taken in between.
    let mut w = win(1, SNIP, "Screen Snipping");
    w.cloaked = true;
    w.shell_ui = true;
    assert!(is_screen_snip(&w));
    assert!(!is_screen_snip(&win(2, "Claude_pzs8sxrjxfjjc!Claude", "Claude")));
}

#[test]
fn snip_tracker_reports_start_and_end_once() {
    let mut t = SnipTracker::default();
    assert_eq!(t.update(1, true), Some(true));
    assert_eq!(t.update(2, true), None); // its IME helper window
    assert_eq!(t.update(1, true), None);
    assert_eq!(t.update(9, false), None); // unrelated window
    assert_eq!(t.update(1, false), None);
    assert_eq!(t.update(2, false), Some(false));
    assert!(!t.active());
}
