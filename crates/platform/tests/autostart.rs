use newwin_platform::autostart::{autostart_get_named, autostart_set_named};

#[test]
fn autostart_value_is_written_once_and_removed() {
    autostart_set_named("NewWinTest", true, r#""C:\x\NewWin.exe""#).unwrap();
    assert_eq!(autostart_get_named("NewWinTest").as_deref(), Some(r#""C:\x\NewWin.exe""#));
    autostart_set_named("NewWinTest", false, "").unwrap();
    assert_eq!(autostart_get_named("NewWinTest"), None);
    autostart_set_named("NewWinTest", false, "").unwrap(); // already absent: Ok
}

#[test]
fn run_value_quotes_paths_with_spaces_and_cyrillic() {
    use newwin_platform::autostart::run_value_for;
    let exe = std::path::Path::new(r"C:\Мої програми\New Win\NewWin.exe");
    assert_eq!(run_value_for(exe).as_deref(), Some(r#""C:\Мої програми\New Win\NewWin.exe""#));
}

#[test]
fn turning_autostart_off_removes_an_expand_sz_value() {
    use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, REG_EXPAND_SZ, RRF_RT_ANY, RegGetValueW, RegSetKeyValueW};
    use windows::core::HSTRING;
    let key = HSTRING::from(r"Software\Microsoft\Windows\CurrentVersion\Run");
    let name = format!("NewWinTest-{}", std::process::id());
    let hname = HSTRING::from(name.as_str());
    let data: Vec<u16> = r"%LOCALAPPDATA%\x\NewWin.exe".encode_utf16().chain([0]).collect();
    let r = unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, &key, &hname, REG_EXPAND_SZ.0, Some(data.as_ptr().cast()), (data.len() * 2) as u32) };
    assert!(r.is_ok());
    autostart_set_named(&name, false, "").unwrap();
    let r = unsafe { RegGetValueW(HKEY_CURRENT_USER, &key, &hname, RRF_RT_ANY, None, None, None) };
    assert_eq!(r, ERROR_FILE_NOT_FOUND, "the REG_EXPAND_SZ value is gone");
}
