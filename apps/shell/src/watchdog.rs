//! Watchdog role: keeps the shell alive and guarantees the native taskbar comes back.

use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::Instant;

use newwin_core::watchdog::{CrashPolicy, ExitDecision, HangMonitor, PingDecision};
use newwin_platform::paths::taskbar_state_file;
use newwin_platform::taskbar::restore_from_state_file;
use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HANDLE, LPARAM, WAIT_TIMEOUT, WPARAM};
use windows::Win32::System::Threading::{CreateMutexW, WaitForSingleObject};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, MB_ICONWARNING, MB_OK, MessageBoxW, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_NULL,
};
use windows::core::{HSTRING, w};

pub use newwin_core::watchdog::PING_WINDOW_CLASS;
const PING_INTERVAL_MS: u32 = 5000;
const PING_TIMEOUT_MS: u32 = 2000;

#[derive(Debug, PartialEq, Eq)]
pub enum SecondLaunch {
    OpenSettings(PathBuf),
    Nothing,
}

/// Starting NewWin.exe while it already runs opens Settings (when it is installed).
pub fn second_launch_action(settings: Option<&Path>) -> SecondLaunch {
    settings.map_or(SecondLaunch::Nothing, |p| SecondLaunch::OpenSettings(p.to_path_buf()))
}

pub fn run_watchdog() -> i32 {
    // Held for the life of the process; a second watchdog exits immediately.
    let _mutex = unsafe { CreateMutexW(None, true, w!("Local\\NewWin.Watchdog")) };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        log::info!("watchdog already running");
        let settings = newwin_platform::launch::settings_exe();
        if let SecondLaunch::OpenSettings(exe) = second_launch_action(settings.as_deref())
            && let Err(e) = newwin_platform::launch::run_settings(&exe)
        {
            log::error!("cannot start {}: {e}", exe.display());
        }
        return 0;
    }

    let state = taskbar_state_file();
    match restore_from_state_file(&state) {
        Ok(true) => log::warn!("restored native taskbar left hidden by a previous run"),
        Ok(false) => {}
        Err(e) => log::error!("restore at startup failed: {e}"),
    }

    let mut crashes = CrashPolicy::new();
    loop {
        let mut child = match spawn_shell() {
            Ok(c) => c,
            Err(e) => {
                log::error!("cannot start shell: {e}");
                give_up(&state);
                return 1;
            }
        };
        log::info!("shell started pid={}", child.id());
        let code = supervise(&mut child);
        let quitting = newwin_platform::processes::event_exists(newwin_core::quit::QUITTING_EVENT);
        match newwin_core::watchdog::while_quitting(crashes.on_exit(code, Instant::now()), quitting) {
            ExitDecision::Stop => {
                log::info!("shell quit, watchdog exiting");
                return 0;
            }
            ExitDecision::Restart => log::warn!("shell exited with code {code:#x}, restarting"),
            ExitDecision::GiveUp => {
                log::error!("shell crashed repeatedly (last code {code:#x}), giving up");
                give_up(&state);
                return 1;
            }
        }
    }
}

fn spawn_shell() -> std::io::Result<Child> {
    Command::new(std::env::current_exe()?).arg("--shell").spawn()
}

/// Waits for the shell to exit, pinging it every 5 s; kills it when it hangs. Returns its exit code.
fn supervise(child: &mut Child) -> u32 {
    let handle = HANDLE(child.as_raw_handle());
    let mut hangs = HangMonitor::new();
    loop {
        if unsafe { WaitForSingleObject(handle, PING_INTERVAL_MS) } != WAIT_TIMEOUT {
            break;
        }
        if hangs.on_ping(ping_shell()) == PingDecision::Kill {
            log::error!("shell not responding, killing it");
            let _ = child.kill();
            break;
        }
    }
    child.wait().ok().and_then(|s| s.code()).map(|c| c as u32).unwrap_or(1)
}

/// True when the shell's UI thread answered, or has not created its window yet.
fn ping_shell() -> bool {
    let Ok(hwnd) = (unsafe { FindWindowW(&HSTRING::from(PING_WINDOW_CLASS), None) }) else { return true };
    let r = unsafe {
        SendMessageTimeoutW(hwnd, WM_NULL, WPARAM(0), LPARAM(0), SMTO_ABORTIFHUNG, PING_TIMEOUT_MS, None)
    };
    r.0 != 0
}

fn give_up(state: &std::path::Path) {
    if let Err(e) = restore_from_state_file(state) {
        log::error!("restore after giving up failed: {e}");
    }
    unsafe {
        MessageBoxW(
            None,
            w!("NewWin stopped after repeated crashes. The Windows taskbar has been restored."),
            w!("NewWin"),
            MB_OK | MB_ICONWARNING,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn a_second_launch_opens_settings_when_present() {
        let p = Path::new(r"C:\x\NewWinSettings.exe");
        assert_eq!(second_launch_action(Some(p)), SecondLaunch::OpenSettings(p.into()));
        assert_eq!(second_launch_action(None), SecondLaunch::Nothing);
    }
}
