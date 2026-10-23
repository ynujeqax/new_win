use newwin_core::visibility::*;

fn cfg(mode: Mode) -> VisConfig {
    VisConfig { mode, show_delay_ms: 150, hide_delay_ms: 400, ignore_fullscreen: true }
}

fn shown() -> Visibility {
    let (mut v, _) = Visibility::new(cfg(Mode::AutoHide));
    v.handle(VisInput::ZoneEnter);
    v.handle(VisInput::Timer);
    v
}

#[test]
fn autohide_starts_hidden_with_strip() {
    let (v, c) = Visibility::new(cfg(Mode::AutoHide));
    assert_eq!(v.state(), VisState::Hidden);
    assert_eq!(c, vec![VisCmd::SetCatcher(Catcher::Strip)]);
}

#[test]
fn pinned_starts_shown_without_catcher() {
    let (v, c) = Visibility::new(cfg(Mode::Pinned));
    assert_eq!(v.state(), VisState::Shown);
    assert_eq!(c, vec![VisCmd::SetCatcher(Catcher::Off), VisCmd::AnimateIn]);
}

#[test]
fn reveal_after_delay() {
    let (mut v, _) = Visibility::new(cfg(Mode::AutoHide));
    assert_eq!(v.handle(VisInput::ZoneEnter), vec![VisCmd::StartTimer(150)]);
    assert_eq!(v.handle(VisInput::Timer), vec![VisCmd::SetCatcher(Catcher::Zone), VisCmd::AnimateIn]);
    assert_eq!(v.state(), VisState::Shown);
}

#[test]
fn leaving_before_delay_cancels() {
    let (mut v, _) = Visibility::new(cfg(Mode::AutoHide));
    v.handle(VisInput::ZoneEnter);
    assert_eq!(v.handle(VisInput::ZoneLeave), vec![VisCmd::CancelTimer]);
    assert_eq!(v.state(), VisState::Hidden);
    assert_eq!(v.handle(VisInput::Timer), vec![]);
}

#[test]
fn hide_after_delay_and_reenter_cancels() {
    let mut v = shown();
    assert_eq!(v.handle(VisInput::ZoneLeave), vec![VisCmd::StartTimer(400)]);
    assert_eq!(v.handle(VisInput::ZoneEnter), vec![VisCmd::CancelTimer]);
    v.handle(VisInput::ZoneLeave);
    assert_eq!(v.handle(VisInput::Timer), vec![VisCmd::AnimateOut, VisCmd::SetCatcher(Catcher::Strip)]);
    assert_eq!(v.state(), VisState::Hidden);
}

#[test]
fn overlay_blocks_hiding() {
    let mut v = shown();
    v.handle(VisInput::OverlayOpened);
    assert_eq!(v.handle(VisInput::ZoneLeave), vec![]);
    assert_eq!(v.state(), VisState::Shown);
    assert_eq!(v.handle(VisInput::OverlayClosed), vec![VisCmd::StartTimer(400)]);
}

#[test]
fn overlay_closed_while_pointer_inside_keeps_shown() {
    let mut v = shown();
    v.handle(VisInput::OverlayOpened);
    assert_eq!(v.handle(VisInput::OverlayClosed), vec![]);
}

#[test]
fn jitter_never_sticks() {
    let (mut v, _) = Visibility::new(cfg(Mode::AutoHide));
    for _ in 0..50 {
        v.handle(VisInput::ZoneEnter);
        v.handle(VisInput::ZoneLeave);
    }
    assert_eq!(v.state(), VisState::Hidden);
    v.handle(VisInput::ZoneEnter);
    v.handle(VisInput::Timer);
    v.handle(VisInput::ZoneLeave);
    v.handle(VisInput::ZoneEnter);
    assert_eq!(v.state(), VisState::Shown);
}

#[test]
fn fullscreen_disables_trigger_only_when_hidden_and_enabled() {
    let (mut v, _) = Visibility::new(cfg(Mode::AutoHide));
    assert_eq!(v.handle(VisInput::Fullscreen(true)), vec![VisCmd::SetCatcher(Catcher::Off)]);
    assert_eq!(v.handle(VisInput::ZoneEnter), vec![]);
    assert_eq!(v.handle(VisInput::Fullscreen(false)), vec![VisCmd::SetCatcher(Catcher::Strip)]);
    let (mut w, _) = Visibility::new(VisConfig { ignore_fullscreen: false, ..cfg(Mode::AutoHide) });
    assert_eq!(w.handle(VisInput::Fullscreen(true)), vec![]);
}

#[test]
fn hiding_during_fullscreen_turns_catcher_off() {
    let mut v = shown();
    assert_eq!(v.handle(VisInput::Fullscreen(true)), vec![]);
    v.handle(VisInput::ZoneLeave);
    assert_eq!(v.handle(VisInput::Timer), vec![VisCmd::AnimateOut, VisCmd::SetCatcher(Catcher::Off)]);
}

#[test]
fn switching_modes() {
    let (mut v, _) = Visibility::new(cfg(Mode::AutoHide));
    assert_eq!(v.set_mode(Mode::Pinned), vec![VisCmd::CancelTimer, VisCmd::SetCatcher(Catcher::Off), VisCmd::AnimateIn]);
    assert_eq!(v.handle(VisInput::ZoneLeave), vec![]);
    assert_eq!(v.set_mode(Mode::AutoHide), vec![VisCmd::SetCatcher(Catcher::Zone), VisCmd::StartTimer(400)]);
}

#[test]
fn new_delays_apply_to_the_next_timer() {
    let (mut v, _) = Visibility::new(cfg(Mode::AutoHide));
    assert_eq!(v.set_config(VisConfig { show_delay_ms: 0, ..cfg(Mode::AutoHide) }), vec![]);
    assert_eq!(v.handle(VisInput::ZoneEnter), vec![VisCmd::StartTimer(0)]);
    v.handle(VisInput::Timer);
    v.set_config(VisConfig { hide_delay_ms: 1200, ..cfg(Mode::AutoHide) });
    assert_eq!(v.handle(VisInput::ZoneLeave), vec![VisCmd::StartTimer(1200)]);
}

#[test]
fn switching_to_pinned_shows_and_stops_the_catcher() {
    let (mut v, _) = Visibility::new(cfg(Mode::AutoHide));
    // Through set_mode: the bar was hidden, so it comes in; no timer may be left running.
    assert_eq!(v.set_config(cfg(Mode::Pinned)), vec![VisCmd::CancelTimer, VisCmd::SetCatcher(Catcher::Off), VisCmd::AnimateIn]);
    assert_eq!(v.state(), VisState::Shown);
    assert_eq!(v.set_config(cfg(Mode::Pinned)), vec![], "an unchanged mode does nothing");
}

#[test]
fn turning_ignore_fullscreen_off_while_hidden_brings_the_strip_back() {
    let (mut v, _) = Visibility::new(cfg(Mode::AutoHide));
    assert_eq!(v.handle(VisInput::Fullscreen(true)), vec![VisCmd::SetCatcher(Catcher::Off)]);
    assert_eq!(v.set_config(VisConfig { ignore_fullscreen: false, ..cfg(Mode::AutoHide) }), vec![VisCmd::SetCatcher(Catcher::Strip)]);
    assert_eq!(v.handle(VisInput::ZoneEnter), vec![VisCmd::StartTimer(150)]);
}
