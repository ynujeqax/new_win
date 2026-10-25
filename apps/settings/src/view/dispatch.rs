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

use super::{CONFIRM_TIMER, FocusKind, SIZE, Section, SettingsView, newwin_exe, reveals_focus};
use crate::body::{BodyEvent, FocusKey};
use crate::font_list::{FontList, ListReply};
use crate::fonts;
use crate::sections::about;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
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
        self.confirms.armed(now)
    }

    /// The focusable index of the body's control `j`.
    fn body_focus(&self, j: Option<usize>) -> Option<usize> {
        j.map(|j| j + usize::from(self.has_button()))
    }

    pub(super) fn on_body(&mut self, ev: BodyEvent, now: u64) {
        self.on_body_by(ev, now, false);
    }

    /// A body event; `by_pointer` = from a click or a drop, not a key.
    pub(super) fn on_body_by(&mut self, ev: BodyEvent, now: u64, by_pointer: bool) {
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
                if reveals_focus(&e, by_pointer) {
                    self.set_focus(self.focus, now);
                }
            }
        }
    }

    fn press_button(&mut self, id: RowId, now: u64) {
        match id {
            RowId::ClearHistory => {
                if self.confirms.click(id, now) == Click::Fire {
                    let path = newwin_config::usage_path();
                    match spotlight::clear_history(&path) {
                        Ok(()) => log::info!("launch history cleared"),
                        Err(e) => log::error!("cannot delete {}: {e}", path.display()),
                    }
                }
            }
            RowId::RestoreTaskbar => {
                if self.confirms.click(id, now) == Click::Fire {
                    self.restore_taskbar();
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
        match self.confirms.next_expiry(now) {
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

    /// *Restore Windows taskbar*: `NewWin.exe --quit` ends every NewWin process, this window
    /// included; it saves and closes first.
    fn restore_taskbar(&mut self) {
        about::run_quit(newwin_exe().as_deref());
        self.flush();
        unsafe {
            let _ = PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
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
