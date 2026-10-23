//! `tray_seen.toml`: the apps that have had a tray icon, for the settings window's tray list.

use std::path::Path;

use serde::{Deserialize, Serialize};

const MAX_APPS: usize = 200;

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct SeenApp {
    pub exe: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct TraySeen {
    #[serde(rename = "app")]
    pub apps: Vec<SeenApp>,
}

impl TraySeen {
    /// A missing file is empty; an unreadable or invalid one is empty too, with the error to log.
    pub fn load(path: &Path) -> (TraySeen, Option<String>) {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (TraySeen::default(), None),
            Err(e) => return (TraySeen::default(), Some(e.to_string())),
        };
        match toml::from_str::<TraySeen>(&text) {
            Ok(mut s) => {
                s.apps.truncate(MAX_APPS);
                (s, None)
            }
            Err(e) => (TraySeen::default(), Some(e.to_string())),
        }
    }

    /// True when something changed: a new app (at the end; past 200 the oldest goes) or a new name.
    pub fn note(&mut self, exe: &str, name: &str) -> bool {
        if let Some(a) = self.apps.iter_mut().find(|a| a.exe.eq_ignore_ascii_case(exe)) {
            if a.name == name {
                return false;
            }
            a.name = name.to_string();
            return true;
        }
        self.apps.push(SeenApp { exe: exe.to_string(), name: name.to_string() });
        if self.apps.len() > MAX_APPS {
            let extra = self.apps.len() - MAX_APPS;
            self.apps.drain(..extra);
        }
        true
    }

    pub fn to_toml(&self) -> String {
        toml::to_string(self).unwrap_or_default()
    }
}
