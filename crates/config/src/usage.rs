//! `usage.toml`: how often each Spotlight entry was launched (spec: M3 §2.6).

use std::collections::BTreeMap;
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Usage {
    launches: BTreeMap<String, u32>,
}

impl Usage {
    /// A missing file is empty; an unreadable or invalid one is empty too, with the error to log.
    /// One bad value drops only its own entry.
    pub fn load(path: &Path) -> (Usage, Option<String>) {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (Usage::default(), None),
            Err(e) => return (Usage::default(), Some(e.to_string())),
        };
        let table: toml::Table = match text.parse() {
            Ok(t) => t,
            Err(e) => return (Usage::default(), Some(e.to_string())),
        };
        let mut launches = BTreeMap::new();
        if let Some(toml::Value::Table(t)) = table.get("launches") {
            for (k, v) in t {
                if let Some(n) = v.as_integer().and_then(|n| u32::try_from(n).ok()) {
                    launches.insert(k.clone(), n);
                }
            }
        }
        (Usage { launches }, None)
    }

    pub fn get(&self, key: &str) -> u32 {
        self.launches.get(key).copied().unwrap_or(0)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, u32)> {
        self.launches.iter().map(|(k, &n)| (k.as_str(), n))
    }

    pub fn bump(&mut self, key: &str) {
        let n = self.launches.entry(key.to_string()).or_insert(0);
        *n = n.saturating_add(1);
    }

    pub fn retain(&mut self, keep: impl Fn(&str) -> bool) {
        self.launches.retain(|k, _| keep(k));
    }

    pub fn to_toml(&self) -> String {
        let mut t = toml::Table::new();
        let launches: toml::Table = self.launches.iter().map(|(k, &n)| (k.clone(), toml::Value::Integer(i64::from(n)))).collect();
        t.insert("launches".into(), toml::Value::Table(launches));
        t.to_string()
    }
}

pub use crate::save_atomic;
