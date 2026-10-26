use newwin_core::web::{ENGINES, Engine, encode, search_url};

#[test]
fn ddg_url() {
    assert_eq!(search_url(Engine::DuckDuckGo, "rust lang"), "https://duckduckgo.com/?q=rust+lang");
}

#[test]
fn other_engines() {
    assert_eq!(search_url(Engine::Google, "a"), "https://www.google.com/search?q=a");
    assert_eq!(search_url(Engine::Bing, "a"), "https://www.bing.com/search?q=a");
    assert_eq!(search_url(Engine::Brave, "a"), "https://search.brave.com/search?q=a");
    assert_eq!(search_url(Engine::Startpage, "a"), "https://www.startpage.com/do/search?q=a");
}

#[test]
fn encodes_cyrillic_and_specials() {
    assert_eq!(encode("кіт & #1+1"), "%D0%BA%D1%96%D1%82+%26+%231%2B1");
    assert_eq!(encode("a-b_c.d~e/f?g=h"), "a-b_c.d~e%2Ff%3Fg%3Dh");
}

#[test]
fn query_is_trimmed() {
    assert_eq!(search_url(Engine::DuckDuckGo, "  hi  "), "https://duckduckgo.com/?q=hi");
}

#[test]
fn keys_round_trip_and_off_is_none() {
    for e in ENGINES {
        assert_eq!(Engine::from_key(e.key()), Some(e));
    }
    assert_eq!(Engine::from_key("off"), None);
    assert_eq!(Engine::from_key("altavista"), None);
    assert_eq!(Engine::from_key("DuckDuckGo"), Some(Engine::DuckDuckGo), "case-insensitive");
    assert_eq!(Engine::DuckDuckGo.name(), "DuckDuckGo");
    assert_eq!(ENGINES[0], Engine::DuckDuckGo, "the default comes first");
}

#[test]
fn web_row_text_in_both_languages() {
    use newwin_core::strings::strings_for;
    use newwin_core::web::row_text;
    assert_eq!(row_text(strings_for("en-US"), Engine::DuckDuckGo, " кіт "), "Search DuckDuckGo for “кіт”");
    assert_eq!(row_text(strings_for("uk-UA"), Engine::Google, "cat"), "Шукати «cat» у Google");
    assert_eq!((strings_for("en-US").kind_web, strings_for("uk-UA").kind_web), ("Web", "Інтернет"));
}
