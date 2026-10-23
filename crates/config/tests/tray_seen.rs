use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use newwin_config::tray_seen::*;

fn temp_file(text: Option<&str>) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!("newwin-trayseen-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    let path = dir.join("tray_seen.toml");
    if let Some(t) = text {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, t).unwrap();
    }
    path
}

#[test]
fn note_adds_once_and_updates_the_name() {
    let mut s = TraySeen::default();
    assert!(s.note("Discord.exe", "Discord"));
    assert!(!s.note("discord.exe", "Discord"));
    assert!(s.note("Discord.exe", "Discord (3)"));
    assert_eq!(s.apps.len(), 1);
}

#[test]
fn seen_list_is_capped_at_200() {
    let mut s = TraySeen::default();
    for i in 0..201 {
        assert!(s.note(&format!("app{i}.exe"), "x"));
    }
    assert_eq!(s.apps.len(), 200);
    assert_eq!(s.apps[0].exe, "app1.exe");
    assert_eq!(s.apps[199].exe, "app200.exe");
}

#[test]
fn garbage_tray_seen_loads_empty_with_an_error() {
    let (s, err) = TraySeen::load(&temp_file(Some("[[app]]\nexe = 5")));
    assert!(s.apps.is_empty());
    assert!(err.is_some());
    let (s, err) = TraySeen::load(&temp_file(Some("not toml [[[")));
    assert!(s.apps.is_empty());
    assert!(err.is_some());
}

#[test]
fn missing_file_is_empty_and_round_trip_works() {
    let path = temp_file(None);
    let (s, err) = TraySeen::load(&path);
    assert_eq!((s.apps.len(), err), (0, None));
    let mut s = TraySeen::default();
    s.note("a.exe", "A");
    s.note("b.exe", "B \"quoted\"");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, s.to_toml()).unwrap();
    let (back, err) = TraySeen::load(&path);
    assert_eq!(err, None);
    assert_eq!(back, s);
}
