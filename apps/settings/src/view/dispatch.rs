//! The window's side of the section bodies: control events become config edits; the buttons
//! (*Clear*, *Open logs*, *Open config file*, *Restore Windows taskbar*) and their two-click
//! confirmations; the font list.

use std::rc::Rc;

use newwin_platform::dpi::scale;
use newwin_platform::launch::shell_execute;
use newwin_ui::RectF;
use newwin_ui::app_window::CAPTION_H;
use newwin_ui::panel::PanelInput;

use std::path::Path;

use newwin_config::tray_seen::TraySeen;
use newwin_platform::clock::now_ms;

use super::{CONFIRM_TIMER, FocusKind, RESTORE_TIMER, SIZE, Section, SettingsView, newwin_exe, shell_running};
use crate::body::{BodyEvent, FocusKey};
use crate::font_list::{FontList, ListReply};
use crate::fonts;
use crate::sections::about::{self, RESTORE_POLL_MS, RestoreStep, RestoreWait};
use crate::sections::{self, Click, ListEdit, RowId, RowValue, spotlight};

/// `tray_seen.toml`; a missing file is empty, an unreadable or broken one is `None` (logged).
pub(super) fn load_seen(path: &Path) -> Option<TraySeen> {
    match TraySeen::load(path) {
        (seen, None) => Some(seen),
        (_, Some(e)) => {
            log::warn!("cannot read {}: {e}", path.display());
            None
        }
    }
}

impl SettingsView {
    /// The body shows the current config (and, for Tray, the apps seen). The focused control
    /// keeps the focus when rows come or go around it; a control that went loses it.
    pub(super) fn sync_body(&mut self, now: u64) {
        let focused = self.focused_key();
        self.sync_body_keeping(focused, now);
    }

    /// What the focus is on, if a section control.
    fn focused_key(&self) -> Option<FocusKey> {
        self.focus.and_then(|i| self.body_row(i)).and_then(|row| self.body.focus_key(row))
    }

    /// [`SettingsView::sync_body`], with the focus going to `focused` (taken before a change).
    fn sync_body_keeping(&mut self, focused: Option<FocusKey>, now: u64) {
        match sections::list_for(self.section, &self.config, &self.seen, self.strings) {
            Some(list) => self.body.set_list(list, now),
            None => {
                let rows = sections::rows_for(self.section, &self.config, self.date_custom);
                self.body.set_rows(rows, self.strings, now);
            }
        }
        self.body.set_armed(self.armed_ids(now));
        if let Some(key) = focused {
            self.focus = self.body_focus(self.body.focus_of(&key));
            if self.focus.is_none() {
                self.body.field.focused = false;
            }
        }
    }

    /// `tray_seen.toml` may have changed: the Tray list follows. An unreadable file keeps the
    /// list as it was.
    pub(super) fn reload_seen(&mut self) {
        let Some(seen) = load_seen(&self.seen_path) else { return };
        if seen != self.seen {
            self.seen = seen;
            if self.section == Section::Tray {
                self.sync_body(now_ms());
            }
        }
    }

    fn armed_ids(&self, now: u64) -> Vec<RowId> {
        [(RowId::ClearHistory, self.clear_confirm), (RowId::RestoreTaskbar, self.restore_confirm)]
            .into_iter()
            .filter(|(_, c)| c.armed(now))
            .map(|(id, _)| id)
            .collect()
    }

    /// The focusable index of the body's control `j`.
    fn body_focus(&self, j: Option<usize>) -> Option<usize> {
        j.map(|j| j + usize::from(self.has_button()))
    }

    pub(super) fn on_body(&mut self, ev: BodyEvent, now: u64) {
        match ev {
            BodyEvent::Set(id, v) => {
                let custom_picked = id == RowId::DateFormat && v == RowValue::Choice(1);
                match (id, &v) {
                    (RowId::DateFormat, RowValue::Choice(i)) => self.date_custom = *i == 1,
                    (RowId::DateTemplate, _) => self.date_custom = true,
                    _ => {}
                }
                self.edit(|c| sections::apply(c, id, v));
                self.sync_body(now);
                if custom_picked {
                    self.set_focus(self.body_focus(self.body.field_focus_index()), now);
                }
            }
            BodyEvent::Press(id) => self.press_button(id, now),
            BodyEvent::OpenFonts => self.open_fonts(now),
            BodyEvent::FocusField => self.set_focus(self.body_focus(self.body.field_focus_index()), now),
            BodyEvent::List(e) => {
                let keys = self.body.list_keys().to_vec();
                // Taken before the move: a row moved by the keyboard keeps the focus.
                let focused = self.focused_key();
                let section = self.section;
                self.edit(|c| sections::apply_list(section, c, &keys, e));
                if let ListEdit::Moved(from, to) = e {
                    self.body.commit_move(from, to, now);
                }
                self.sync_body_keeping(focused, now);
                // A row moved by the keyboard stays in view.
                if matches!(e, ListEdit::Moved(..)) {
                    self.set_focus(self.focus, now);
                }
            }
        }
    }

