//! Spotlight 1.0.6 rows: currency answers, emoji, settings pages, quick links and web search.

use std::collections::BTreeMap;
use std::rc::Rc;

use newwin_core::currency::Rates;
use newwin_core::links::{Link, Target};
use newwin_core::spotlight::{Candidate, Kind, link_candidates};
use newwin_core::web::Engine;
use newwin_ui::panel::{PanelContent, PanelInput};
use newwin_ui::spotlight::{HintKind, Row, SpotlightExtras, SpotlightOp, SpotlightView, action_id, decode_action};

const RETURN: u16 = 0x0D;
const DOWN: u16 = 0x28;
const NOW: u64 = 1_800_000_000;

fn app(name: &str) -> Candidate {
    Candidate { name: name.into(), id: format!("id:{name}"), kind: Kind::App, file_path: None, packaged: false, words: Vec::new() }
}

fn rates(fetched_at: u64) -> Rc<Rates> {
    let per_uah: BTreeMap<String, f64> = [("UAH", 1.0), ("USD", 41.3), ("EUR", 44.8)].into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    Rc::new(Rates { date: "2026-10-27".into(), fetched_at, per_uah })
}

fn extras() -> SpotlightExtras {
    SpotlightExtras { engine: Some(Engine::DuckDuckGo), rates: None, currency_on: true, uk: false, now_secs: NOW, links: Vec::new() }
}

fn view_with(catalog: Vec<Candidate>, max: usize, x: SpotlightExtras) -> SpotlightView {
    SpotlightView::for_test_with(Rc::new(catalog), max, x)
}

fn view(x: SpotlightExtras) -> SpotlightView {
    view_with(vec![app("Steam"), app("Spotify")], 8, x)
}

/// Types `s`; the last keystroke's action.
fn typed(v: &mut SpotlightView, s: &str) -> Option<u32> {
    let mut last = None;
    for c in s.chars() {
        last = v.input(PanelInput::Char(c), 0).action;
    }
    last
}

fn key(v: &mut SpotlightView, vk: u16) -> Option<u32> {
    v.input(PanelInput::Key(vk), 0).action
}

#[test]
fn web_row_is_last_and_alone_when_nothing_is_found() {
    let mut v = view(extras());
    typed(&mut v, "ste");
    assert!(matches!(v.rows(), [Row::Hit(_), Row::Web]), "{:?}", v.rows());
    let mut v = view(extras());
    typed(&mut v, "zzzq");
    assert!(matches!(v.rows(), [Row::Web]), "{:?}", v.rows());
    assert_eq!(v.selected(), Some(0));
    let mut v = view(SpotlightExtras { engine: None, ..extras() });
    typed(&mut v, "zzzq");
    assert!(v.rows().is_empty(), "web search off: the No results row");
}

#[test]
fn max_results_counts_the_answer_and_the_web_row() {
    let apps = (0..10).map(|i| app(&format!("App {i}"))).collect();
    let mut v = view_with(apps, 5, extras());
    typed(&mut v, "app");
    assert_eq!(v.rows().len(), 5);
    assert!(matches!(v.rows()[4], Row::Web));
    let mut v = view(extras());
    typed(&mut v, "2*3");
    assert!(matches!(v.rows(), [Row::Answer(_), Row::Web]), "{:?}", v.rows());
}

#[test]
fn colon_query_shows_only_emoji() {
    let mut v = view(extras());
    typed(&mut v, ":heart");
    assert_eq!(v.rows().len(), 8);
    assert!(v.rows().iter().all(|r| matches!(r, Row::Emoji(_))));
    assert!(matches!(v.rows()[0], Row::Emoji(e) if e.glyph == "❤️"));
}

#[test]
fn lone_colon_shows_nothing_to_pick() {
    let mut v = view(extras());
    typed(&mut v, ":");
    assert!(v.rows().is_empty());
    assert_eq!(v.selected(), None);
    assert_eq!(key(&mut v, RETURN), None);
}

#[test]
fn currency_row_from_rates() {
    let mut v = view(SpotlightExtras { rates: Some(rates(NOW)), ..extras() });
    typed(&mut v, "100 usd in uah");
    match &v.rows()[0] {
        Row::Currency { text, date } => assert_eq!((text.as_str(), date.as_str()), ("4\u{2009}130.00 UAH", "2026-10-27")),
        r => panic!("{r:?}"),
    }
    assert_eq!(v.copy_text().as_deref(), Some("4\u{2009}130.00 UAH"));
    assert_eq!(key(&mut v, RETURN), Some(action_id(0, SpotlightOp::Copy)));
}

