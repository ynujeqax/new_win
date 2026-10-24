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
    assert_eq!(c.appearance.height, 36);
    assert_eq!(c.appearance.margin, 6);
    assert_eq!(c.appearance.corner_radius, 18.0);
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
    assert!(matches!(parse("[bar\nmode="), Err(ConfigError::Parse { .. })));
}

#[test]
fn wrong_type_is_error() {
    assert!(matches!(parse("[bar]\nshow_delay_ms = \"slow\"\n"), Err(ConfigError::Parse { .. })));
}

#[test]
fn sanitize_clamps() {
    let c = parse("[bar]\ntrigger_height_px = 9\nshow_delay_ms = 99999\n[appearance]\nheight = 3\n").unwrap();
    assert_eq!(c.bar.trigger_height_px, 2);
    assert_eq!(c.bar.show_delay_ms, 1000);
    assert_eq!(c.appearance.height, 28);
}

#[test]
fn out_of_type_range_integers_are_clamped_not_rejected() {
    let c = parse("[bar]\ntrigger_height_px = 300\nshow_delay_ms = -5\n[appearance]\nheight = -1\nmargin = 99999999999\n").unwrap();
    assert_eq!(c.bar.trigger_height_px, 2);
    assert_eq!(c.bar.show_delay_ms, 0);
    assert_eq!(c.appearance.height, 28);
    assert_eq!(c.appearance.margin, 16);
}

#[test]
fn sanitize_fixes_nan_and_negative_floats() {
    let c = parse("[appearance]\ncorner_radius = -5.0\nfont_size = inf\n").unwrap();
    assert_eq!(c.appearance.corner_radius, 0.0);
    assert_eq!(c.appearance.font_size, 18.0);
    // A capsule needs half the tallest bar (48 px).
    assert_eq!(parse("[appearance]\ncorner_radius = 99.0\n").unwrap().appearance.corner_radius, 24.0);
}

#[test]
fn glass_flag_defaults_on_and_old_opacity_is_ignored() {
    assert!(Appearance::default().glass);
    assert!(!parse("[appearance]\nglass = false\n").unwrap().appearance.glass);
    let old = parse("[appearance]\nbackground_opacity = 0.2\nheight = 32\n").unwrap();
    assert!(old.appearance.glass);
    assert_eq!(old.appearance.height, 32);
}

#[test]
fn write_default_if_missing_creates_once() {
    let dir = std::env::temp_dir().join(format!("newwin-cfg-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("sub").join("config.toml");
    newwin_config::write_default_if_missing(&path).unwrap();
    assert_eq!(newwin_config::load(&path).unwrap(), newwin_config::Config::default());
    std::fs::write(&path, "[bar]\nmode = \"pinned\"\n").unwrap();
    newwin_config::write_default_if_missing(&path).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "[bar]\nmode = \"pinned\"\n", "an existing file is left alone");
    let _ = std::fs::remove_dir_all(&dir);
}

fn tempdir_in_target(name: &str) -> std::path::PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-tmp").join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn out_of_range_values_are_clamped_to_the_m4_bounds() {
    let c: Config = toml::from_str("[bar]\nshow_delay_ms = 5000\n[appearance]\nheight = 20\n[spotlight]\nmax_results = 99").unwrap();
    let c = c.sanitized();
    assert_eq!((c.bar.show_delay_ms, c.appearance.height, c.spotlight.max_results), (1000, 28, 15));
    let c: Config = toml::from_str("[spotlight]\nmax_results = 0").unwrap();
    assert_eq!(c.sanitized().spotlight.max_results, 3);
}

#[test]
fn item_lists_are_normalized() {
    let c: Config = toml::from_str("[items]\nright = [\"time\", \"time\", \"clock\", \"date\"]\nhidden = [\"clock\", \"date\"]").unwrap();
    let c = c.sanitized();
    assert_eq!(c.items.right, ["time", "date", "layout", "network", "volume", "search"]);
    assert_eq!(c.items.hidden, ["date"]);
}

#[test]
fn tray_lists_drop_duplicates_ignoring_case() {
    let c: Config = toml::from_str("[tray]\norder = [\"Discord.exe\", \"discord.EXE\", \"\"]\nhidden = [\"A.exe\"]").unwrap();
    assert_eq!(c.sanitized().tray.order, ["Discord.exe"]);
}

#[test]
fn round_trip_keeps_every_field() {
    let mut c = Config::default();
    c.items.hidden = vec!["search".into()];
    c.tray.hidden = vec!["MSI Afterburner.exe".into()];
    let text = to_toml(&c);
    assert!(text.starts_with("# NewWin settings. NewWinSettings.exe rewrites this file.\n"));
    assert_eq!(toml::from_str::<Config>(&text).unwrap().sanitized(), c);
}

#[test]
fn a_parse_error_names_its_line() {
    let dir = tempdir_in_target("parse_line");
    let p = dir.join("config.toml");
    std::fs::write(&p, "version = 1\n[bar]\nmode = \n").unwrap();
    match load(&p) { Err(ConfigError::Parse { line, .. }) => assert_eq!(line, Some(3)), r => panic!("{r:?}") }
}

#[test]
fn a_file_that_is_not_utf8_is_a_parse_error_without_a_line() {
    let dir = tempdir_in_target("not_utf8");
    let p = dir.join("config.toml");
    std::fs::write(&p, b"[bar]\nmode = \"\xff\xfe\"\n").unwrap();
    match load(&p) { Err(ConfigError::Parse { line, .. }) => assert_eq!(line, None), r => panic!("{r:?}") }
}

#[test]
fn save_atomic_replaces_the_file_and_leaves_no_temp() {
    let dir = tempdir_in_target("save_atomic");
    let p = dir.join("config.toml");
    save_atomic(&p, "a").unwrap();
    save_atomic(&p, "b").unwrap();
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "b");
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
}
