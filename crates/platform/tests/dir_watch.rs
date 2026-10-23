use std::path::PathBuf;
use std::time::{Duration, Instant};

use newwin_platform::dir_watch::DirWatch;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{MSG, PM_REMOVE, PeekMessageW, WM_APP, WS_EX_TOOLWINDOW, WS_POPUP};

const MSG_ID: u32 = WM_APP + 19;

fn test_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test-tmp").join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn window(title: &str) -> HWND {
    newwin_platform::window::create_test_window(WS_EX_TOOLWINDOW, WS_POPUP, RECT::default(), title).unwrap()
}

fn got_msg(w: HWND, within: Duration) -> bool {
    let end = Instant::now() + within;
    let mut m = MSG::default();
    while Instant::now() < end {
        if unsafe { PeekMessageW(&mut m, Some(w), MSG_ID, MSG_ID, PM_REMOVE) }.as_bool() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

#[test]
fn a_write_in_the_dir_posts_the_message() {
    let dir = test_dir("dw-write");
    let w = window("dw-write");
    let _watch = DirWatch::start(&dir, w, MSG_ID).unwrap();
    std::fs::write(dir.join("config.toml"), "a = 1").unwrap();
    assert!(got_msg(w, Duration::from_secs(2)));
}

#[test]
fn a_rename_over_the_file_is_seen() {
    let dir = test_dir("dw-rename");
    let file = dir.join("config.toml");
    std::fs::write(&file, "a = 1").unwrap();
    let w = window("dw-rename");
    let _watch = DirWatch::start(&dir, w, MSG_ID).unwrap();
    newwin_config::save_atomic(&file, "a = 2").unwrap();
    assert!(got_msg(w, Duration::from_secs(2)));
}

#[test]
fn dropping_the_watch_stops_messages() {
    let dir = test_dir("dw-drop");
    let w = window("dw-drop");
    let watch = DirWatch::start(&dir, w, MSG_ID).unwrap();
    drop(watch);
    std::fs::write(dir.join("config.toml"), "a = 1").unwrap();
    assert!(!got_msg(w, Duration::from_millis(300)));
}
