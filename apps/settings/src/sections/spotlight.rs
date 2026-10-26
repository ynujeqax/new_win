//! Spotlight (spec: M4 §3.4, 1.0.6 §2.7): the Win key, the number of results, clearing the launch
//! history (a button; the window deletes `usage.toml`), the web search engine, currency rates, and
//! the quick links: one field ("keyword address", Enter adds) and a row with *Remove* per link.

use newwin_config::{Config, LinkConfig};
use newwin_core::links::{label, parse_entry};
use newwin_core::strings::Strings;
use newwin_core::web::ENGINES;

use super::{Control, Group, Row, RowId, RowValue, clamp_to};

/// The link field's placeholder: the same in every language.
pub const LINK_HINT: &str = "gh  https://github.com";

pub fn rows(c: &Config) -> Vec<Row> {
    let s = &c.spotlight;
    let engine = ENGINES.iter().position(|e| e.key() == s.web_search).unwrap_or(ENGINES.len());
    let mut rows = vec![
        Row::new(RowId::WinKey, Control::Toggle(s.win_key)),
        Row::new(RowId::MaxResults, Control::Slider(i32::try_from(s.max_results).unwrap_or(i32::MAX))),
        Row::new(RowId::ClearHistory, Control::Button),
        Row::new(RowId::WebSearch, Control::Choice(engine)).opens(Group::Web),
        Row::new(RowId::Currency, Control::Toggle(s.currency)),
        Row::new(RowId::CurrencyNote, Control::Note),
        Row::new(RowId::LinkAdd, Control::Text { text: String::new(), hint: LINK_HINT }).opens(Group::Links),
        Row::new(RowId::LinkPreview, Control::Note),
    ];
    rows.extend(s.links.iter().enumerate().map(|(i, l)| Row::new(RowId::Link(i), Control::Link { keyword: l.keyword.clone(), target: label(&l.target) })));
    rows
}

pub fn apply(c: &mut Config, id: RowId, v: RowValue) {
    let s = &mut c.spotlight;
    match (id, v) {
        (RowId::WinKey, RowValue::Bool(on)) => s.win_key = on,
        (RowId::MaxResults, RowValue::Int(v)) => s.max_results = clamp_to(id, v).unsigned_abs(),
        (RowId::WebSearch, RowValue::Choice(i)) => s.web_search = ENGINES.get(i).map_or("off", |e| e.key()).to_string(),
        (RowId::Currency, RowValue::Bool(on)) => s.currency = on,
        (RowId::LinkAdd, RowValue::Text(t)) => {
            if let Some(l) = parse_entry(&t) {
                s.links.retain(|o| !o.keyword.eq_ignore_ascii_case(&l.keyword));
                s.links.push(LinkConfig { keyword: l.keyword, target: l.target });
            }
        }
        (RowId::Link(i), RowValue::Remove) if i < s.links.len() => {
            s.links.remove(i);
        }
        _ => {}
    }
}

/// The line under the link field: what Enter would add, or how to fill it in.
pub fn link_preview(typed: &str, s: &Strings) -> String {
    match parse_entry(typed) {
        Some(l) => s.link_entry_ready.replace("{keyword}", &l.keyword).replace("{target}", &label(&l.target)),
        None => s.link_entry_help.to_string(),
    }
}

