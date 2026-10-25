//! App icons (1.0.1 spec): the image format, where an icon comes from, and the cache that keeps
//! the ones in use. No Win32 here; `newwin_platform::app_icon` fetches the pixels.

use std::collections::HashMap;

use crate::apps::GroupKey;
use crate::spotlight::{Candidate, Kind};

/// The icon's side and its gap to the text, in logical px.
pub const ICON_LOGICAL_PX: f32 = 16.0;
pub const ICON_GAP_LOGICAL_PX: f32 = 6.0;
/// How long a freshly arrived icon takes to show.
pub const ICON_FADE_MS: u64 = 150;
pub const CACHE_CAPACITY: usize = 128;

/// Premultiplied BGRA, rows top to bottom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconImage {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

/// Straight BGRA to premultiplied, in place.
pub fn premultiply(bgra: &mut [u8]) {
    for px in bgra.as_chunks_mut::<4>().0 {
        let a = u32::from(px[3]);
        for c in &mut px[..3] {
            *c = ((u32::from(*c) * a + 127) / 255) as u8;
        }
    }
}

/// An image with no alpha at all (an old icon format) is meant to be opaque.
pub fn opaque_if_no_alpha(bgra: &mut [u8]) {
    if bgra.as_chunks::<4>().0.iter().all(|px| px[3] == 0) {
        for px in bgra.as_chunks_mut::<4>().0 {
            px[3] = 255;
        }
    }
}

/// What an icon is cached by; the size is part of it (a DPI change asks again).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum IconKey {
    /// As Explorer shows it: a file path or a `shell:AppsFolder\…` name.
    Shell { source: String, px: u32 },
    /// The `hIcon` an app gave the tray; a new handle is a new icon.
    Tray { hicon: u32, px: u32 },
}

/// Where the bar's app group takes its icon from.
pub fn source_for_group(key: &GroupKey) -> String {
    match key {
        GroupKey::AppId(id) => format!(r"shell:AppsFolder\{id}"),
        GroupKey::ExePath(path) => path.clone(),
    }
}

/// Where a Spotlight result takes its icon from; system commands have none.
pub fn source_for_candidate(c: &Candidate) -> Option<String> {
    match c.kind {
        Kind::System(_) => None,
        _ => Some(format!(r"shell:AppsFolder\{}", c.id)),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lookup<T> {
    Ready(T),
    /// There is no icon; it is not asked for again.
    Missing,
    /// Unknown until now: the caller fetches it and calls [`IconCache::fill`].
    Ask,
    /// Already being fetched.
    Waiting,
}

#[derive(Clone)]
enum State<T> {
    Waiting,
    Ready(T),
    Missing,
}

/// The icons in use, the least recently used going first past [`CACHE_CAPACITY`].
pub struct IconCache<T: Clone> {
    entries: HashMap<IconKey, (State<T>, u64)>,
    tick: u64,
}

impl<T: Clone> IconCache<T> {
    pub fn new() -> Self {
        Self { entries: HashMap::new(), tick: 0 }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&mut self, key: &IconKey) -> Lookup<T> {
        self.tick += 1;
        if let Some((state, used)) = self.entries.get_mut(key) {
            *used = self.tick;
            return match state {
                State::Waiting => Lookup::Waiting,
                State::Ready(v) => Lookup::Ready(v.clone()),
                State::Missing => Lookup::Missing,
            };
        }
        self.insert(key.clone(), State::Waiting);
        Lookup::Ask
    }

    /// The fetched icon, or `None` when there is none.
    pub fn fill(&mut self, key: IconKey, value: Option<T>) {
        self.tick += 1;
        let state = value.map_or(State::Missing, State::Ready);
        match self.entries.get_mut(&key) {
            Some(e) => *e = (state, self.tick),
            None => self.insert(key, state),
        }
    }

    /// After a re-index: files that had no icon may have one now. Tray handles do not come back.
    pub fn forget_missing_shell(&mut self) {
        self.entries.retain(|k, (s, _)| !(matches!(k, IconKey::Shell { .. }) && matches!(s, State::Missing)));
    }

    fn insert(&mut self, key: IconKey, state: State<T>) {
        if self.entries.len() >= CACHE_CAPACITY
            && let Some(oldest) = self.entries.iter().min_by_key(|(_, (_, used))| *used).map(|(k, _)| k.clone())
        {
            self.entries.remove(&oldest);
        }
        self.entries.insert(key, (state, self.tick));
    }
}

impl<T: Clone> Default for IconCache<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// How visible an icon is `now_ms`, having arrived at `arrived_ms`: 0 → 1 over [`ICON_FADE_MS`].
pub fn fade(arrived_ms: u64, now_ms: u64) -> f32 {
    (now_ms.saturating_sub(arrived_ms) as f32 / ICON_FADE_MS as f32).clamp(0.0, 1.0)
}
