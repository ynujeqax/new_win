#![windows_subsystem = "windows"]
// A misspelled or unimported message constant in a `match` arm silently becomes a catch-all binding.
#![deny(non_snake_case)]

mod app;
mod bar;
mod quit;
mod roles;
mod watchdog;

use newwin_platform::paths::taskbar_state_file;
use newwin_platform::taskbar::restore_from_state_file;
use windows::Win32::System::Diagnostics::Debug::{EXCEPTION_POINTERS, SetUnhandledExceptionFilter};

use roles::{Role, role_from_args};

/// `EXCEPTION_CONTINUE_SEARCH`: let Windows terminate the process after we restored the taskbar.
const CONTINUE_SEARCH: i32 = 0;

unsafe extern "system" fn on_unhandled_exception(info: *const EXCEPTION_POINTERS) -> i32 {
    let code = unsafe { info.as_ref().and_then(|i| i.ExceptionRecord.as_ref()).map(|r| r.ExceptionCode.0) };
    log::error!("unhandled exception {code:#x?}, restoring taskbar");
    let _ = restore_from_state_file(&taskbar_state_file());
    log::logger().flush();
    CONTINUE_SEARCH
}

fn install_crash_handlers() {
    std::panic::set_hook(Box::new(|info| {
        log::error!("panic: {info}");
        let _ = restore_from_state_file(&taskbar_state_file());
        log::logger().flush();
    }));
    unsafe {
        SetUnhandledExceptionFilter(Some(on_unhandled_exception));
    }
}

fn main() {
    let _ = newwin_platform::log::init_logging();
    install_crash_handlers();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match role_from_args(&args) {
        Role::Watchdog => watchdog::run_watchdog(),
        Role::Shell => app::run_shell(),
        Role::Quit => quit::run_quit(),
        Role::RestoreTaskbar => match restore_from_state_file(&taskbar_state_file()) {
            Ok(_) => 0,
            Err(e) => {
                log::error!("restore failed: {e}");
                1
            }
        },
    };
    log::logger().flush();
    std::process::exit(code);
}
