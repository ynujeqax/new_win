//! About (spec: M4 §3.6): the version and three buttons — *Open logs*, *Open config file*,
//! *Restore Windows taskbar*. Nothing here changes the config.

use std::path::Path;

use newwin_config::Config;
use newwin_platform::launch::shell_execute;
use newwin_platform::paths::log_dir;

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

/// `NewWin.exe --quit` next to this exe: ends every NewWin process, this one included, and
/// shows the native taskbar again.
pub fn run_quit(newwin_exe: Option<&Path>) {
    match newwin_exe {
        Some(exe) => {
            log::info!("restoring the Windows taskbar with {} --quit", exe.display());
            if let Err(e) = shell_execute("open", &exe.to_string_lossy(), "--quit") {
                log::error!("cannot run {} --quit: {e}", exe.display());
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
    fn about_shows_version_1_0_1() {
        let rows = rows(&Config::default());
        assert!(matches!(&rows[0].control, Control::Info(v) if v == "1.0.2"));
    }

    #[test]
    fn about_buttons_leave_the_config_alone() {
        let mut c = Config::default();
        for id in [RowId::OpenLogs, RowId::OpenConfig, RowId::RestoreTaskbar] {
            apply(&mut c, id, RowValue::Bool(true));
        }
        assert_eq!(c, Config::default());
    }

}
