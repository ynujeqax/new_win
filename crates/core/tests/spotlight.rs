use newwin_core::spotlight::*;

fn app(name: &str) -> Candidate {
    Candidate { name: name.into(), id: name.into(), kind: Kind::App, file_path: None, packaged: false, words: Vec::new() }
}

fn none(_: &str) -> u32 {
    0
}

#[test]
fn levels_in_order() {
    assert_eq!(match_name("Steam", "ste").unwrap(), Match { level: MatchLevel::Prefix, ranges: vec![(0, 3)] });
    assert_eq!(match_name("Visual Studio Code", "code").unwrap(), Match { level: MatchLevel::WordStart, ranges: vec![(14, 4)] });
    assert_eq!(match_name("Windows PowerShell", "shell").unwrap().level, MatchLevel::WordStart, "CamelCase starts a word");
    assert_eq!(match_name("Discord", "ord").unwrap(), Match { level: MatchLevel::Substring, ranges: vec![(4, 3)] });
    assert_eq!(match_name("Visual Studio Code", "vsc").unwrap(), Match { level: MatchLevel::Subsequence, ranges: vec![(0, 1), (7, 1), (14, 1)] });
    assert_eq!(match_name("Steam", "xyz"), None);
}

#[test]
fn case_and_cyrillic() {
    assert_eq!(match_name("Блокнот", "БЛОК").unwrap().level, MatchLevel::Prefix);
    assert_eq!(match_name("STEAM", "steam").unwrap().ranges, vec![(0, 5)]);
}

#[test]
fn subsequence_prefers_word_starts_then_leftmost() {
    // "ms" at word starts: M(icrosoft) S(tore), not the "s" inside "Microsoft".
    assert_eq!(match_name("Microsoft Store", "ms").unwrap().ranges, vec![(0, 1), (10, 1)]);
    // No word-start placement exists: greedy leftmost, adjacent chars merged.
    assert_eq!(match_name("Discord", "dco").unwrap(), Match { level: MatchLevel::Subsequence, ranges: vec![(0, 1), (3, 2)] });
}

#[test]
fn order_level_then_launches_then_kind_then_name() {
    let c = vec![app("Notepad++"), app("Notepad"), app("Snote"), Candidate { kind: Kind::System(SystemCommand::Settings), ..app("Note sys") }];
    let launches = |k: &str| if k == "Notepad++" { 5 } else { 0 };
    let names: Vec<_> = rank("note", &c, &launches, 8).iter().map(|h| c[h.index].name.clone()).collect();
    assert_eq!(names, ["Notepad++", "Notepad", "Note sys", "Snote"]);
}

#[test]
fn max_results() {
    let c: Vec<_> = (0..20).map(|i| app(&format!("App {i}"))).collect();
    assert_eq!(rank("app", &c, &none, 8).len(), 8);
}

#[test]
fn empty_query_no_hits() {
    assert!(rank("  ", &[app("Steam")], &none, 8).is_empty());
}

#[test]
fn query_is_trimmed() {
    assert_eq!(rank(" ste ", &[app("Steam")], &none, 8).len(), 1);
}

#[test]
fn hits_carry_ranges() {
    let hits = rank("code", &[app("Visual Studio Code")], &none, 8);
    assert_eq!(hits, vec![Hit { index: 0, ranges: vec![(14, 4)] }]);
}

#[test]
fn swap_layout_both_ways() {
    assert_eq!(swap_layout("іеуфь").as_deref(), Some("steam"));
    assert_eq!(swap_layout("ВШІСЩКВ").as_deref(), Some("DISCORD"));
    assert_eq!(swap_layout("steam").as_deref(), Some("іеуфь"));
    assert_eq!(swap_layout("ґ'").as_deref(), Some("\\`"));
    assert_eq!(swap_layout("123"), None);
}

#[test]
fn wrong_layout_hits_come_after_direct_ones_without_duplicates() {
    let c = vec![app("Steam"), app("іеуфь notes")];
    let hits = rank("іеуфь", &c, &none, 8);
    assert_eq!(hits.iter().map(|h| h.index).collect::<Vec<_>>(), [1, 0]);
    // "steam" itself would also match "Steam" through its swap: still listed once.
    assert_eq!(rank("steam", &c, &none, 8).iter().filter(|h| h.index == 0).count(), 1);
}

