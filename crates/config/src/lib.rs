//! NewWin settings: `config.toml` schema and loading.
//!
//! Every field has a default, unknown keys are ignored and out-of-range values are clamped,
//! so a hand-edited file can never stop the bar from starting.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub mod diff;
pub mod usage;

/// Integer fields accept any TOML integer and saturate into the field's type,
/// so `-5` or `300` in a `u8` field is clamped later instead of rejecting the file.
mod sat {
    use serde::{Deserialize, Deserializer};

    pub fn u8<'de, D: Deserializer<'de>>(d: D) -> Result<u8, D::Error> {
        Ok(i64::deserialize(d)?.clamp(0, i64::from(u8::MAX)) as u8)
    }

    pub fn u32<'de, D: Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
        Ok(i64::deserialize(d)?.clamp(0, i64::from(u32::MAX)) as u32)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BarMode {
    Pinned,
    AutoHide,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct BarConfig {
    pub mode: BarMode,
    #[serde(deserialize_with = "sat::u32")]
    pub show_delay_ms: u32,
    #[serde(deserialize_with = "sat::u32")]
    pub hide_delay_ms: u32,
    #[serde(deserialize_with = "sat::u8")]
    pub trigger_height_px: u8,
    pub ignore_fullscreen: bool,
}

impl Default for BarConfig {
    fn default() -> Self {
        Self { mode: BarMode::AutoHide, show_delay_ms: 150, hide_delay_ms: 400, trigger_height_px: 1, ignore_fullscreen: true }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Appearance {
    #[serde(deserialize_with = "sat::u32")]
    pub height: u32,
    #[serde(deserialize_with = "sat::u32")]
    pub margin: u32,
    pub corner_radius: f32,
    /// Liquid glass: blur what is under the bar (false: tint only).
    pub glass: bool,
    pub font_family: String,
    pub font_size: f32,
    pub date_format: String,
    pub show_seconds: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            height: 36,
            margin: 6,
            corner_radius: 18.0,
            glass: true,
            font_family: "Segoe UI Variable Text".into(),
            font_size: 13.0,
            date_format: "system".into(),
            show_seconds: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Items {
    pub right: Vec<String>,
    pub hidden: Vec<String>,
}

/// The known right-side item names, in default order.
pub const ITEM_NAMES: [&str; 6] = ["layout", "network", "volume", "search", "date", "time"];

impl Default for Items {
    fn default() -> Self {
        Self { right: ITEM_NAMES.map(String::from).to_vec(), hidden: Vec::new() }
    }
}

/// Tray icons by exe file name (compared case-insensitively).
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct TrayConfig {
    pub order: Vec<String>,
    pub hidden: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct SpotlightConfig {
    pub win_key: bool,
    #[serde(deserialize_with = "sat::u32")]
    pub max_results: u32,
}

impl Default for SpotlightConfig {
    fn default() -> Self {
        Self { win_key: true, max_results: 8 }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct SystemConfig {
    pub start_with_windows: bool,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self { start_with_windows: true }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    #[serde(deserialize_with = "sat::u32")]
    pub version: u32,
    pub bar: BarConfig,
    pub appearance: Appearance,
    pub items: Items,
    pub spotlight: SpotlightConfig,
    pub system: SystemConfig,
    pub tray: TrayConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            bar: BarConfig::default(),
            appearance: Appearance::default(),
            items: Items::default(),
            spotlight: SpotlightConfig::default(),
            system: SystemConfig::default(),
            tray: TrayConfig::default(),
        }
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    /// `line` is 1-based, when the parser could say where.
    Parse { line: Option<usize>, message: String },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "cannot read config: {e}"),
            ConfigError::Parse { line: Some(l), message } => write!(f, "invalid config (line {l}): {message}"),
            ConfigError::Parse { line: None, message } => write!(f, "invalid config: {message}"),
        }
    }
}

/// NaN falls back to the default; everything else is clamped into range.
fn clamp_f(v: f32, lo: f32, hi: f32, default: f32) -> f32 {
    if v.is_nan() { default } else { v.clamp(lo, hi) }
}

impl Config {
    pub fn sanitized(mut self) -> Self {
        let d = Appearance::default();
        let b = &mut self.bar;
        b.trigger_height_px = b.trigger_height_px.clamp(1, 2);
        b.show_delay_ms = b.show_delay_ms.min(1000);
        b.hide_delay_ms = b.hide_delay_ms.min(2000);
        let a = &mut self.appearance;
        a.height = a.height.clamp(28, 48);
        a.margin = a.margin.min(16);
        a.corner_radius = clamp_f(a.corner_radius, 0.0, 24.0, d.corner_radius);
        a.font_size = clamp_f(a.font_size, 10.0, 18.0, d.font_size);
        self.spotlight.max_results = self.spotlight.max_results.clamp(3, 15);
        normalize_items(&mut self.items);
        dedupe_ci(&mut self.tray.order);
        dedupe_ci(&mut self.tray.hidden);
        self
    }
}

/// Known names only, first occurrence wins, missing ones appended in default order;
/// `hidden` keeps only names that are in `right`.
fn normalize_items(items: &mut Items) {
    let mut right: Vec<String> = Vec::with_capacity(ITEM_NAMES.len());
    for n in &items.right {
        if ITEM_NAMES.contains(&n.as_str()) && !right.contains(n) {
            right.push(n.clone());
        }
    }
    for n in ITEM_NAMES {
        if !right.iter().any(|r| r == n) {
            right.push(n.to_string());
        }
    }
    let mut hidden: Vec<String> = Vec::new();
    for n in &items.hidden {
        if right.contains(n) && !hidden.contains(n) {
            hidden.push(n.clone());
        }
    }
    items.right = right;
    items.hidden = hidden;
}

/// Trims, drops empty entries and duplicates (case-insensitive), keeping the first spelling.
fn dedupe_ci(list: &mut Vec<String>) {
    let mut seen: Vec<String> = Vec::with_capacity(list.len());
    let mut out: Vec<String> = Vec::with_capacity(list.len());
    for n in list.iter() {
        let t = n.trim();
        if t.is_empty() {
            continue;
        }
        let key = t.to_lowercase();
        if !seen.contains(&key) {
            seen.push(key);
            out.push(t.to_string());
        }
    }
    *list = out;
}

/// A missing file yields the defaults; an unreadable or invalid one is an error.
pub fn load(path: &Path) -> Result<Config, ConfigError> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(e) => return Err(ConfigError::Io(e)),
    };
    let config: Config = toml::from_str(&text).map_err(|e| {
        let line = e.span().map(|sp| text.get(..sp.start).unwrap_or("").matches('\n').count() + 1);
        ConfigError::Parse { line, message: e.message().to_string() }
    })?;
    Ok(config.sanitized())
}

/// `%APPDATA%\NewWin\config.toml`
pub fn default_path() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("NewWin").join("config.toml")
}

/// `%APPDATA%\NewWin\usage.toml`
pub fn usage_path() -> PathBuf {
    default_path().with_file_name("usage.toml")
}

/// `%APPDATA%\NewWin\tray_seen.toml`
pub fn tray_seen_path() -> PathBuf {
    default_path().with_file_name("tray_seen.toml")
}

const CONFIG_HEADER: &str = "# NewWin settings. NewWinSettings.exe rewrites this file.";

/// The config file text: a header line and pretty TOML.
pub fn to_toml(c: &Config) -> String {
    let body = toml::to_string_pretty(c).unwrap_or_default();
    format!("{CONFIG_HEADER}\n\n{body}")
}

/// Writes `text` next to `path` and renames it over, so a crash never leaves half a file.
pub fn save_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

/// Writes the default settings to `path` unless a file is already there (to open it for
/// editing before the settings window exists).
pub fn write_default_if_missing(path: &Path) -> std::io::Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    save_atomic(path, &to_toml(&Config::default()))
}