/// *Clear*: deletes `usage.toml`; a missing file is already clear. The shell sees the file go
/// through its folder watch and forgets its counts.
pub fn clear_history(path: &std::path::Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        r => r,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sections::Group;
    use newwin_config::LinkConfig;
    use crate::sections::range_of;
    use newwin_ui::slider::Range;

    #[test]
    fn win_key_toggle_maps_to_win_key() {
        let mut c = Config::default();
        apply(&mut c, RowId::WinKey, RowValue::Bool(false));
        assert!(!c.spotlight.win_key);
    }

    #[test]
    fn results_slider_is_3_to_15() {
        assert_eq!(range_of(RowId::MaxResults), Range { min: 3, max: 15, step: 1 });
        let mut c = Config::default();
        apply(&mut c, RowId::MaxResults, RowValue::Int(12));
        assert_eq!(c.spotlight.max_results, 12);
        apply(&mut c, RowId::MaxResults, RowValue::Int(1));
        assert_eq!(c.spotlight.max_results, 3);
    }

    #[test]
    fn rows_are_win_key_results_clear_web_currency_and_links() {
        let mut c = Config::default();
        c.spotlight.links.push(LinkConfig { keyword: "gh".into(), target: "https://github.com".into() });
        let r = rows(&c);
        let ids: Vec<_> = r.iter().map(|r| r.id).collect();
        use RowId::*;
        assert_eq!(ids, [WinKey, MaxResults, ClearHistory, WebSearch, Currency, CurrencyNote, LinkAdd, LinkPreview, Link(0)]);
        assert_eq!(r[3].control, Control::Choice(0), "DuckDuckGo");
        assert_eq!(r[3].group, Some(Group::Web));
        assert_eq!(r[4].control, Control::Toggle(true));
        assert_eq!(r[6].group, Some(Group::Links));
        assert_eq!(r[8].control, Control::Link { keyword: "gh".into(), target: "github.com".into() });
    }

    #[test]
    fn engine_choice_maps_to_its_key() {
        let mut c = Config::default();
        apply(&mut c, RowId::WebSearch, RowValue::Choice(5));
        assert_eq!(c.spotlight.web_search, "off");
        assert_eq!(rows(&c)[3].control, Control::Choice(5));
        apply(&mut c, RowId::WebSearch, RowValue::Choice(2));
        assert_eq!(c.spotlight.web_search, "bing");
        let s = newwin_core::strings::strings_for("en-US");
        assert_eq!(crate::sections::choices(RowId::WebSearch, s), ["DuckDuckGo", "Google", "Bing", "Brave", "Startpage", "Off"]);
    }

    #[test]
    fn currency_toggle() {
        let mut c = Config::default();
        apply(&mut c, RowId::Currency, RowValue::Bool(false));
        assert!(!c.spotlight.currency);
    }

    #[test]
    fn add_link_parses_and_replaces_the_same_keyword() {
        let mut c = Config::default();
        apply(&mut c, RowId::LinkAdd, RowValue::Text("gh  https://github.com".into()));
        apply(&mut c, RowId::LinkAdd, RowValue::Text(r"dl %USERPROFILE%Downloads".into()));
        apply(&mut c, RowId::LinkAdd, RowValue::Text("GH https://gitlab.com".into()));
        let got: Vec<_> = c.spotlight.links.iter().map(|l| (l.keyword.as_str(), l.target.as_str())).collect();
        assert_eq!(got, [("dl", r"%USERPROFILE%Downloads"), ("gh", "https://gitlab.com")]);
    }

    #[test]
    fn an_invalid_entry_changes_nothing() {
        let mut c = Config::default();
        apply(&mut c, RowId::LinkAdd, RowValue::Text("onlyword".into()));
        apply(&mut c, RowId::LinkAdd, RowValue::Text("   ".into()));
        assert!(c.spotlight.links.is_empty());
    }

    #[test]
    fn remove_a_link() {
        let mut c = Config::default();
        apply(&mut c, RowId::LinkAdd, RowValue::Text("a x".into()));
        apply(&mut c, RowId::LinkAdd, RowValue::Text("b y".into()));
        apply(&mut c, RowId::Link(0), RowValue::Remove);
        assert_eq!(c.spotlight.links.iter().map(|l| l.keyword.as_str()).collect::<Vec<_>>(), ["b"]);
        apply(&mut c, RowId::Link(9), RowValue::Remove);
        assert_eq!(c.spotlight.links.len(), 1, "out of range: nothing");
    }

    #[test]
    fn link_preview_reads_the_field() {
        let s = newwin_core::strings::strings_for("en-US");
        assert_eq!(link_preview("gh https://github.com", s), "Enter adds gh → github.com");
        assert_eq!(link_preview("", s), s.link_entry_help);
        assert_eq!(link_preview("onlyword", s), s.link_entry_help);
    }

    #[test]
    fn clear_deletes_usage_toml_and_a_missing_file_is_fine() {
        let dir = std::env::temp_dir().join(format!("newwin-settings-clear-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("usage.toml");
        std::fs::write(&path, "[launches]\n\"a\" = 3\n").unwrap();
        clear_history(&path).unwrap();
        assert!(!path.exists());
        clear_history(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
