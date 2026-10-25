//! `NewWin.exe --quit` (M5 spec §3): no NewWin process left, native taskbar shown.

use std::time::Instant;

use newwin_core::quit::{QUIT_POLL_MS, QUITTING_EVENT, QuitPlan, QuitStep};
use newwin_core::watchdog::PING_WINDOW_CLASS;
use newwin_platform::paths::taskbar_state_file;
use newwin_platform::processes::{close_windows_of, create_event, other_newwin_pids, settings_pids, terminate};
use newwin_platform::taskbar::{native_taskbar_visible, restore_from_state_file, show_native};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::Sleep;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW};
use windows::core::{HSTRING, PCWSTR};

use crate::app::WM_APP_QUIT;

pub fn run_quit() -> i32 {
    // Held until we exit: the watchdog sees it and does not restart the shell.
    let _quitting = create_event(QUITTING_EVENT);
    if let Ok(hwnd) = unsafe { FindWindowW(&HSTRING::from(PING_WINDOW_CLASS), PCWSTR::null()) } {
        let _ = unsafe { PostMessageW(Some(hwnd), WM_APP_QUIT, WPARAM(0), LPARAM(0)) };
        log::info!("quit: asked the shell");
    }
    for pid in settings_pids() {
        close_windows_of(pid);
    }

    let start = Instant::now();
    let mut plan = QuitPlan::new(0);
    let code = loop {
        let now = start.elapsed().as_millis() as u64;
        match plan.step(now, !other_newwin_pids().is_empty()) {
            QuitStep::Wait => unsafe { Sleep(QUIT_POLL_MS) },
            QuitStep::Kill => {
                for pid in other_newwin_pids() {
                    let ok = terminate(pid);
                    log::warn!("quit: killed pid={pid} ok={ok}");
                }
                unsafe { Sleep(QUIT_POLL_MS) };
            }
            QuitStep::Restore { code } => break code,
        }
    };

    match restore_from_state_file(&taskbar_state_file()) {
        Ok(true) => {}
        Ok(false) if !native_taskbar_visible() => {
            if let Err(e) = show_native() {
                log::error!("quit: cannot show the taskbar: {e}");
            }
        }
        Ok(false) => {}
        Err(e) => log::error!("quit: restore failed: {e}"),
    }
    log::info!("quit: done code={code}");
    code
}
