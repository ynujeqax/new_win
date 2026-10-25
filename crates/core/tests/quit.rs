use newwin_core::quit::{QUIT_WAIT_MS, QuitPlan, QuitStep};
use newwin_core::watchdog::{ExitDecision, while_quitting};

#[test]
fn quit_with_nothing_running_restores_at_once() {
    assert_eq!(QuitPlan::new(0).step(0, false), QuitStep::Restore { code: 0 });
}

#[test]
fn a_quitting_shell_is_waited_for() {
    let mut q = QuitPlan::new(0);
    assert_eq!(q.step(100, true), QuitStep::Wait);
    assert_eq!(q.step(2999, true), QuitStep::Wait);
    assert_eq!(q.step(3100, false), QuitStep::Restore { code: 0 });
}

#[test]
fn a_hung_shell_is_killed_after_3_s() {
    let mut q = QuitPlan::new(1000);
    assert_eq!(q.step(1000 + QUIT_WAIT_MS - 1, true), QuitStep::Wait);
    assert_eq!(q.step(1000 + QUIT_WAIT_MS, true), QuitStep::Kill);
}

#[test]
fn three_kill_rounds_then_give_up_with_code_1() {
    let mut q = QuitPlan::new(0);
    for t in [3000, 3100, 3200] {
        assert_eq!(q.step(t, true), QuitStep::Kill);
    }
    assert_eq!(q.step(3300, true), QuitStep::Restore { code: 1 });
}

#[test]
fn a_kill_that_worked_restores_with_code_0() {
    let mut q = QuitPlan::new(0);
    assert_eq!(q.step(3000, true), QuitStep::Kill);
    assert_eq!(q.step(3100, false), QuitStep::Restore { code: 0 });
}

#[test]
fn watchdog_stops_instead_of_restarting_while_quitting() {
    assert_eq!(while_quitting(ExitDecision::Restart, true), ExitDecision::Stop);
    assert_eq!(while_quitting(ExitDecision::GiveUp, true), ExitDecision::Stop);
    assert_eq!(while_quitting(ExitDecision::Stop, true), ExitDecision::Stop);
    assert_eq!(while_quitting(ExitDecision::Restart, false), ExitDecision::Restart);
    assert_eq!(while_quitting(ExitDecision::GiveUp, false), ExitDecision::GiveUp);
}
