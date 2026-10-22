//! Reading text the user copied (for pasting into Spotlight).

use windows::Win32::Foundation::HGLOBAL;
use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
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
