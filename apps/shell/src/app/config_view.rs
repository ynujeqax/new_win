//! Live config (spec: M4 §2, §4.2): the shell watches `%APPDATA%\NewWin`, re-reads `config.toml`
//! after a short debounce and applies only what changed.

use std::sync::atomic::Ordering;

use newwin_config::diff::changes;
use newwin_config::usage::Usage;
use newwin_config::{Config, ConfigError, Items, default_path, load, usage_path};
use newwin_core::visibility::VisConfig;
use newwin_platform::autostart::autostart_set;
use newwin_platform::dir_watch::DirWatch;
use newwin_ui::TextCache;
use newwin_ui::backdrop::Backdrops;
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer, WM_APP};

use super::App;

/// Something in `%APPDATA%\NewWin` was written, created, renamed or deleted (watch → ping).
pub(super) const WM_APP_CONFIG: u32 = WM_APP + 19;
/// One-shot: one save makes several directory events; the file is read once they stop.
pub(super) const CONFIG_DEBOUNCE_TIMER: usize = 11;
pub(super) const CONFIG_DEBOUNCE_MS: u32 = 50;

/// What to apply after a reload: `None` when the file is broken (the last good config stays) or
/// says nothing new. A missing file loads as the defaults, which then apply.
pub(super) fn next_config(current: &Config, loaded: Result<Config, ConfigError>) -> Option<Config> {
    loaded.ok().filter(|c| c != current)
}

/// `usage.toml` was deleted from outside: it was on disk (loaded, or saved by us) and is gone now.
/// Counts not yet written to a file that never existed stay.
pub(super) fn usage_deleted(on_disk: bool, exists: bool) -> bool {
    on_disk && !exists
}

/// Whether to log "config file missing": only when it goes from present to missing, not on
/// every event in the folder.
pub(super) fn config_went_missing(was_present: bool, exists: bool) -> bool {
    was_present && !exists
}

/// The right items the bar shows: `right` in its order, minus `hidden`.
pub(super) fn visible_items(items: &Items) -> Vec<String> {
    items.right.iter().filter(|n| !items.hidden.contains(n)).cloned().collect()
}

/// Brings the Run value in line with the config; a failure is logged, never fatal.
pub(super) fn sync_autostart(on: bool) {
    if let Err(e) = autostart_set(on) {
        log::error!("start with Windows ({on}): {e}");
    }
}

impl App {
    /// Watches the config folder (created if missing, so the watch has something to watch).
    pub(super) fn start_config_watch(&mut self) {
        let path = default_path();
        let Some(dir) = path.parent() else { return };
        if let Err(e) = std::fs::create_dir_all(dir) {
            log::error!("config folder {}: {e}", dir.display());
        }
        self.config_watch = DirWatch::start(dir, self.ping, WM_APP_CONFIG).map_err(|e| log::error!("config watch: {e}")).ok();
    }

    /// A directory event: (re)starts the one-shot debounce; re-arming an id restarts its count.
    pub(super) fn on_config_dir_changed(&mut self) {
        unsafe { SetTimer(Some(self.bar), CONFIG_DEBOUNCE_TIMER, CONFIG_DEBOUNCE_MS, None) };
    }

    /// The debounce ran out (`on_timer` already killed it): re-read and apply.
    pub(super) fn reload_config(&mut self) {
        let path = default_path();
        let loaded = load(&path);
        let exists = path.exists();
        match &loaded {
            Err(ConfigError::Parse { line, message }) => {
                let line = line.map_or_else(|| "?".to_string(), |n| n.to_string());
                log::info!("config error at line {line}, keeping the last good one: {message}");
            }
            Err(ConfigError::Io(e)) => log::info!("config error at line ?, keeping the last good one: {e}"),
            Ok(_) if config_went_missing(self.config_on_disk, exists) => log::info!("config file missing: defaults"),
            Ok(_) => {}
        }
        self.config_on_disk = exists;
        if let Some(new) = next_config(&self.config, loaded) {
            self.apply_config(new);
        }
        self.reset_usage_if_deleted();
    }

    /// `usage.toml` deleted by hand (or by Settings' reset): forget the counts too. Nothing is
    /// written; the next launch saves as usual. Counts never written yet (no file so far) stay.
    pub(super) fn reset_usage_if_deleted(&mut self) {
        let on_disk = self.usage_on_disk.load(Ordering::Acquire);
        if !usage_deleted(on_disk, usage_path().exists()) {
            return;
        }
        self.usage_on_disk.store(false, Ordering::Release);
        log::info!("usage.toml deleted: launch counts reset");
        unsafe {
            let _ = KillTimer(Some(self.bar), super::spotlight_view::USAGE_SAVE_TIMER);
        }
        self.usage = Usage::default();
    }

