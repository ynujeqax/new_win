//! Finding and ending NewWin's own processes, for `NewWin.exe --quit` (M5 spec §3).

use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, WPARAM};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::{
    CreateEventW, GetCurrentProcessId, OpenEventW, OpenProcess, PROCESS_TERMINATE, SYNCHRONIZATION_SYNCHRONIZE,
    TerminateProcess,
};
use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, WM_CLOSE};
use windows::core::{BOOL, HSTRING};

/// The only images `--quit` ever touches.
pub const NEWWIN_IMAGES: [&str; 2] = ["NewWin.exe", "NewWinSettings.exe"];

/// An exact, case-insensitive match with one of [`NEWWIN_IMAGES`].
pub fn is_newwin_image(name: &str) -> bool {
    NEWWIN_IMAGES.iter().any(|n| n.eq_ignore_ascii_case(name))
}

/// Every NewWin process in this session except this one.
pub fn other_newwin_pids() -> Vec<u32> {
    newwin_processes().into_iter().map(|(pid, _)| pid).collect()
}

/// The `NewWinSettings.exe` processes in this session.
pub fn settings_pids() -> Vec<u32> {
    newwin_processes().into_iter().filter(|(_, name)| name.eq_ignore_ascii_case(NEWWIN_IMAGES[1])).map(|(pid, _)| pid).collect()
}

fn session_of(pid: u32) -> Option<u32> {
    let mut s = 0;
    unsafe { ProcessIdToSessionId(pid, &mut s) }.ok().map(|_| s)
}

fn newwin_processes() -> Vec<(u32, String)> {
    let me = unsafe { GetCurrentProcessId() };
    let Some(session) = session_of(me) else { return Vec::new() };
    let Ok(snap) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else { return Vec::new() };
    let mut out = Vec::new();
    let mut e = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
    let mut ok = unsafe { Process32FirstW(snap, &mut e) }.is_ok();
    while ok {
        let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
        let name = String::from_utf16_lossy(&e.szExeFile[..len]);
        let pid = e.th32ProcessID;
        if pid != me && is_newwin_image(&name) && session_of(pid) == Some(session) {
            out.push((pid, name));
        }
        ok = unsafe { Process32NextW(snap, &mut e) }.is_ok();
    }
    unsafe {
        let _ = CloseHandle(snap);
    }
    out
}

/// Posts `WM_CLOSE` to every visible top-level window of `pid`.
pub fn close_windows_of(pid: u32) {
    unsafe extern "system" fn each(hwnd: HWND, lp: LPARAM) -> BOOL {
        let mut owner = 0;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut owner)) };
        if owner == lp.0 as u32 && unsafe { IsWindowVisible(hwnd).as_bool() } {
            let _ = unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) };
        }
        true.into()
    }
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(pid as isize));
    }
}

/// Ends `pid`; false when it could not be opened or ended.
pub fn terminate(pid: u32) -> bool {
    let Ok(h) = (unsafe { OpenProcess(PROCESS_TERMINATE, false, pid) }) else { return false };
    let ok = unsafe { TerminateProcess(h, 1) }.is_ok();
    unsafe {
        let _ = CloseHandle(h);
    }
    ok
}

/// A named event; it exists while this handle (or another) is open.
pub struct EventHandle(HANDLE);

impl Drop for EventHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub fn create_event(name: &str) -> Option<EventHandle> {
    unsafe { CreateEventW(None, true, false, &HSTRING::from(name)) }.ok().map(EventHandle)
}

pub fn event_exists(name: &str) -> bool {
    match unsafe { OpenEventW(SYNCHRONIZATION_SYNCHRONIZE, false, &HSTRING::from(name)) } {
        Ok(h) => {
            unsafe {
                let _ = CloseHandle(h);
            }
            true
        }
        Err(_) => false,
    }
}
