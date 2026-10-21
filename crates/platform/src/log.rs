//! File logger behind the `log` facade: `newwin.log`, rotated at 1 MB, 3 files kept.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use windows::Win32::System::SystemInformation::GetLocalTime;

const MAX_BYTES: u64 = 1024 * 1024;
const KEEP: usize = 3;

struct FileLogger {
    dir: PathBuf,
    file: Mutex<Option<File>>,
}

impl FileLogger {
    fn path(&self, n: usize) -> PathBuf {
        if n == 0 { self.dir.join("newwin.log") } else { self.dir.join(format!("newwin.{n}.log")) }
    }

    fn rotate(&self) {
        for n in (0..KEEP - 1).rev() {
            let _ = std::fs::rename(self.path(n), self.path(n + 1));
        }
    }

    fn open(&self) -> Option<File> {
        OpenOptions::new().create(true).append(true).open(self.path(0)).ok()
    }
}

impl ::log::Log for FileLogger {
    fn enabled(&self, metadata: &::log::Metadata) -> bool {
        metadata.level() <= ::log::max_level()
    }

    fn log(&self, record: &::log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let t = unsafe { GetLocalTime() };
        let line = format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03} [{}] pid={} {}\n",
            t.wYear,
            t.wMonth,
            t.wDay,
            t.wHour,
            t.wMinute,
            t.wSecond,
            t.wMilliseconds,
            record.level(),
            std::process::id(),
            record.args()
        );
        let Ok(mut guard) = self.file.lock() else { return };
        if guard.as_ref().and_then(|f| f.metadata().ok()).is_some_and(|m| m.len() > MAX_BYTES) {
            *guard = None;
            self.rotate();
        }
        if guard.is_none() {
            *guard = self.open();
        }
        if let Some(f) = guard.as_mut() {
            let _ = f.write_all(line.as_bytes());
        }
    }

    fn flush(&self) {
        if let Ok(mut guard) = self.file.lock()
            && let Some(f) = guard.as_mut()
        {
            let _ = f.flush();
        }
    }
}

pub fn init_logging() -> std::io::Result<()> {
    let logger = FileLogger { dir: crate::paths::log_dir(), file: Mutex::new(None) };
    ::log::set_boxed_logger(Box::new(logger)).map_err(std::io::Error::other)?;
    // NEWWIN_DEBUG=1 adds a detailed event trace for diagnosing problems.
    let debug = std::env::var_os("NEWWIN_DEBUG").is_some_and(|v| v == "1");
    ::log::set_max_level(if debug { ::log::LevelFilter::Debug } else { ::log::LevelFilter::Info });
    Ok(())
}
