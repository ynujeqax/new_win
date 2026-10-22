//! Spotlight search (spec: M3 §2.2): match levels, ranking, wrong-layout queries.

use std::cmp::Reverse;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchLevel {
    Prefix,
    WordStart,
    Substring,
    Subsequence,
}

/// How a name matched; `ranges` are UTF-16 (start, len) in the name, to draw in bold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub level: MatchLevel,
    pub ranges: Vec<(usize, usize)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SystemCommand {
    Settings,
    ShutDown,
    Restart,
    Sleep,
    Lock,
    SignOut,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    App,
    System(SystemCommand),
}

/// One thing Spotlight can find: an app from `shell:AppsFolder` or a system command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub name: String,
    /// The AppsFolder parsing name (AUMID or path) of an app.
    pub id: String,
    pub kind: Kind,
    /// The file the app's shortcut points at, when that is a file on disk.
    pub file_path: Option<String>,
    /// A packaged (UWP) app.
    pub packaged: bool,
}

impl Candidate {
    /// The key of the launch count in `usage.toml`.
    pub fn usage_key(&self) -> String {
        match self.kind {
            Kind::App => self.id.clone(),
            Kind::System(c) => format!(
                "system:{}",
                match c {
                    SystemCommand::Settings => "settings",
                    SystemCommand::ShutDown => "shut_down",
                    SystemCommand::Restart => "restart",
                    SystemCommand::Sleep => "sleep",
                    SystemCommand::Lock => "lock",
                    SystemCommand::SignOut => "sign_out",
                }
            ),
        }
    }

    pub fn can_run_as_admin(&self) -> bool {
        self.kind == Kind::App && !self.packaged && self.file_path.is_some()
    }
}

/// A result row: `index` into the candidates, with the ranges to draw in bold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub index: usize,
    pub ranges: Vec<(usize, usize)>,
}

fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

struct Folded {
    chars: Vec<char>,
    /// UTF-16 offset of each char, plus the total at the end.
    offsets: Vec<usize>,
    word_start: Vec<bool>,
}

fn folded(name: &str) -> Folded {
    let raw: Vec<char> = name.chars().collect();
    let mut offsets = Vec::with_capacity(raw.len() + 1);
    let mut at = 0;
    for c in &raw {
        offsets.push(at);
        at += c.len_utf16();
    }
    offsets.push(at);
    let word_start = (0..raw.len())
        .map(|i| {
            i == 0 || {
                let prev = raw[i - 1];
                prev.is_whitespace() || matches!(prev, '-' | '_' | '.' | '(') || (raw[i].is_uppercase() && prev.is_lowercase())
            }
        })
        .collect();
    Folded { chars: raw.into_iter().map(fold).collect(), offsets, word_start }
}

impl Folded {
    fn at(&self, i: usize, q: &[char]) -> bool {
        self.chars.get(i..i + q.len()).is_some_and(|s| s == q)
    }

    fn range(&self, from: usize, to: usize) -> (usize, usize) {
        (self.offsets[from], self.offsets[to] - self.offsets[from])
    }

    /// Positions of each query char in order, using only positions `allowed` admits.
    fn subsequence(&self, q: &[char], allowed: impl Fn(usize) -> bool) -> Option<Vec<usize>> {
        let mut out = Vec::with_capacity(q.len());
        let mut i = 0;
        for &c in q {
            i = (i..self.chars.len()).find(|&j| self.chars[j] == c && allowed(j))?;
            out.push(i);
            i += 1;
        }
        Some(out)
    }
}

