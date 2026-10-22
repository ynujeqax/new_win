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
