//! Date/time text in the user's regional format, and the UI language.

use windows::Win32::Globalization::{DATE_LONGDATE, GetDateFormatEx, GetTimeFormatEx, GetUserDefaultUILanguage, LCIDToLocaleName, TIME_FORMAT_FLAGS, TIME_NOSECONDS};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::core::{HSTRING, PCWSTR};

/// Abbreviated weekday, month and day; names come out in the user's locale.
const SYSTEM_DATE_PATTERN: &str = "ddd MMM d";

/// `"system"` → e.g. `Wed Oct 21`; anything else is a `GetDateFormatEx` picture string.
pub fn format_bar_date(date_format: &str) -> String {
    let pattern = HSTRING::from(if date_format == "system" { SYSTEM_DATE_PATTERN } else { date_format });
    let mut buf = [0u16; 128];
    let n = unsafe { GetDateFormatEx(PCWSTR::null(), Default::default(), None, &pattern, Some(&mut buf), PCWSTR::null()) };
    String::from_utf16_lossy(&buf[..(n.max(1) - 1) as usize])
}

/// The long date in the user's format, e.g. "Thursday, October 22, 2026" (the date tooltip).
pub fn format_long_date() -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetDateFormatEx(PCWSTR::null(), DATE_LONGDATE, None, PCWSTR::null(), Some(&mut buf), PCWSTR::null()) };
    String::from_utf16_lossy(&buf[..(n.max(1) - 1) as usize])
}

/// The user's short or long time format (12/24 h as configured in Windows).
pub fn format_bar_time(show_seconds: bool) -> String {
    let flags = if show_seconds { TIME_FORMAT_FLAGS(0) } else { TIME_NOSECONDS };
    let mut buf = [0u16; 64];
    let n = unsafe { GetTimeFormatEx(PCWSTR::null(), flags, None, PCWSTR::null(), Some(&mut buf)) };
    String::from_utf16_lossy(&buf[..(n.max(1) - 1) as usize])
}

pub fn local_second_and_millis() -> (u16, u16) {
    let t = unsafe { GetLocalTime() };
    (t.wSecond, t.wMilliseconds)
}

/// Windows display language as a BCP-47 tag, e.g. `en-US`.
pub fn ui_language_tag() -> String {
    let mut buf = [0u16; 85];
    let n = unsafe { LCIDToLocaleName(u32::from(GetUserDefaultUILanguage()), Some(&mut buf), 0) };
    if n <= 1 {
        return "en-US".into();
    }
    String::from_utf16_lossy(&buf[..(n - 1) as usize])
}

/// Milliseconds since boot, for animations; never goes back (unlike the wall clock).
pub fn now_ms() -> u64 {
    unsafe { windows::Win32::System::SystemInformation::GetTickCount64() }
}