/// `query` is folded already and not empty.
fn match_folded(name: &Folded, q: &[char]) -> Option<Match> {
    let n = q.len();
    if name.at(0, q) {
        return Some(Match { level: MatchLevel::Prefix, ranges: vec![name.range(0, n)] });
    }
    let len = name.chars.len();
    if let Some(i) = (1..len).find(|&i| name.word_start[i] && name.at(i, q)) {
        return Some(Match { level: MatchLevel::WordStart, ranges: vec![name.range(i, i + n)] });
    }
    if let Some(i) = (1..len).find(|&i| name.at(i, q)) {
        return Some(Match { level: MatchLevel::Substring, ranges: vec![name.range(i, i + n)] });
    }
    let pos = name.subsequence(q, |j| name.word_start[j]).or_else(|| name.subsequence(q, |_| true))?;
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for p in pos {
        match runs.last_mut() {
            Some((_, end)) if *end == p => *end = p + 1,
            _ => runs.push((p, p + 1)),
        }
    }
    for (from, to) in runs {
        ranges.push(name.range(from, to));
    }
    Some(Match { level: MatchLevel::Subsequence, ranges })
}

pub fn match_name(name: &str, query: &str) -> Option<Match> {
    let q: Vec<char> = query.chars().map(fold).collect();
    if q.is_empty() {
        return None;
    }
    match_folded(&folded(name), &q)
}

/// The Ukrainian layout's letters on the keys of the US one (same position, same index).
const LATIN: &str = "`qwertyuiop[]asdfghjkl;'zxcvbnm,./\\";
const CYRILLIC: &str = "'йцукенгшщзхїфівапролджєячсмитьбю.ґ";

fn swap_char(c: char, from: &[char], to: &[char]) -> Option<char> {
    let lower = fold(c);
    let i = from.iter().position(|&f| f == lower)?;
    let out = to[i];
    Some(if c != lower { out.to_uppercase().next().unwrap_or(out) } else { out })
}

/// The query as it would read typed in the other layout (`ENG` ↔ `UKR`); `None` when it has no
/// letter of either layout.
pub fn swap_layout(query: &str) -> Option<String> {
    let latin: Vec<char> = LATIN.chars().collect();
    let cyrillic: Vec<char> = CYRILLIC.chars().collect();
    let first = query.chars().map(fold).find(|c| c.is_alphabetic() && (latin.contains(c) || cyrillic.contains(c)))?;
    let (from, to) = if cyrillic.contains(&first) { (&cyrillic, &latin) } else { (&latin, &cyrillic) };
    Some(query.chars().map(|c| swap_char(c, from, to).unwrap_or(c)).collect())
}

fn rank_one(query: &str, cands: &[Candidate], launches: &dyn Fn(&str) -> u32) -> Vec<Hit> {
    let q: Vec<char> = query.chars().map(fold).collect();
    if q.is_empty() {
        return Vec::new();
    }
    let mut found: Vec<(MatchLevel, Reverse<u32>, bool, Vec<char>, usize, Vec<(usize, usize)>)> = Vec::new();
    for (i, c) in cands.iter().enumerate() {
        let name = folded(&c.name);
        if let Some(m) = match_folded(&name, &q) {
            let system = matches!(c.kind, Kind::System(_));
            found.push((m.level, Reverse(launches(&c.usage_key())), system, name.chars, i, m.ranges));
        }
    }
    found.sort_by(|a, b| (a.0, a.1, a.2, &a.3, a.4).cmp(&(b.0, b.1, b.2, &b.3, b.4)));
    found.into_iter().map(|f| Hit { index: f.4, ranges: f.5 }).collect()
}

/// Up to `max` results for `query`: direct matches first, then matches of the query typed in
/// the other layout. `launches` gives the launch count for a usage key.
pub fn rank(query: &str, cands: &[Candidate], launches: &dyn Fn(&str) -> u32, max: usize) -> Vec<Hit> {
    let query = query.trim();
    let mut hits = rank_one(query, cands, launches);
    if let Some(swapped) = swap_layout(query).filter(|s| s != query) {
        for h in rank_one(&swapped, cands, launches) {
            if !hits.iter().any(|x| x.index == h.index) {
                hits.push(h);
            }
        }
    }
    hits.truncate(max);
    hits
}
