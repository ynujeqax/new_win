use newwin_platform::clock::{format_bar_date, format_bar_time, local_second_and_millis, ui_language_tag};
use newwin_platform::launch::{Action, SystemTarget, action, apps_folder_path, open_file_location};

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

#[test]
fn now_ms_never_goes_back() {
    let a = newwin_platform::clock::now_ms();
    std::thread::sleep(std::time::Duration::from_millis(20));
    let b = newwin_platform::clock::now_ms();
    assert!(a > 0 && b >= a + 10, "{a} then {b}");
}

#[test]
fn long_date_names_the_year() {
    let d = newwin_platform::clock::format_long_date();
    assert!(d.chars().filter(|c| c.is_ascii_digit()).count() >= 4, "{d:?}");
}

#[test]
fn apps_folder_path_form() {
    assert_eq!(apps_folder_path("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"), r"shell:AppsFolder\Microsoft.WindowsCalculator_8wekyb3d8bbwe!App");
}

#[test]
fn lock_is_lock_workstation() {
    assert_eq!(action(SystemTarget::Lock), Action::Lock);
}

#[test]
fn open_file_location_of_missing_file_errs() {
    let missing = std::env::temp_dir().join("newwin-no-such-dir-4e57").join("nothing.exe");
    assert!(open_file_location(&missing.to_string_lossy()).is_err());
}

#[test]
fn clipboard_read_never_panics() {
    // Whatever the user copied: text or `None`.
    let _ = newwin_platform::clipboard::text();
}
