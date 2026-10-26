use super::*;
use crate::sections::{Control, rows_for};
use crate::view::Section;
use newwin_config::Config;
use newwin_core::strings::strings_for;

fn body(section: Section) -> Body {
    let mut b = Body::new();
    b.set_rows(rows_for(section, &Config::default(), false), strings_for("en-US"), 0);
    b
}

#[test]
fn every_bar_control_is_focusable_in_order() {
    let kinds: Vec<_> = body(Section::Bar).focusables().iter().map(|f| f.0).collect();
    use FocusKind::*;
    assert_eq!(kinds, [Segmented, Slider, Slider, Segmented, Toggle, Toggle]);
}

#[test]
fn about_focuses_its_buttons_not_the_version() {
    let f = body(Section::About).focusables();
    assert_eq!(f.iter().map(|f| (f.0, f.2)).collect::<Vec<_>>(), [(FocusKind::Button, 1), (FocusKind::Button, 2), (FocusKind::Button, 3)]);
}

#[test]
fn keys_change_the_focused_control() {
    let mut b = body(Section::Bar);
    assert_eq!(b.activate(0, KeyAction::Step(1), 0), Some(BodyEvent::Set(RowId::Mode, RowValue::Choice(1))));
    assert_eq!(b.activate(0, KeyAction::Step(1), 0), None, "already on the last segment");
    assert_eq!(b.activate(1, KeyAction::Step(-1), 0), Some(BodyEvent::Set(RowId::ShowDelay, RowValue::Int(140))));
    assert_eq!(b.activate(4, KeyAction::Activate, 0), Some(BodyEvent::Set(RowId::IgnoreFullscreen, RowValue::Bool(false))));
    let mut a = body(Section::About);
    assert_eq!(a.activate(3, KeyAction::Activate, 0), Some(BodyEvent::Press(RowId::RestoreTaskbar)));
    let mut ap = body(Section::Appearance);
    let font = rows_for(Section::Appearance, &Config::default(), false).iter().position(|r| matches!(r.control, Control::Font(_))).unwrap();
    assert_eq!(ap.activate(font, KeyAction::Activate, 0), Some(BodyEvent::OpenFonts));
}

use newwin_config::tray_seen::{SeenApp, TraySeen};

/// Rows are 40 px high at 96 DPI; the toggle sits at x 438–474 (area 500 wide, 14 px for the
/// scroll thumb, 12 px padding), y 10–30 within its row.
fn list_body(section: Section, c: &Config, seen: &TraySeen) -> Body {
    let mut b = Body::new();
    b.set_list(sections::list_for(section, c, seen, strings_for("en-US")).unwrap(), 0);
    b.set_area(RectF::new(0.0, 0.0, 500.0, 400.0));
    b
}

fn items() -> Body {
    list_body(Section::Items, &Config::default(), &TraySeen::default())
}

fn seen(exes: &[&str]) -> TraySeen {
    TraySeen { apps: exes.iter().map(|e| SeenApp { exe: (*e).into(), name: (*e).into() }).collect() }
}

fn pt(x: f32, y: f32) -> PointF {
    PointF { x, y }
}

#[test]
fn a_press_on_a_row_toggle_switches_it_and_starts_no_drag() {
    let mut b = items();
    assert_eq!(b.press(pt(456.0, 60.0), 0), Some(BodyEvent::List(ListEdit::Toggled(1, false))));
    assert_eq!(b.move_to(pt(456.0, 200.0), 5), (None, false), "no row follows the pointer");
    assert_eq!(b.release(pt(456.0, 200.0), 10).0, None);
    assert_eq!(b.list_keys(), ["media", "layout", "network", "volume", "search", "date", "time"]);
}

#[test]
fn dragging_a_row_by_its_body_moves_it() {
    let mut b = items();
    assert_eq!(b.press(pt(50.0, 20.0), 0), None);
    assert_eq!(b.move_to(pt(50.0, 150.0), 5), (None, true));
    assert!(b.animating(10), "the rows in between glide aside");
    assert_eq!(b.release(pt(50.0, 150.0), 10), (Some(BodyEvent::List(ListEdit::Moved(0, 3))), true));
    b.commit_move(0, 3, 10);
    assert_eq!(b.list_keys(), ["layout", "network", "volume", "media", "search", "date", "time"]);
    assert_eq!(b.release(pt(50.0, 150.0), 20), (None, false), "a second release does nothing");
}

#[test]
fn a_press_and_release_in_place_moves_nothing() {
    let mut b = items();
    assert_eq!(b.press(pt(50.0, 100.0), 0), None);
    assert_eq!(b.release(pt(50.0, 100.0), 5), (None, true));
    assert_eq!(b.press(pt(50.0, 300.0), 0), None, "below the last row");
    assert_eq!(b.move_to(pt(50.0, 20.0), 5), (None, false));
    assert_eq!(b.release(pt(50.0, 20.0), 5).0, None);
}

