use newwin_core::audio::*;

#[test]
fn waves_thresholds() {
    assert_eq!([0, 1, 33, 34, 66, 67, 100].map(waves_for), [0, 1, 1, 2, 2, 3, 3]);
}

#[test]
fn percent_rounds_and_clamps() {
    assert_eq!([0.0, 0.424, 0.425, 1.0, 1.7, -0.2, f32::NAN].map(percent), [0, 42, 43, 100, 100, 0, 0]);
}

#[test]
fn wheel_accumulates_touchpad_deltas() {
    let mut w = Wheel::default();
    assert_eq!(w.notches(40), 0);
    assert_eq!(w.notches(40), 0);
    assert_eq!(w.notches(40), 1);
    assert_eq!(w.notches(-240), -2);
}

#[test]
fn step_clamps() {
    assert_eq!(step(99, 1, 2), 100);
    assert_eq!(step(1, -1, 2), 0);
    assert_eq!(step(50, 3, 2), 56);
}

#[test]
fn no_device_maps_to_unavailable() {
    assert_eq!(speaker_icon(None), SpeakerIcon::Unavailable);
    assert_eq!(speaker_icon(Some((0, false))), SpeakerIcon::Level { waves: 0, muted: false });
    assert_eq!(speaker_icon(Some((80, true))), SpeakerIcon::Level { waves: 3, muted: true });
}
