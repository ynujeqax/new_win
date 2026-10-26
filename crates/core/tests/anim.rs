use newwin_core::anim::Tween;

#[test]
fn tween_eases_out_and_rests() {
    let mut t = Tween::at_rest(0.0);
    t.to(100.0, 1000, 100);
    assert_eq!(t.value(1000), 0.0);
    assert_eq!(t.value(1050), 87.5);
    assert_eq!(t.value(1100), 100.0);
    assert!(t.running(1099) && !t.running(1100));
}

#[test]
fn retarget_starts_from_current_value() {
    let mut t = Tween::at_rest(0.0);
    t.to(100.0, 0, 100);
    t.to(0.0, 50, 100);
    assert_eq!(t.value(50), 87.5);
    assert_eq!(t.value(150), 0.0);
    let before = t;
    t.to(0.0, 60, 100);
    assert_eq!(t, before, "same target is a no-op");
}

#[test]
fn at_rest_is_not_running() {
    let t = Tween::at_rest(3.0);
    assert_eq!((t.value(0), t.target(), t.running(0)), (3.0, 3.0, false));
}

#[test]
fn a_pill_glides_between_items_and_appears_in_place() {
    use newwin_core::anim::GlidePill;
    let mut p = GlidePill::hidden();
    assert_eq!(p.at(0), None);
    p.show(100.0, 40.0, 0, 100, 50);
    let (x, w, _) = p.at(10).unwrap();
    assert_eq!((x, w), (100.0, 40.0), "first show: in place, only fading in");
    p.show(200.0, 60.0, 1000, 100, 50);
    let (x, _, o) = p.at(1050).unwrap();
    assert!(x > 100.0 && x < 200.0 && o == 1.0, "glides while fully shown: {x}");
    assert!(p.running(1050) && !p.running(1100));
    p.hide(2000, 50);
    assert_eq!(p.at(2050), None);
    p.show(10.0, 20.0, 3000, 100, 50);
    assert_eq!(p.at(3001).map(|(x, ..)| x), Some(10.0), "after hiding it appears in place again");
}
