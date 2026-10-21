use std::cell::Cell;

use newwin_platform::appinfo::process_creation_time;
use newwin_platform::windows_watch::ExeCache;

/// Windows reuses process ids: a new process with an old pid must not inherit the old exe.
#[test]
fn a_reused_pid_is_resolved_again() {
    let calls = Cell::new(0);
    let mut cache = ExeCache::default();
    let mut get = |pid, created, path: &str| {
        cache.get(pid, created, || {
            calls.set(calls.get() + 1);
            Some(path.to_string())
        })
    };
    assert_eq!(get(42, Some(100), "a.exe").as_deref(), Some("a.exe"));
    assert_eq!(get(42, Some(100), "ignored.exe").as_deref(), Some("a.exe"));
    assert_eq!(calls.get(), 1);
    assert_eq!(get(42, Some(200), "b.exe").as_deref(), Some("b.exe"));
    assert_eq!(calls.get(), 2);
    // A process we cannot open is not cached.
    assert_eq!(get(7, None, "c.exe").as_deref(), Some("c.exe"));
    assert_eq!(get(7, None, "d.exe").as_deref(), Some("d.exe"));
}

#[test]
fn own_process_has_a_stable_creation_time() {
    let t = process_creation_time(std::process::id());
    assert!(t.is_some());
    assert_eq!(t, process_creation_time(std::process::id()));
}
