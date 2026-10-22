use newwin_core::spotlight::*;

fn app(name: &str) -> Candidate {
    Candidate { name: name.into(), id: name.into(), kind: Kind::App, file_path: None, packaged: false }
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
