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
    // all at min 40 + gaps need > 120; reserve 24 for the » button: 40+4+40 = 84 ≤ 120-24-4 → 2 fit,
    // and the 2 visible labels share the whole 92 - 4 = 88 → 44 each
    assert_eq!(fit_labels(&[90., 90., 90., 90.], 120., 40., 4., 24.), LabelFit { widths: vec![44., 44.], overflow_from: Some(2) });
}

#[test]
fn visible_labels_use_spare_space_when_overflowing() {
    // budget 150-24-4 = 122 → 2 labels at min; 118 left for [200, 50] → 50 stays, the other gets 68
    assert_eq!(fit_labels(&[200., 50., 200., 200.], 150., 40., 4., 24.), LabelFit { widths: vec![68., 50.], overflow_from: Some(2) });
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
fn system_menu_strings() {
    let en = strings_for("en-US");
    assert_eq!(en.task_manager, "Task Manager");
    assert_eq!(en.powershell_admin, "Windows PowerShell (Admin)");
    assert_eq!(en.shut_down_or_sign_out, "Shut down or sign out");
    assert_eq!(en.newwin_settings, "NewWin settings…");
    assert_eq!(en.restart_newwin, "Restart NewWin");
    let uk = strings_for("uk-UA");
    assert_eq!(uk.task_manager, "Диспетчер завдань");
    assert_eq!(uk.desktop, "Робочий стіл");
}

#[test]
fn strings_fallback() {
    assert_eq!(strings_for("uk-UA").quit, "Вийти з NewWin");
    assert_eq!(strings_for("en-US").quit, "Quit NewWin");
    assert_eq!(strings_for("de-DE").quit, "Quit NewWin");
}

#[test]
fn pill_widths_keep_room_for_the_icon() {
    use newwin_core::layout::{fit_labels, pill_widths};
    assert_eq!(pill_widths(50.0, 10.0, 16.0, 6.0), (92.0, 36.0));
    let fit = fit_labels(&[92.0, 92.0], 80.0, 36.0, 4.0, 24.0);
    let shown = fit.overflow_from.unwrap_or(fit.widths.len());
    assert!(fit.widths[..shown].iter().all(|&w| w >= 36.0), "{fit:?}");
}
