use newwin_core::settings_pages::{PAGES, candidates};
use newwin_core::spotlight::{Candidate, Kind, rank, system_candidates};
use newwin_core::strings::strings_for;

fn none(_: &str) -> u32 {
    0
}

fn first_uri(q: &str, cands: &[Candidate]) -> Option<&'static str> {
    let hit = rank(q, cands, &none, 8).into_iter().next()?;
    match cands[hit.index].kind {
        Kind::SettingsPage(i) => Some(PAGES[i].uri),
        _ => None,
    }
}

#[test]
fn bluetooth_finds_bluetooth_page() {
    assert_eq!(first_uri("bluetooth", &candidates(false)), Some("ms-settings:bluetooth"));
}

#[test]
fn ukrainian_word_finds_display() {
    assert_eq!(first_uri("монітор", &candidates(false)), Some("ms-settings:display"));
}

#[test]
fn english_query_in_ukrainian_ui_shows_ukrainian_name() {
    let c = candidates(true);
    let hit = &rank("sound", &c, &none, 8)[0];
    assert!(matches!(c[hit.index].kind, Kind::SettingsPage(i) if PAGES[i].uri == "ms-settings:sound"));
    assert_eq!(c[hit.index].name, "Звук");
}

#[test]
fn wifi_and_update_by_everyday_words() {
    let c = candidates(false);
    assert_eq!(first_uri("wifi", &c), Some("ms-settings:network-wifi"));
    assert_eq!(first_uri("оновлення", &c), Some("ms-settings:windowsupdate"));
    assert_eq!(first_uri("яскравість", &c), Some("ms-settings:display"));
}

#[test]
fn every_uri_is_ms_settings_and_unique() {
    assert!(PAGES.len() >= 60, "{}", PAGES.len());
    let mut uris: Vec<_> = PAGES.iter().map(|p| p.uri).collect();
    assert!(uris.iter().all(|u| u.starts_with("ms-settings:")));
    uris.sort();
    uris.dedup();
    assert_eq!(uris.len(), PAGES.len(), "a page twice");
    assert!(PAGES.iter().all(|p| !p.en.is_empty() && !p.uk.is_empty()));
}

#[test]
fn usage_key_and_id() {
    let c = candidates(false);
    let bt = c.iter().find(|c| matches!(c.kind, Kind::SettingsPage(i) if PAGES[i].uri == "ms-settings:bluetooth")).unwrap();
    assert_eq!(bt.usage_key(), "settings:bluetooth");
    assert_eq!(bt.id, "settings:bluetooth");
}

#[test]
fn apps_before_pages_before_system_at_the_same_level() {
    let app = Candidate { name: "Sound Recorder".into(), id: "rec".into(), kind: Kind::App, file_path: None, packaged: false, words: Vec::new() };
    let mut c = system_candidates(strings_for("en-US"));
    c.extend(candidates(false));
    c.push(app);
    let hits = rank("sound", &c, &none, 8);
    let kinds: Vec<_> = hits.iter().map(|h| &c[h.index].kind).collect();
    // "Sound" (page) is a prefix match like "Sound Recorder" (app); the app goes first.
    assert_eq!(kinds[0], &Kind::App, "{kinds:?}");
    assert!(matches!(kinds[1], Kind::SettingsPage(_)), "{kinds:?}");
}

#[test]
fn a_hit_through_a_word_bolds_nothing() {
    let c = candidates(false);
    let hit = &rank("монітор", &c, &none, 8)[0];
    assert!(hit.ranges.is_empty(), "{:?}", hit.ranges);
}

#[test]
fn strings_know_their_language_and_label_pages() {
    let (en, uk) = (strings_for("en-US"), strings_for("uk-UA"));
    assert!(!en.ukrainian && uk.ukrainian);
    assert_eq!((en.kind_settings_page, uk.kind_settings_page), ("Settings", "Параметри"));
}