#[test]
fn list_rows_are_focusable_space_toggles_and_steps_move() {
    let mut b = items();
    let f = b.focusables();
    assert_eq!(f.len(), 7);
    assert!(f.iter().all(|f| f.0 == FocusKind::ListRow));
    assert_eq!(b.activate(0, KeyAction::Activate, 0), Some(BodyEvent::List(ListEdit::Toggled(0, false))));
    assert_eq!(b.activate(0, KeyAction::Step(-1), 0), None, "the first row cannot go up");
    assert_eq!(b.activate(6, KeyAction::Step(1), 0), None, "the last row cannot go down");
    assert_eq!(b.activate(2, KeyAction::Step(1), 0), Some(BodyEvent::List(ListEdit::Moved(2, 3))));
}

#[test]
fn a_reload_with_the_same_rows_keeps_the_list_and_glides_toggles() {
    let mut b = items();
    let mut c = Config::default();
    c.items.hidden = vec!["date".into()];
    b.set_list(sections::list_for(Section::Items, &c, &TraySeen::default(), strings_for("en-US")).unwrap(), 100);
    assert!(b.animating(150));
    assert_eq!(b.activate(5, KeyAction::Activate, 200), Some(BodyEvent::List(ListEdit::Toggled(5, true))));
}

#[test]
fn a_new_tray_app_during_a_drag_waits_for_the_release() {
    let c = Config::default();
    let mut b = list_body(Section::Tray, &c, &seen(&["b.exe", "c.exe"]));
    assert_eq!(b.press(pt(50.0, 20.0), 0), None);
    b.set_list(sections::list_for(Section::Tray, &c, &seen(&["a.exe", "b.exe", "c.exe"]), strings_for("en-US")).unwrap(), 5);
    assert_eq!(b.list_keys(), ["b.exe", "c.exe"]);
    assert_eq!(b.move_to(pt(50.0, 70.0), 6), (None, true), "still dragging");
    assert_eq!(b.release(pt(50.0, 70.0), 7).0, Some(BodyEvent::List(ListEdit::Moved(0, 1))));
    b.set_list(sections::list_for(Section::Tray, &c, &seen(&["a.exe", "b.exe", "c.exe"]), strings_for("en-US")).unwrap(), 8);
    assert_eq!(b.list_keys(), ["a.exe", "b.exe", "c.exe"]);
}

#[test]
fn the_focus_follows_its_tray_row_when_apps_appear() {
    let c = Config::default();
    let mut b = list_body(Section::Tray, &c, &seen(&["b.exe", "c.exe"]));
    let key = b.focus_key(1).unwrap();
    assert_eq!(key, FocusKey::Item("c.exe".into()));
    b.set_list(sections::list_for(Section::Tray, &c, &seen(&["a.exe", "b.exe", "c.exe"]), strings_for("en-US")).unwrap(), 5);
    assert_eq!(b.focus_of(&key), Some(2));
    b.set_list(sections::list_for(Section::Tray, &c, &seen(&["a.exe"]), strings_for("en-US")).unwrap(), 6);
    assert_eq!(b.focus_of(&key), None, "its app is gone");
}

#[test]
fn a_row_focus_survives_a_row_appearing_above_it() {
    let en = strings_for("en-US");
    let mut c = Config::default();
    let mut b = Body::new();
    b.set_rows(rows_for(Section::Appearance, &c, false), en, 0);
    let seconds = b.rows.iter().position(|r| r.id == RowId::ShowSeconds).unwrap();
    let key = b.focus_key(seconds).unwrap();
    let before = b.focus_of(&key).unwrap();
    c.appearance.date_format = "dd.MM".into();
    b.set_rows(rows_for(Section::Appearance, &c, false), en, 0);
    assert_eq!(b.focus_of(&key), Some(before + 1), "the template field came in above");
}

#[test]
fn empty_tray_list_shows_the_hint() {
    let b = list_body(Section::Tray, &Config::default(), &TraySeen::default());
    assert_eq!(b.hint(), Some("Apps appear here after they put an icon in the tray."));
    assert!(b.focusables().is_empty());
    assert_eq!(items().hint(), None);
}

#[test]
fn an_outside_change_keeps_the_typed_template_while_the_field_is_focused() {
    let en = strings_for("en-US");
    let mut c = Config::default();
    c.appearance.date_format = "dd.MM".into();
    let mut b = Body::new();
    b.set_rows(rows_for(Section::Appearance, &c, false), en, 0);
    assert_eq!(b.field.edit.text(), "dd.MM");
    b.field.focused = true;
    c.appearance.date_format = "system".into();
    b.set_rows(rows_for(Section::Appearance, &c, true), en, 0);
    assert_eq!(b.field.edit.text(), "dd.MM");
}

fn long_tray() -> Body {
    let exes: Vec<String> = (0..20).map(|i| format!("app{i:02}.exe")).collect();
    let exes: Vec<&str> = exes.iter().map(String::as_str).collect();
    list_body(Section::Tray, &Config::default(), &seen(&exes))
}

