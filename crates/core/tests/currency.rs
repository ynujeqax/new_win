use std::collections::BTreeMap;

use newwin_core::currency::{Rates, answer, convert, format_money, parse, parse_nbu, short_date};

fn rates() -> Rates {
    let per_uah: BTreeMap<String, f64> =
        [("UAH", 1.0), ("USD", 41.3), ("EUR", 44.8), ("PLN", 10.4), ("GBP", 53.6), ("JPY", 0.27)].into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    Rates { date: "2026-10-27".into(), fetched_at: 0, per_uah }
}

fn value(q: &str) -> f64 {
    let r = rates();
    let query = parse(q, &|c| r.per_uah.contains_key(c)).unwrap_or_else(|| panic!("{q} did not parse"));
    convert(&query, &r).unwrap()
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 0.01, "{a} vs {b}");
}

#[test]
fn usd_to_uah() {
    close(value("100 usd in uah"), 4130.0);
    assert_eq!(answer("100 usd in uah", &rates()).as_deref(), Some("4\u{2009}130.00 UAH"));
}

#[test]
fn symbols_and_ukrainian() {
    close(value("50 € в грн"), 2240.0);
    close(value("1000 грн в злотих"), 96.15);
    close(value("$20 to eur"), 18.44);
    close(value("20$ to eur"), 18.44);
    close(value("5 доларів у гривні"), 206.5);
    close(value("10 EUR = USD"), 10.85);
    close(value("50zł into ₴"), 520.0);
}

#[test]
fn expression_amount() {
    close(value("2*15 eur to pln"), 129.23);
}

#[test]
fn cross_rate() {
    close(value("10 gbp in jpy"), 1985.19);
}

#[test]
fn units_and_lone_words_are_not_currencies() {
    let r = rates();
    let known = |c: &str| r.per_uah.contains_key(c);
    for q in ["10 m in ft", "23c to f", "5 км в м", "cs2", "usd", "100 usd", "usd in uah", "3dmark", "10 kg in lb"] {
        assert!(parse(q, &known).is_none(), "{q}");
    }
}

#[test]
fn unknown_code_is_none() {
    let r = rates();
    assert!(parse("10 xyz in usd", &|c| r.per_uah.contains_key(c)).is_none());
    assert!(answer("10 chf in usd", &r).is_none(), "CHF is not in these rates");
}

#[test]
fn money_format() {
    assert_eq!(format_money(4130.0, "UAH"), "4\u{2009}130.00 UAH");
    assert_eq!(format_money(0.5, "USD"), "0.50 USD");
    assert_eq!(format_money(1234567.891, "JPY"), "1\u{2009}234\u{2009}567.89 JPY");
    assert_eq!(format_money(-12.346, "EUR"), "-12.35 EUR");
}

#[test]
fn nbu_parser_real_sample() {
    let json = r#"[
{ "r030":840,"txt":"Долар США","rate":41.3012,"cc":"USD","exchangedate":"27.10.2026" }
,{ "r030":978,"txt":"Євро","rate":44.8015,"cc":"EUR","exchangedate":"27.10.2026" }
,{ "r030":985,"txt":"Злотий","rate":10.4,"cc":"PLN","exchangedate":"27.10.2026" }
]"#;
    let r = parse_nbu(json, "2026-01-01").unwrap();
    assert_eq!(r.date, "2026-10-27");
    assert_eq!(r.per_uah["USD"], 41.3012);
    assert_eq!(r.per_uah["UAH"], 1.0);
    assert_eq!(r.per_uah.len(), 4);
}

#[test]
fn nbu_parser_rejects_garbage() {
    for bad in ["", "[]", "<html>blocked</html>", r#"[{"cc":"USD","rate":-1}]"#, r#"[{"cc":"USD","rate":"abc"}]"#, r#"[{"cc":"US","rate":1}]"#] {
        assert!(parse_nbu(bad, "2026-01-01").is_none(), "{bad}");
    }
    // A bad entry is skipped, the good one kept; a missing date falls back.
    let r = parse_nbu(r#"[{"cc":"XXX","rate":0},{"cc":"EUR","rate":44.8}]"#, "2026-01-01").unwrap();
    assert_eq!((r.date.as_str(), r.per_uah.len()), ("2026-01-01", 2));
}

#[test]
fn short_date_label() {
    let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    assert_eq!(short_date("2026-10-27", &months), "27 Oct");
    assert_eq!(short_date("2026-01-05", &months), "5 Jan");
    assert_eq!(short_date("garbage", &months), "");
}

#[test]
fn currency_strings() {
    use newwin_core::strings::strings_for;
    let (en, uk) = (strings_for("en-US"), strings_for("uk-UA"));
    assert_eq!((en.kind_currency, uk.kind_currency), ("Currency", "Валюта"));
    assert_eq!(short_date("2026-10-27", &en.months_short), "27 Oct");
    assert_eq!(short_date("2026-10-27", &uk.months_short), "27 жов");
}
