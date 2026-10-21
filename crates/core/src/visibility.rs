//! When the bar is shown or hidden. The shell feeds pointer/overlay/fullscreen events
//! and executes the returned commands (timers, animations, catcher window size).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Pinned,
    AutoHide,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisState {
    Hidden,
    PendingShow,
    Shown,
    PendingHide,
}

/// Size of the invisible pointer-catching window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Catcher {
    Off,
    /// Thin strip on the top edge (bar hidden).
    Strip,
    /// Whole hot zone incl. margins (bar shown), so the gaps never lose the pointer.
    Zone,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisInput {
    ZoneEnter,
    ZoneLeave,
    OverlayOpened,
    OverlayClosed,
    Timer,
    Fullscreen(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisCmd {
    StartTimer(u32),
    CancelTimer,
    AnimateIn,
    AnimateOut,
    SetCatcher(Catcher),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisConfig {
    pub mode: Mode,
    pub show_delay_ms: u32,
    pub hide_delay_ms: u32,
    pub ignore_fullscreen: bool,
}

pub struct Visibility {
    cfg: VisConfig,
    state: VisState,
    pointer_in_zone: bool,
    overlays: u32,
    fullscreen: bool,
}

impl Visibility {
    pub fn new(cfg: VisConfig) -> (Self, Vec<VisCmd>) {
        let (state, cmds) = match cfg.mode {
            Mode::AutoHide => (VisState::Hidden, vec![VisCmd::SetCatcher(Catcher::Strip)]),
            Mode::Pinned => (VisState::Shown, vec![VisCmd::SetCatcher(Catcher::Off), VisCmd::AnimateIn]),
        };
        (Self { cfg, state, pointer_in_zone: false, overlays: 0, fullscreen: false }, cmds)
    }

    pub fn state(&self) -> VisState {
        self.state
    }

    fn trigger_blocked(&self) -> bool {
        self.fullscreen && self.cfg.ignore_fullscreen
    }

    fn rest_catcher(&self) -> Catcher {
        if self.trigger_blocked() { Catcher::Off } else { Catcher::Strip }
    }

    pub fn handle(&mut self, input: VisInput) -> Vec<VisCmd> {
        use VisCmd::*;
        use VisState::*;

        if self.cfg.mode == Mode::Pinned {
            match input {
                VisInput::OverlayOpened => self.overlays += 1,
                VisInput::OverlayClosed => self.overlays = self.overlays.saturating_sub(1),
                VisInput::Fullscreen(fs) => self.fullscreen = fs,
                VisInput::ZoneEnter => self.pointer_in_zone = true,
                VisInput::ZoneLeave => self.pointer_in_zone = false,
                VisInput::Timer => {}
            }
            return vec![];
        }

        match input {
            VisInput::ZoneEnter => match self.state {
                Hidden if self.trigger_blocked() => vec![],
                Hidden => {
                    self.pointer_in_zone = true;
                    self.state = PendingShow;
                    vec![StartTimer(self.cfg.show_delay_ms)]
                }
                PendingHide => {
                    self.pointer_in_zone = true;
                    self.state = Shown;
                    vec![CancelTimer]
                }
                _ => {
                    self.pointer_in_zone = true;
                    vec![]
                }
            },
            VisInput::ZoneLeave => {
                self.pointer_in_zone = false;
                match self.state {
                    PendingShow => {
                        self.state = Hidden;
                        vec![CancelTimer]
                    }
                    Shown if self.overlays == 0 => {
                        self.state = PendingHide;
                        vec![StartTimer(self.cfg.hide_delay_ms)]
                    }
                    _ => vec![],
                }
            }
            VisInput::OverlayOpened => {
                self.overlays += 1;
                if self.state == PendingHide {
                    self.state = Shown;
                    vec![CancelTimer]
                } else {
                    vec![]
                }
            }
            VisInput::OverlayClosed => {
                self.overlays = self.overlays.saturating_sub(1);
                if self.overlays == 0 && !self.pointer_in_zone && self.state == Shown {
                    self.state = PendingHide;
                    vec![StartTimer(self.cfg.hide_delay_ms)]
                } else {
                    vec![]
                }
            }
            VisInput::Timer => match self.state {
                PendingShow => {
                    self.state = Shown;
                    vec![SetCatcher(Catcher::Zone), AnimateIn]
                }
                PendingHide if self.overlays == 0 && !self.pointer_in_zone => {
                    self.state = Hidden;
                    vec![AnimateOut, SetCatcher(self.rest_catcher())]
                }
                PendingHide => {
                    self.state = Shown;
                    vec![]
                }
                _ => vec![],
            },
            VisInput::Fullscreen(fs) => {
                let was_blocked = self.trigger_blocked();
                self.fullscreen = fs;
                let blocked = self.trigger_blocked();
                if was_blocked == blocked {
                    return vec![];
                }
                match self.state {
                    Hidden => vec![SetCatcher(self.rest_catcher())],
                    PendingShow if blocked => {
                        self.state = Hidden;
                        self.pointer_in_zone = false;
                        vec![CancelTimer, SetCatcher(Catcher::Off)]
                    }
                    _ => vec![],
                }
            }
        }
    }

    pub fn set_mode(&mut self, mode: Mode) -> Vec<VisCmd> {
        use VisCmd::*;
        if self.cfg.mode == mode {
            return vec![];
        }
        self.cfg.mode = mode;
        match mode {
            Mode::Pinned => {
                let was_visible = matches!(self.state, VisState::Shown | VisState::PendingHide);
                self.state = VisState::Shown;
                let mut cmds = vec![CancelTimer, SetCatcher(Catcher::Off)];
                if !was_visible {
                    cmds.push(AnimateIn);
                }
                cmds
            }
            Mode::AutoHide => {
                // Assume the pointer is outside; a following ZoneEnter cancels the timer.
                self.pointer_in_zone = false;
                self.state = VisState::PendingHide;
                vec![SetCatcher(Catcher::Zone), StartTimer(self.cfg.hide_delay_ms)]
            }
        }
    }
}
