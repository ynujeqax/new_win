use newwin_platform::processes::{create_event, event_exists, is_newwin_image, other_newwin_pids};

#[test]
fn only_exact_newwin_images_match() {
    for yes in ["NewWin.exe", "newwin.EXE", "NewWinSettings.exe", "NEWWINSETTINGS.EXE"] {
        assert!(is_newwin_image(yes), "{yes}");
    }
    for no in ["NewWinHelper.exe", "newwin.exe.bak", "NewWin", "explorer.exe", "xNewWin.exe", ""] {
        assert!(!is_newwin_image(no), "{no}");
    }
}

#[test]
fn other_newwin_pids_excludes_this_process() {
    assert!(!other_newwin_pids().contains(&std::process::id()));
}

#[test]
fn an_event_exists_only_while_its_handle_is_held() {
    let name = format!("Local\\NewWin.Test.{}", std::process::id());
    assert!(!event_exists(&name));
    let held = create_event(&name).expect("event");
    assert!(event_exists(&name));
    drop(held);
    assert!(!event_exists(&name));
}