#[test]
fn the_wheel_during_a_drag_moves_the_slot() {
    let mut b = long_tray();
    assert_eq!(b.press(pt(50.0, 20.0), 0), None);
    assert_eq!(b.move_to(pt(50.0, 25.0), 5), (None, true));
    // Three wheel notches scroll 3 rows or more; the pointer stays put, the row goes with the list.
    assert!(b.wheel(-360, 10));
    let (ev, _) = b.release(pt(50.0, 25.0), 20);
    let Some(BodyEvent::List(ListEdit::Moved(0, to))) = ev else { panic!("no move: {ev:?}") };
    assert!(to >= 3, "slot {to}");
}

#[test]
fn dragging_near_the_bottom_edge_scrolls_the_list() {
    let mut b = long_tray();
    assert_eq!(b.press(pt(50.0, 20.0), 0), None);
    assert_eq!(b.move_to(pt(50.0, 398.0), 5), (None, true));
    assert!(b.animating(5), "an autoscroll keeps the frames coming");
    let before = b.scroll_offset();
    b.autoscroll(5);
    assert!(b.autoscroll(105));
    assert!(b.scroll_offset() > before, "{} > {before}", b.scroll_offset());
    b.release(pt(50.0, 398.0), 110);
    assert!(!b.autoscroll(200), "no drag, no autoscroll");
}

#[test]
fn no_autoscroll_away_from_the_edges() {
    let mut b = long_tray();
    b.press(pt(50.0, 20.0), 0);
    b.move_to(pt(50.0, 200.0), 5);
    b.autoscroll(5);
    assert!(!b.autoscroll(105));
    assert_eq!(b.scroll_offset(), 0.0);
}

fn spotlight_body(c: &Config) -> Body {
    let mut b = Body::new();
    b.set_rows(rows_for(Section::Spotlight, c, false), strings_for("en-US"), 0);
    b.field.focused = true;
    b
}

fn type_in(b: &mut Body, s: &str) -> Vec<BodyEvent> {
    s.chars().filter_map(|c| b.field_input(newwin_ui::panel::PanelInput::Char(c), 0)).collect()
}

#[test]
fn the_link_field_adds_on_enter_and_clears() {
    let mut b = spotlight_body(&Config::default());
    assert!(type_in(&mut b, "gh https://github.com").is_empty(), "typing changes nothing yet");
    let enter = b.field_input(newwin_ui::panel::PanelInput::Key(0x0D), 0);
    assert_eq!(enter, Some(BodyEvent::Set(RowId::LinkAdd, RowValue::Text("gh https://github.com".into()))));
    assert_eq!(b.field.edit.text(), "", "ready for the next one");
}

#[test]
fn an_unfinished_link_stays_in_the_field() {
    let mut b = spotlight_body(&Config::default());
    type_in(&mut b, "onlyword");
    assert_eq!(b.field_input(newwin_ui::panel::PanelInput::Key(0x0D), 0), None);
    assert_eq!(b.field.edit.text(), "onlyword");
}

#[test]
fn a_link_row_is_a_remove_button() {
    let mut c = Config::default();
    c.spotlight.links.push(newwin_config::LinkConfig { keyword: "gh".into(), target: "https://github.com".into() });
    let mut b = spotlight_body(&c);
    let row = rows_for(Section::Spotlight, &c, false).iter().position(|r| r.id == RowId::Link(0)).unwrap();
    assert!(b.focusables().iter().any(|f| f.0 == FocusKind::Button && f.2 == row));
    assert_eq!(b.activate(row, KeyAction::Activate, 0), Some(BodyEvent::Press(RowId::Link(0))));
}

/// A picture of the Spotlight section for checking by eye: `cargo test -p newwin-settings
/// spotlight_section_preview -- --ignored`, then open `target/preview/settings_spotlight.bmp`.
#[test]
#[ignore]
fn spotlight_section_preview() {
    let mut c = Config::default();
    c.spotlight.links.push(newwin_config::LinkConfig { keyword: "gh".into(), target: "https://github.com/ynujeqax".into() });
    c.spotlight.links.push(newwin_config::LinkConfig { keyword: "dl".into(), target: r"%USERPROFILE%\Downloads".into() });
    let mut b = spotlight_body(&c);
    type_in(&mut b, "yt https://youtube.com");
    let gfx = newwin_ui::Gfx::new().unwrap();
    let mut cache = TextCache::new(&gfx);
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/preview");
    std::fs::create_dir_all(dir).unwrap();
    let (w, h) = (520, 560);
    newwin_ui::preview::render_bmp(&format!("{dir}/settings_spotlight.bmp"), w, h, theme::GLASS_TINT_SOLID.with_alpha(1.0), |p| {
        b.paint(p, &mut cache, strings_for("en-US"), RectF::new(16.0, 8.0, w as f32 - 32.0, h as f32 - 16.0), 10_000, None);
    })
    .unwrap();
}
