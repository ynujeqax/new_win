//! Currency conversion in Spotlight (spec: 1.0.6 §2.3): "100 usd in uah", "50 € в грн". Rates are
//! the National Bank of Ukraine's (UAH per unit); cross rates go through UAH. Unit conversions are
//! tried before this, so "10 m in ft" never reaches it.

use std::collections::BTreeMap;

use crate::calc::eval;

/// Official rates: UAH per one unit of each currency, `UAH` = 1.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rates {
    /// The day the rates are for, "2026-10-27".
    pub date: String,
    /// When they were downloaded, Unix seconds.
    pub fetched_at: u64,
    pub per_uah: BTreeMap<String, f64>,
}

/// Names and symbols people type, all lowercase forms, to ISO codes.
const NAMES: &[(&str, &[&str])] = &[
    ("USD", &["$", "dollar", "dollars", "бакс", "бакси", "баксів", "долар", "долари", "доларів", "долара", "доларах"]),
    ("EUR", &["€", "euro", "euros", "євро"]),
    ("UAH", &["₴", "hryvnia", "hryvnias", "hryvnas", "грн", "гривня", "гривні", "гривень", "гривню", "гривнях"]),
    ("PLN", &["zł", "zloty", "złoty", "zlotys", "злотий", "злотих", "злоті", "злотого"]),
    ("GBP", &["£", "pound", "pounds", "фунт", "фунти", "фунтів"]),
    ("CHF", &["franc", "francs", "франк", "франки", "франків"]),
    ("JPY", &["¥", "yen", "єна", "єни", "єн"]),
    ("CZK", &["koruna", "korunas", "крона", "крони", "крон"]),
    ("TRY", &["lira", "liras", "ліра", "ліри", "лір"]),
    ("CNY", &["yuan", "юань", "юані", "юанів"]),
];

/// The ISO code for a currency's name or symbol ("долари" → USD); codes themselves are checked by
/// the caller against the rates.
pub fn currency_code(word: &str) -> Option<&'static str> {
    let w = word.trim().to_lowercase();
    NAMES.iter().find(|(_, names)| names.contains(&w.as_str())).map(|(code, _)| *code)
}

fn resolve(word: &str, known: &dyn Fn(&str) -> bool) -> Option<String> {
    if let Some(code) = currency_code(word) {
        return Some(code.to_string());
    }
    let w = word.trim();
    let code = w.to_ascii_uppercase();
    (w.len() == 3 && w.chars().all(|c| c.is_ascii_alphabetic()) && known(&code)).then_some(code)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Query {
    pub amount: f64,
    pub from: String,
    pub to: String,
}

/// The amount and its currency: "100 usd", "20$", "$20", "€ 5*3".
fn amount_and_currency(left: &str, known: &dyn Fn(&str) -> bool) -> Option<(f64, String)> {
    let left = left.trim();
    let is_num = |c: char| c.is_ascii_digit() || c == ')';
    // The currency after the number: everything after the last digit.
    if let Some(end) = left.char_indices().rev().find(|&(_, c)| is_num(c)).map(|(i, c)| i + c.len_utf8()) {
        let (amount, word) = left.split_at(end);
        if !word.trim().is_empty() {
            return Some((eval(amount)?, resolve(word, known)?));
        }
    }
    // Or before it: everything up to the first digit, bracket or sign.
    let start = left.find(|c: char| c.is_ascii_digit() || matches!(c, '(' | '-' | '.' | ','))?;
    let (word, amount) = left.split_at(start);
    if word.trim().is_empty() {
        return None;
    }
    Some((eval(amount)?, resolve(word, known)?))
}

/// "<amount> <currency> in|to|в|у|=|into <currency>"; `known` says whether a 3-letter code has a
/// rate.
pub fn parse(query: &str, known: &dyn Fn(&str) -> bool) -> Option<Query> {
    let lower = query.trim().to_lowercase();
    let (left, right) = [" in ", " to ", " в ", " у ", " = ", " into "].iter().find_map(|sep| lower.rsplit_once(sep))?;
    let to = resolve(right, known)?;
    let (amount, from) = amount_and_currency(left, known)?;
    Some(Query { amount, from, to })
}

pub fn convert(q: &Query, r: &Rates) -> Option<f64> {
    let (from, to) = (r.per_uah.get(&q.from)?, r.per_uah.get(&q.to)?);
    let v = q.amount * from / to;
    v.is_finite().then_some(v)
}

/// The answer text for a currency query, when the rates have both currencies: "4 130.00 UAH".
pub fn answer(query: &str, r: &Rates) -> Option<String> {
    let q = parse(query, &|c| r.per_uah.contains_key(c))?;
    Some(format_money(convert(&q, r)?, &q.to))
}

/// Whether a query reads as a currency conversion at all (any 3-letter code counts): worth
/// fetching rates for when there are none yet.
pub fn looks_like_currency(query: &str) -> bool {
    parse(query, &|_| true).is_some()
}

/// Two decimals, thin spaces between thousands: "1 234.50 USD".
pub fn format_money(v: f64, code: &str) -> String {
    let s = format!("{:.2}", v.abs());
    let (int, frac) = s.split_once('.').unwrap_or((&s, "00"));
    let mut grouped = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            grouped.push('\u{2009}');
        }
        grouped.push(c);
    }
    let sign = if v < 0.0 && s != "0.00" { "-" } else { "" };
    format!("{sign}{grouped}.{frac} {code}")
}