    fn press_button(&mut self, id: RowId, now: u64) {
        match id {
            RowId::ClearHistory => {
                if self.clear_confirm.click(now) == Click::Fire {
                    let path = newwin_config::usage_path();
                    match spotlight::clear_history(&path) {
                        Ok(()) => log::info!("launch history cleared"),
                        Err(e) => log::error!("cannot delete {}: {e}", path.display()),
                    }
                }
            }
            RowId::RestoreTaskbar => {
                if self.restore_confirm.click(now) == Click::Fire {
                    self.restore_taskbar(now);
                }
            }
            RowId::OpenLogs => about::open_logs(),
            RowId::OpenConfig => self.open_config(),
            _ => {}
        }
        self.refresh_armed(now);
    }

    /// Button labels follow the confirmations; the timer wakes when the next one runs out.
    pub(super) fn refresh_armed(&mut self, now: u64) {
        self.body.set_armed(self.armed_ids(now));
        let next = [self.clear_confirm, self.restore_confirm].iter().filter(|c| c.armed(now)).filter_map(|c| c.expires_at()).min();
        match next {
            Some(t) => self.set_timer(CONFIRM_TIMER, u32::try_from(t.saturating_sub(now)).unwrap_or(u32::MAX).max(1)),
            None => self.kill_timer(CONFIRM_TIMER),
        }
    }

    /// *Open config file*: a missing file is written first, so there is something to open.
    fn open_config(&mut self) {
        if !self.path.exists() {
            self.writer.borrow_mut().edit(self.config.clone());
            self.flush();
        }
        if let Err(e) = shell_execute("open", &self.path.to_string_lossy(), "") {
            log::error!("cannot open {}: {e}", self.path.display());
        }
    }

    /// *Restore Windows taskbar*: asks the shell to quit, then waits up to 3 s for it to go.
    fn restore_taskbar(&mut self, now: u64) {
        if about::request_quit() {
            log::info!("asked NewWin to quit (restore the Windows taskbar)");
            self.restore_wait = Some(RestoreWait::start(now));
            self.set_timer(RESTORE_TIMER, RESTORE_POLL_MS);
        } else {
            about::run_restore(newwin_exe().as_deref());
            self.check_shell();
        }
    }

    pub(super) fn poll_restore(&mut self, now: u64) {
        let step = self.restore_wait.map(|w| w.poll(now, shell_running()));
        if step == Some(RestoreStep::Wait) {
            return;
        }
        self.kill_timer(RESTORE_TIMER);
        self.restore_wait = None;
        if step == Some(RestoreStep::Fallback) {
            log::warn!("NewWin did not quit within 3 s");
            about::run_restore(newwin_exe().as_deref());
        }
        self.check_shell();
    }

    fn open_fonts(&mut self, now: u64) {
        let families = self
            .families
            .get_or_insert_with(|| {
                let f = fonts::families();
                log::info!("{} font families", f.len());
                Rc::new(f)
            })
            .clone();
        if families.is_empty() {
            log::warn!("no font families to choose from");
            return;
        }
        self.font_list = Some(FontList::new(families, &self.config.appearance.font_family, self.dpi));
        if self.focused_kind() != Some(FocusKind::FontList) {
            self.set_focus(self.body_focus(self.body.font_focus_index()), now);
        }
    }

    /// Where the open font list is, under the font row, inside the window below the caption.
    pub(super) fn list_rect(&self) -> Option<RectF> {
        let list = self.font_list.as_ref()?;
        let anchor = self.body.font_anchor()?;
        let top = scale(CAPTION_H, self.dpi);
        let window = RectF::new(0.0, top, scale(SIZE.0, self.dpi), scale(SIZE.1, self.dpi) - top);
        Some(list.rect_at(anchor, window))
    }

    /// Input while the font list is open; true = repaint.
    pub(super) fn list_input(&mut self, ev: PanelInput, now: u64) -> bool {
        let Some(rect) = self.list_rect() else {
            self.font_list = None;
            return true;
        };
        let Some(list) = self.font_list.as_mut() else { return false };
        match list.input(ev, rect) {
            ListReply::Ignored => false,
            ListReply::Repaint => true,
            ListReply::Close => {
                self.font_list = None;
                true
            }
            ListReply::Pick(name) => {
                self.font_list = None;
                self.on_body(BodyEvent::Set(RowId::Font, RowValue::Text(name)), now);
                true
            }
        }
    }
}
