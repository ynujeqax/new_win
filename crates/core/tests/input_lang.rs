use newwin_core::input_lang::*;

const SHIFT: u16 = 0xA0; // VK_LSHIFT
const ALT: u16 = 0xA4; // VK_LMENU
const CTRL: u16 = 0xA2; // VK_LCONTROL
const WIN: u16 = 0x5B; // VK_LWIN
const SPACE: u16 = 0x20;
const A: u16 = 0x41;

#[test]
fn codes() {
    assert_eq!(display_code("eng"), "ENG");
    assert_eq!(display_code(" ukr "), "UKR");
    assert_eq!(display_code(""), "—");
}

#[test]
fn alt_shift_in_either_order() {
    let mut h = LayoutHotkeys::default();
    assert!(!h.key(ALT, true));
    assert!(!h.key(SHIFT, true));
    assert!(h.key(SHIFT, false));
    assert!(!h.key(ALT, false), "one report per chord");
    assert!(!h.key(SHIFT, true));
    assert!(!h.key(ALT, true));
    assert!(h.key(ALT, false));
    h.key(SHIFT, false);
}

#[test]
fn ctrl_shift_counts_but_not_with_another_key() {
    let mut h = LayoutHotkeys::default();
    h.key(CTRL, true);
    h.key(SHIFT, true);
    assert!(h.key(SHIFT, false));
    h.key(CTRL, false);
    h.key(CTRL, true);
    h.key(SHIFT, true);
    h.key(A, true);
    h.key(A, false);
    assert!(!h.key(SHIFT, false), "Ctrl+Shift+A is a shortcut, not a switch");
    h.key(CTRL, false);
}

#[test]
fn win_space_reports_each_press() {
    let mut h = LayoutHotkeys::default();
    h.key(WIN, true);
    assert!(h.key(SPACE, true));
    h.key(SPACE, false);
    assert!(h.key(SPACE, true));
    h.key(SPACE, false);
    h.key(WIN, false);
    assert!(!h.key(SPACE, true));
}

#[test]
fn key_repeat_does_not_break_a_chord() {
    let mut h = LayoutHotkeys::default();
    h.key(ALT, true);
    h.key(ALT, true);
    h.key(SHIFT, true);
    h.key(SHIFT, true);
    assert!(h.key(SHIFT, false));
    h.key(ALT, false);
}

const RWIN: u16 = 0x5C;
const ESC: u16 = 0x1B;
const E: u16 = 0x45;

#[test]
fn lone_win_is_a_tap() {
    let mut t = WinKeyTap::new(true);
    assert_eq!(t.key(WIN, true), TapAction::Pass);
    assert_eq!(t.key(WIN, false), TapAction::WinTap);
}

#[test]
fn held_win_with_autorepeat_is_one_tap() {
    let mut t = WinKeyTap::new(true);
    for _ in 0..5 {
        assert_eq!(t.key(WIN, true), TapAction::Pass);
    }
    assert_eq!(t.key(WIN, false), TapAction::WinTap);
    assert_eq!(t.key(WIN, false), TapAction::Pass, "a stray second up is not a tap");
}

#[test]
fn win_combination_is_not_a_tap() {
    let mut t = WinKeyTap::new(true);
    t.key(WIN, true);
    assert_eq!(t.key(E, true), TapAction::Pass);
    t.key(E, false);
    assert_eq!(t.key(WIN, false), TapAction::Pass);
}

#[test]
fn next_tap_after_combination_works() {
    let mut t = WinKeyTap::new(true);
    t.key(WIN, true);
    t.key(E, true);
    t.key(E, false);
    t.key(WIN, false);
    t.key(WIN, true);
    assert_eq!(t.key(WIN, false), TapAction::WinTap);
}

#[test]
fn right_win_taps_too() {
    let mut t = WinKeyTap::new(true);
    t.key(RWIN, true);
    assert_eq!(t.key(RWIN, false), TapAction::WinTap);
}

#[test]
fn ctrl_esc_is_swallowed() {
    let mut t = WinKeyTap::new(true);
    assert_eq!(t.key(CTRL, true), TapAction::Pass);
    assert_eq!(t.key(ESC, true), TapAction::CtrlEsc);
    assert_eq!(t.key(ESC, false), TapAction::Swallow);
    assert_eq!(t.key(CTRL, false), TapAction::Pass);
    assert_eq!(t.key(ESC, true), TapAction::Pass, "Esc alone");
    assert_eq!(t.key(ESC, false), TapAction::Pass);
}

#[test]
fn win_ctrl_esc_is_not_ctrl_esc() {
    let mut t = WinKeyTap::new(true);
    t.key(WIN, true);
    t.key(CTRL, true);
    assert_eq!(t.key(ESC, true), TapAction::Pass);
    assert_eq!(t.key(ESC, false), TapAction::Pass);
    t.key(CTRL, false);
    assert_eq!(t.key(WIN, false), TapAction::Pass);
}

#[test]
fn disabled_passes_everything() {
    let mut t = WinKeyTap::new(false);
    t.key(WIN, true);
    assert_eq!(t.key(WIN, false), TapAction::Pass);
    t.key(CTRL, true);
    assert_eq!(t.key(ESC, true), TapAction::Pass);
}
