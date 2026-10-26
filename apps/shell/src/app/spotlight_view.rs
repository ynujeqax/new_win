//! Spotlight (spec: M3): opened by a Win tap, `Ctrl+Esc`, the bar's magnifier or the bar menu's
//! Search. It is not over the bar, so it never reveals it.

use std::rc::Rc;
use std::sync::atomic::Ordering;

use newwin_config::usage::{Usage, save_atomic};
use newwin_config::usage_path;
use newwin_core::spotlight::{Candidate, Kind, SystemCommand, system_candidates};
use newwin_platform::apps_index::AppsWatch;
use newwin_platform::launch::{self, SystemTarget};
use newwin_platform::worker::{AppsIndexed, Job};
use newwin_ui::panel::Panel;
use newwin_ui::popup::{CLOSED, MenuItem, PICKED, REGIONS};
use newwin_ui::spotlight::{SpotlightLabels, SpotlightOp, SpotlightView, decode_action};
use newwin_ui::{PointF, theme};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::{
    GetCaretBlinkTime, GetCursorPos, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId, KillTimer, PostMessageW, SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    SetForegroundWindow, SetTimer, SystemParametersInfoW, WM_APP,
};

use super::App;
use super::menus::MenuCtx;

/// The keyboard hook saw a lone Win tap or `Ctrl+Esc`.
pub(super) const WM_APP_SPOTLIGHT: u32 = WM_APP + 14;
/// A Start-menu folder or the package catalog changed.
pub(super) const WM_APP_APPS_CHANGED: u32 = WM_APP + 15;
/// The worker rebuilt the app index.
pub(super) const WM_APP_APPS: u32 = WM_APP + 16;
/// The Spotlight panel picked or closed.
pub(super) const WM_APP_SPOTLIGHT_PANEL: u32 = WM_APP + 17;
/// A launch on the worker succeeded: `Box<String>` usage key in LPARAM.
pub(super) const WM_APP_LAUNCHED: u32 = WM_APP + 18;

pub(super) const APPS_DEBOUNCE_TIMER: usize = 9;
pub(super) const USAGE_SAVE_TIMER: usize = 10;
const DEBOUNCE_MS: u32 = 2000;
/// The one re-index after start (see `start_spotlight`).
const STARTUP_REINDEX_MS: u32 = 5000;

const MENU_OPEN: u32 = 1;
const MENU_ADMIN: u32 = 2;
const MENU_LOCATION: u32 = 3;

/// A toggle opens a fresh Spotlight unless one is open and staying (a quick second tap while it
/// fades out opens it again).
pub(super) fn toggle_opens(open: bool, closing: bool) -> bool {
    !open || closing
}

fn post_launched(ping: isize, key: String) {
    let boxed = Box::into_raw(Box::new(key));
    if unsafe { PostMessageW(Some(HWND(ping as *mut _)), WM_APP_LAUNCHED, WPARAM(0), LPARAM(boxed as isize)) }.is_err() {
        drop(unsafe { Box::from_raw(boxed) });
    }
}

impl App {
    pub(super) fn start_spotlight(&mut self) {
        let path = usage_path();
        let (usage, err) = Usage::load(&path);
        if let Some(e) = err {
            log::warn!("usage.toml unreadable, starting from zero: {e}");
        }
        self.usage = usage;
        self.usage_on_disk.store(path.exists(), Ordering::Release);
        self.rebuild_catalog();
        self.submit_job(Job::Apps);
        self.apps_watch = AppsWatch::start(self.ping, WM_APP_APPS_CHANGED).map_err(|e| log::warn!("app index watch: {e}")).ok();
        // Windows lists a Start-menu shortcut made just before we started (the installer's
        // *NewWin Settings*, an app installed during sign-in) only a moment later, and its folder
        // change came before the watch: one more look once that has settled.
        unsafe {
            SetTimer(Some(self.bar), APPS_DEBOUNCE_TIMER, STARTUP_REINDEX_MS, None);
        }
    }

    pub(super) fn submit_job(&self, job: Job) {
        match self.names.as_ref().and_then(|n| n.worker()) {
            Some(w) => w.submit_job(job),
            None => log::warn!("no worker thread"),
        }
    }

