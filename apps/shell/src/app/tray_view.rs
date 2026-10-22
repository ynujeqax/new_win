//! The tray panel under `⌃` (spec: M2 §3.1): NewWin, then the apps of our tray host.

use newwin_core::tray::row_label;
use newwin_platform::appinfo::process_display_name;
use newwin_platform::clock::now_ms;
use newwin_platform::dpi::scale;
use newwin_platform::launch::shell_execute;
use newwin_platform::tray_host::{TrayClick, send_click};
use newwin_ui::panel::Panel;
use newwin_ui::popup::PopupStyle;
use newwin_ui::tray_list::{TrayList, TrayOp, TrayRow, decode_action};
use newwin_ui::{PointF, theme};
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

use super::App;
use super::panels::{ShellPanel, WM_APP_PANEL};

impl App {
    /// NewWin, then every shown icon in order of addition; `tray_keys[i]` belongs to row `i + 1`.
    fn tray_rows(&mut self) -> Vec<TrayRow> {
        let icons = self.tray_host.as_ref().map(|h| h.icons()).unwrap_or_default();
        let mut rows = vec![TrayRow { label: "NewWin".into(), header: true }];
        self.tray_keys.clear();
        for icon in icons.iter().filter(|i| !i.hidden) {
            let name = self.tray_names.entry(icon.pid).or_insert_with(|| process_display_name(icon.pid).unwrap_or_default());
            let label = row_label(&icon.tip, name);
            if label.is_empty() {
                continue;
            }
            rows.push(TrayRow { label, header: false });
            self.tray_keys.push(icon.key);
        }
        rows
    }

    fn popup_style_px(&self) -> PopupStyle {
        let a = &self.config.appearance;
        PopupStyle { font_family: a.font_family.clone(), font_size_px: scale(a.font_size, self.dpi), dpi: self.dpi }
    }

    pub(super) fn open_tray(&mut self) {
        if let Some(h) = &self.tray_host {
            h.prune();
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
        if !matches!(self.panel, Some(ShellPanel::Tray(_))) {
            return;
        }
        let rows = self.tray_rows();
        if let Some(ShellPanel::Tray(p)) = &self.panel {
            p.update(|l| l.set_rows(rows));
        }
        self.sync_glass();
    }

    pub(super) fn open_settings_file(&self) {
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
                TrayOp::Left | TrayOp::Key => self.open_settings_file(),
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
        send_click(&icon, click, at);
    }
}
