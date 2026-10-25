use std::time::{Duration, Instant};

use newwin_platform::appinfo::ComGuard;
use newwin_platform::apps_index::{AppsWatch, enumerate_apps};
use newwin_platform::worker::{Job, Worker, take_apps};
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::WindowsAndMessaging::{MSG, PM_REMOVE, PeekMessageW, WM_APP, WS_EX_TOOLWINDOW, WS_POPUP};

#[test]
fn index_has_known_apps() {
    let _com = ComGuard::init();
    let apps = enumerate_apps().unwrap();
    eprintln!("{} apps", apps.len());
    assert!(!apps.is_empty());
    assert!(apps.iter().any(|a| a.name == "Notepad" || a.name == "Блокнот"), "Notepad");
    let settings = apps.iter().find(|a| ["Settings", "Параметри", "Настройки"].contains(&a.name.as_str())).expect("Settings");
    assert!(settings.packaged && settings.file_path.is_none(), "{settings:?}");
    let exe = apps.iter().find(|a| a.file_path.as_deref().is_some_and(|p| p.to_lowercase().ends_with(".exe"))).expect("a classic app");
    assert!(std::path::Path::new(exe.file_path.as_deref().unwrap()).exists(), "{exe:?}");
    assert!(apps.iter().all(|a| a.file_path.as_deref().is_none_or(|p| !p.starts_with("::"))), "shell targets are not files");
    let mut ids: Vec<_> = apps.iter().map(|a| a.id.to_lowercase()).collect();
    ids.sort();
    let n = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), n, "no duplicate ids");
    assert!(apps.iter().all(|a| !a.name.to_lowercase().starts_with("uninstall")));
}

#[test]
fn watch_starts_and_stops() {
    let notify = newwin_platform::window::create_test_window(WS_EX_TOOLWINDOW, WS_POPUP, RECT::default(), "apps-watch").unwrap();
    let start = Instant::now();
    let w = AppsWatch::start(notify, WM_APP + 15).unwrap();
    drop(w);
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn worker_posts_apps() {
    let notify = newwin_platform::window::create_test_window(WS_EX_TOOLWINDOW, WS_POPUP, RECT::default(), "apps-worker").unwrap();
    let w = Worker::spawn(notify, WM_APP + 4, WM_APP + 16, WM_APP + 21);
    w.submit_job(Job::Apps);
    let end = Instant::now() + Duration::from_secs(10);
    let mut m = MSG::default();
    while Instant::now() < end {
        if unsafe { PeekMessageW(&mut m, Some(notify), WM_APP + 16, WM_APP + 16, PM_REMOVE) }.as_bool() {
            let r = unsafe { take_apps(m.lParam) };
            assert!(!r.apps.as_ref().unwrap().is_empty());
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("no apps posted");
}

#[test]
fn worker_runs_closures() {
    let notify = newwin_platform::window::create_test_window(WS_EX_TOOLWINDOW, WS_POPUP, RECT::default(), "apps-run").unwrap();
    let w = Worker::spawn(notify, WM_APP + 4, WM_APP + 16, WM_APP + 21);
    let (tx, rx) = std::sync::mpsc::channel();
    w.submit_job(Job::Run(Box::new(move || tx.send(7).unwrap())));
    assert_eq!(rx.recv_timeout(Duration::from_secs(5)), Ok(7));
}
