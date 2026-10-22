//! Keyboard layout on the bar (spec: M2 §2, §4.3): the code shown, and spotting layout hotkeys.

/// The 3-letter code on the bar ("eng" → "ENG").
pub fn display_code(iso639_2: &str) -> String {
    let code = iso639_2.trim().to_uppercase();
    if code.is_empty() { "—".to_string() } else { code }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Chord {
    AltShift,
    CtrlShift,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key {
    Alt,
    Shift,
    Ctrl,
    Win,
    Space,
    Other,
}

fn classify(vk: u16) -> Key {
    match vk {
        0x12 | 0xA4 | 0xA5 => Key::Alt,
        0x10 | 0xA0 | 0xA1 => Key::Shift,
        0x11 | 0xA2 | 0xA3 => Key::Ctrl,
        0x5B | 0x5C => Key::Win,
        0x20 => Key::Space,
        _ => Key::Other,
    }
}

/// Watches key events from the low-level hook for Windows' layout switches: Alt+Shift and
/// Ctrl+Shift (reported when the chord is released, unless another key joined it) and
/// Win+Space (every Space press while Win is held).
#[derive(Default)]
pub struct LayoutHotkeys {
    alt: bool,
    shift: bool,
    ctrl: bool,
    win: bool,
    chord: Option<Chord>,
    spoiled: bool,
    reported: bool,
}

impl LayoutHotkeys {
    /// True when the layout may just have changed.
    pub fn key(&mut self, vk: u16, down: bool) -> bool {
        let key = classify(vk);
        if down {
            match key {
                Key::Space if self.win => return true,
                Key::Alt => self.alt = true,
                Key::Shift => self.shift = true,
                Key::Ctrl => self.ctrl = true,
                Key::Win => self.win = true,
                Key::Space | Key::Other => self.spoiled = true,
            }
            if self.chord.is_none() {
                self.chord = match (self.alt, self.ctrl, self.shift) {
                    (true, false, true) => Some(Chord::AltShift),
                    (false, true, true) => Some(Chord::CtrlShift),
                    _ => None,
                };
                if self.chord.is_some() {
                    self.spoiled = false;
                    self.reported = false;
                }
            }
            return false;
        }
        let in_chord = matches!((self.chord, key), (Some(_), Key::Shift) | (Some(Chord::AltShift), Key::Alt) | (Some(Chord::CtrlShift), Key::Ctrl));
        match key {
            Key::Alt => self.alt = false,
            Key::Shift => self.shift = false,
            Key::Ctrl => self.ctrl = false,
            Key::Win => self.win = false,
            Key::Space | Key::Other => {}
        }
        if !in_chord {
            return false;
        }
        let report = !self.spoiled && !self.reported;
        self.reported = true;
        let held = match self.chord {
            Some(Chord::AltShift) => self.alt || self.shift,
            Some(Chord::CtrlShift) => self.ctrl || self.shift,
            None => false,
        };
        if !held {
            self.chord = None;
        }
        report
    }
}

/// What the hook does with a key event for Spotlight (spec: M3 §4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TapAction {
    /// Let it through.
    Pass,
    /// The first press of Win: let it through, and warm the glass up (a tap may follow).
    WinDown,
    /// Win was pressed and released alone: swallow this Win up, replay it masked, open Spotlight.
    WinTap,
    /// `Ctrl+Esc`: swallow the Esc, open Spotlight.
    CtrlEsc,
    /// The Esc up of a `Ctrl+Esc`.
    Swallow,
}

/// Spots a lone Win tap and `Ctrl+Esc` in the hook's key events.
pub struct WinKeyTap {
    enabled: bool,
    win: bool,
    spoiled: bool,
    ctrl: bool,
    shift: bool,
    alt: bool,
    eat_esc_up: bool,
}

impl WinKeyTap {
    pub fn new(enabled: bool) -> Self {
        Self { enabled, win: false, spoiled: false, ctrl: false, shift: false, alt: false, eat_esc_up: false }
    }

    /// The keys' real state before the next event (`GetAsyncKeyState` in the hook): corrects ups
    /// the hook never saw, sent to the secure desktop (Ctrl+Alt+Del, Win+L).
    pub fn sync(&mut self, ctrl: bool, shift: bool, alt: bool, win: bool) {
        (self.ctrl, self.shift, self.alt) = (ctrl, shift, alt);
        if !win {
            self.win = false;
        }
    }

    pub fn key(&mut self, vk: u16, down: bool) -> TapAction {
        if !self.enabled {
            return TapAction::Pass;
        }
        const ESC: u16 = 0x1B;
        match (classify(vk), down) {
            (Key::Win, true) => {
                let first = !self.win;
                if first {
                    self.spoiled = false;
                }
                self.win = true;
                return if first { TapAction::WinDown } else { TapAction::Pass };
            }
            (Key::Win, false) => {
                let tap = self.win && !self.spoiled;
                self.win = false;
                return if tap { TapAction::WinTap } else { TapAction::Pass };
            }
            (Key::Ctrl, d) => self.ctrl = d,
            (Key::Shift, d) => self.shift = d,
            (Key::Alt, d) => self.alt = d,
            _ => {}
        }
        if down && self.win {
            self.spoiled = true;
            return TapAction::Pass;
        }
        // Ctrl alone: Ctrl+Shift+Esc is Task Manager.
        if vk == ESC && down && self.ctrl && !self.shift && !self.alt {
            self.eat_esc_up = true;
            return TapAction::CtrlEsc;
        }
        if vk == ESC && !down && self.eat_esc_up {
            self.eat_esc_up = false;
            return TapAction::Swallow;
        }
        TapAction::Pass
    }
}