    /// Stores `new` and runs only the actions its changes need, in a fixed order (geometry →
    /// glass → texts → visibility → the rest), with one paint at the end.
    pub(super) fn apply_config(&mut self, new: Config) {
        let c = changes(&self.config, &new);
        if !c.any() {
            // Only `version` differs.
            self.config = new;
            return;
        }
        log::info!("config applied: {c:?}");
        self.config = new;
        if c.items {
            self.hide_tooltip();
            self.hover = None;
            self.right.set_items(&visible_items(&self.config.items));
            self.right.hover_to(None, newwin_platform::clock::now_ms());
        }
        if c.fonts {
            // Old formats are never asked for again: drop them.
            self.text = TextCache::new(&self.gfx);
        }
        if c.geometry || c.fonts || c.items {
            self.reposition();
        }
        if c.glass {
            self.set_glass(self.config.appearance.glass);
        }
        // Hiding or showing the time changes the clock cadence too.
        if c.clock || c.items {
            self.tick_clock();
        }
        if c.visibility {
            let b = &self.config.bar;
            let cfg = VisConfig { mode: self.mode(), show_delay_ms: b.show_delay_ms, hide_delay_ms: b.hide_delay_ms, ignore_fullscreen: b.ignore_fullscreen };
            let cmds = self.vis.as_mut().map(|v| v.set_config(cfg)).unwrap_or_default();
            self.exec(cmds);
            // A new trigger height resizes the strip.
            self.set_catcher(self.catcher_mode);
            if c.mode {
                self.reserve_space();
            }
        }
        if c.win_key
            && let Some(h) = &self.input_hook
        {
            h.set_win_key(self.config.spotlight.win_key);
        }
        if c.tray {
            self.refresh_tray();
        }
        // c.spotlight: read on the next open.
        if c.autostart {
            sync_autostart(self.config.system.start_with_windows);
        }
        self.paint();
    }

    /// Starts or stops the glass thread; the bar shows the plain tint until a first frame comes.
    fn set_glass(&mut self, on: bool) {
        self.glass_sent.clear();
        self.bar_glass = false;
        if on {
            if self.backdrops.is_none() {
                self.backdrops = Some(Backdrops::spawn(self.ping, super::WM_APP_GLASS));
            }
        } else {
            self.backdrops = None;
            if let Some(layer) = self.bar_layer.as_mut() {
                let _ = layer.clear_backdrop();
            }
        }
        self.sync_glass();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use newwin_config::{Config, ConfigError};

    fn good() -> Config {
        let mut c = Config::default();
        c.appearance.height = 44;
        c
    }

    #[test]
    fn a_broken_file_keeps_the_last_good_config() {
        let parse_err = ConfigError::Parse { line: Some(3), message: "expected `=`".into() };
        assert_eq!(next_config(&good(), Err(parse_err)), None);
        let io_err = ConfigError::Io(std::io::Error::other("sharing violation"));
        assert_eq!(next_config(&good(), Err(io_err)), None);
    }

    #[test]
    fn a_deleted_file_means_defaults() {
        assert_eq!(next_config(&good(), Ok(Config::default())), Some(Config::default()));
    }

    #[test]
    fn an_unchanged_file_does_nothing() {
        assert_eq!(next_config(&good(), Ok(good())), None);
    }

    #[test]
    fn debounce_restarts_on_each_event() {
        assert_eq!(CONFIG_DEBOUNCE_TIMER, 11);
        assert_eq!(CONFIG_DEBOUNCE_MS, 50);
    }

    #[test]
    fn unsaved_counts_survive_until_the_file_was_there_and_is_gone() {
        assert!(!usage_deleted(false, false), "never written yet: the pending counts stay");
        assert!(!usage_deleted(true, true));
        assert!(usage_deleted(true, false), "on disk before, gone now: deleted from outside");
        assert!(!usage_deleted(false, true));
    }

    #[test]
    fn a_missing_config_is_logged_once() {
        assert!(config_went_missing(true, false));
        assert!(!config_went_missing(false, false), "still missing: no new line per event");
        assert!(!config_went_missing(true, true));
        assert!(!config_went_missing(false, true));
    }

    #[test]
    fn hidden_items_are_left_out_in_order() {
        let mut c = Config::default();
        c.items.right = ["time", "date", "search"].map(String::from).to_vec();
        c.items.hidden = vec!["date".into()];
        assert_eq!(visible_items(&c.items), vec!["time".to_string(), "search".to_string()]);
    }
}
