//! Quick links (spec: 1.0.6 §2.5): a keyword of the user's that opens a site or a folder.

pub const MAX_KEYWORD: usize = 32;
pub const MAX_TARGET: usize = 1024;
pub const MAX_LINKS: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    /// Lowercase, no spaces.
    pub keyword: String,
    /// A URL or a path (`%VARIABLES%` allowed).
    pub target: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Url,
    Path,
}

/// A site: a scheme, `www.`, or a bare domain like `github.com/x` (a dot, no spaces, no drive,
/// backslash or `%VARIABLE%`). Anything else is a folder or file.
pub fn target_kind(t: &str) -> Target {
    let t = t.trim();
    let domain = t.contains('.') && !t.contains([' ', '\\', ':', '%']) && !t.starts_with(['/', '.']);
    if t.contains("://") || t.to_ascii_lowercase().starts_with("www.") || domain { Target::Url } else { Target::Path }
}

/// What the shell opens for a URL target: `www.x` gets `https://`.
pub fn url_for(target: &str) -> String {
    let t = target.trim();
    if t.contains("://") { t.to_string() } else { format!("https://{t}") }
}

/// A short form for the row's label: the site's host, or the last folder of a path.
pub fn label(target: &str) -> String {
    let t = target.trim();
    if target_kind(t) == Target::Url {
        let rest = match t.split_once("://") {
            Some((scheme, rest)) if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") => rest,
            Some(_) => return t.to_string(),
            None => t,
        };
        let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
        return host.strip_prefix("www.").unwrap_or(host).to_string();
    }
    let trimmed = t.trim_end_matches(['\\', '/']);
    match trimmed.rsplit_once(['\\', '/']) {
        Some((_, last)) if !last.is_empty() => format!("…\\{last}"),
        _ => t.to_string(),
    }
}

/// "keyword address": the first word is the keyword (lowercased), the rest the target.
pub fn parse_entry(line: &str) -> Option<Link> {
    let line = line.trim();
    let (keyword, target) = line.split_once(char::is_whitespace)?;
    let (keyword, target) = (keyword.to_lowercase(), target.trim());
    let ok = !keyword.is_empty() && keyword.chars().count() <= MAX_KEYWORD && !target.is_empty() && target.chars().count() <= MAX_TARGET;
    ok.then(|| Link { keyword, target: target.to_string() })
}
