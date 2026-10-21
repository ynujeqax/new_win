//! Acting on other apps' windows. Call these synchronously from a click handler: Windows only
//! lets the process that received the last input move the foreground.

use newwin_core::apps::{GroupKey, Hwnd};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::launch::shell_execute;

fn hwnd(h: Hwnd) -> HWND {
    HWND(h as usize as *mut _)
}

pub fn activate(h: Hwnd) {
    let w = hwnd(h);
    unsafe {
        if IsIconic(w).as_bool() {
            let _ = ShowWindow(w, SW_RESTORE);
        }
        if !SetForegroundWindow(w).as_bool() {
            // Foreground lock refused us; this is what the native taskbar falls back to.
            SwitchToThisWindow(w, true);
        }
    }
}

/// Posted rather than forced, so the app runs its own minimize logic (e.g. "minimize to tray").
pub fn minimize(h: Hwnd) {
    unsafe {
        let _ = PostMessageW(Some(hwnd(h)), WM_SYSCOMMAND, WPARAM(SC_MINIMIZE as usize), LPARAM(0));
    }
}

pub fn restore(h: Hwnd) {
    let w = hwnd(h);
    unsafe {
        let _ = ShowWindow(w, SW_RESTORE);
        let _ = SetForegroundWindow(w);
    }
}

/// Asks the window to close, exactly like its own close button.
pub fn close(h: Hwnd) {
    unsafe {
        let _ = PostMessageW(Some(hwnd(h)), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}

/// Starts another instance of the app a group belongs to.
pub fn launch_new_instance(key: &GroupKey, exe: Option<&str>) {
    let r = match (key, exe) {
        (GroupKey::AppId(id), _) => shell_execute("open", &format!(r"shell:AppsFolder\{id}"), ""),
        (GroupKey::ExePath(p), None) => shell_execute("open", p, ""),
        (GroupKey::ExePath(_), Some(e)) => shell_execute("open", e, ""),
    };
    if let Err(e) = r {
        log::error!("cannot start a new instance of {key:?}: {e}");
    }
}
