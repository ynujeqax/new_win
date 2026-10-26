use newwin_core::links::{Link, Target, label, parse_entry, target_kind, url_for};

#[test]
fn kinds() {
    assert_eq!(target_kind("https://github.com"), Target::Url);
    assert_eq!(target_kind("www.example.com"), Target::Url);
    assert_eq!(target_kind("steam://open/games"), Target::Url);
    assert_eq!(target_kind(r"C:\Users\me\Downloads"), Target::Path);
    assert_eq!(target_kind(r"%USERPROFILE%\Downloads"), Target::Path);
}

#[test]
fn labels() {
    assert_eq!(label("https://github.com/ynujeqax/new_win"), "github.com");
    assert_eq!(label("https://www.github.com"), "github.com");
    assert_eq!(label("www.example.com/x"), "example.com");
    assert_eq!(label(r"C:\Users\me\Downloads"), r"…\Downloads");
    assert_eq!(label(r"%USERPROFILE%\Downloads\"), r"…\Downloads");
    assert_eq!(label(r"D:\"), r"D:\");
    assert_eq!(label("steam://open/games"), "steam://open/games");
}

#[test]
fn urls() {
    assert_eq!(url_for("www.example.com"), "https://www.example.com");
    assert_eq!(url_for("https://x.org"), "https://x.org");
}

#[test]
fn entries() {
    assert_eq!(parse_entry("gh  https://github.com"), Some(Link { keyword: "gh".into(), target: "https://github.com".into() }));
    assert_eq!(parse_entry("  GH x  "), Some(Link { keyword: "gh".into(), target: "x".into() }));
    assert_eq!(parse_entry(r"dl C:\My Files\Downloads"), Some(Link { keyword: "dl".into(), target: r"C:\My Files\Downloads".into() }));
    assert_eq!(parse_entry("onlyword"), None);
    assert_eq!(parse_entry(""), None);
    assert_eq!(parse_entry(&format!("{} x", "a".repeat(33))), None);
    assert_eq!(parse_entry(&format!("k {}", "x".repeat(1025))), None);
}

#[test]
fn link_label_strings() {
    use newwin_core::strings::strings_for;
    assert_eq!((strings_for("en-US").kind_link, strings_for("uk-UA").kind_link), ("Link", "Посилання"));
}

#[test]
fn a_bare_domain_is_a_url() {
    assert_eq!(target_kind("github.com"), Target::Url);
    assert_eq!(target_kind("github.com/ynujeqax"), Target::Url);
    assert_eq!(url_for("github.com"), "https://github.com");
    assert_eq!(label("github.com/x"), "github.com");
    assert_eq!(target_kind(r"D:\Games"), Target::Path);
    assert_eq!(target_kind(r"\nas\share"), Target::Path);
    assert_eq!(target_kind(r"%USERPROFILE%\Downloads"), Target::Path);
    assert_eq!(target_kind("my folder"), Target::Path, "no dot: not a site");
}
