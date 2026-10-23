//! The config writer (spec: M4 §2): edits are held as `pending` and written once the 100 ms
//! debounce timer fires, atomically; the text last written is remembered so the window's own
//! write is not taken for an outside change when the directory watch reports it.

use std::path::Path;

use newwin_config::Config;

#[derive(Debug, Default)]
pub struct Writer {
    pub pending: Option<Config>,
    pub last_written: Option<String>,
}

/// True when `text` has a line other than blanks and `#` comments: a header alone means
/// serialization failed, and writing it would wipe the user's settings.
fn has_settings(text: &str) -> bool {
    text.lines().map(str::trim).any(|l| !l.is_empty() && !l.starts_with('#'))
}

impl Writer {
    /// Holds `c` (within bounds) for the next [`Writer::flush`], replacing an earlier edit. True:
    /// (re)arm the debounce timer.
    pub fn edit(&mut self, c: Config) -> bool {
        self.pending = Some(c.sanitized());
        true
    }

    /// Writes the pending edit, if any. On an error the edit stays pending (the next edit or
    /// flush tries again) and the file is left as it was.
    pub fn flush(&mut self, path: &Path) -> std::io::Result<()> {
        let Some(c) = self.pending.as_ref() else { return Ok(()) };
        let text = newwin_config::to_toml(c);
        self.write(path, text)
    }

    fn write(&mut self, path: &Path, text: String) -> std::io::Result<()> {
        if !has_settings(&text) {
            log::error!("config serialization produced no settings; not writing {}", path.display());
            return Err(std::io::Error::other("the settings could not be written out"));
        }
        newwin_config::save_atomic(path, &text)?;
        self.pending = None;
        self.last_written = Some(text);
        Ok(())
    }

    /// The file holds exactly what this window wrote last.
    pub fn is_own(&self, file_text: &str) -> bool {
        self.last_written.as_deref() == Some(file_text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A fresh directory per test; the file inside does not exist yet.
    fn temp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("newwin-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("config.toml")
    }

    fn mtime(path: &Path) -> std::time::SystemTime {
        std::fs::metadata(path).unwrap().modified().unwrap()
    }

    #[test]
    fn debounce_writes_the_last_value_once() {
        let path = temp_file("debounce");
        let mut w = Writer::default();
        let (mut a, mut b) = (Config::default(), Config::default());
        a.appearance.height = 40;
        b.appearance.height = 44;
        assert!(w.edit(a));
        assert!(w.edit(b)); // both arm (re-arming restarts the timer)
        w.flush(&path).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("height = 44"));
        let first = mtime(&path);
        std::thread::sleep(std::time::Duration::from_millis(30));
        w.flush(&path).unwrap(); // nothing pending: no second write (mtime unchanged)
        assert_eq!(mtime(&path), first);
        assert!(w.pending.is_none());
    }

    #[test]
    fn own_write_is_not_a_change() {
        let path = temp_file("own");
        let mut w = Writer::default();
        let mut c = Config::default();
        c.bar.show_delay_ms = 250;
        w.edit(c);
        w.flush(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(w.is_own(&text));
        let edited = text.replace("show_delay_ms = 250", "show_delay_ms = 300");
        std::fs::write(&path, &edited).unwrap();
        assert!(!w.is_own(&std::fs::read_to_string(&path).unwrap()));
    }

    #[test]
    fn a_broken_file_is_not_overwritten_until_an_edit() {
        let path = temp_file("broken");
        let broken = "[bar\nmode = \"pinned\"\n";
        std::fs::write(&path, broken).unwrap();
        let (config, banner) = crate::view::load_for_view(&path);
        assert_eq!(config, Config::default());
        assert_eq!(banner, Some(crate::view::Banner::ParseError { line: Some(1) }));
        let mut w = Writer::default();
        w.flush(&path).unwrap(); // the timer may fire without an edit: nothing is written
        assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);
        w.edit(config);
        w.flush(&path).unwrap();
        assert!(newwin_config::load(&path).is_ok());
    }

    #[test]
    fn a_text_without_settings_is_never_written() {
        let path = temp_file("bodyless");
        std::fs::write(&path, "[bar]\nmode = \"pinned\"\n").unwrap();
        let mut w = Writer { pending: Some(Config::default()), last_written: None };
        assert!(w.write(&path, "# NewWin settings. NewWinSettings.exe rewrites this file.\n\n".into()).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[bar]\nmode = \"pinned\"\n");
        assert!(w.pending.is_some(), "the edit stays pending");
        assert!(w.last_written.is_none());
    }

    #[test]
    fn a_failed_save_keeps_the_edit_pending() {
        let path = temp_file("failed");
        // A directory where the file should be: the rename over it fails.
        std::fs::create_dir_all(&path).unwrap();
        let mut w = Writer::default();
        w.edit(Config::default());
        assert!(w.flush(&path).is_err());
        assert!(w.pending.is_some());
    }
}
