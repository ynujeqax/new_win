//! The active window's keyboard layout (spec: M2 §4.3).

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::Globalization::{GetLocaleInfoEx, LCIDToLocaleName};
use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayout;
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId, PostMessageW};
use windows::core::PCWSTR;

const LOCALE_SLOCALIZEDDISPLAYNAME: u32 = 0x2;
const LOCALE_SISO639LANGNAME2: u32 = 0x67;
const WM_INPUTLANGCHANGEREQUEST: u32 = 0x50;
const INPUTLANGCHANGE_FORWARD: usize = 0x2;

#[derive(Clone, Debug, PartialEq)]
pub struct LayoutInfo {
    /// ISO 639-2, e.g. "eng".
    pub code: String,
    /// Localized, e.g. "English (United States)".
    pub name: String,
}

fn locale_info(locale: &[u16], kind: u32) -> Option<String> {
    let mut buf = [0u16; 128];
    let n = unsafe { GetLocaleInfoEx(PCWSTR(locale.as_ptr()), kind, Some(&mut buf)) };
    (n > 1).then(|| String::from_utf16_lossy(&buf[..(n - 1) as usize]))
}

/// The layout of the thread that owns the foreground window.
pub fn current_layout() -> Option<LayoutInfo> {
    let thread = unsafe { GetWindowThreadProcessId(GetForegroundWindow(), None) };
    let lang = unsafe { GetKeyboardLayout(thread) }.0 as usize & 0xFFFF;
    let mut name = [0u16; 85];
    let n = unsafe { LCIDToLocaleName(lang as u32, Some(&mut name), 0) };
    if n <= 1 {
        return None;
    }
    let locale = &name[..n as usize];
    Some(LayoutInfo { code: locale_info(locale, LOCALE_SISO639LANGNAME2)?, name: locale_info(locale, LOCALE_SLOCALIZEDDISPLAYNAME).unwrap_or_default() })
}

/// Asks the foreground window to move to the next layout (like Alt+Shift).
pub fn switch_next() {
    unsafe {
        let _ = PostMessageW(Some(GetForegroundWindow()), WM_INPUTLANGCHANGEREQUEST, WPARAM(INPUTLANGCHANGE_FORWARD), LPARAM(0));
    }
}
