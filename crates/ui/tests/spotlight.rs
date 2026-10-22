use std::rc::Rc;

use newwin_core::spotlight::{CONFIRM_MS, Candidate, Kind, system_candidates};
use newwin_core::strings::strings_for;
use newwin_ui::panel::{PanelContent, PanelInput};
use newwin_ui::spotlight::{SpotlightOp, SpotlightView, action_id};
use newwin_ui::theme;

const RETURN: u16 = 0x0D;
const UP: u16 = 0x26;
const DOWN: u16 = 0x28;

fn app(name: &str, file: Option<&str>, packaged: bool) -> Candidate {
    Candidate { name: name.into(), id: format!("id:{name}"), kind: Kind::App, file_path: file.map(str::to_string), packaged }
}

fn catalog() -> Rc<Vec<Candidate>> {
    let mut c = vec![app("Steam", Some(r"C:\Programs\Steam\Steam.exe"), false), app("Visual Studio Code", Some(r"C:\VS\Code.exe"), false), app("Settings", None, true)];
    c.extend(system_candidates(strings_for("en")));
    Rc::new(c)
}

fn index_of(c: &[Candidate], name: &str, system: bool) -> usize {
    c.iter().position(|x| x.name == name && matches!(x.kind, Kind::System(_)) == system).unwrap()
}

fn view() -> SpotlightView {
    SpotlightView::for_test(catalog(), 8)
}

fn typed(v: &mut SpotlightView, s: &str) {
    for c in s.chars() {
        v.input(PanelInput::Char(c), 0);
    }
}

fn key(v: &mut SpotlightView, vk: u16) -> Option<u32> {
    v.input(PanelInput::Key(vk), 0).action
}

#[test]
fn empty_query_is_just_the_field() {
    let v = view();
    assert_eq!(v.size(), (theme::SPOTLIGHT_WIDTH, theme::SPOTLIGHT_FIELD_H + theme::SPOTLIGHT_HINTS_H));
    assert!(v.hits().is_empty());
    assert_eq!(v.selected(), None);
}

#[test]
fn typing_selects_first_row_and_grows() {
    let mut v = view();
    let h0 = v.size().1;
    let r = v.input(PanelInput::Char('s'), 0);
    assert!(r.resize && r.repaint);
    assert_eq!(v.query(), "s");
    assert_eq!(v.selected(), Some(0));
    assert!(v.size().1 > h0);
    assert_eq!(v.size().1, theme::SPOTLIGHT_FIELD_H + 2.0 * theme::SPOTLIGHT_PAD + v.hits().len() as f32 * theme::SPOTLIGHT_ROW_H + theme::SPOTLIGHT_HINTS_H);
}

#[test]
fn arrows_wrap() {
    let mut v = view();
    typed(&mut v, "s");
    let n = v.hits().len();
    assert!(n > 2);
    key(&mut v, UP);
    assert_eq!(v.selected(), Some(n - 1));
    key(&mut v, DOWN);
    assert_eq!(v.selected(), Some(0));
    key(&mut v, DOWN);
    assert_eq!(v.selected(), Some(1));
}

#[test]
fn enter_opens_the_selected_app() {
    let mut v = view();
    typed(&mut v, "ste");
    assert_eq!(key(&mut v, RETURN), Some(action_id(index_of(&catalog(), "Steam", false), SpotlightOp::Open)));
}

#[test]
fn shut_down_needs_two_enters() {
    let mut v = view();
    typed(&mut v, "shut");
    assert_eq!(key(&mut v, RETURN), None);
    assert_eq!(key(&mut v, RETURN), Some(action_id(index_of(&catalog(), "Shut down", true), SpotlightOp::Open)));
}

#[test]
fn ctrl_enter_only_for_classic_apps() {
    let mut v = view();
    typed(&mut v, "settings");
    assert_eq!(v.catalog()[v.hits()[0].index].kind, Kind::App, "the app comes before the system command");
    assert_eq!(v.input(PanelInput::CtrlKey(RETURN), 0).action, None);
    let mut v = view();
    typed(&mut v, "visual");
    assert_eq!(v.input(PanelInput::CtrlKey(RETURN), 0).action, Some(action_id(index_of(&catalog(), "Visual Studio Code", false), SpotlightOp::Admin)));
}

#[test]
fn context_menu_only_for_apps() {
    let mut v = view();
    typed(&mut v, "lock");
    assert_eq!(v.input(PanelInput::ContextMenu(None), 0).action, None);
    let mut v = view();
    typed(&mut v, "steam");
    assert_eq!(v.input(PanelInput::ContextMenu(None), 0).action, Some(action_id(index_of(&catalog(), "Steam", false), SpotlightOp::Menu)));
}

