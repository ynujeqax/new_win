//! Input routing of the settings window: pointer events to the navigation, the banner's button
//! and the section body; keys to the focused control (spec: M4 §5). Tab / Shift+Tab move the
//! focus, Ctrl+Tab the section, Esc closes the font list; in the Items / Tray list Space switches
//! the focused row and Ctrl+↑/↓ move it.

use newwin_ui::RectF;
use newwin_ui::panel::PanelInput;
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_ESCAPE, VK_SHIFT, VK_TAB};

use super::{FocusKind, KeyAction, Section, SettingsView, ctrl_key_action, focus_next, key_action, key_down};

impl SettingsView {
    /// The controls Tab moves over, in order: the banner's button, then the section's controls.
    pub(super) fn focusables(&self) -> Vec<(FocusKind, RectF)> {
        let mut out = Vec::new();
        if self.has_button() {
            // Its rectangle is known once painted.
            out.push((FocusKind::Button, self.geo.button.unwrap_or_default()));
        }
        out.extend(self.section_controls());
        out
    }

    /// The section's focusable controls.
    fn section_controls(&self) -> Vec<(FocusKind, RectF)> {
        self.body.focusables().into_iter().map(|(k, r, _)| (k, r)).collect()
    }

    /// Focusable `i` of [`SettingsView::focusables`] as a body row.
    pub(super) fn body_row(&self, i: usize) -> Option<usize> {
        let j = i.checked_sub(usize::from(self.has_button()))?;
        self.body.focusables().get(j).map(|f| f.2)
    }

    pub(super) fn focused_kind(&self) -> Option<FocusKind> {
        self.focus.and_then(|i| self.focusables().get(i).map(|f| f.0))
    }

    /// Moves the focus; the template field takes the caret when it gets it, and the focused row
    /// scrolls into view.
    pub(super) fn set_focus(&mut self, focus: Option<usize>, now: u64) {
        self.focus = focus;
        let field = self.focused_kind() == Some(FocusKind::TextField);
        if field && !self.body.field.focused {
            self.body.field.blink_from = now;
        }
        self.body.field.focused = field;
        if let Some(row) = focus.and_then(|i| self.body_row(i)) {
            self.body.reveal(row);
        }
    }

    /// A key on the focused control `i` of [`SettingsView::focusables`].
    fn act(&mut self, i: usize, action: KeyAction, now: u64) {
        if self.has_button() && i == 0 {
            if action == KeyAction::Activate {
                self.press_banner_button();
            }
            return;
        }
        if let Some(ev) = self.body_row(i).and_then(|row| self.body.activate(row, action, now)) {
            self.on_body(ev, now);
        }
    }

    /// Esc: closes the font list. True when it was open.
    fn close_lists(&mut self) -> bool {
        self.font_list.take().is_some()
    }

    fn on_key(&mut self, ev: PanelInput, now: u64) -> bool {
        let vk = match ev {
            PanelInput::Key(vk) => Some(vk),
            _ => None,
        };
        if vk == Some(VK_ESCAPE.0) {
            return self.close_lists();
        }
        if self.font_list.is_some() && vk != Some(VK_TAB.0) {
            return self.list_input(ev, now);
        }
        if vk == Some(VK_TAB.0) {
            self.font_list = None;
            let count = self.focusables().len();
            let next = focus_next(self.focus, count, key_down(VK_SHIFT.0));
            let changed = next != self.focus;
            self.set_focus(next, now);
            return changed;
        }
        if self.focused_kind() == Some(FocusKind::TextField) {
            if let Some(ev) = self.body.field_input(ev, now) {
                self.on_body(ev, now);
            }
            return true;
        }
        let Some(i) = self.focus else { return false };
        let Some(&(kind, _)) = self.focusables().get(i) else { return false };
        let action = match ev {
            PanelInput::Key(vk) => key_action(kind, vk),
            PanelInput::CtrlKey(vk) => ctrl_key_action(kind, vk),
            _ => None,
        };
        match action {
            Some(a) => {
                self.act(i, a, now);
                true
            }
            None => false,
        }
    }

    /// Any input but the timers; true = repaint.
    pub(super) fn route(&mut self, ev: PanelInput, now_ms: u64) -> bool {
        let list_open = self.font_list.is_some();
        match ev {
            PanelInput::Move(_) | PanelInput::Leave | PanelInput::Wheel(_) | PanelInput::Release(_) if list_open => self.list_input(ev, now_ms),
            // A press outside closes the list and does nothing else.
            PanelInput::Press(_) if list_open => {
                self.list_input(ev, now_ms);
                true
            }
            PanelInput::Move(pt) => {
                let hover = self.geo.button.is_some_and(|b| b.contains(pt));
                let changed = hover != self.button_hover;
                self.button_hover = hover;
                let (ev, body) = self.body.move_to(pt, now_ms);
                let edited = ev.is_some();
                if let Some(ev) = ev {
                    self.on_body(ev, now_ms);
                }
                changed || body || edited
            }
            PanelInput::Leave => {
                let changed = self.button_hover;
                self.button_hover = false;
                self.body.leave() || changed
            }
            PanelInput::Wheel(delta) => self.body.wheel(delta, now_ms),
            PanelInput::Press(pt) => {
                if let Some(i) = self.nav.hit(&self.geo.nav, pt) {
                    self.select_section(i, now_ms);
                    true
                } else if self.geo.button.is_some_and(|b| b.contains(pt)) {
                    self.button_pressed = true;
                    true
                } else {
                    // A click elsewhere takes the focus from the template field.
                    if self.focused_kind() == Some(FocusKind::TextField) {
                        self.set_focus(None, now_ms);
                    }
                    if let Some(ev) = self.body.press(pt, now_ms) {
                        self.on_body_by(ev, now_ms, true);
                    }
                    true
                }
            }
            PanelInput::Release(pt) => {
                let was = std::mem::take(&mut self.button_pressed);
                if was && self.geo.button.is_some_and(|b| b.contains(pt)) {
                    self.press_banner_button();
                }
                let (ev, body) = self.body.release(pt, now_ms);
                match ev {
                    Some(ev) => self.on_body_by(ev, now_ms, true),
                    // A tray app that came in during a drag joins the list now.
                    None if body && matches!(self.section, Section::Items | Section::Tray) => self.sync_body(now_ms),
                    None => {}
                }
                was || body
            }
            PanelInput::CtrlKey(vk) if vk == VK_TAB.0 => {
                let next = focus_next(Some(self.section.index()), Section::ALL.len(), key_down(VK_SHIFT.0));
                if let Some(i) = next {
                    self.select_section(i, now_ms);
                }
                true
            }
            PanelInput::Key(_) | PanelInput::CtrlKey(_) | PanelInput::Char(_) => self.on_key(ev, now_ms),
            _ => false,
        }
    }
}
