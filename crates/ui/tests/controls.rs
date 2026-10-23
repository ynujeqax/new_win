use newwin_ui::nav::Nav;
use newwin_ui::panel::PanelInput;
use newwin_ui::reorder_list::{ReorderList, ReorderRow};
use newwin_ui::scroll::Scroll;
use newwin_ui::text_field::TextField;
use newwin_ui::segmented::Segmented;
use newwin_ui::slider::{Range, level_at, thumb_x, thumb_x_for, value_at};
use newwin_ui::toggle::Toggle;
use newwin_ui::{PointF, RectF};

#[test]
fn value_at_snaps_to_step_and_clamps() {
    let track = RectF::new(0.0, 0.0, 100.0, 4.0);
    let r = Range { min: 0, max: 1000, step: 10 };
    assert_eq!(value_at(14.6, track, r), 150);
    assert_eq!(value_at(-5.0, track, r), 0);
    assert_eq!(value_at(500.0, track, r), 1000);
}

#[test]
fn value_at_never_panics_on_degenerate_input() {
    let r = Range { min: 5, max: 5, step: 1 };
    assert_eq!(value_at(10.0, RectF::new(0.0, 0.0, 100.0, 4.0), r), 5);
    assert_eq!(thumb_x_for(5, RectF::new(7.0, 0.0, 100.0, 4.0), r), 7.0);
    let zero = RectF::new(0.0, 0.0, 0.0, 4.0);
    assert_eq!(value_at(3.0, zero, Range { min: 2, max: 9, step: 0 }), 2);
    assert_eq!(value_at(f32::NAN, RectF::new(0.0, 0.0, 100.0, 4.0), Range { min: 0, max: 10, step: 1 }), 0);
}

#[test]
fn thumb_round_trips_through_value() {
    let track = RectF::new(20.0, 0.0, 130.0, 4.0);
    let r = Range { min: 3, max: 15, step: 1 };
    for v in [3, 9, 15] {
        assert_eq!(value_at(thumb_x_for(v, track, r), track, r), v);
    }
}

#[test]
fn segmented_hit_picks_the_segment_under_the_pointer() {
    let s = Segmented::new(vec!["A".into(), "B".into()], 0);
    let layout = s.layout(PointF { x: 10.0, y: 10.0 }, &[20.0, 30.0], 96);
    assert_eq!(layout.len(), 2);
    assert!(layout[1].x >= layout[0].right());
    let second = PointF { x: layout[1].x + layout[1].w / 2.0, y: layout[1].y + layout[1].h / 2.0 };
    assert_eq!(s.hit(&layout, second), Some(1));
    assert_eq!(s.hit(&layout, PointF { x: 0.0, y: 0.0 }), None);
    let empty = Segmented::new(Vec::new(), 3);
    assert!(empty.layout(PointF::default(), &[], 96).is_empty());
    assert_eq!(empty.hit(&[], second), None);
}

#[test]
fn segmented_select_moves_the_pill_over_time() {
    let mut s = Segmented::new(vec!["A".into(), "B".into()], 0);
    s.select(1, 0);
    assert_eq!(s.selected, 1);
    assert!(s.animating(100));
    assert!(!s.animating(500));
    s.select(9, 600);
    assert_eq!(s.selected, 1, "out of range is ignored");
}

#[test]
fn toggle_animates_for_150_ms_after_a_flip() {
    let mut t = Toggle::new(false);
    assert!(!t.animating(0));
    t.flip(0);
    assert!(t.on);
    assert!(t.animating(100));
    assert!(!t.animating(151));
    let r = Toggle::rect(PointF { x: 1.0, y: 2.0 }, 192);
    assert_eq!((r.w, r.h), (72.0, 40.0));
}

#[test]
fn volume_slider_levels_are_unchanged() {
    let track = RectF::new(100.0, 10.0, 200.0, 4.0);
    assert_eq!(level_at(100.0, track), 0);
    assert_eq!(level_at(200.0, track), 50);
    assert_eq!(level_at(999.0, track), 100);
    assert_eq!(level_at(-5.0, track), 0);
    assert_eq!(thumb_x(50, track), 200.0);
    assert_eq!(thumb_x(100, track), 300.0);
}

#[test]
fn scroll_clamps_and_hides_thumb_when_content_fits() {
    let view = RectF::new(0.0, 0.0, 300.0, 400.0);
    let mut s = Scroll { offset: 0.0, content_h: 300.0, view_h: 400.0 };
    assert_eq!(s.thumb_rect(view), None);
    s.wheel(-120);
    assert_eq!(s.offset, 0.0);
    let mut s = Scroll { offset: 0.0, content_h: 900.0, view_h: 400.0 };
    s.wheel(-120);
    assert_eq!(s.offset, 48.0);
    s.wheel(120);
    s.wheel(120);
    assert_eq!(s.offset, 0.0);
    for _ in 0..50 {
        s.wheel(-120);
    }
    assert_eq!(s.offset, 500.0);
    let t = s.thumb_rect(view).unwrap();
    assert!((t.bottom() - 400.0).abs() < 0.01 && t.h > 0.0 && t.h < 400.0);
    let mut nan = Scroll { offset: f32::NAN, content_h: f32::NAN, view_h: 0.0 };
    nan.clamp();
    assert_eq!(nan.offset, 0.0);
    assert_eq!(nan.thumb_rect(RectF::new(0.0, 0.0, 10.0, 0.0)), None);
}