#[test]
fn ranges_stay_in_bounds_for_odd_names() {
    let long = "x".repeat(300);
    for name in ["", long.as_str(), "🎮 Game 🎮", "Cafe\u{301} Bar", "İstanbul"] {
        let units = name.encode_utf16().count();
        for q in ["a", "g", "e", "i", "🎮", "ist", "x"] {
            if let Some(m) = match_name(name, q) {
                assert!(m.ranges.iter().all(|&(s, l)| l > 0 && s + l <= units), "{name:?} {q:?} {m:?}");
            }
        }
    }
}

#[test]
#[ignore = "timing; run with --release -- --ignored"]
fn rank_1000_apps_under_1ms() {
    let c: Vec<_> = (0..1000).map(|i| app(&format!("Application Number {i} Studio"))).collect();
    let t = std::time::Instant::now();
    for q in ["a", "ap", "stu", "ans"] {
        rank(q, &c, &none, 8);
    }
    assert!(t.elapsed().as_micros() < 4000, "{:?}", t.elapsed());
}

#[test]
fn system_commands_in_order_with_usage_keys() {
    let s = newwin_core::strings::strings_for("en-US");
    let c = system_candidates(s);
    let names: Vec<_> = c.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["NewWin Settings", "Shut down", "Restart", "Sleep", "Lock", "Sign out"], "NewWin's own, not Windows' Settings app");
    let uk = system_candidates(newwin_core::strings::strings_for("uk-UA"));
    assert_eq!(uk[0].name, "Налаштування NewWin");
    let keys: Vec<_> = c.iter().map(Candidate::usage_key).collect();
    assert_eq!(keys, ["system:settings", "system:shut_down", "system:restart", "system:sleep", "system:lock", "system:sign_out"]);
    assert!(c.iter().all(|c| !c.can_run_as_admin() && c.id == c.usage_key()));
    assert_eq!(c[1].kind, Kind::System(SystemCommand::ShutDown));
}

#[test]
fn only_shut_down_and_restart_need_confirm() {
    use SystemCommand::*;
    let need: Vec<_> = [Settings, ShutDown, Restart, Sleep, Lock, SignOut].into_iter().filter(|c| c.needs_confirm()).collect();
    assert_eq!(need, [ShutDown, Restart]);
}

#[test]
fn confirm_needs_second_enter() {
    let mut c = Confirm::default();
    assert!(!c.press(2, true, 0));
    assert_eq!(c.armed(100), Some(2));
    assert_eq!(c.expires_at(), Some(CONFIRM_MS));
    assert!(c.press(2, true, 100));
    assert_eq!(c.armed(100), None);
}

#[test]
fn confirm_resets_on_other_row_and_after_3s() {
    let mut c = Confirm::default();
    assert!(!c.press(2, true, 0));
    assert!(!c.press(3, true, 10));
    assert_eq!(c.armed(10), Some(3));
    assert_eq!(c.armed(3010), None, "expired");
    assert!(!c.press(3, true, 3010), "after expiry the next Enter arms again");
    c.reset();
    assert_eq!(c.armed(3011), None);
}

#[test]
fn no_confirm_runs_at_once() {
    let mut c = Confirm::default();
    c.press(2, true, 0);
    assert!(c.press(4, false, 1));
    assert_eq!(c.armed(1), None);
}

#[test]
fn uninstallers_and_documents_are_not_listed() {
    assert!(!is_listed("Uninstall Steam", Some(r"C:\G\Steam\uninstall.exe")));
    assert!(!is_listed("Steam Remover", Some(r"C:\G\unins000.exe")));
    assert!(!is_listed("Readme", Some(r"C:\G\readme.TXT")));
    assert!(!is_listed("Website", Some(r"C:\G\site.url")));
    assert!(!is_listed("  ", None));
    assert!(is_listed("Steam", Some(r"C:\Programs\Steam\Steam.exe")));
    assert!(is_listed("Settings", None));
    // The installer's Start-menu shortcut: Spotlight already has the NewWin Settings command.
    assert!(!is_listed("NewWin Settings", Some(r"C:\Programs\NewWin\NewWinSettings.exe")));
    assert!(!is_listed("NewWin Settings", Some(r"c:\x\newwinsettings.EXE")));
    assert!(is_listed("Settings", Some(r"C:\Windows\ImmersiveControlPanel\SystemSettings.exe")));
}

#[test]
fn uk_strings_present() {
    let s = newwin_core::strings::strings_for("uk-UA");
    assert_eq!(s.lock, "Заблокувати");
    assert_eq!(s.press_again_shut_down, "Натисніть ↵ ще раз, щоб завершити роботу");
    assert_eq!(newwin_core::strings::strings_for("en").spotlight_keys, "↑↓ select · ↵ open · Ctrl ↵ run as administrator · Esc close");
}
