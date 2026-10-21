//! Acting on other apps' windows. Call these synchronously from a click handler: Windows only
//! lets the process that received the last input move the foreground.

use newwin_core::apps::{GroupKey, Hwnd};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::appinfo::{appsfolder_display_name, process_image_path, window_relaunch_command};
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
        let ok = SetForegroundWindow(w).as_bool();
        log::debug!("activate {h:#x}: SetForegroundWindow={ok}");
        if !ok {
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

/// `"C:\Program Files\x.exe" -a` → (`C:\Program Files\x.exe`, `-a`). Unquoted, the whole line is
/// the file when it exists, else it splits at the first space.
fn split_command_line(cmd: &str) -> Option<(String, String)> {
    let cmd = cmd.trim();
    if cmd.is_empty() {
        return None;
    }
    if let Some(rest) = cmd.strip_prefix('"') {
        let (file, args) = rest.split_once('"').unwrap_or((rest, ""));
        return Some((file.to_string(), args.trim().to_string()));
    }
    if std::path::Path::new(cmd).is_file() {
        return Some((cmd.to_string(), String::new()));
    }
    let (file, args) = cmd.split_once(' ').unwrap_or((cmd, ""));
    Some((file.to_string(), args.trim().to_string()))
}

/// What to open for "New window", as (file, arguments). An app id opens through the Start menu
/// only when an entry has it: Chromium browsers tag windows with a per-profile id that no entry
/// has, and opening that does nothing. Then the window's relaunch command is used (Steam windows
/// belong to steamwebhelper.exe but relaunch Steam.exe), else the window's own exe.
pub fn new_instance_target(
    key: &GroupKey,
    exe: Option<&str>,
    relaunch_command: Option<&str>,
    registered: impl Fn(&str) -> bool,
) -> Option<(String, String)> {
    let file = |f: &str| (f.to_string(), String::new());
    match key {
        GroupKey::AppId(id) if registered(id) => Some(file(&format!(r"shell:AppsFolder\{id}"))),
        GroupKey::AppId(_) => relaunch_command.and_then(split_command_line).or_else(|| exe.map(file)),
        GroupKey::ExePath(p) => Some(file(exe.unwrap_or(p))),
    }
}

/// Starts another instance of the app a group belongs to; `window` is one of its windows.
/// Needs COM on the calling thread.
pub fn launch_new_instance(key: &GroupKey, window: Option<Hwnd>) {
    let exe = window.and_then(|h| {
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(hwnd(h), Some(&mut pid)) };
        process_image_path(pid)
    });
    let relaunch = window.and_then(|h| window_relaunch_command(hwnd(h)));
    let registered = |id: &str| appsfolder_display_name(id).is_some();
    let Some((file, args)) = new_instance_target(key, exe.as_deref(), relaunch.as_deref(), registered) else {
        log::error!("cannot start a new instance of {key:?}: no app entry and no exe");
        return;
    };
    log::debug!("new instance of {key:?}: {file} {args}");
    if let Err(e) = shell_execute("open", &file, &args) {
        log::error!("cannot start a new instance of {key:?}: {e}");
    }
}
