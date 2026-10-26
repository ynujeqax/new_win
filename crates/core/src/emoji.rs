//! Emoji for Spotlight's `:` queries (spec: 1.0.6 §2.4): names and keywords in English and
//! Ukrainian from Unicode CLDR, built by `scripts/gen-emoji.ps1` into `data/emoji.txt`.

use std::cmp::Reverse;
use std::sync::OnceLock;

const DATA: &str = include_str!("../data/emoji.txt");

#[derive(Debug, PartialEq, Eq)]
pub struct Emoji {
    pub glyph: &'static str,
    pub en: &'static str,
    pub uk: &'static str,
}

/// Lowercased search text of one emoji.
struct Text {
    /// Names (en, uk).
    names: [String; 2],
    /// Keywords of both languages.
    keywords: Vec<String>,
}

struct Table {
    emoji: Vec<Emoji>,
    /// Parallel to `emoji`.
    text: Vec<Text>,
}

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| {
        let (mut emoji, mut text) = (Vec::new(), Vec::new());
        for line in DATA.lines() {
            let f: Vec<&'static str> = line.split('\t').collect();
            let &[glyph, en, en_kw, uk, uk_kw] = f.as_slice() else { continue };
            let keywords = en_kw.split('|').chain(uk_kw.split('|')).filter(|k| !k.is_empty()).map(str::to_lowercase).collect();
            emoji.push(Emoji { glyph, en, uk });
            text.push(Text { names: [en.to_lowercase(), uk.to_lowercase()], keywords });
        }
        Table { emoji, text }
    })
}

/// Every emoji, in Unicode's order.
pub fn all() -> &'static [Emoji] {
    &table().emoji
}

pub fn usage_key(e: &Emoji) -> String {
    format!("emoji:{}", e.glyph)
}

fn boundary(c: Option<char>) -> bool {
    c.is_none_or(|c| matches!(c, ' ' | ':' | '-' | ',' | '’' | '\''))
}

/// `q` in `s` from a word start; `whole`: up to a word end as well.
fn at_word(s: &str, q: &str, whole: bool) -> bool {
    s.match_indices(q).any(|(i, _)| boundary(s[..i].chars().next_back()) && (!whole || boundary(s[i + q.len()..].chars().next())))
}

/// How well `e` matches the folded query, best first: 0 a name is the query, 1 the query is a
/// whole word of a name or a keyword, 2 a name, a word of one or a keyword starts with it, 3 it
/// is inside a name or keyword.
fn level(e: &Text, q: &str) -> Option<u8> {
    if e.names.iter().any(|n| n == q) {
        Some(0)
    } else if e.names.iter().any(|n| at_word(n, q, true)) || e.keywords.iter().any(|k| k == q) {
        Some(1)
    } else if e.names.iter().any(|n| at_word(n, q, false)) || e.keywords.iter().any(|k| k.starts_with(q)) {
        Some(2)
    } else if e.names.iter().chain(&e.keywords).any(|s| s.contains(q)) {
        Some(3)
    } else {
        None
    }
}

/// The most used emoji: among equal matches they come first ("серце" gives the red heart before
/// the heart with an arrow, which Unicode lists earlier).
const POPULAR: &[&str] = &[
    "❤️", "😂", "🤣", "👍", "🙏", "😭", "😊", "😍", "🔥", "🥰", "😘", "✅", "🎉", "💯", "😁", "🙂", "😉", "👏", "💪", "🤔", "😅", "😎", "👌",
    "🙌", "✨", "💔", "😢", "😡", "🤷", "🤦", "👀", "🥲", "⭐", "🇺🇦",
];

/// Up to `max` emoji for `query` (without the leading `:`): best match first, then the ones
/// copied more often, then the most used ones (most used first), then Unicode's order (common emoji first).
pub fn search(query: &str, max: usize, launches: &dyn Fn(&str) -> u32) -> Vec<&'static Emoji> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let t = table();
    type Key = (u8, Reverse<u32>, usize, usize);
    let mut found: Vec<(Key, &'static Emoji)> =
        t.text.iter().enumerate().filter_map(|(i, text)| level(text, &q).map(|l| ((l, Reverse(0), POPULAR.iter().position(|p| *p == t.emoji[i].glyph).unwrap_or(usize::MAX), i), &t.emoji[i]))).collect();
    // Launch counts only for the few that matched: no key strings for the whole table.
    for (key, e) in &mut found {
        key.1 = Reverse(launches(&usage_key(e)));
    }
    found.sort_by_key(|(k, _)| *k);
    found.into_iter().take(max).map(|(_, e)| e).collect()
}
