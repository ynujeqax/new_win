use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use newwin_config::usage::*;

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
    let dir = std::env::temp_dir().join(format!("newwin-usage-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    // A directory left by an earlier run with the same pid must not count as this test's file.
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("usage.toml");
    if let Some(t) = text {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, t).unwrap();
    }
    TempFile { dir, path }
}

#[test]
fn missing_file_is_empty() {
    let (u, err) = Usage::load(&temp_file(None));
    assert_eq!(u, Usage::default());
    assert_eq!(err, None);
}

#[test]
fn round_trip() {
    let path = temp_file(None);
    let mut u = Usage::default();
    u.bump("Microsoft.WindowsTerminal_8wekyb3d8bbwe!App");
    u.bump("Microsoft.WindowsTerminal_8wekyb3d8bbwe!App");
    u.bump("system:lock");
    save_atomic(&path, &u.to_toml()).unwrap();
    let (back, err) = Usage::load(&path);
    assert_eq!(err, None);
    assert_eq!(back.get("Microsoft.WindowsTerminal_8wekyb3d8bbwe!App"), 2);
    assert_eq!(back.get("system:lock"), 1);
    assert_eq!(back.get("unknown"), 0);
    let dir = path.parent().unwrap();
    assert_eq!(std::fs::read_dir(dir).unwrap().count(), 1, "the temporary file is renamed away");
}

#[test]
fn corrupt_file_is_empty_with_error() {
    let (u, err) = Usage::load(&temp_file(Some("[[[ nope")));
    assert_eq!(u, Usage::default());
    assert!(err.is_some());
}

#[test]
fn bump_saturates() {
    let (mut u, _) = Usage::load(&temp_file(Some("[launches]\n\"a\" = 4294967295\n")));
    u.bump("a");
    assert_eq!(u.get("a"), u32::MAX);
}

#[test]
fn out_of_range_entry_dropped() {
    let (u, err) = Usage::load(&temp_file(Some("[launches]\n\"a\" = -1\n\"b\" = 5\n\"c\" = \"x\"\n\"d\" = 99999999999\n")));
    assert_eq!(err, None);
    assert_eq!((u.get("a"), u.get("b"), u.get("c"), u.get("d")), (0, 5, 0, 0));
}

#[test]
fn retain_drops_stale_keys() {
    let mut u = Usage::default();
    u.bump("gone");
    u.bump("kept");
    u.retain(|k| k == "kept");
    assert_eq!((u.get("gone"), u.get("kept")), (0, 1));
}

#[test]
fn usage_lives_next_to_the_config() {
    assert_eq!(newwin_config::usage_path().parent(), newwin_config::default_path().parent());
    assert!(newwin_config::usage_path().ends_with("usage.toml"));
}
