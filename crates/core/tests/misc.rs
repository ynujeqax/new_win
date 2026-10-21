use std::time::{Duration, Instant};

use newwin_core::clock::ms_until_next_minute;
use newwin_core::layout::{LabelFit, fit_labels};
use newwin_core::strings::strings_for;
use newwin_core::watchdog::{CrashPolicy, ExitDecision, HangMonitor, PingDecision};

#[test]
fn labels_fit_naturally() {
    assert_eq!(fit_labels(&[50., 60.], 200., 40., 4., 24.), LabelFit { widths: vec![50., 60.], overflow_from: None });
}

#[test]
fn longest_labels_shrink_first() {
    // available 150 = gaps 4 + widths 146 → cap c with 30 + min(100,c) + min(80,c) = 146 → c = 58
    assert_eq!(fit_labels(&[30., 100., 80.], 150., 40., 2., 24.), LabelFit { widths: vec![30., 58., 58.], overflow_from: None });
}

#[test]
fn overflow_when_min_widths_do_not_fit() {
    // all at min 40 + gaps need > 120; reserve 24 for the » button: 40+4+40 = 84 ≤ 120-24-4 → 2 fit
    assert_eq!(fit_labels(&[90., 90., 90., 90.], 120., 40., 4., 24.), LabelFit { widths: vec![40., 40.], overflow_from: Some(2) });
}

#[test]
fn empty_and_tiny_inputs() {
    assert_eq!(fit_labels(&[], 100., 40., 4., 24.), LabelFit { widths: vec![], overflow_from: None });
    assert_eq!(fit_labels(&[90.], 10., 40., 4., 24.), LabelFit { widths: vec![], overflow_from: Some(0) });
}

#[test]
fn three_crashes_in_a_minute_give_up() {
    let mut p = CrashPolicy::new();
    let t = Instant::now();
    assert_eq!(p.on_exit(1, t), ExitDecision::Restart);
    assert_eq!(p.on_exit(1, t + Duration::from_secs(10)), ExitDecision::Restart);
    assert_eq!(p.on_exit(1, t + Duration::from_secs(20)), ExitDecision::GiveUp);
}

#[test]
fn old_crashes_expire_and_codes_0_and_3_are_not_crashes() {
    let mut p = CrashPolicy::new();
    let t = Instant::now();
    p.on_exit(1, t);
    p.on_exit(1, t + Duration::from_secs(5));
    assert_eq!(p.on_exit(1, t + Duration::from_secs(70)), ExitDecision::Restart);
    assert_eq!(p.on_exit(3, t + Duration::from_secs(71)), ExitDecision::Restart);
    assert_eq!(p.on_exit(0, t + Duration::from_secs(72)), ExitDecision::Stop);
}

#[test]
fn hang_after_three_missed_pings() {
    let mut h = HangMonitor::new();
    assert_eq!(h.on_ping(false), PingDecision::Healthy);
    assert_eq!(h.on_ping(false), PingDecision::Healthy);
    assert_eq!(h.on_ping(true), PingDecision::Healthy);
    h.on_ping(false);
    h.on_ping(false);
    assert_eq!(h.on_ping(false), PingDecision::Kill);
}

#[test]
fn minute_alignment() {
    assert_eq!(ms_until_next_minute(0, 0), 60_050);
    assert_eq!(ms_until_next_minute(59, 990), 60);
}

#[test]
fn strings_fallback() {
    assert_eq!(strings_for("uk-UA").quit, "Вийти з NewWin");
    assert_eq!(strings_for("en-US").quit, "Quit NewWin");
    assert_eq!(strings_for("de-DE").quit, "Quit NewWin");
}
