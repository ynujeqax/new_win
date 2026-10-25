use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use newwin_config::tray_seen::*;

/// A file in a directory of its own, removed with it when the test ends (even on a panic).
struct TempFile {
    dir: PathBuf,
    path: PathBuf,
}

impl std::ops::Deref for TempFile {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.path
    }
}

impl AsRef<std::path::Path> for TempFile {
    fn as_ref(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn temp_file(text: Option<&str>) -> TempFile {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!("newwin-trayseen-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    // A directory left by an earlier run with the same pid must not count as this test's file.
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("tray_seen.toml");
    if let Some(t) = text {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, t).unwrap();
    }
    TempFile { dir, path }
}

#[test]
fn note_adds_once_and_updates_the_name() {
    let mut s = TraySeen::default();
    assert_eq!(s.note("Discord.exe", "Discord"), Noted::Added);
    assert_eq!(s.note("discord.exe", "Discord"), Noted::Same);
    assert_eq!(s.note("Discord.exe", "Discord (3)"), Noted::Renamed);
    assert_eq!(s.apps.len(), 1);
    assert_eq!(s.apps[0].name, "Discord (3)");
}

#[test]
fn an_add_outranks_a_rename_outranks_nothing() {
    assert!(Noted::Added > Noted::Renamed && Noted::Renamed > Noted::Same);
}

#[test]
fn seen_list_is_capped_at_200() {
    let mut s = TraySeen::default();
    for i in 0..201 {
        assert_eq!(s.note(&format!("app{i}.exe"), "x"), Noted::Added);
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
