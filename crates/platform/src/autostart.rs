//! "Start with Windows": the `HKCU\...\Run` value `NewWin`.

use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW};
use windows::core::{Error, HRESULT, HSTRING, PCWSTR, Result};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "NewWin";

/// The quoted path of the running exe, as the Run value wants it.
pub fn autostart_command() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(format!("\"{}\"", exe.to_str()?))
}

/// The current `NewWin` Run value, if any.
pub fn autostart_get() -> Option<String> {
    autostart_get_named(VALUE)
}

/// Turns the Run value on (for the running exe) or off. Writes only when it differs.
pub fn autostart_set(on: bool) -> Result<()> {
    let command = if on { autostart_command().ok_or_else(|| Error::from(HRESULT(0x80004005u32 as i32)))? } else { String::new() };
    autostart_set_named(VALUE, on, &command)
}

pub fn autostart_get_named(name: &str) -> Option<String> {
    let key = HSTRING::from(RUN_KEY);
    let name = HSTRING::from(name);
    let mut bytes = 0u32;
    let probe = unsafe { RegGetValueW(HKEY_CURRENT_USER, &key, &name, RRF_RT_REG_SZ, None, None, Some(&mut bytes)) };
    if probe != ERROR_SUCCESS || bytes < 2 {
        return None;
    }
    let mut buf = vec![0u16; (bytes as usize).div_ceil(2) + 1];
    let mut size = (buf.len() * 2) as u32;
    let r = unsafe { RegGetValueW(HKEY_CURRENT_USER, &key, &name, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr().cast()), Some(&mut size)) };
    if r != ERROR_SUCCESS {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16(&buf[..len]).ok()
}

pub fn autostart_set_named(name: &str, on: bool, command: &str) -> Result<()> {
    let current = autostart_get_named(name);
    let key = HSTRING::from(RUN_KEY);
    let hname = HSTRING::from(name);
    if on {
        if current.as_deref() == Some(command) {
            return Ok(());
        }
        let wide: Vec<u16> = command.encode_utf16().chain(std::iter::once(0)).collect();
        let r = unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, &key, &hname, REG_SZ.0, Some(wide.as_ptr().cast()), (wide.len() * 2) as u32) };
        r.ok()
    } else {
        if current.is_none() {
            return Ok(());
        }
        let r = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr()), PCWSTR(hname.as_ptr())) };
        if r == ERROR_FILE_NOT_FOUND { Ok(()) } else { r.ok() }
    }
}
