//! NewWin settings: `config.toml` schema and loading.
//!
//! Every field has a default, unknown keys are ignored and out-of-range values are clamped,
//! so a hand-edited file can never stop the bar from starting.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

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
            height: 30,
            margin: 6,
            corner_radius: 11.0,
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
}

impl Default for Items {
    fn default() -> Self {
        Self { right: ["layout", "network", "volume", "search", "date", "time"].map(String::from).to_vec() }
    }
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
        }
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Parse(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "cannot read config: {e}"),
            ConfigError::Parse(e) => write!(f, "invalid config: {e}"),
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
        b.show_delay_ms = b.show_delay_ms.min(2000);
        b.hide_delay_ms = b.hide_delay_ms.min(2000);
        let a = &mut self.appearance;
        a.height = a.height.clamp(24, 48);
        a.margin = a.margin.min(16);
        a.corner_radius = clamp_f(a.corner_radius, 0.0, 16.0, d.corner_radius);
        a.font_size = clamp_f(a.font_size, 10.0, 18.0, d.font_size);
        self
    }
}

/// A missing file yields the defaults; an unreadable or invalid one is an error.
pub fn load(path: &Path) -> Result<Config, ConfigError> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(e) => return Err(ConfigError::Io(e)),
    };
    let config: Config = toml::from_str(&text).map_err(|e| ConfigError::Parse(e.to_string()))?;
    Ok(config.sanitized())
}

/// `%APPDATA%\NewWin\config.toml`
pub fn default_path() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("NewWin").join("config.toml")
}
