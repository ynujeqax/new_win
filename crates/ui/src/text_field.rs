//! A one-line text field for the settings window (the tray-app filter, the config path): the same
//! `TextEdit` and caret blink as Spotlight's query, on a translucent track.

use newwin_core::text_edit::{Edit, TextEdit};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_BACK, VK_DELETE, VK_END, VK_HOME, VK_LEFT, VK_RIGHT};

use crate::panel::PanelInput;
use crate::{Color, Painter, PointF, RectF, TextLayout, theme};

const TRACK: Color = Color::rgba(255, 255, 255, 0.07);
const TRACK_FOCUS: Color = Color::rgba(255, 255, 255, 0.12);

#[derive(Clone, Debug, Default)]
pub struct TextField {
    pub edit: TextEdit,
    pub focused: bool,
    /// When the caret last restarted its blink.
    pub blink_from: u64,
}

/// What a key or character means to a line editor (`None` = not an editing key).
pub fn edit_for(ev: &PanelInput) -> Option<Edit> {
    match ev {
        PanelInput::Char(c) => Some(Edit::Insert(c.to_string())),
        PanelInput::Key(k) => match *k {
            k if k == VK_BACK.0 => Some(Edit::Backspace),
            k if k == VK_DELETE.0 => Some(Edit::Delete),
            k if k == VK_LEFT.0 => Some(Edit::Left),
            k if k == VK_RIGHT.0 => Some(Edit::Right),
            k if k == VK_HOME.0 => Some(Edit::Home),
            k if k == VK_END.0 => Some(Edit::End),
            _ => None,
        },
        PanelInput::CtrlKey(k) => match *k {
            k if k == VK_BACK.0 => Some(Edit::BackspaceWord),
            0x41 => Some(Edit::SelectAll),
            0x56 => newwin_platform::clipboard::text().map(Edit::Insert),
            _ => None,
        },
        _ => None,
    }
}

impl TextField {
    /// Applies a key or character when focused; true = the text changed.
    pub fn input(&mut self, ev: PanelInput) -> bool {
        if !self.focused {
            return false;
        }
        match edit_for(&ev) {
            Some(e) => self.edit.apply(e),
            None => false,
        }
    }

    /// Like [`TextField::input`], restarting the caret blink on any editing key.
    pub fn input_at(&mut self, ev: PanelInput, now: u64) -> bool {
        if self.focused && edit_for(&ev).is_some() {
            self.blink_from = now;
        }
        self.input(ev)
    }

    pub fn caret_on(&self, now: u64, blink_ms: Option<u32>) -> bool {
        match blink_ms {
            Some(b) if b > 0 => (now.saturating_sub(self.blink_from) / u64::from(b)).is_multiple_of(2),
            _ => true,
        }
    }

    /// The next moment the caret flips, when focused and blinking.
    pub fn next_blink(&self, now: u64, blink_ms: Option<u32>) -> Option<u64> {
        let b = u64::from(blink_ms.filter(|&b| b > 0)?);
        self.focused.then(|| self.blink_from + (now.saturating_sub(self.blink_from) / b + 1) * b)
    }

    /// `text` is the laid-out content, `hint` the placeholder shown while empty, `clock` the
    /// time and the caret blink period.
    pub fn paint(&self, p: &mut Painter, r: RectF, text: Option<&TextLayout>, hint: Option<&TextLayout>, clock: (u64, Option<u32>), dpi: u32) {
        let (now, blink_ms) = clock;
        let radius = r.h / 2.0;
        p.fill_round_rect(r, radius, if self.focused { TRACK_FOCUS } else { TRACK });
        let x = r.x + newwin_platform::dpi::scale(theme::TEXT_FIELD_PAD_X, dpi);
        let mid = r.y + r.h / 2.0;
        match text {
            Some(t) => {
                let y = (mid - t.height() / 2.0).round();
                if self.focused && self.edit.all_selected() {
                    p.fill_round_rect(RectF::new(x - 2.0, y, t.width() + 4.0, t.height()), 4.0, theme::ACTIVE_PILL);
                }
                p.text(t, PointF { x, y }, theme::TEXT_STRONG);
                if self.focused && self.caret_on(now, blink_ms) && !self.edit.all_selected() {
                    self.caret(p, x + t.caret_x(self.edit.caret_utf16()), mid, t.height(), dpi);
                }
            }
            None => {
                if let Some(h) = hint {
                    p.text(h, PointF { x, y: (mid - h.height() / 2.0).round() }, theme::TEXT_INACTIVE);
                }
                if self.focused && self.caret_on(now, blink_ms) {
                    self.caret(p, x, mid, hint.map_or(r.h * 0.5, |h| h.height()), dpi);
                }
            }
        }
    }

    fn caret(&self, p: &mut Painter, x: f32, mid: f32, text_h: f32, dpi: u32) {
        let x = x.round() + 0.5;
        let half = (text_h * 0.42).round();
        p.line(PointF { x, y: mid - half }, PointF { x, y: mid + half }, theme::TEXT_STRONG, newwin_platform::dpi::scale(theme::CARET_W, dpi).max(1.0));
    }
}