    fn rebuild_catalog(&mut self) {
        let mut c = system_candidates(self.strings);
        c.extend(self.apps.iter().cloned());
        self.catalog = Rc::new(c);
    }

    pub(super) fn toggle_spotlight(&mut self) {
        let open = self.spotlight.is_some();
        let closing = self.spotlight.as_ref().is_some_and(|p| p.is_closing());
        if toggle_opens(open, closing) {
            self.open_spotlight();
        } else if let Some(p) = &self.spotlight {
            p.close();
        }
    }

    fn open_spotlight(&mut self) {
        self.spotlight = None;
        self.close_popup();
        self.close_panel();
        let s = self.strings;
        let labels = SpotlightLabels {
            hint: s.search.into(),
            no_results: s.no_results.into(),
            kind_app: s.kind_app.into(),
            kind_system: s.kind_system.into(),
            kind_calculator: s.kind_calculator.into(),
            kind_conversion: s.kind_conversion.into(),
            press_again_shut_down: s.press_again_shut_down.into(),
            press_again_restart: s.press_again_restart.into(),
            keys: s.spotlight_keys.into(),
        };
        let launches = self.catalog.iter().map(|c| c.usage_key()).map(|k| (k.clone(), self.usage.get(&k))).filter(|(_, n)| *n > 0).collect();
        let blink = unsafe { GetCaretBlinkTime() };
        let blink = (blink != 0 && blink != u32::MAX).then_some(blink);
        let style = self.popup_style_px();
        let view = SpotlightView::new(self.catalog.clone(), launches, labels, &style, self.config.spotlight.max_results as usize, blink, &self.gfx);
        let mut work = RECT::default();
        if unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut work as *mut RECT as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)) }.is_err() {
            work = self.monitor;
        }
        let anchor = PointF { x: (work.left + work.right) as f32 / 2.0, y: (work.top as f32 + theme::SPOTLIGHT_TOP * (work.bottom - work.top) as f32).round() };
        // Without the glass thread no frame will come: show at once (the tint is all there is).
        let shown = if self.backdrops.is_some() { Panel::show_after_glass(&self.gfx, view, anchor, self.dpi, self.bar, WM_APP_SPOTLIGHT_PANEL) } else { Panel::show(&self.gfx, view, anchor, self.dpi, self.bar, WM_APP_SPOTLIGHT_PANEL) };
        match shown {
            Ok(p) => self.spotlight = Some(p),
            Err(e) => log::error!("cannot open Spotlight: {e}"),
        }
        self.sync_glass();
    }

    fn close_spotlight(&self) {
        if let Some(p) = &self.spotlight {
            p.close();
        }
    }

    pub(super) fn on_spotlight_panel(&mut self, wp: WPARAM, lp: LPARAM) {
        match lp.0 {
            PICKED => self.on_spotlight_action(wp.0 as u32),
            REGIONS => self.sync_glass(),
            CLOSED if self.spotlight.as_ref().map(|p| p.hwnd().0 as usize) == Some(wp.0) => {
                self.spotlight = None;
                self.sync_glass();
            }
            _ => {}
        }
    }

    /// The catalog the open Spotlight shows (action indices point into it).
    pub(super) fn shown_catalog(&self) -> Rc<Vec<Candidate>> {
        self.spotlight.as_ref().and_then(|p| p.read(|v| v.catalog().clone())).unwrap_or_else(|| self.catalog.clone())
    }

    fn on_spotlight_action(&mut self, id: u32) {
        let (index, op) = decode_action(id);
        match op {
            SpotlightOp::Open => self.launch_candidate(index, false),
            SpotlightOp::Admin => self.launch_candidate(index, true),
            SpotlightOp::Menu => self.open_spotlight_menu(index),
            SpotlightOp::Icons => self.fill_spotlight_icons(),
            SpotlightOp::Copy => self.copy_spotlight_answer(),
        }
    }

    /// Enter on a calculation: its result goes to the clipboard and Spotlight closes.
    fn copy_spotlight_answer(&mut self) {
        let text = self.spotlight.as_ref().and_then(|p| p.read(|v| v.answer().map(|a| a.text.clone()))).flatten();
        if let Some(t) = text
            && !newwin_platform::clipboard::set_text(&t)
        {
            log::warn!("cannot copy the answer");
        }
        self.close_spotlight();
    }

    fn open_spotlight_menu(&mut self, index: usize) {
        let catalog = self.shown_catalog();
        let Some(c) = catalog.get(index) else { return };
        let s = self.strings;
        let mut items = vec![MenuItem::new(MENU_OPEN, s.open)];
        if c.can_run_as_admin() {
            items.push(MenuItem::new(MENU_ADMIN, s.run_as_admin));
        }
        if c.file_path.is_some() {
            items.push(MenuItem::new(MENU_LOCATION, s.open_file_location));
        }
        let Some(p) = &self.spotlight else { return };
        // A right click: at the pointer; from the keyboard (pointer elsewhere): under the row.
        let at = if self.pointer_over(p.hwnd()) {
            let mut pt = POINT::default();
            unsafe {
                let _ = GetCursorPos(&mut pt);
            }
            PointF { x: pt.x as f32, y: pt.y as f32 }
        } else {
            let row = p.read(|v| v.selected().map(|r| v.row_rect(r))).flatten().unwrap_or_default();
            let mut r = RECT::default();
            unsafe {
                let _ = GetWindowRect(p.hwnd(), &mut r);
            }
            PointF { x: r.left as f32 + row.x + newwin_platform::dpi::scale(theme::POPUP_ITEM_PAD_X, self.dpi), y: r.top as f32 + row.bottom() }
        };
        p.hold();
        self.open(items, at, MenuCtx::SpotlightRow { item: c.clone() });
    }

    fn pointer_over(&self, hwnd: HWND) -> bool {
        let (mut pt, mut r) = (POINT::default(), RECT::default());
        unsafe {
            let _ = GetCursorPos(&mut pt);
            let _ = GetWindowRect(hwnd, &mut r);
        }
        pt.x >= r.left && pt.x < r.right && pt.y >= r.top && pt.y < r.bottom
    }

    /// `item` was taken when the menu opened: a re-index meanwhile cannot change what it runs.
    pub(super) fn on_spotlight_menu_pick(&mut self, item: Candidate, id: u32) {
        match id {
            MENU_OPEN => self.launch(item, false),
            MENU_ADMIN => self.launch(item, true),
            MENU_LOCATION => {
                if let Some(path) = item.file_path.clone() {
                    self.submit_job(Job::Run(Box::new(move || {
                        if let Err(e) = launch::open_file_location(&path) {
                            log::warn!("open file location {path}: {e}");
                        }
                    })));
                }
                self.close_spotlight();
            }
            _ => {}
        }
    }

    /// The row menu closed: typing goes on in Spotlight, unless focus went somewhere else.
    pub(super) fn spotlight_menu_closed(&mut self) {
        let Some(p) = &self.spotlight else { return };
        if p.is_closing() {
            return;
        }
        let ours = unsafe {
            let mut pid = 0;
            GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid));
            pid == GetCurrentProcessId()
        };
        if ours {
            unsafe {
                let _ = SetForegroundWindow(p.hwnd());
            }
        } else {
            p.close();
        }
    }

    /// Runs entry `index` of the shown catalog; apps launch on the worker (a UAC prompt or a slow
    /// app must not freeze the bar) and count once they started.
    pub(super) fn launch_candidate(&mut self, index: usize, admin: bool) {
        if let Some(c) = self.shown_catalog().get(index).cloned() {
            self.launch(c, admin);
        }
    }

    fn launch(&mut self, c: Candidate, admin: bool) {
        self.close_spotlight();
        let key = c.usage_key();
        match c.kind {
            Kind::App => {
                let ping = self.ping.0 as isize;
                self.submit_job(Job::Run(Box::new(move || {
                    let r = if admin { launch::run_as_admin(&c) } else { launch::open_app(&c.id) };
                    match r {
                        Ok(()) => post_launched(ping, key),
                        Err(e) => log::warn!("launch {:?} (admin: {admin}): {e}", c.name),
                    }
                })));
            }
            Kind::SettingsPage(i) => {
                let Some(page) = newwin_core::settings_pages::PAGES.get(i) else { return };
                let ping = self.ping.0 as isize;
                self.submit_job(Job::Run(Box::new(move || match launch::shell_execute("open", page.uri, "") {
                    Ok(()) => post_launched(ping, key),
                    Err(e) => log::warn!("open {}: {e}", page.uri),
                })));
            }
            Kind::Link(i) => {
                let Some(target) = self.config.spotlight.links.get(i).map(|l| l.target.clone()) else { return };
                let ping = self.ping.0 as isize;
                self.submit_job(Job::Run(Box::new(move || match launch::shell_execute("open", &target, "") {
                    Ok(()) => post_launched(ping, key),
                    Err(e) => log::warn!("open link {target}: {e}"),
                })));
            }
            Kind::System(cmd) => {
                // The session may end before the save timer: count and write first.
                if matches!(cmd, SystemCommand::ShutDown | SystemCommand::Restart | SystemCommand::SignOut) {
                    self.usage.bump(&key);
                    self.save_usage_now();
                }
                let r = match cmd {
                    SystemCommand::Settings => {
                        self.open_settings();
                        Ok(())
                    }
                    SystemCommand::ShutDown => launch::run(SystemTarget::ShutDown),
                    SystemCommand::Restart => launch::run(SystemTarget::Restart),
                    SystemCommand::Sleep => launch::run(SystemTarget::Sleep),
                    SystemCommand::Lock => launch::run(SystemTarget::Lock),
                    SystemCommand::SignOut => launch::run(SystemTarget::SignOut),
                };
                match r {
                    Ok(()) if !matches!(cmd, SystemCommand::ShutDown | SystemCommand::Restart | SystemCommand::SignOut) => self.on_launched(key),
                    Ok(()) => {}
                    Err(e) => log::warn!("{cmd:?} failed: {e}"),
                }
            }
        }
    }

    pub(super) fn on_launched(&mut self, key: String) {
        self.usage.bump(&key);
        unsafe { SetTimer(Some(self.bar), USAGE_SAVE_TIMER, DEBOUNCE_MS, None) };
    }

    /// Writes `usage.toml` at once, on this thread (the file is tiny): before the session or
    /// NewWin ends, when the save timer would be too late.
    pub(super) fn save_usage_now(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.bar), USAGE_SAVE_TIMER);
        }
        match save_atomic(&usage_path(), &self.usage.to_toml()) {
            Ok(()) => self.usage_on_disk.store(true, Ordering::Release),
            Err(e) => log::warn!("cannot write usage.toml: {e}"),
        }
    }

    pub(super) fn save_usage(&mut self) {
        if !self.apps.is_empty() {
            let catalog = self.catalog.clone();
            self.usage.retain(|k| k.starts_with("system:") || catalog.iter().any(|c| c.id == k));
        }
        let text = self.usage.to_toml();
        let on_disk = self.usage_on_disk.clone();
        self.submit_job(Job::Run(Box::new(move || match save_atomic(&usage_path(), &text) {
            Ok(()) => on_disk.store(true, Ordering::Release),
            Err(e) => log::warn!("cannot write usage.toml: {e}"),
        })));
    }

    pub(super) fn on_apps_changed(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.bar), APPS_DEBOUNCE_TIMER);
            SetTimer(Some(self.bar), APPS_DEBOUNCE_TIMER, DEBOUNCE_MS, None);
        }
    }

    pub(super) fn reindex_apps(&mut self) {
        self.submit_job(Job::Apps);
    }

    pub(super) fn on_apps_indexed(&mut self, r: AppsIndexed) {
        match r.apps {
            Ok(apps) => {
                log::info!("apps indexed: {} in {} ms", apps.len(), r.ms);
                // Apps that had no icon may have one now.
                self.icons.forget_missing_shell();
                self.apps = apps;
                self.rebuild_catalog();
                let catalog = self.catalog.clone();
                if let Some(p) = &self.spotlight {
                    p.update(|v| v.set_catalog(catalog));
                    self.fill_spotlight_icons();
                }
            }
            Err(e) => log::warn!("app index failed, keeping the old one: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_while_closing_opens_again() {
        assert!(toggle_opens(false, false), "nothing open: open");
        assert!(toggle_opens(true, true), "fading out: open a fresh one");
        assert!(!toggle_opens(true, false), "open: close it");
    }
}
