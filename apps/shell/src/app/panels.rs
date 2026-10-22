//! Glass panels hanging from the bar (spec: M2 §3); at most one is open, never with a menu.

use newwin_core::visibility::VisInput;
use newwin_ui::GlassWindow;

use super::App;

/// The open panel; variants arrive with their contents (tray, volume).
pub(super) enum ShellPanel {}

impl ShellPanel {
    pub(super) fn glass(&self) -> &dyn GlassWindow {
        match *self {}
    }
}

impl App {
    pub(super) fn close_panel(&mut self) {
        if self.panel.take().is_some() {
            self.feed(VisInput::OverlayClosed);
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
