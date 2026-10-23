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