/// "2026-10-27" → "27 Oct" with the given month names; "" when the date is not one.
pub fn short_date(date: &str, months: &[&str; 12]) -> String {
    let mut parts = date.split('-');
    let (_, m, d) = (parts.next(), parts.next().and_then(|m| m.parse::<usize>().ok()), parts.next().and_then(|d| d.parse::<u32>().ok()));
    match (m, d) {
        (Some(m @ 1..=12), Some(d @ 1..=31)) => format!("{d} {}", months[m - 1]),
        _ => String::new(),
    }
}

/// The value of `"key":` in one JSON object's text: a string's contents or a bare number.
fn field<'a>(obj: &'a str, key: &str) -> Option<&'a str> {
    let at = obj.find(&format!("\"{key}\""))? + key.len() + 2;
    let rest = obj[at..].trim_start().strip_prefix(':')?.trim_start();
    match rest.strip_prefix('"') {
        Some(s) => Some(&s[..s.find('"')?]),
        None => Some(rest[..rest.find([',', '}']).unwrap_or(rest.len())].trim()),
    }
}

/// NBU's `exchange?json`: a flat array of `{"cc":"USD","rate":41.3,"exchangedate":"27.10.2026",…}`.
/// Entries without a 3-letter code or a positive rate are skipped; `None` when none is left.
/// `today` is the date when the answer has none.
pub fn parse_nbu(json: &str, today: &str) -> Option<Rates> {
    let body = json.trim().strip_prefix('[')?.strip_suffix(']')?;
    let mut per_uah = BTreeMap::new();
    let mut date = None;
    for obj in body.split('}').filter_map(|o| o.split_once('{').map(|(_, o)| o)) {
        let Some(cc) = field(obj, "cc").filter(|c| c.len() == 3 && c.chars().all(|c| c.is_ascii_uppercase())) else { continue };
        let Some(rate) = field(obj, "rate").and_then(|r| r.parse::<f64>().ok()).filter(|r| r.is_finite() && *r > 0.0) else { continue };
        per_uah.insert(cc.to_string(), rate);
        if date.is_none()
            && let Some(d) = field(obj, "exchangedate")
            && let [dd, mm, yyyy] = d.split('.').collect::<Vec<_>>()[..]
        {
            date = Some(format!("{yyyy}-{mm}-{dd}"));
        }
    }
    if per_uah.is_empty() {
        return None;
    }
    per_uah.insert("UAH".into(), 1.0);
    Some(Rates { date: date.unwrap_or_else(|| today.to_string()), fetched_at: 0, per_uah })
}
