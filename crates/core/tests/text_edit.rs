use newwin_core::text_edit::*;

fn typed(s: &str) -> TextEdit {
    let mut e = TextEdit::default();
    e.apply(Edit::Insert(s.into()));
    e
}

#[test]
fn insert_and_move() {
    let mut e = typed("stam");
    assert!(!e.apply(Edit::Left));
    e.apply(Edit::Left);
    assert!(e.apply(Edit::Insert("e".into())));
    assert_eq!(e.text(), "steam");
    assert_eq!(e.caret_utf16(), 3);
    e.apply(Edit::Home);
    assert_eq!(e.caret_utf16(), 0);
    e.apply(Edit::End);
    assert_eq!(e.caret_utf16(), 5);
}

#[test]
fn backspace_and_delete() {
    let mut e = typed("steam");
    assert!(e.apply(Edit::Backspace));
    assert_eq!(e.text(), "stea");
    e.apply(Edit::Home);
    assert!(e.apply(Edit::Delete));
    assert_eq!(e.text(), "tea");
}

#[test]
fn backspace_word() {
    let mut e = typed("visual studio  ");
    assert!(e.apply(Edit::BackspaceWord));
    assert_eq!(e.text(), "visual ");
    e.apply(Edit::BackspaceWord);
    assert_eq!(e.text(), "");
}

#[test]
fn select_all_then_type_replaces() {
    let mut e = typed("steam");
    e.apply(Edit::SelectAll);
    assert!(e.all_selected());
    e.apply(Edit::Insert("c".into()));
    assert_eq!(e.text(), "c");
    assert!(!e.all_selected());
    e.apply(Edit::SelectAll);
    e.apply(Edit::Backspace);
    assert_eq!(e.text(), "");
    let mut e = typed("ab");
    e.apply(Edit::SelectAll);
    e.apply(Edit::Left);
    assert!(!e.all_selected(), "moving clears the selection");
    assert_eq!(e.text(), "ab");
}

#[test]
fn paste_takes_first_line_and_caps_length() {
    assert_eq!(typed("a\nb").text(), "a");
    assert_eq!(typed("a\r\nb").text(), "a");
    assert_eq!(typed("x\ty").text(), "xy", "control chars dropped");
    assert_eq!(typed(&"x".repeat(250)).text().chars().count(), MAX_CHARS);
    let mut e = typed(&"x".repeat(MAX_CHARS));
    assert!(!e.apply(Edit::Insert("y".into())), "full");
}

#[test]
fn caret_counts_utf16() {
    assert_eq!(typed("🎮a").caret_utf16(), 3);
}

#[test]
fn edges_do_not_panic() {
    let mut e = TextEdit::default();
    for op in [Edit::Left, Edit::Backspace, Edit::BackspaceWord, Edit::Delete, Edit::Right, Edit::Home, Edit::End] {
        assert!(!e.apply(op));
    }
    let mut e = typed("ab");
    assert!(!e.apply(Edit::Right));
    assert!(!e.apply(Edit::Delete));
    e.clear();
    assert_eq!((e.text(), e.caret_utf16()), ("", 0));
}
