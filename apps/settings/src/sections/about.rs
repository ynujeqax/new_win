//! About (spec: M4 §3.6): the version and three buttons — *Open logs*, *Open config file*,
//! *Restore Windows taskbar*. Nothing here changes the config.

use std::path::Path;

use newwin_config::Config;
use newwin_core::watchdog::PING_WINDOW_CLASS;
use newwin_platform::launch::shell_execute;
use newwin_platform::paths::log_dir;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_APP};
use windows::core::{HSTRING, PCWSTR};

use super::{Control, Row, RowId, RowValue};

pub fn rows(_c: &Config) -> Vec<Row> {
    vec![
        Row::new(RowId::Version, Control::Info(env!("CARGO_PKG_VERSION").into())),
        Row::new(RowId::OpenLogs, Control::Button),
        Row::new(RowId::OpenConfig, Control::Button),
        Row::new(RowId::RestoreTaskbar, Control::Button),
    ]
}

/// About has no settings.
pub fn apply(_c: &mut Config, _id: RowId, _v: RowValue) {}

/// The shell's quit request (global constraints; the shell side is Task 12).
pub const WM_QUIT_REQUEST: u32 = WM_APP + 20;
/// How long *Restore Windows taskbar* waits for the shell to go before restoring it itself.
pub const RESTORE_WAIT_MS: u64 = 3000;
/// How often the window looks for the shell while it waits (only during those 3 s).
pub const RESTORE_POLL_MS: u32 = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestoreStep {
    Wait,
    /// The shell quit (and restored the taskbar itself).
    Gone,
    /// It is still there after [`RESTORE_WAIT_MS`]: run `NewWin.exe --restore-taskbar`.
    Fallback,
}

/// The wait after the quit request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RestoreWait {
    pub until: u64,
}

impl RestoreWait {
    pub fn start(now: u64) -> RestoreWait {
        RestoreWait { until: now.saturating_add(RESTORE_WAIT_MS) }
    }

    pub fn poll(&self, now: u64, shell_running: bool) -> RestoreStep {
        if !shell_running {
            RestoreStep::Gone
        } else if now >= self.until {
            RestoreStep::Fallback
        } else {
            RestoreStep::Wait
        }
    }
}

/// Asks the shell to quit through its `NewWin.Ping` window; false when there is none.
pub fn request_quit() -> bool {
    let Ok(hwnd) = (unsafe { FindWindowW(&HSTRING::from(PING_WINDOW_CLASS), PCWSTR::null()) }) else { return false };
    match unsafe { PostMessageW(Some(hwnd), WM_QUIT_REQUEST, WPARAM(0), LPARAM(0)) } {
        Ok(()) => true,
        Err(e) => {
            log::warn!("cannot ask NewWin to quit: {e}");
            false
        }
    }
}

/// `NewWin.exe --restore-taskbar` next to this exe: shows the native taskbar again.
pub fn run_restore(newwin_exe: Option<&Path>) {
    match newwin_exe {
        Some(exe) => {
            log::info!("restoring the Windows taskbar with {} --restore-taskbar", exe.display());
            if let Err(e) = shell_execute("open", &exe.to_string_lossy(), "--restore-taskbar") {
                log::error!("cannot run {} --restore-taskbar: {e}", exe.display());
            }
        }
        None => log::error!("cannot restore the taskbar: NewWin.exe is not next to NewWinSettings.exe"),
    }
}

/// *Open logs*: `%LOCALAPPDATA%\NewWin\logs` in Explorer.
pub fn open_logs() {
    let dir = log_dir();
    if let Err(e) = shell_execute("open", &dir.to_string_lossy(), "") {
        log::error!("cannot open {}: {e}", dir.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn about_shows_the_version_and_three_buttons() {
        let r = rows(&Config::default());
        let got: Vec<_> = r.iter().map(|r| (r.id, r.control.clone())).collect();
        assert_eq!(
            got,
            [
                (RowId::Version, Control::Info(env!("CARGO_PKG_VERSION").into())),
                (RowId::OpenLogs, Control::Button),
                (RowId::OpenConfig, Control::Button),
                (RowId::RestoreTaskbar, Control::Button),
            ]
        );
    }

    #[test]
    fn about_shows_version_1_0_0() {
        let rows = rows(&Config::default());
        assert!(matches!(&rows[0].control, Control::Info(v) if v == "1.0.0"));
    }

    #[test]
    fn about_buttons_leave_the_config_alone() {
        let mut c = Config::default();
        for id in [RowId::OpenLogs, RowId::OpenConfig, RowId::RestoreTaskbar] {
            apply(&mut c, id, RowValue::Bool(true));
        }
        assert_eq!(c, Config::default());
    }

    #[test]
    fn restore_waits_up_to_3_s_for_the_shell_then_falls_back() {
        let w = RestoreWait::start(1_000);
        assert_eq!(w.poll(1_100, true), RestoreStep::Wait);
        assert_eq!(w.poll(2_500, false), RestoreStep::Gone);
        assert_eq!(w.poll(3_999, true), RestoreStep::Wait);
        assert_eq!(w.poll(4_000, true), RestoreStep::Fallback);
        assert_eq!(w.poll(4_000, false), RestoreStep::Gone, "gone wins over the deadline");
    }
}
