//! The tray panel under `⌃` (spec: M2 §3.1): NewWin, then the apps of our tray host.

use newwin_config::tray_seen::TraySeen;
use newwin_config::{save_atomic, tray_seen_path};
use newwin_core::tray::{arrange_rows, is_bar_duplicate, missing_known, row_label};
use newwin_platform::appinfo::{process_display_name, process_image_path, session_processes};
use newwin_platform::clock::now_ms;
use newwin_platform::dpi::scale;
use newwin_platform::launch::shell_execute;
use newwin_platform::tray_host::{TrayClick, TrayHost, find_process_window, send_click, watch_foreground};
use newwin_platform::worker::Job;
use newwin_ui::panel::Panel;
use newwin_ui::popup::PopupStyle;
use newwin_ui::tray_list::{TrayList, TrayOp, TrayRow, decode_action};
use newwin_ui::{PointF, theme};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::{GWL_EXSTYLE, GWL_STYLE, GetCursorPos, GetWindowLongW, GetWindowThreadProcessId, IsWindowVisible, WM_APP, WS_CAPTION, WS_EX_TOOLWINDOW};

use super::App;
use super::panels::{ShellPanel, WM_APP_PANEL};

pub(super) const WM_APP_FOREGROUND: u32 = WM_APP + 13;

impl App {
    /// Apps that had a tray icon before: read once at startup, then brought up to date.
    pub(super) fn load_tray_seen(&mut self) {
        let (seen, err) = TraySeen::load(&tray_seen_path());
        if let Some(e) = err {
            log::warn!("tray_seen.toml unreadable, starting empty: {e}");
        }
        self.tray_seen = seen;
        self.note_seen_icons();
    }

    /// The exe file name of an icon's process, read once per pid; empty when unreadable.
    fn tray_exe(&mut self, pid: u32) -> String {
        self.tray_exes.entry(pid).or_insert_with(|| exe_file_name(pid)).clone()
    }

    /// Records every icon's app in `tray_seen.toml` (the name is the row label); saved on the
    /// worker only when something changed. Runs on every tray-host change, panel open or not.
    pub(super) fn note_seen_icons(&mut self) {
        let icons = self.tray_host.as_ref().map(|h| h.icons()).unwrap_or_default();
        let own = unsafe { GetCurrentProcessId() };
        let mut changed = false;
        for icon in icons.iter().filter(|i| !i.hidden && !is_bar_duplicate(&i.key)) {
            let name = self.tray_names.entry(icon.pid).or_insert_with(|| process_display_name(icon.pid).unwrap_or_default());
            let label = row_label(&icon.tip, name);
            let exe = self.tray_exe(icon.pid);
            if label.is_empty() || !is_noted(&exe, icon.pid, own) {
                continue;
            }
            changed |= self.tray_seen.note(&exe, &label);
        }
        if changed {
            let text = self.tray_seen.to_toml();
            self.submit_job(Job::Run(Box::new(move || {
                if let Err(e) = save_atomic(&tray_seen_path(), &text) {
                    log::warn!("cannot write tray_seen.toml: {e}");
                }
            })));
        }
    }

    /// NewWin, then the shown icons as configured (`tray.order` first, `tray.hidden` left out, the
    /// rest in order of addition); `tray_keys[i]` belongs to row `i + 1`.
    fn tray_rows(&mut self) -> Vec<TrayRow> {
        let icons = self.tray_host.as_ref().map(|h| h.icons()).unwrap_or_default();
        let mut pending = Vec::new();
        for icon in icons.iter().filter(|i| !i.hidden && !is_bar_duplicate(&i.key)) {
            let name = self.tray_names.entry(icon.pid).or_insert_with(|| process_display_name(icon.pid).unwrap_or_default());
            let label = row_label(&icon.tip, name);
            if label.is_empty() {
                continue;
            }
            let exe = self.tray_exe(icon.pid);
            pending.push((exe, (TrayRow { label, header: false }, icon.key)));
        }
        let tray = &self.config.tray;
        let mut rows = vec![TrayRow { label: "NewWin".into(), header: true }];
        self.tray_keys.clear();
        for (row, key) in arrange_rows(pending, &tray.order, &tray.hidden) {
            rows.push(row);
            self.tray_keys.push(key);
        }
        rows
    }

