use super::*;
use newwin_core::strings::strings_for;
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_LEFT, VK_RETURN, VK_RIGHT, VK_SPACE, VK_UP};

#[test]
fn keyboard_focus_moves_and_wraps() {
    assert_eq!(focus_next(None, 3, false), Some(0));
    assert_eq!(focus_next(Some(2), 3, false), Some(0));
    assert_eq!(focus_next(Some(0), 3, true), Some(2));
    assert_eq!(focus_next(None, 0, false), None);
    assert_eq!(focus_next(None, 3, true), Some(2));
    assert_eq!(focus_next(Some(1), 3, false), Some(2));
    assert_eq!(focus_next(Some(7), 3, false), Some(0), "a stale index starts over");
}

#[test]
fn a_banner_button_coming_or_going_keeps_the_focused_control() {
    assert_eq!(shift_focus(Some(2), false, true), Some(3));
    assert_eq!(shift_focus(Some(3), true, false), Some(2));
    assert_eq!(shift_focus(Some(0), true, false), None, "the button itself went");
    assert_eq!(shift_focus(Some(1), true, true), Some(1));
    assert_eq!(shift_focus(None, false, true), None);
}

#[test]
fn keys_act_on_the_focused_control() {
    assert_eq!(key_action(FocusKind::Slider, VK_LEFT.0), Some(KeyAction::Step(-1)));
    assert_eq!(key_action(FocusKind::Segmented, VK_RIGHT.0), Some(KeyAction::Step(1)));
    assert_eq!(key_action(FocusKind::Toggle, VK_SPACE.0), Some(KeyAction::Activate));
    assert_eq!(key_action(FocusKind::Button, VK_RETURN.0), Some(KeyAction::Activate));
    assert_eq!(key_action(FocusKind::Button, VK_SPACE.0), Some(KeyAction::Activate));
    assert_eq!(key_action(FocusKind::FontList, VK_SPACE.0), Some(KeyAction::Activate));
    assert_eq!(key_action(FocusKind::Toggle, VK_LEFT.0), None);
    assert_eq!(key_action(FocusKind::Slider, VK_UP.0), None);
}

#[test]
fn list_rows_switch_with_space_and_move_with_ctrl_arrows() {
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_DOWN;
    assert_eq!(key_action(FocusKind::ListRow, VK_SPACE.0), Some(KeyAction::Activate));
    assert_eq!(key_action(FocusKind::ListRow, VK_UP.0), None, "plain arrows do not move rows");
    assert_eq!(ctrl_key_action(FocusKind::ListRow, VK_UP.0), Some(KeyAction::Step(-1)));
    assert_eq!(ctrl_key_action(FocusKind::ListRow, VK_DOWN.0), Some(KeyAction::Step(1)));
    assert_eq!(ctrl_key_action(FocusKind::Slider, VK_UP.0), None);
}

#[test]
fn six_sections_in_spec_order() {
    let en = strings_for("en-US");
    let titles: Vec<_> = Section::ALL.iter().map(|s| s.title(en)).collect();
    assert_eq!(titles, ["Bar", "Appearance", "Items", "Spotlight", "Tray", "About"]);
}

#[test]
fn banner_copy_is_the_spec_text() {
    let en = strings_for("en-US");
    let parse = Banner::ParseError { line: Some(12) };
    assert_eq!(parse.text(en), "config.toml has an error on line 12. The bar uses the last good settings.");
    assert_eq!(parse.button(en), Some("Open file"));
    let saved = Banner::SaveFailed("Access is denied".into());
    assert_eq!(saved.text(en), "Couldn't save settings: Access is denied.");
    assert_eq!(saved.button(en), None);
    assert_eq!(Banner::NotRunning.text(en), "NewWin is not running. Changes apply when it starts.");
    assert_eq!(Banner::NotRunning.button(en), Some("Start NewWin"));
    assert!(!Banner::ParseError { line: None }.text(en).contains("{line}"));
}

#[test]
fn one_banner_at_a_time() {
    assert_eq!(pick_banner(None, None, true), None);
    assert_eq!(pick_banner(None, None, false), Some(Banner::NotRunning));
    assert_eq!(pick_banner(None, Some(Some(3)), false), Some(Banner::ParseError { line: Some(3) }));
    assert_eq!(pick_banner(Some("disk full"), Some(Some(3)), false), Some(Banner::SaveFailed("disk full".into())));
}

#[test]
fn save_reason_drops_the_os_code_and_full_stop() {
    let e = std::io::Error::from_raw_os_error(5);
    let r = save_reason(&e);
    assert!(!r.contains("os error"), "{r}");
    assert!(!r.ends_with('.'), "{r}");
    assert!(!r.is_empty());
}
