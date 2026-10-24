//! `NewWinSettings.exe` (spec: M4 §2, §4.1, §5): the settings window. One instance; it writes
//! `config.toml` and never touches the taskbar, `usage.toml` or `tray_seen.toml`.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// A misspelled or unimported message constant in a `match` arm silently becomes a catch-all binding.
#![deny(non_snake_case)]

mod body;
mod font_list;
mod fonts;
mod sections;
mod view;
mod writer;

use std::cell::RefCell;
use std::rc::Rc;

use newwin_core::strings::strings_for;
use newwin_platform::appinfo::ComGuard;
use newwin_platform::clock::ui_language_tag;
use newwin_platform::dir_watch::DirWatch;
use newwin_platform::window::run_message_loop;
use newwin_ui::Gfx;
use newwin_ui::app_window::AppWindow;
use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, IsIconic, SW_RESTORE, SetForegroundWindow, ShowWindow};
use windows::core::{PCWSTR, w};

use view::{CLASS, DIR_CHANGED, SIZE, SettingsView, TITLE};
use writer::Writer;

fn main() {
    let _ = newwin_platform::log::init_logging_to("settings");
    std::panic::set_hook(Box::new(|info| {
        log::error!("panic: {info}");
        log::logger().flush();
    }));
    let code = run();
    log::logger().flush();
    std::process::exit(code);
}

/// Brings the running instance's window forward (restored if minimized).
fn focus_running() {
    match unsafe { FindWindowW(w!("NewWin.Settings"), PCWSTR::null()) } {
        Ok(hwnd) => unsafe {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            let _ = SetForegroundWindow(hwnd);
        },
        Err(_) => log::info!("settings already running, its window is not up yet"),
    }
}

fn run() -> i32 {
    // Held for the life of the process.
    let _mutex = unsafe { CreateMutexW(None, true, w!("Local\\NewWin.Settings")) };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        log::info!("settings already running, focusing it");
        focus_running();
        return 0;
    }
    // ShellExecute (Open file, Start NewWin) wants COM on the UI thread.
    let _com = ComGuard::init();
    let path = newwin_config::default_path();
    let writer = Rc::new(RefCell::new(Writer::default()));
    let gfx = match Gfx::new() {
        Ok(g) => g,
        Err(e) => {
            log::error!("settings: no graphics device: {e}");
            return 1;
        }
    };
    let view = SettingsView::new(&gfx, strings_for(&ui_language_tag()), path.clone(), writer.clone());
    let win = match AppWindow::create(&gfx, view, CLASS, TITLE, SIZE) {
        Ok(w) => w,
        Err(e) => {
            log::error!("settings window failed: {e}");
            return 1;
        }
    };
    let hwnd = win.hwnd();
    win.update(|v| v.hwnd = hwnd);
    let watch = path.parent().and_then(|dir| {
        let _ = std::fs::create_dir_all(dir);
        DirWatch::start(dir, hwnd, DIR_CHANGED).map_err(|e| log::warn!("cannot watch {}: {e}", dir.display())).ok()
    });
    log::info!("settings started");
    let code = run_message_loop();
    drop(watch);
    // An edit made less than 100 ms before closing is still pending.
    if let Err(e) = writer.borrow_mut().flush(&path) {
        log::error!("cannot save {} on exit: {e}", path.display());
    }
    drop(win);
    code
}
