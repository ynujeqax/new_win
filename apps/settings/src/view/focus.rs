//! Keyboard focus: moving it and what a key does to the focused control.

use super::*;
use crate::sections::ListEdit;

/// Tab / Shift+Tab (`back`) over `count` controls, wrapping; `None` when there are none.
pub fn focus_next(current: Option<usize>, count: usize, back: bool) -> Option<usize> {
    let last = count.checked_sub(1)?;
    Some(match current.filter(|&i| i <= last) {
        None if back => last,
        None => 0,
        Some(i) if back => i.checked_sub(1).unwrap_or(last),
        Some(i) if i == last => 0,
        Some(i) => i + 1,
    })
}

/// The focus index after the banner's button (focusable 0) came (`has`) or went (`had`): the
/// same section control stays focused; the button itself loses the focus when it goes.
pub fn shift_focus(focus: Option<usize>, had: bool, has: bool) -> Option<usize> {
    match (focus, had, has) {
        (Some(0), true, false) => None,
        (Some(i), true, false) => Some(i - 1),
        (Some(i), false, true) => Some(i + 1),
        (f, _, _) => f,
    }
}

/// What a focusable control is, for the keys it takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusKind {
    Button,
    Toggle,
    Segmented,
    Slider,
    /// The font row: opens its list.
    FontList,
    /// Takes the typing itself (the date template).
    TextField,
    /// A row of the Items / Tray list: Space switches it, Ctrl+↑/↓ move it.
    ListRow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyAction {
    /// Press a button, flip a toggle, open the font list.
    Activate,
    /// A slider by one step, a segment by one (-1 left, +1 right), a list row by one (-1 up).
    Step(i32),
}

/// The key `vk` on a focused control of `kind` (spec §5).
pub fn key_action(kind: FocusKind, vk: u16) -> Option<KeyAction> {
    match (kind, vk) {
        (FocusKind::Slider | FocusKind::Segmented, v) if v == VK_LEFT.0 => Some(KeyAction::Step(-1)),
        (FocusKind::Slider | FocusKind::Segmented, v) if v == VK_RIGHT.0 => Some(KeyAction::Step(1)),
        (FocusKind::Toggle | FocusKind::ListRow, v) if v == VK_SPACE.0 => Some(KeyAction::Activate),
        (FocusKind::Button | FocusKind::FontList, v) if v == VK_SPACE.0 || v == VK_RETURN.0 => Some(KeyAction::Activate),
        _ => None,
    }
}

/// The key `vk` pressed with Ctrl on a focused control of `kind`: Ctrl+↑/↓ move a list row.
pub fn ctrl_key_action(kind: FocusKind, vk: u16) -> Option<KeyAction> {
    match (kind, vk) {
        (FocusKind::ListRow, v) if v == VK_UP.0 => Some(KeyAction::Step(-1)),
        (FocusKind::ListRow, v) if v == VK_DOWN.0 => Some(KeyAction::Step(1)),
        _ => None,
    }
}

/// After a list edit, is the focused row brought into view? Only for a row moved by the keyboard:
/// a mouse drop leaves the list where the user scrolled it.
pub fn reveals_focus(e: &ListEdit, by_pointer: bool) -> bool {
    matches!(e, ListEdit::Moved(..)) && !by_pointer
}