    /// `tray.order` or `tray.hidden` changed: an open tray follows.
    pub(super) fn refresh_tray(&mut self) {
        self.on_tray_changed(0);
    }

    pub(super) fn popup_style_px(&self) -> PopupStyle {
        let a = &self.config.appearance;
        PopupStyle { font_family: a.font_family.clone(), font_size_px: scale(a.font_size, self.dpi), dpi: self.dpi }
    }

    pub(super) fn open_tray(&mut self) {
        if let Some(h) = &self.tray_host {
            h.prune();
            restore_known_icons(h);
        }
        let rows = self.tray_rows();
        log::debug!("tray opened: {:?}", rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>());
        let list = TrayList::new(rows, self.strings.no_background_apps.into(), &self.popup_style_px(), &self.gfx);
        let c = self.right.chevron;
        let (left, top) = (self.bar_rect.left as f32, self.bar_rect.top as f32);
        if let Some(h) = &self.tray_host {
            h.set_anchor(RECT { left: (left + c.x) as i32, top: (top + c.y) as i32, right: (left + c.right()) as i32, bottom: (top + c.bottom()) as i32 });
        }
        // Left-aligned with the bar: centred on the button it would stick out past the bar's end.
        let width = newwin_ui::panel::PanelContent::size(&list).0;
        let anchor = PointF { x: left + width / 2.0, y: self.bar_rect.bottom as f32 + scale(4.0, self.dpi).round() };
        match Panel::show(&self.gfx, list, anchor, self.dpi, self.bar, WM_APP_PANEL) {
            Ok(p) => {
                self.open_panel(ShellPanel::Tray(p));
                self.right.chevron_turn.to(1.0, now_ms(), theme::HOVER_GLIDE_MS);
                self.paint();
            }
            Err(e) => log::error!("cannot open the tray: {e}"),
        }
    }

    /// The tray host's model changed: an open tray follows.
    pub(super) fn on_tray_changed(&mut self, _change: usize) {
        self.note_seen_icons();
        if !matches!(self.panel, Some(ShellPanel::Tray(_))) {
            return;
        }
        let rows = self.tray_rows();
        if let Some(ShellPanel::Tray(p)) = &self.panel {
            p.update(|l| {
                l.set_rows(rows);
            });
        }
        self.sync_glass();
    }

    /// Opens NewWinSettings.exe; without it (or when it cannot start) the config file opens.
    pub(super) fn open_settings(&self) {
        let Some(exe) = newwin_platform::launch::settings_exe() else {
            self.open_settings_file();
            return;
        };
        if let Err(e) = newwin_platform::launch::run_settings(&exe) {
            log::error!("cannot start {}: {e}", exe.display());
            self.open_settings_file();
        }
    }

    fn open_settings_file(&self) {
        let path = newwin_config::default_path();
        if let Err(e) = newwin_config::write_default_if_missing(&path) {
            log::error!("cannot create {}: {e}", path.display());
            return;
        }
        if let Err(e) = shell_execute("open", &path.to_string_lossy(), "") {
            log::error!("cannot open {}: {e}", path.display());
        }
    }

    pub(super) fn on_tray_action(&mut self, id: u32) {
        let (row, op) = decode_action(id);
        let mut at = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut at);
        }
        if row == 0 {
            match op {
                TrayOp::Left | TrayOp::Key => self.open_settings(),
                TrayOp::Right => {
                    self.close_panel();
                    self.open_newwin_menu(PointF { x: at.x as f32, y: at.y as f32 });
                }
            }
            return;
        }
        let Some(key) = self.tray_keys.get(row - 1).copied() else { return };
        let Some(icon) = self.tray_host.as_ref().and_then(|h| h.get(&key)) else { return };
        let click = match op {
            TrayOp::Left => TrayClick::Left,
            TrayOp::Right => TrayClick::Right,
            TrayOp::Key => TrayClick::Key,
        };
        log::debug!("tray click {:?} on {:?} ({})", op, icon.key, icon.tip);
        if op == TrayOp::Right {
            // The app's menu takes focus; the tray stays open under it (as in Windows 11).
            if let Some(ShellPanel::Tray(p)) = &self.panel {
                p.hold();
            }
            self.held_pid = icon.pid;
            self.foreground_watch = watch_foreground(self.ping, WM_APP_FOREGROUND).map_err(|e| log::warn!("foreground watch: {e}")).ok();
        }
        send_click(&icon, click, at);
    }

    /// While the tray is held open under an app's menu: close it once focus really moves on.
    pub(super) fn on_foreground(&mut self, hwnd: HWND) {
        let Some(ShellPanel::Tray(p)) = &self.panel else {
            self.foreground_watch = None;
            return;
        };
        let change = unsafe {
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            ForegroundChange {
                is_panel: hwnd == p.hwnd(),
                visible: IsWindowVisible(hwnd).as_bool(),
                same_app: pid == self.held_pid,
                captioned: (GetWindowLongW(hwnd, GWL_STYLE) as u32 & WS_CAPTION.0) == WS_CAPTION.0,
                tool: (GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0) != 0,
            }
        };
        log::debug!("held tray: foreground {hwnd:?} {change:?}");
        if change.is_panel {
            self.foreground_watch = None;
        } else if held_tray_closes(change) {
            self.foreground_watch = None;
            self.begin_close_panel();
        }
    }
}

