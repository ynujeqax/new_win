//! Directory change notifications without a thread of our own: the OS thread pool waits on a
//! change handle and posts a message to the owner window. Nothing here polls.

use windows::Win32::Foundation::{HANDLE, HWND, INVALID_HANDLE_VALUE, LPARAM, WPARAM};
use windows::Win32::Storage::FileSystem::{
    FILE_NOTIFY_CHANGE, FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE, FindCloseChangeNotification, FindFirstChangeNotificationW,
    FindNextChangeNotification,
};
use windows::Win32::System::Threading::{INFINITE, RegisterWaitForSingleObject, UnregisterWaitEx, WT_EXECUTEDEFAULT};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
use windows::core::{HSTRING, Result};

use std::path::Path;

/// What a thread-pool wait callback needs; boxed, freed after the wait is gone.
struct Wait {
    change: HANDLE,
    notify: isize,
    msg: u32,
}

unsafe extern "system" fn on_change(ctx: *mut core::ffi::c_void, _timed_out: bool) {
    let w = unsafe { &*(ctx as *const Wait) };
    unsafe {
        let _ = PostMessageW(Some(HWND(w.notify as *mut _)), w.msg, WPARAM(0), LPARAM(0));
        let _ = FindNextChangeNotification(w.change);
    }
}

/// One armed change notification on a directory; unregistered and closed on drop.
pub(crate) struct Registration {
    wait: HANDLE,
    ctx: *mut Wait,
}

impl Registration {
    pub(crate) fn new(dir: &Path, recursive: bool, filter: FILE_NOTIFY_CHANGE, notify: HWND, msg: u32) -> Result<Registration> {
        let change = unsafe { FindFirstChangeNotificationW(&HSTRING::from(dir.as_os_str()), recursive, filter)? };
        let ctx = Box::into_raw(Box::new(Wait { change, notify: notify.0 as isize, msg }));
        let mut wait = HANDLE::default();
        if let Err(e) = unsafe { RegisterWaitForSingleObject(&mut wait, change, Some(on_change), Some(ctx as *const _), INFINITE, WT_EXECUTEDEFAULT) } {
            unsafe {
                let _ = FindCloseChangeNotification(change);
                drop(Box::from_raw(ctx));
            }
            return Err(e);
        }
        Ok(Registration { wait, ctx })
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        unsafe {
            // Blocks until a running callback has returned: then the context can go.
            let _ = UnregisterWaitEx(self.wait, Some(INVALID_HANDLE_VALUE));
            let _ = FindCloseChangeNotification((*self.ctx).change);
            drop(Box::from_raw(self.ctx));
        }
    }
}

/// Posts `msg` (`WPARAM(0)`) to `notify` when a file in `dir` is written, created, renamed or
/// deleted. Several posts can arrive for one save: the owner debounces.
pub struct DirWatch {
    _reg: Registration,
}

impl DirWatch {
    pub fn start(dir: &Path, notify: HWND, msg: u32) -> Result<DirWatch> {
        Ok(DirWatch { _reg: Registration::new(dir, false, FILE_NOTIFY_CHANGE_LAST_WRITE | FILE_NOTIFY_CHANGE_FILE_NAME, notify, msg)? })
    }
}