#[test]
fn right_click_selects_the_row_then_asks_for_its_menu() {
    let mut v = view();
    typed(&mut v, "s");
    let r = v.row_rect(1);
    let target = v.hits()[1].index;
    let reply = v.input(PanelInput::RightRelease(newwin_ui::PointF { x: r.x + 4.0, y: r.y + 4.0 }), 0);
    assert_eq!(v.selected(), Some(1));
    if v.catalog()[target].kind == Kind::App {
        assert_eq!(reply.action, Some(action_id(target, SpotlightOp::Menu)));
    } else {
        assert_eq!(reply.action, None);
    }
}

#[test]
fn no_results_row() {
    let mut v = view();
    typed(&mut v, "zzzz");
    assert!(v.hits().is_empty());
    assert_eq!(v.selected(), None);
    assert_eq!(v.size().1, theme::SPOTLIGHT_FIELD_H + 2.0 * theme::SPOTLIGHT_PAD + theme::SPOTLIGHT_ROW_H + theme::SPOTLIGHT_HINTS_H);
    assert_eq!(key(&mut v, RETURN), None);
}

#[test]
fn backspace_back_to_empty_shrinks_again() {
    let mut v = view();
    typed(&mut v, "st");
    v.input(PanelInput::Key(0x08), 0);
    v.input(PanelInput::Key(0x08), 0);
    assert_eq!(v.query(), "");
    assert_eq!(v.selected(), None);
    assert_eq!(v.size().1, theme::SPOTLIGHT_FIELD_H + theme::SPOTLIGHT_HINTS_H);
}

#[test]
fn new_catalog_keeps_selection_in_range() {
    let mut v = view();
    typed(&mut v, "s");
    key(&mut v, DOWN);
    key(&mut v, DOWN);
    assert_eq!(v.selected(), Some(2));
    v.set_catalog(Rc::new(vec![app("Steam", None, false)]));
    assert_eq!(v.selected(), Some(0));
    assert_eq!(key(&mut v, RETURN), Some(action_id(0, SpotlightOp::Open)));
}

#[test]
fn new_catalog_keeps_the_same_app_selected() {
    let mut v = view();
    typed(&mut v, "ste");
    let mut c = (*catalog()).clone();
    c.insert(0, app("Stellarium", None, false));
    let steam = index_of(&c, "Steam", false);
    v.set_catalog(Rc::new(c));
    assert_eq!(v.hits()[v.selected().unwrap()].index, steam);
}

#[test]
fn wake_at_follows_confirm() {
    let mut v = view();
    typed(&mut v, "shut");
    v.input(PanelInput::Key(RETURN), 1000);
    let w = v.wake_at(1000).unwrap();
    assert!(w > 1000 && w <= 1000 + CONFIRM_MS, "{w}");
}

/// With real text: every state draws through Direct2D without an error.
#[test]
fn view_draws_with_text() {
    use newwin_ui::popup::PopupStyle;
    use newwin_ui::spotlight::SpotlightLabels;
    use newwin_ui::{Backdrop, Gfx, Layer};
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_POPUP};
    let s = strings_for("en");
    let labels = SpotlightLabels {
        hint: s.search.into(),
        no_results: s.no_results.into(),
        kind_app: s.kind_app.into(),
        kind_system: s.kind_system.into(),
        press_again_shut_down: s.press_again_shut_down.into(),
        press_again_restart: s.press_again_restart.into(),
        keys: s.spotlight_keys.into(),
    };
    let gfx = Gfx::new().unwrap();
    let style = PopupStyle { font_family: "Segoe UI Variable Text".into(), font_size_px: 13.0, dpi: 96 };
    let mut v = SpotlightView::new(catalog(), Default::default(), labels, &style, 8, Some(530), &gfx);
    let hwnd = newwin_platform::window::create_test_window(WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP, WS_POPUP, RECT { left: 0, top: 0, right: 560, bottom: 400 }, "spot").unwrap();
    let mut layer = Layer::new(&gfx, hwnd, Backdrop::Tint, 560, 400).unwrap();
    let mut draw = |v: &mut SpotlightView, now| layer.draw(None, |p| v.paint(p, now)).unwrap();
    draw(&mut v, 0);
    typed(&mut v, "shut");
    draw(&mut v, 10);
    v.input(PanelInput::Key(RETURN), 20);
    draw(&mut v, 30);
    typed(&mut v, "zzz");
    draw(&mut v, 40);
}