/// Whether an icon's app goes into `tray_seen.toml`: its exe is known, and it is neither
/// explorer's own icon nor ours.
fn is_noted(exe: &str, pid: u32, own_pid: u32) -> bool {
    !exe.is_empty() && pid != own_pid && !exe.eq_ignore_ascii_case("explorer.exe")
}

/// The exe file name of a process, empty when it cannot be read (gone, denied).
fn exe_file_name(pid: u32) -> String {
    process_image_path(pid).and_then(|p| p.rsplit(['\\', '/']).next().map(str::to_string)).unwrap_or_default()
}

/// Icons of apps that registered with explorer before us and never will with us (NVIDIA): found
/// by their icon window and put back, so the tray has them and clicks reach the app.
fn restore_known_icons(h: &TrayHost) {
    let pids: Vec<u32> = h.icons().iter().map(|i| i.pid).collect();
    for (k, candidates) in missing_known(&session_processes(), &pids) {
        if let Some(w) = find_process_window(&candidates, k.window_class) {
            let mut pid = 0;
            unsafe { GetWindowThreadProcessId(w, Some(&mut pid)) };
            h.restore(k, w, pid);
        }
    }
}

/// A foreground change seen while the tray is held open under an app's menu.
#[derive(Clone, Copy, Debug)]
pub(super) struct ForegroundChange {
    pub is_panel: bool,
    pub visible: bool,
    /// Same process as the icon whose menu is open.
    pub same_app: bool,
    pub captioned: bool,
    /// `WS_EX_TOOLWINDOW`: menus and popups are, app windows are not.
    pub tool: bool,
}

/// The app's invisible helper window and its popup menu (or any menu: tool windows, even from a
/// helper process) keep the tray open; anything else with
/// a visible window (the app's main window, the desktop, another app) closes it.
pub(super) fn held_tray_closes(c: ForegroundChange) -> bool {
    !c.is_panel && c.visible && !c.tool && !(c.same_app && !c.captioned)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// After a right click the tray stays open under the app's own menu.
    #[test]
    fn a_held_tray_closes_only_when_focus_really_moves_on() {
        let fg = |is_panel, visible, same_app, captioned| ForegroundChange { is_panel, visible, same_app, captioned, tool: false };
        assert!(!held_tray_closes(fg(false, false, true, false)), "the app's hidden helper window takes focus for its menu");
        assert!(!held_tray_closes(fg(false, true, true, false)), "the app's popup menu itself");
        assert!(!held_tray_closes(fg(true, true, false, true)), "back to the tray");
        assert!(held_tray_closes(fg(false, true, true, true)), "a menu item opened the app's window");
        assert!(held_tray_closes(fg(false, true, false, true)), "a click on another app or the desktop");
        // Steam draws its tray menu in steamwebhelper.exe: another process, a caption style, but a
        // tool window (topmost popup), as menus are.
        let steam_menu = ForegroundChange { is_panel: false, visible: true, same_app: false, captioned: true, tool: true };
        assert!(!held_tray_closes(steam_menu), "a helper process's menu");
    }

    #[test]
    fn explorer_ourselves_and_unknown_exes_are_not_noted() {
        assert!(is_noted("Discord.exe", 10, 99));
        assert!(!is_noted("", 10, 99), "process gone or denied");
        assert!(!is_noted("Explorer.EXE", 10, 99));
        assert!(!is_noted("NewWin.exe", 99, 99), "our own icon");
    }
}
