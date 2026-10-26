//! `rates.toml`: the last currency rates downloaded from the NBU (spec: 1.0.6 §2.3), so currency
//! answers work offline and at once after a restart.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use newwin_core::currency::Rates;

/// `%LOCALAPPDATA%\NewWin\rates.toml`: a cache, next to the logs.
pub fn rates_path() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("NewWin").join("rates.toml")
}

/// The cached rates; `None` when the file is missing, unreadable or has no usable rate.
pub fn load(path: &Path) -> Option<Rates> {
    let table: toml::Table = std::fs::read_to_string(path).ok()?.parse().ok()?;
    let date = table.get("date")?.as_str()?.to_string();
    let fetched_at = table.get("fetched_at").and_then(|v| v.as_integer()).and_then(|n| u64::try_from(n).ok()).unwrap_or(0);
    let mut per_uah: BTreeMap<String, f64> = table
        .get("rates")?
        .as_table()?
        .iter()
        .filter_map(|(k, v)| {
            let rate = v.as_float().or_else(|| v.as_integer().map(|n| n as f64))?;
            (k.len() == 3 && rate.is_finite() && rate > 0.0).then(|| (k.clone(), rate))
        })
        .filter(|(k, _)| k != "UAH")
        .collect();
    if per_uah.is_empty() {
        return None;
    }
    per_uah.insert("UAH".into(), 1.0);
    Some(Rates { date, fetched_at, per_uah })
}

pub fn to_toml(r: &Rates) -> String {
    let mut t = toml::Table::new();
    t.insert("date".into(), toml::Value::String(r.date.clone()));
    t.insert("fetched_at".into(), toml::Value::Integer(i64::try_from(r.fetched_at).unwrap_or(i64::MAX)));
    let rates: toml::Table = r.per_uah.iter().map(|(k, &v)| (k.clone(), toml::Value::Float(v))).collect();
    t.insert("rates".into(), toml::Value::Table(rates));
    t.to_string()
}
