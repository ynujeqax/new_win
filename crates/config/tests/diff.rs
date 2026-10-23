use newwin_config::diff::{changes, Changes};
use newwin_config::{BarMode, Config};

fn only(f: impl FnOnce(&mut Config)) -> Changes {
    let mut new = Config::default();
    f(&mut new);
    changes(&Config::default(), &new)
}

#[test]
fn identical_configs_change_nothing() {
    assert!(!changes(&Config::default(), &Config::default()).any());
}

#[test]
fn version_is_ignored() {
    assert!(!only(|c| c.version = 7).any());
}

#[test]
fn mode_change_is_mode_and_visibility() {
    let mut new = Config::default();
    new.bar.mode = BarMode::Pinned;
    assert_eq!(changes(&Config::default(), &new), Changes { mode: true, visibility: true, ..Default::default() });
}

#[test]
fn delay_is_visibility_only() {
    assert_eq!(only(|c| c.bar.show_delay_ms = 300), Changes { visibility: true, ..Default::default() });
    assert_eq!(only(|c| c.bar.hide_delay_ms = 900), Changes { visibility: true, ..Default::default() });
    assert_eq!(only(|c| c.bar.trigger_height_px = 2), Changes { visibility: true, ..Default::default() });
    assert_eq!(only(|c| c.bar.ignore_fullscreen = false), Changes { visibility: true, ..Default::default() });
}

#[test]
fn height_is_geometry() {
    assert_eq!(only(|c| c.appearance.height = 40), Changes { geometry: true, ..Default::default() });
    assert_eq!(only(|c| c.appearance.margin = 2), Changes { geometry: true, ..Default::default() });
    assert_eq!(only(|c| c.appearance.corner_radius = 4.0), Changes { geometry: true, ..Default::default() });
}

#[test]
fn font_size_is_fonts() {
    assert_eq!(only(|c| c.appearance.font_size = 15.0), Changes { fonts: true, ..Default::default() });
    assert_eq!(only(|c| c.appearance.font_family = "Arial".into()), Changes { fonts: true, ..Default::default() });
}

#[test]
fn glass_is_glass() {
    assert_eq!(only(|c| c.appearance.glass = false), Changes { glass: true, ..Default::default() });
}

#[test]
fn seconds_is_clock() {
    assert_eq!(only(|c| c.appearance.show_seconds = true), Changes { clock: true, ..Default::default() });
    assert_eq!(only(|c| c.appearance.date_format = "%d.%m".into()), Changes { clock: true, ..Default::default() });
}

#[test]
fn hidden_item_is_items() {
    assert_eq!(only(|c| c.items.hidden = vec!["volume".into()]), Changes { items: true, ..Default::default() });
    assert_eq!(only(|c| c.items.right.reverse()), Changes { items: true, ..Default::default() });
}

#[test]
fn win_key_is_win_key() {
    assert_eq!(only(|c| c.spotlight.win_key = false), Changes { win_key: true, ..Default::default() });
}

#[test]
fn max_results_is_spotlight() {
    assert_eq!(only(|c| c.spotlight.max_results = 12), Changes { spotlight: true, ..Default::default() });
}

#[test]
fn tray_hidden_is_tray() {
    assert_eq!(only(|c| c.tray.hidden = vec!["a.exe".into()]), Changes { tray: true, ..Default::default() });
    assert_eq!(only(|c| c.tray.order = vec!["a.exe".into()]), Changes { tray: true, ..Default::default() });
}

#[test]
fn autostart_is_autostart() {
    assert_eq!(only(|c| c.system.start_with_windows = false), Changes { autostart: true, ..Default::default() });
}
