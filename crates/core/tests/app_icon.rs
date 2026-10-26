use newwin_core::app_icon::*;
use newwin_core::apps::GroupKey;
use newwin_core::spotlight::{Candidate, Kind, SystemCommand};

fn app(id: &str) -> Candidate {
    Candidate { name: "x".into(), id: id.into(), kind: Kind::App, file_path: None, packaged: false, words: Vec::new() }
}

fn tray(h: u32) -> IconKey {
    IconKey::Tray { hicon: h, px: 16 }
}

#[test]
fn premultiply_scales_colour_by_alpha() {
    let mut px = vec![200, 100, 50, 128, 9, 9, 9, 0, 1, 2, 3, 255];
    premultiply(&mut px);
    assert_eq!(px, [100, 50, 25, 128, 0, 0, 0, 0, 1, 2, 3, 255]);
}

#[test]
fn an_image_without_alpha_becomes_opaque() {
    let mut none = vec![10, 20, 30, 0, 40, 50, 60, 0];
    opaque_if_no_alpha(&mut none);
    assert_eq!(none, [10, 20, 30, 255, 40, 50, 60, 255]);
    let mut some = vec![10, 20, 30, 0, 40, 50, 60, 7];
    opaque_if_no_alpha(&mut some);
    assert_eq!(some, [10, 20, 30, 0, 40, 50, 60, 7]);
}

#[test]
fn sources() {
    assert_eq!(source_for_group(&GroupKey::AppId("Claude_x!App".into())), r"shell:AppsFolder\Claude_x!App");
    assert_eq!(source_for_group(&GroupKey::ExePath(r"c:\a\b.exe".into())), r"c:\a\b.exe");
    assert_eq!(
        source_for_candidate(&app("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App")).as_deref(),
        Some(r"shell:AppsFolder\Microsoft.WindowsCalculator_8wekyb3d8bbwe!App")
    );
    let lock = Candidate { kind: Kind::System(SystemCommand::Lock), ..app("") };
    assert_eq!(source_for_candidate(&lock), None);
}

#[test]
fn one_job_per_key_while_waiting() {
    let mut c: IconCache<u32> = IconCache::new();
    assert_eq!(c.get(&tray(1)), Lookup::Ask);
    assert_eq!(c.get(&tray(1)), Lookup::Waiting);
    c.fill(tray(1), Some(7));
    assert_eq!(c.get(&tray(1)), Lookup::Ready(7));
}

#[test]
fn missing_is_not_asked_again() {
    let mut c: IconCache<u32> = IconCache::new();
    let shell = IconKey::Shell { source: r"c:\gone.exe".into(), px: 16 };
    for k in [shell.clone(), tray(2)] {
        assert_eq!(c.get(&k), Lookup::Ask);
        c.fill(k.clone(), None);
        assert_eq!(c.get(&k), Lookup::Missing);
    }
    c.forget_missing_shell();
    assert_eq!(c.get(&shell), Lookup::Ask, "a re-index looks again");
    assert_eq!(c.get(&tray(2)), Lookup::Missing, "a tray handle does not come back");
}

#[test]
fn the_cache_never_holds_more_than_128() {
    let mut c: IconCache<u32> = IconCache::new();
    for h in 0..500 {
        assert_eq!(c.get(&tray(h)), Lookup::Ask);
        c.fill(tray(h), Some(h));
        assert!(c.len() <= CACHE_CAPACITY);
    }
    assert_eq!(c.get(&tray(499)), Lookup::Ready(499));
    assert_eq!(c.get(&tray(0)), Lookup::Ask, "the oldest went first");
}

#[test]
fn a_key_includes_its_size() {
    let mut c: IconCache<u32> = IconCache::new();
    let at = |px| IconKey::Shell { source: "s".into(), px };
    assert_eq!(c.get(&at(16)), Lookup::Ask);
    c.fill(at(16), Some(16));
    assert_eq!(c.get(&at(24)), Lookup::Ask);
    assert_eq!(c.get(&at(16)), Lookup::Ready(16));
}

#[test]
fn fade_takes_150_ms() {
    assert_eq!(fade(1000, 1000), 0.0);
    assert_eq!(fade(1000, 1075), 0.5);
    assert_eq!(fade(1000, 1150), 1.0);
    assert_eq!(fade(1000, 5), 0.0);
}

#[test]
fn an_app_id_falls_back_to_the_windows_exe() {
    // Chromium browsers tag windows with a per-profile id no Start entry has.
    let id = GroupKey::AppId("Helium.Profile1".into());
    assert_eq!(fallback_for_group(&id, Some(r"C:\Apps\helium.exe")).as_deref(), Some(r"C:\Apps\helium.exe"));
    assert_eq!(fallback_for_group(&id, None), None);
    assert_eq!(fallback_for_group(&GroupKey::ExePath(r"c:\a.exe".into()), Some(r"C:\a.exe")), None, "the exe is already the source");
}
