//! Spotlight (spec: M3): opened by a Win tap, `Ctrl+Esc` or the bar's magnifier.

use windows::Win32::UI::WindowsAndMessaging::WM_APP;

use super::App;

/// The keyboard hook saw a lone Win tap or `Ctrl+Esc`.
pub(super) const WM_APP_SPOTLIGHT: u32 = WM_APP + 14;
/// The worker rebuilt the app index.
pub(super) const WM_APP_APPS: u32 = WM_APP + 16;

impl App {
    pub(super) fn toggle_spotlight(&mut self) {
        log::debug!("spotlight toggle");
    }
}
