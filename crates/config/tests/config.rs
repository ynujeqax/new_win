use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

use newwin_config::*;

fn parse(s: &str) -> Result<Config, ConfigError> {
    static N: AtomicU32 = AtomicU32::new(0);
    let path = std::env::temp_dir().join(format!(
        "newwin-config-test-{}-{}.toml",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, s).unwrap();
    let r = load(&path);
    std::fs::remove_file(&path).ok();
    r
}

#[test]
fn missing_file_gives_defaults() {
    let c = load(Path::new("Z:/definitely/missing.toml")).unwrap();
    assert_eq!(c.bar.mode, BarMode::AutoHide);
    assert_eq!(c.bar.show_delay_ms, 150);
    assert_eq!(c.bar.hide_delay_ms, 400);
    assert_eq!(c.bar.trigger_height_px, 1);
    assert!(c.bar.ignore_fullscreen);
    assert_eq!(c.appearance.height, 30);
    assert_eq!(c.appearance.margin, 6);
    assert_eq!(c.appearance.corner_radius, 11.0);
    assert_eq!(c.appearance.background_opacity, 0.065);
    assert_eq!(c.appearance.font_family, "Segoe UI Variable Text");
    assert_eq!(c.appearance.font_size, 13.0);
    assert_eq!(c.appearance.date_format, "system");
    assert!(!c.appearance.show_seconds);
    assert_eq!(c.items.right, ["layout", "network", "volume", "search", "date", "time"]);
    assert!(c.spotlight.win_key);
    assert_eq!(c.spotlight.max_results, 8);
    assert!(c.system.start_with_windows);
    assert_eq!(c.version, 1);
}

#[test]
fn partial_file_keeps_other_defaults_and_ignores_unknown() {
    let c = parse("[bar]\nmode = \"pinned\"\nfuture_key = 1\n").unwrap();
    assert_eq!(c.bar.mode, BarMode::Pinned);
    assert_eq!(c.bar.hide_delay_ms, 400);
}

#[test]
fn invalid_toml_is_error() {
    assert!(matches!(parse("[bar\nmode="), Err(ConfigError::Parse(_))));
}

#[test]
fn wrong_type_is_error() {
    assert!(matches!(parse("[bar]\nshow_delay_ms = \"slow\"\n"), Err(ConfigError::Parse(_))));
}

#[test]
fn sanitize_clamps() {
    let c = parse("[bar]\ntrigger_height_px = 9\nshow_delay_ms = 99999\n[appearance]\nheight = 3\n").unwrap();
    assert_eq!(c.bar.trigger_height_px, 2);
    assert_eq!(c.bar.show_delay_ms, 2000);
    assert_eq!(c.appearance.height, 24);
}

#[test]
fn sanitize_fixes_nan_and_negative_floats() {
    let c = parse("[appearance]\nbackground_opacity = nan\ncorner_radius = -5.0\nfont_size = inf\n").unwrap();
    assert_eq!(c.appearance.background_opacity, 0.065);
    assert_eq!(c.appearance.corner_radius, 0.0);
    assert_eq!(c.appearance.font_size, 18.0);
}
