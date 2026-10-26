//! Web search from Spotlight (spec: 1.0.6 §2.2): the engines and their search URLs.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    DuckDuckGo,
    Google,
    Bing,
    Brave,
    Startpage,
}

/// In the order Settings lists them; the first is the default.
pub const ENGINES: [Engine; 5] = [Engine::DuckDuckGo, Engine::Google, Engine::Bing, Engine::Brave, Engine::Startpage];

impl Engine {
    /// The config value (`spotlight.web_search`); `"off"` or anything else is no engine.
    pub fn from_key(key: &str) -> Option<Engine> {
        ENGINES.into_iter().find(|e| e.key().eq_ignore_ascii_case(key))
    }

    pub fn key(self) -> &'static str {
        match self {
            Engine::DuckDuckGo => "duckduckgo",
            Engine::Google => "google",
            Engine::Bing => "bing",
            Engine::Brave => "brave",
            Engine::Startpage => "startpage",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Engine::DuckDuckGo => "DuckDuckGo",
            Engine::Google => "Google",
            Engine::Bing => "Bing",
            Engine::Brave => "Brave",
            Engine::Startpage => "Startpage",
        }
    }

    fn base(self) -> &'static str {
        match self {
            Engine::DuckDuckGo => "https://duckduckgo.com/?q=",
            Engine::Google => "https://www.google.com/search?q=",
            Engine::Bing => "https://www.bing.com/search?q=",
            Engine::Brave => "https://search.brave.com/search?q=",
            Engine::Startpage => "https://www.startpage.com/do/search?q=",
        }
    }
}

pub fn search_url(e: Engine, query: &str) -> String {
    format!("{}{}", e.base(), encode(query.trim()))
}

/// Form encoding of a query: unreserved ASCII kept, space as `+`, every other UTF-8 byte `%XX`.
pub fn encode(query: &str) -> String {
    let mut out = String::with_capacity(query.len() * 3);
    for b in query.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The text of the web search row: "Search DuckDuckGo for “cats”".
pub fn row_text(s: &crate::strings::Strings, e: Engine, query: &str) -> String {
    s.web_search_row.replace("{engine}", e.name()).replace("{query}", query.trim())
}
