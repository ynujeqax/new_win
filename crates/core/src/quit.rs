//! `NewWin.exe --quit` (M5 spec §3): ask every NewWin process to go, wait, then end the rest.

/// Exists while a `--quit` runs; the watchdog does not restart the shell while it does.
pub const QUITTING_EVENT: &str = "Local\\NewWin.Quitting";
/// How long the processes get to quit by themselves.
pub const QUIT_WAIT_MS: u64 = 3000;
/// How often `--quit` looks for them while it waits.
pub const QUIT_POLL_MS: u32 = 100;
/// Rounds of terminating what is left before giving up.
pub const MAX_KILL_ROUNDS: u32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuitStep {
    /// Still going by itself: look again in [`QUIT_POLL_MS`].
    Wait,
    /// Terminate every NewWin process that is left.
    Kill,
    /// Done: restore the taskbar and exit with `code` (1 = processes are left).
    Restore { code: i32 },
}

#[derive(Clone, Copy, Debug)]
pub struct QuitPlan {
    started: u64,
    kills: u32,
}

impl QuitPlan {
    pub fn new(now_ms: u64) -> QuitPlan {
        QuitPlan { started: now_ms, kills: 0 }
    }

    /// The next step at `now_ms`; `alive` = some NewWin process other than this one runs.
    pub fn step(&mut self, now_ms: u64, alive: bool) -> QuitStep {
        if !alive {
            QuitStep::Restore { code: 0 }
        } else if now_ms.saturating_sub(self.started) < QUIT_WAIT_MS {
            QuitStep::Wait
        } else if self.kills < MAX_KILL_ROUNDS {
            self.kills += 1;
            QuitStep::Kill
        } else {
            QuitStep::Restore { code: 1 }
        }
    }
}
