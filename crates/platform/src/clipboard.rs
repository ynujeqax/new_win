//! Text on the clipboard: read for pasting into Spotlight, written for its answers.

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::core::PCWSTR;

const CF_UNICODETEXT: u32 = 13;

/// The clipboard's text, or `None` when it holds none or is busy.
pub fn text() -> Option<String> {
    unsafe {
        OpenClipboard(None).ok()?;
        let s = GetClipboardData(CF_UNICODETEXT).ok().and_then(|h| {
            let g = HGLOBAL(h.0);
            let p = GlobalLock(g) as *const u16;
            if p.is_null() {
                return None;
            }
            let s = PCWSTR(p).to_string().ok();
            let _ = GlobalUnlock(g);
            s
        });
        let _ = CloseClipboard();
        s
    }
}

/// Puts `s` on the clipboard as text; false when the clipboard is busy.
pub fn set_text(s: &str) -> bool {
    let wide: Vec<u16> = s.encode_utf16().chain(Some(0)).collect();
    unsafe {
        if OpenClipboard(None).is_err() {
            return false;
        }
        let ok = (|| {
            EmptyClipboard().ok()?;
            let g = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2).ok()?;
            let p = GlobalLock(g) as *mut u16;
            if p.is_null() {
                let _ = GlobalFree(Some(g));
                return None;
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
            let _ = GlobalUnlock(g);
            // The clipboard owns the memory once this succeeds.
            if SetClipboardData(CF_UNICODETEXT, Some(HANDLE(g.0))).is_err() {
                let _ = GlobalFree(Some(g));
                return None;
            }
            Some(())
        })()
        .is_some();
        let _ = CloseClipboard();
        ok
    }
}
