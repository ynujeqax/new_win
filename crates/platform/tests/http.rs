use newwin_platform::http::get;
use newwin_platform::launch::expand_env;

#[test]
fn only_https_urls() {
    assert!(get("http://bank.gov.ua/", 1000, 1024).is_err());
    assert!(get("not a url", 1000, 1024).is_err());
    assert!(get("ftp://x/", 1000, 1024).is_err());
}

#[test]
fn unreachable_host_fails_fast() {
    let start = std::time::Instant::now();
    assert!(get("https://127.0.0.1:9/", 2000, 1024).is_err());
    assert!(start.elapsed().as_millis() < 5000, "{:?}", start.elapsed());
}

#[test]
fn env_variables_expand() {
    let p = expand_env(r"%SystemRoot%\System32");
    assert!(p.to_ascii_lowercase().ends_with(r"\windows\system32"), "{p}");
    assert_eq!(expand_env(r"C:\plain"), r"C:\plain");
}

/// Online: `cargo test -p newwin-platform --test http -- --ignored`.
#[test]
#[ignore]
fn nbu_live() {
    let body = get("https://bank.gov.ua/NBUStatService/v1/statdirectory/exchange?json", 5000, 262_144).unwrap();
    let r = newwin_core::currency::parse_nbu(&body, "2026-01-01").unwrap();
    assert!(r.per_uah["USD"] > 1.0, "{:?}", r.per_uah.get("USD"));
    assert!(r.per_uah.len() > 30);
}

#[test]
#[ignore]
fn body_over_the_limit_is_an_error() {
    assert!(get("https://bank.gov.ua/NBUStatService/v1/statdirectory/exchange?json", 5000, 100).is_err());
}
