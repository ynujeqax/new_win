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
