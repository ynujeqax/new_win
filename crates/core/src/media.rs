//! What is playing (1.0.5): the current media session as the bar shows it.

/// The current media session: Spotify, a browser tab, any player that tells Windows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MediaInfo {
    pub title: String,
    pub artist: String,
    /// The app playing it (its AppUserModelID or exe name), for its icon and name.
    pub app: String,
    pub playing: bool,
    pub can_toggle: bool,
    pub can_prev: bool,
    pub can_next: bool,
}

impl MediaInfo {
    /// Worth showing: a session with a title. Players that only hold the session open report
    /// none.
    pub fn shown(&self) -> bool {
        !self.title.trim().is_empty()
    }

    /// The bar's text: "Title · Artist", or the title alone.
    pub fn label(&self) -> String {
        let (t, a) = (clean(&self.title), clean(&self.artist));
        // Browsers often put the artist into the title already ("Song - Artist").
        if a.is_empty() || t.contains(&a) { t } else { format!("{t} · {a}") }
    }
}

/// Collapses runs of whitespace, trims, and drops a browser's "- YouTube" suffix.
fn clean(s: &str) -> String {
    let s: Vec<&str> = s.split_whitespace().collect();
    let s = s.join(" ");
    s.strip_suffix(" - YouTube").unwrap_or(&s).to_string()
}

/// A media command from the bar or the media panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaCmd {
    PlayPause,
    Next,
    Prev,
}
