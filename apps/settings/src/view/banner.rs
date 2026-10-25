//! The status strip above the section title and how the window picks it.

use super::*;

/// The status strip above the section title (spec §3.7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Banner {
    ParseError { line: Option<usize> },
    /// The reason, without a final full stop.
    SaveFailed(String),
    NotRunning,
}

impl Banner {
    pub fn text(&self, s: &Strings) -> String {
        match self {
            Banner::ParseError { line: Some(n) } => s.banner_parse_error_line.replace("{line}", &n.to_string()),
            Banner::ParseError { line: None } => s.banner_parse_error.to_owned(),
            Banner::SaveFailed(reason) => s.banner_save_failed.replace("{reason}", reason),
            Banner::NotRunning => s.banner_not_running.to_owned(),
        }
    }

    pub fn button(&self, s: &Strings) -> Option<&'static str> {
        match self {
            Banner::ParseError { .. } => Some(s.open_file),
            Banner::SaveFailed(_) => None,
            Banner::NotRunning => Some(s.start_newwin),
        }
    }
}

/// The one banner to show: a failed save first (it is about what the user just did), then a
/// broken file, then a stopped shell.
pub fn pick_banner(save_error: Option<&str>, parse_error: Option<Option<usize>>, shell_running: bool) -> Option<Banner> {
    if let Some(reason) = save_error {
        Some(Banner::SaveFailed(reason.to_owned()))
    } else if let Some(line) = parse_error {
        Some(Banner::ParseError { line })
    } else if !shell_running {
        Some(Banner::NotRunning)
    } else {
        None
    }
}

/// An I/O error as the banner's reason: the system message without "(os error N)" and without
/// its final full stop (the banner adds one).
pub fn save_reason(e: &std::io::Error) -> String {
    let text = e.to_string();
    let text = match text.rfind(" (os error ") {
        Some(i) => text.get(..i).unwrap_or(&text).to_owned(),
        None => text,
    };
    let reason = text.trim().trim_end_matches('.').trim_end();
    if reason.is_empty() { "unknown error".to_owned() } else { reason.to_owned() }
}

/// The parse banner a load result needs (`Some(line)`), or `None` when the file loaded. A file
/// that is there but cannot be read (locked, denied) gets the same banner, without a line: the
/// window never shows silent defaults for an existing file.
pub fn parse_error_of(r: &Result<Config, ConfigError>) -> Option<Option<usize>> {
    match r {
        Ok(_) => None,
        Err(ConfigError::Parse { line, .. }) => Some(*line),
        Err(ConfigError::Io(_)) => Some(None),
    }
}

/// The config to show at start and the banner it needs: a broken or unreadable file shows the
/// defaults and the parse banner, and is left alone until an edit.
pub fn load_for_view(path: &Path) -> (Config, Option<Banner>) {
    let r = newwin_config::load(path);
    match parse_error_of(&r) {
        None => (r.unwrap_or_default(), None),
        Some(line) => {
            if let Err(e) = &r {
                log::warn!("config.toml cannot be used, showing defaults: {e}");
            }
            (Config::default(), Some(Banner::ParseError { line }))
        }
    }
}