#[test]
fn scroll_thumb_drag_maps_pointer_to_offset() {
    let view = RectF::new(0.0, 0.0, 300.0, 400.0);
    let mut s = Scroll { offset: 0.0, content_h: 900.0, view_h: 400.0 };
    let t = s.thumb_rect(view).unwrap();
    s.drag_thumb(view, 400.0, t.h / 2.0);
    assert_eq!(s.offset, 500.0);
    s.drag_thumb(view, -100.0, t.h / 2.0);
    assert_eq!(s.offset, 0.0);
}

fn rows(n: usize) -> Vec<ReorderRow> {
    (0..n).map(|i| ReorderRow { label: format!("r{i}"), detail: None, on: Toggle::new(true) }).collect()
}

#[test]
fn reorder_list_drag_reports_from_and_to() {
    let mut l = ReorderList::new(rows(4));
    l.set_layout(PointF { x: 0.0, y: 0.0 }, 300.0, 30.0, 96);
    assert!(l.press(PointF { x: 20.0, y: 15.0 }));
    l.drag_to(PointF { x: 20.0, y: 85.0 });
    assert_eq!(l.release(), Some((0, 2)));
    assert!(l.drag.is_none());
    assert_eq!(l.release(), None);
    // a click without moving is not a reorder
    assert!(l.press(PointF { x: 20.0, y: 45.0 }));
    assert_eq!(l.release(), None);
    // far outside the list clamps
    assert!(l.press(PointF { x: 20.0, y: 15.0 }));
    l.drag_to(PointF { x: 900.0, y: 9000.0 });
    assert_eq!(l.release(), Some((0, 3)));
}

#[test]
fn reorder_list_never_panics_on_degenerate_input() {
    let mut l = ReorderList::new(Vec::new());
    l.set_layout(PointF::default(), 0.0, 0.0, 96);
    assert!(!l.press(PointF { x: 1.0, y: 1.0 }));
    l.drag_to(PointF { x: f32::NAN, y: f32::NAN });
    assert_eq!(l.release(), None);
    let mut l = ReorderList::new(rows(2));
    l.set_layout(PointF::default(), 300.0, 0.0, 96);
    assert!(!l.press(PointF { x: 1.0, y: 1.0 }));
}

#[test]
fn reorder_list_commit_moves_rows_and_glides_150_ms() {
    let mut l = ReorderList::new(rows(3));
    l.set_layout(PointF::default(), 300.0, 30.0, 96);
    l.press(PointF { x: 20.0, y: 15.0 });
    l.drag_to(PointF { x: 20.0, y: 85.0 });
    let (from, to) = l.release().unwrap();
    l.commit(from, to, 0);
    assert_eq!(l.rows[2].label, "r0");
    assert_eq!(l.rows[0].label, "r1");
    assert!(l.animating(100));
    assert!(!l.animating(151));
}

#[test]
fn reorder_list_press_on_the_toggle_does_not_drag() {
    let mut l = ReorderList::new(rows(2));
    l.set_layout(PointF::default(), 300.0, 30.0, 96);
    let t = l.toggle_rect(0);
    assert_eq!(l.toggle_hit(PointF { x: t.x + 2.0, y: t.y + 2.0 }), Some(0));
    assert!(!l.press(PointF { x: t.x + 2.0, y: t.y + 2.0 }));
}

#[test]
fn nav_hit_selects_the_row() {
    let items: Vec<String> = (0..6).map(|i| format!("Item {i}")).collect();
    let mut n = Nav::new(items, 0);
    let layout = n.layout(PointF { x: 10.0, y: 10.0 }, 160.0, 96);
    assert_eq!(layout.len(), 6);
    let third = PointF { x: layout[2].x + 5.0, y: layout[2].y + layout[2].h / 2.0 };
    assert_eq!(n.hit(&layout, third), Some(2));
    assert_eq!(n.hit(&layout, PointF { x: 0.0, y: 0.0 }), None);
    n.select(2, 0);
    assert_eq!(n.selected, 2);
    assert!(n.animating(100));
    assert!(!n.animating(200));
    n.select(99, 300);
    assert_eq!(n.selected, 2);
    assert!(Nav::new(Vec::new(), 4).layout(PointF::default(), 100.0, 96).is_empty());
}

#[test]
fn text_field_edits_only_when_focused() {
    let mut f = TextField::default();
    assert!(!f.input(PanelInput::Char('a')));
    f.focused = true;
    assert!(f.input(PanelInput::Char('a')));
    assert!(f.input(PanelInput::Char('b')));
    assert_eq!(f.edit.text(), "ab");
    assert!(!f.input(PanelInput::Key(0x25)), "Left moves the caret only");
    assert!(f.input(PanelInput::Key(0x08)));
    assert_eq!(f.edit.text(), "b");
    assert!(!f.input(PanelInput::Key(0x23)), "End");
    assert!(f.input(PanelInput::CtrlKey(0x08)));
    assert_eq!(f.edit.text(), "");
    assert!(!f.input(PanelInput::Wheel(1)));
}

#[test]
fn text_field_caret_blinks_and_restarts_on_edit() {
    let mut f = TextField { focused: true, ..Default::default() };
    assert!(f.caret_on(0, Some(500)));
    assert!(!f.caret_on(500, Some(500)));
    f.input_at(PanelInput::Char('x'), 700);
    assert!(f.caret_on(900, Some(500)));
    assert!(f.caret_on(10_000, None));
}
