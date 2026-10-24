//! Watchdog decisions: restart the shell after a crash, give up after repeated crashes,
//! kill it after repeated unanswered pings.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// The shell's hidden window: the watchdog pings it, Settings posts its quit request to it.
pub const PING_WINDOW_CLASS: &str = "NewWin.Ping";

const CRASH_WINDOW: Duration = Duration::from_secs(60);
const MAX_CRASHES: usize = 3;
const MAX_MISSED_PINGS: u32 = 3;

/// Shell exit codes with a meaning other than "crashed".
pub const EXIT_QUIT: u32 = 0;
pub const EXIT_RESTART: u32 = 3;

#[derive(Debug, PartialEq, Eq)]
pub enum ExitDecision {
    Restart,
    GiveUp,
    Stop,
}

pub struct CrashPolicy {
    crashes: VecDeque<Instant>,
}

impl CrashPolicy {
    pub fn new() -> Self {
        Self { crashes: VecDeque::new() }
    }

    pub fn on_exit(&mut self, exit_code: u32, now: Instant) -> ExitDecision {
        match exit_code {
            EXIT_QUIT => return ExitDecision::Stop,
            EXIT_RESTART => return ExitDecision::Restart,
            _ => {}
        }
        while self.crashes.front().is_some_and(|&t| now.duration_since(t) > CRASH_WINDOW) {
            self.crashes.pop_front();
        }
        self.crashes.push_back(now);
        if self.crashes.len() >= MAX_CRASHES { ExitDecision::GiveUp } else { ExitDecision::Restart }
    }
}

impl Default for CrashPolicy {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum PingDecision {
    Healthy,
    Kill,
}

pub struct HangMonitor {
    missed: u32,
}

impl HangMonitor {
    pub fn new() -> Self {
        Self { missed: 0 }
    }

    pub fn on_ping(&mut self, responded: bool) -> PingDecision {
        if responded {
            self.missed = 0;
            return PingDecision::Healthy;
        }
        self.missed += 1;
        if self.missed >= MAX_MISSED_PINGS {
            self.missed = 0;
            PingDecision::Kill
        } else {
            PingDecision::Healthy
        }
    }
}

impl Default for HangMonitor {
    fn default() -> Self {
        Self::new()
    }
}