#[test]
fn stale_or_missing_rates_ask_for_a_refresh() {
    let mut v = view(extras());
    assert_eq!(typed(&mut v, "100 usd in uah"), Some(action_id(0, SpotlightOp::Refresh)));
    assert!(v.wants_rates());
    let mut v = view(SpotlightExtras { rates: Some(rates(NOW - 7 * 3600)), ..extras() });
    assert_eq!(typed(&mut v, "100 usd in uah"), Some(action_id(0, SpotlightOp::Refresh)), "older than 6 h");
    assert!(matches!(v.rows()[0], Row::Currency { .. }), "the old rates still answer");
    let mut v = view(SpotlightExtras { rates: Some(rates(NOW - 3600)), ..extras() });
    assert_eq!(typed(&mut v, "100 usd in uah"), None);
    assert!(!v.wants_rates());
    let mut v = view(SpotlightExtras { currency_on: false, rates: Some(rates(NOW)), ..extras() });
    assert_eq!(typed(&mut v, "100 usd in uah"), None);
    assert!(!v.wants_rates());
    assert!(!v.rows().iter().any(|r| matches!(r, Row::Currency { .. })), "currency off: no answer");
}

#[test]
fn rates_arriving_bring_the_answer_in_and_keep_the_query() {
    let mut v = view(extras());
    typed(&mut v, "100 usd in uah");
    assert!(matches!(v.rows(), [Row::Web]));
    let r = v.set_rates(rates(NOW));
    assert!(r.repaint && r.resize);
    assert_eq!(v.query(), "100 usd in uah");
    assert!(matches!(v.rows(), [Row::Currency { .. }, Row::Web]));
    assert_eq!(v.selected(), Some(0));
}

#[test]
fn rates_arriving_keep_a_moved_selection() {
    let mut v = view_with(vec![app("10 usd in uah tool"), app("x")], 8, extras());
    typed(&mut v, "10 usd in uah");
    key(&mut v, DOWN);
    assert!(matches!(v.rows()[v.selected().unwrap()], Row::Web));
    v.set_rates(rates(NOW));
    assert!(matches!(v.rows()[v.selected().unwrap()], Row::Web), "still on the web row");
}

#[test]
fn enter_on_the_web_row_searches() {
    let mut v = view(extras());
    typed(&mut v, "кіт & пес");
    assert_eq!(key(&mut v, RETURN), Some(action_id(0, SpotlightOp::Web)));
    assert_eq!(v.web_query().as_deref(), Some("кіт & пес"));
}

#[test]
fn enter_on_an_emoji_copies_it() {
    let mut v = view(extras());
    typed(&mut v, ":red heart");
    assert_eq!(key(&mut v, RETURN), Some(action_id(0, SpotlightOp::CopyEmoji)));
    assert_eq!(v.copy_text().as_deref(), Some("❤️"));
    assert_eq!(v.selected_emoji().map(|e| e.glyph), Some("❤️"));
}

#[test]
fn hint_follows_the_selected_row() {
    let mut v = view(extras());
    typed(&mut v, "ste");
    assert_eq!(v.hint_kind(), HintKind::App);
    key(&mut v, DOWN);
    assert_eq!(v.hint_kind(), HintKind::Search);
    let mut v = view(extras());
    typed(&mut v, "2*3");
    assert_eq!(v.hint_kind(), HintKind::Copy);
    let mut c = newwin_core::settings_pages::candidates(false);
    c.push(app("x"));
    let mut v = view_with(c, 8, extras());
    typed(&mut v, "bluetooth");
    assert_eq!(v.hint_kind(), HintKind::Open);
}

#[test]
fn units_still_win_over_currency() {
    let mut v = view(SpotlightExtras { rates: Some(rates(NOW)), ..extras() });
    typed(&mut v, "10 m in ft");
    assert!(matches!(v.rows()[0], Row::Answer(_)), "{:?}", v.rows());
}

#[test]
fn a_quick_link_opens_like_an_app() {
    let mut c = vec![app("GitHub Desktop")];
    c.extend(link_candidates(&[Link { keyword: "gh".into(), target: "https://github.com".into() }]));
    let links = vec![("github.com".to_string(), Target::Url)];
    let mut v = view_with(c, 8, SpotlightExtras { links, ..extras() });
    typed(&mut v, "gh");
    assert!(matches!(&v.rows()[0], Row::Hit(h) if h.index == 1), "{:?}", v.rows());
    assert_eq!(key(&mut v, RETURN), Some(action_id(1, SpotlightOp::Open)));
    assert_eq!(v.hint_kind(), HintKind::Open);
}

#[test]
fn settings_pages_and_links_ask_for_no_shell_icon() {
    let mut c = newwin_core::settings_pages::candidates(false);
    c.extend(link_candidates(&[Link { keyword: "bt".into(), target: "x".into() }]));
    let mut v = view_with(c, 8, extras());
    typed(&mut v, "b");
    assert!(v.shown().iter().all(|(_, source)| source.is_none()), "{:?}", v.shown());
}

#[test]
fn new_ops_round_trip() {
    for op in [SpotlightOp::Web, SpotlightOp::CopyEmoji, SpotlightOp::Refresh, SpotlightOp::Copy, SpotlightOp::Open] {
        assert_eq!(decode_action(action_id(7, op)), (7, op));
    }
}
