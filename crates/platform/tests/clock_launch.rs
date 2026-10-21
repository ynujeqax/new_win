use newwin_platform::clock::{format_bar_date, format_bar_time, local_second_and_millis, ui_language_tag};
use newwin_platform::launch::{Action, SystemTarget, action};

#[test]
fn time_has_no_seconds_unless_asked() {
    let s = format_bar_time(false);
    assert!(!s.is_empty());
    assert!(s.matches(':').count() <= 1, "{s}");
    assert!(format_bar_time(true).matches(':').count() >= 2);
}

#[test]
fn date_uses_the_pattern() {
    let year = format_bar_date("yyyy");
    assert_eq!(year.len(), 4);
    assert!(year.chars().all(|c| c.is_ascii_digit()), "{year}");
    assert!(year.starts_with("20"));
    assert!(!format_bar_date("system").is_empty());
}

#[test]
fn second_and_millis_in_range() {
    let (s, ms) = local_second_and_millis();
    assert!(s < 60 && ms < 1000);
}

#[test]
fn ui_language_is_a_tag() {
    let tag = ui_language_tag();
    assert!(tag.contains('-'), "{tag}");
}

#[test]
fn system_menu_targets() {
    use Action::*;
    assert_eq!(action(SystemTarget::InstalledApps), Shell { verb: "open", file: "ms-settings:appsfeatures", args: "" });
    assert_eq!(action(SystemTarget::PowerShellAdmin), Shell { verb: "runas", file: "powershell.exe", args: "" });
    assert_eq!(action(SystemTarget::DiskManagement), Shell { verb: "open", file: "diskmgmt.msc", args: "" });
    assert_eq!(action(SystemTarget::ShutDown), Shell { verb: "open", file: "shutdown.exe", args: "/s /t 0" });
    assert_eq!(action(SystemTarget::Restart), Shell { verb: "open", file: "shutdown.exe", args: "/r /t 0" });
    assert_eq!(action(SystemTarget::Run), RunDialog);
    assert_eq!(action(SystemTarget::Desktop), ToggleDesktop);
    assert_eq!(action(SystemTarget::SignOut), SignOut);
    assert_eq!(action(SystemTarget::Sleep), Sleep);
}
