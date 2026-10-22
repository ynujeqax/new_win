//! Hiding the native Windows taskbar and putting it back.
//!
//! The original auto-hide setting is written to a state file *before* anything changes,
//! so a crash at any point can be healed by `restore_from_state_file`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::Shell::{ABM_GETSTATE, ABM_SETSTATE, ABS_ALWAYSONTOP, ABS_AUTOHIDE, APPBARDATA, SHAppBarMessage};
use windows::Win32::UI::WindowsAndMessaging::{
    EVENT_OBJECT_SHOW, EnumWindows, GetClassNameW, GetWindowThreadProcessId, IsWindowVisible, OBJID_WINDOW,
    PostMessageW, RegisterWindowMessageW, SW_HIDE, SW_SHOWNA, ShowWindow, WINEVENT_OUTOFCONTEXT,
};
use windows::core::{BOOL, Result, w};

const PRIMARY_CLASS: &str = "Shell_TrayWnd";
const SECONDARY_CLASS: &str = "Shell_SecondaryTrayWnd";

pub struct NativeTaskbar {
    autohide_was_on: bool,
    state_file: PathBuf,
}

impl NativeTaskbar {
    pub fn hide(state_file: &Path) -> Result<Self> {
        let primary = primary_tray()?;
        // A leftover state file means a previous hide was never restored: the current
        // auto-hide state is ours, the original is in the file.
        let autohide_was_on = match read_state(state_file) {
            Some(saved) => saved,
            None => {
                let saved = autohide_on(primary);
                write_state(state_file, saved).map_err(|e| windows::core::Error::new(windows::core::HRESULT(-1), e.to_string()))?;
                saved
            }
        };
        let tb = Self { autohide_was_on, state_file: state_file.to_path_buf() };
        tb.reapply()?;
        Ok(tb)
    }

    /// Hides again after explorer restarted or showed the taskbar on its own.
    pub fn reapply(&self) -> Result<()> {
        let primary = primary_tray()?;
        set_autohide(primary, true);
        for hwnd in all_trays() {
            unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
        }
        Ok(())
    }

    pub fn restore(self) -> Result<()> {
        restore_native(self.autohide_was_on)?;
        let _ = std::fs::remove_file(&self.state_file);
        Ok(())
    }
}

/// Returns true if a state file existed and the taskbar was restored from it.
pub fn restore_from_state_file(state_file: &Path) -> Result<bool> {
    let Some(was_on) = read_state(state_file) else { return Ok(false) };
    restore_native(was_on)?;
    let _ = std::fs::remove_file(state_file);
    Ok(true)
}

pub fn native_taskbar_visible() -> bool {
    primary_tray().is_ok_and(|h| unsafe { IsWindowVisible(h).as_bool() })
}

pub fn taskbar_created_message() -> u32 {
    unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) }
}

fn restore_native(autohide_was_on: bool) -> Result<()> {
    let primary = primary_tray()?;
    set_autohide(primary, autohide_was_on);
    for hwnd in all_trays() {
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNA);
        }
    }
    Ok(())
}

fn primary_tray() -> Result<HWND> {
    explorer_tray().ok_or_else(|| windows::core::Error::new(windows::core::HRESULT(-1), "explorer taskbar not found"))
}

fn pid_of(hwnd: HWND) -> u32 {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

/// Explorer's primary taskbar. NewWin's own tray host has the same class, so windows of this
/// process are skipped (`FindWindow` would return ours first).
pub fn explorer_tray() -> Option<HWND> {
    unsafe extern "system" fn find(hwnd: HWND, lp: LPARAM) -> BOOL {
        let out = unsafe { &mut *(lp.0 as *mut Option<HWND>) };
        if class_name(hwnd) == PRIMARY_CLASS && pid_of(hwnd) != unsafe { GetCurrentProcessId() } {
            *out = Some(hwnd);
            return false.into();
        }
        true.into()
    }
    let mut found = None;
    unsafe {
        let _ = EnumWindows(Some(find), LPARAM(&mut found as *mut _ as isize));
    }
    found
}

fn all_trays() -> Vec<HWND> {
    unsafe extern "system" fn collect(hwnd: HWND, lp: LPARAM) -> BOOL {
        let out = unsafe { &mut *(lp.0 as *mut Vec<HWND>) };
        let class = class_name(hwnd);
        if (class == PRIMARY_CLASS || class == SECONDARY_CLASS) && pid_of(hwnd) != unsafe { GetCurrentProcessId() } {
            out.push(hwnd);
        }
        true.into()
    }
    let mut out: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut out as *mut _ as isize));
    }
    out
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 64];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn appbar_data(hwnd: HWND) -> APPBARDATA {
    APPBARDATA { cbSize: size_of::<APPBARDATA>() as u32, hWnd: hwnd, ..Default::default() }
}

fn autohide_on(tray: HWND) -> bool {
    let mut abd = appbar_data(tray);
    let state = unsafe { SHAppBarMessage(ABM_GETSTATE, &mut abd) } as u32;
    state & ABS_AUTOHIDE != 0
}

fn set_autohide(tray: HWND, on: bool) {
    let mut abd = appbar_data(tray);
    abd.lParam = LPARAM(if on { ABS_AUTOHIDE } else { ABS_ALWAYSONTOP } as isize);
    unsafe {
        SHAppBarMessage(ABM_SETSTATE, &mut abd);
    }
}

fn read_state(path: &Path) -> Option<bool> {
    let text = std::fs::read_to_string(path).ok()?;
    let value = text.lines().find_map(|l| l.trim().strip_prefix("autohide_was_on"))?;
    Some(value.trim_start_matches([' ', '=']).trim() == "true")
}

fn write_state(path: &Path, autohide_was_on: bool) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, format!("autohide_was_on = {autohide_was_on}\n"))
}

static SHOW_NOTIFY: AtomicIsize = AtomicIsize::new(0);
static SHOW_MSG: AtomicU32 = AtomicU32::new(0);

/// Unhooks on drop.
pub struct WinEventHookGuard(HWINEVENTHOOK);

impl Drop for WinEventHookGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWinEvent(self.0);
        }
    }
}

/// Posts `msg` to `notify` whenever explorer shows a taskbar window again.
///
/// The hook is bound to the current explorer process (so it is not woken by every window shown
/// system-wide); after `TaskbarCreated` the caller must drop the guard and call this again.
pub fn watch_taskbar_show(notify: HWND, msg: u32) -> Result<WinEventHookGuard> {
    unsafe extern "system" fn on_show(_: HWINEVENTHOOK, _: u32, hwnd: HWND, id_object: i32, _: i32, _: u32, _: u32) {
        if id_object != OBJID_WINDOW.0 {
            return;
        }
        let class = class_name(hwnd);
        if class == PRIMARY_CLASS || class == SECONDARY_CLASS {
            let notify = HWND(SHOW_NOTIFY.load(Ordering::Relaxed) as *mut _);
            unsafe {
                let _ = PostMessageW(Some(notify), SHOW_MSG.load(Ordering::Relaxed), WPARAM(0), LPARAM(0));
            }
        }
    }
    SHOW_NOTIFY.store(notify.0 as isize, Ordering::Relaxed);
    SHOW_MSG.store(msg, Ordering::Relaxed);
    let pid = pid_of(primary_tray()?);
    let hook = unsafe {
        SetWinEventHook(EVENT_OBJECT_SHOW, EVENT_OBJECT_SHOW, None, Some(on_show), pid, 0, WINEVENT_OUTOFCONTEXT)
    };
    if hook.is_invalid() {
        return Err(windows::core::Error::from_thread());
    }
    Ok(WinEventHookGuard(hook))
}
