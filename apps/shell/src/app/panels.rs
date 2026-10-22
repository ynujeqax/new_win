//! Glass panels hanging from the bar (spec: M2 §3); at most one is open, never with a menu.

use newwin_core::items::RightItem;
use newwin_core::visibility::VisInput;
use newwin_platform::clock::now_ms;
use newwin_ui::GlassWindow;
use newwin_ui::panel::Panel;
use newwin_ui::popup::{CLOSED, PICKED};
use newwin_ui::tray_list::TrayList;
use newwin_ui::volume_panel::VolumePanel;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::WM_APP;

use super::App;
use super::apps_view::Target;

pub(super) const WM_APP_PANEL: u32 = WM_APP + 11;

/// The open panel.
pub(super) enum ShellPanel {
    Tray(Panel<TrayList>),
    Volume(Panel<VolumePanel>),
}

impl ShellPanel {
    pub(super) fn glass(&self) -> &dyn GlassWindow {
        match self {
            ShellPanel::Tray(p) => p,
            ShellPanel::Volume(p) => p,
        }
    }

    pub(super) fn hwnd(&self) -> HWND {
        match self {
            ShellPanel::Tray(p) => p.hwnd(),
            ShellPanel::Volume(p) => p.hwnd(),
        }
    }

    /// The bar item the panel hangs from (a second click on it closes the panel).
    pub(super) fn anchor(&self) -> Target {
        match self {
            ShellPanel::Tray(_) => Target::Tray,
            ShellPanel::Volume(_) => Target::Right(RightItem::Volume),
        }
    }

    fn close(&self) {
        match self {
            ShellPanel::Tray(p) => p.close(),
            ShellPanel::Volume(p) => p.close(),
        }
    }
}

impl App {
    /// Drops the panel at once (another menu or panel replaces it).
    pub(super) fn close_panel(&mut self) {
        self.foreground_watch = None;
        if let Some(p) = self.panel.take() {
            self.panel_gone(p.anchor());
        }
    }

    /// Plays the panel's closing animation; `CLOSED` follows and drops it.
    pub(super) fn begin_close_panel(&mut self) {
        let Some(p) = &self.panel else { return };
        p.close();
        if p.anchor() == Target::Tray {
            self.right.chevron_turn.to(0.0, now_ms(), newwin_ui::theme::HOVER_GLIDE_MS);
            self.paint();
        }
    }

    fn panel_gone(&mut self, anchor: Target) {
        self.feed(VisInput::OverlayClosed);
        if anchor == Target::Tray {
            self.right.chevron_turn.to(0.0, now_ms(), newwin_ui::theme::HOVER_GLIDE_MS);
        }
        self.sync_glass();
        self.paint();
    }

    pub(super) fn open_panel(&mut self, p: ShellPanel) {
        self.close_popup();
        self.close_panel();
        self.panel = Some(p);
        self.feed(VisInput::OverlayOpened);
        self.sync_glass();
    }

    pub(super) fn on_panel(&mut self, wp: WPARAM, lp: LPARAM) {
        match lp.0 {
            PICKED => match self.panel.as_ref().map(ShellPanel::anchor) {
                Some(Target::Tray) => self.on_tray_action(wp.0 as u32),
                Some(Target::Right(RightItem::Volume)) => self.on_volume_action(wp.0 as u32),
                _ => {}
            },
            CLOSED if self.panel.as_ref().map(|p| p.hwnd().0 as usize) == Some(wp.0) => self.close_panel(),
            _ => {}
        }
    }

    /// Every glass window over the bar right now: menu, panel, tooltip.
    pub(super) fn glass_windows(&self) -> Vec<&dyn GlassWindow> {
        let mut out: Vec<&dyn GlassWindow> = Vec::new();
        if let Some((p, _)) = &self.popup {
            out.push(p);
        }
        if let Some(p) = &self.panel {
            out.push(p.glass());
        }
        if let Some(t) = &self.tooltip {
            out.push(t);
        }
        out
    }
}
