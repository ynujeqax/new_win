use newwin_platform::autostart::{autostart_get_named, autostart_set_named};

#[test]
fn autostart_value_is_written_once_and_removed() {
    autostart_set_named("NewWinTest", true, r#""C:\x\NewWin.exe""#).unwrap();
    assert_eq!(autostart_get_named("NewWinTest").as_deref(), Some(r#""C:\x\NewWin.exe""#));
    autostart_set_named("NewWinTest", false, "").unwrap();
    assert_eq!(autostart_get_named("NewWinTest"), None);
    autostart_set_named("NewWinTest", false, "").unwrap(); // already absent: Ok
}
