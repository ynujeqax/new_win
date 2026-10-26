//! Executes the visibility state machine: catcher window, timers, slide animations, AppBar.

use newwin_config::BarMode;
use newwin_core::visibility::{Catcher, Mode, VisCmd, VisConfig, VisInput, Visibility};
use newwin_platform::appbar::{AppBarNotify, parse_appbar_notify};
use newwin_platform::dpi::scale_px;
use newwin_platform::fullscreen::foreground_is_fullscreen;
use newwin_ui::{ease_out_progress, fade, set_offset_opacity, slide_y_both, theme};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent};
use windows::Win32::UI::WindowsAndMessaging::*;

use super::App;

const VIS_TIMER: usize = 1;
const HIDE_WINDOW_TIMER: usize = 2;

/// After a mouse leave, the pointer still counts as in the hot zone only when it is inside the
/// zone AND one of our windows is under it. A window on top of the bar (Screen Snipping, a toast)
/// makes Windows report a leave while the pointer is still over the bar's rectangle; that is a
/// real leave, or no further mouse message would ever come and the bar would stay shown.
fn still_in_zone(inside_zone_rect: bool, ours_under_pointer: bool) -> bool {
    inside_zone_rect && ours_under_pointer
}

fn contains(r: RECT, p: POINT) -> bool {
    p.x >= r.left && p.x < r.right && p.y >= r.top && p.y < r.bottom
}

impl App {
    pub(super) fn mode(&self) -> Mode {
        match self.config.bar.mode {
            BarMode::Pinned => Mode::Pinned,
            BarMode::AutoHide => Mode::AutoHide,
        }
    }

    pub(super) fn start_visibility(&mut self) {
        let b = &self.config.bar;
        let (vis, cmds) = Visibility::new(VisConfig {
            mode: self.mode(),
            show_delay_ms: b.show_delay_ms,
            hide_delay_ms: b.hide_delay_ms,
            ignore_fullscreen: b.ignore_fullscreen,
        });
        self.vis = Some(vis);
        self.reserve_space();
        self.exec(cmds);
    }

    pub(super) fn feed(&mut self, input: VisInput) {
        let Some(vis) = self.vis.as_mut() else { return };
        let cmds = vis.handle(input);
        log::debug!("visibility {input:?} -> {:?} {cmds:?}", vis.state());
        self.exec(cmds);
    }

    pub(super) fn exec(&mut self, cmds: Vec<VisCmd>) {
        for c in cmds {
            match c {
                VisCmd::StartTimer(ms) => unsafe {
                    SetTimer(Some(self.bar), VIS_TIMER, ms.max(1), None);
                },
                VisCmd::CancelTimer => unsafe {
                    let _ = KillTimer(Some(self.bar), VIS_TIMER);
                },
                VisCmd::AnimateIn => self.animate(true),
                VisCmd::AnimateOut => self.animate(false),
                VisCmd::SetCatcher(m) => self.set_catcher(m),
            }
        }
        // Every visibility change passes here, including the first one at start (pinned mode
        // shows the bar there without any input), so the glass regions always follow it.
        self.sync_glass();
    }

    pub(super) fn on_timer(&mut self, id: usize) {
        unsafe {
            let _ = KillTimer(Some(self.bar), id);
        }
        match id {
            VIS_TIMER => self.feed(VisInput::Timer),
            super::clock_view::CLOCK_TIMER => self.tick_clock(),
            super::glass_regions::SNIP_CHECK_TIMER => self.check_snip(),
            super::right_view::ANIM_TIMER => self.paint(),
            super::right_view::TOOLTIP_TIMER => self.show_tooltip(),
            super::network_view::NET_DEBOUNCE_TIMER => self.read_network(),
            super::layout_view::LAYOUT_RECHECK_TIMER => self.read_layout(),
            super::spotlight_view::APPS_DEBOUNCE_TIMER => self.reindex_apps(),
            super::spotlight_view::USAGE_SAVE_TIMER => self.save_usage(),
            super::config_view::CONFIG_DEBOUNCE_TIMER => self.reload_config(),
            HIDE_WINDOW_TIMER => unsafe {
                log::debug!("hide timer: hiding the bar window");
                self.hide_started = None;
                let _ = ShowWindow(self.bar, SW_HIDE);
                self.sync_glass();
            },
            _ => {}
        }
    }

    /// Hidden distance: the bar's height plus its top margin, so it slides fully off-screen.
    fn hidden_offset(&self) -> f32 {
        -(self.bar_rect.bottom - self.monitor.top) as f32
    }

    fn animate(&mut self, show: bool) {
        let hidden = self.hidden_offset();
        let Some(layer) = self.bar_layer.as_mut() else { return };
        let Ok(visual) = layer.visual() else { return };
        // The blurred backdrop slides the opposite way, so it stays still on screen.
        let backdrop = layer.backdrop_visual().ok().flatten();
        log::debug!("animate {} (window visible={})", if show { "in" } else { "out" }, unsafe { IsWindowVisible(self.bar) }.as_bool());
        let r = if show {
            // Coming back mid-hide continues from where the hide animation is now, without a jump.
            let travelled = self.hide_started.take().map_or(1.0, |t| ease_out_progress(t.elapsed().as_secs_f32() * 1000.0 / theme::HIDE_MS as f32));
            let (from_y, from_opacity) = (hidden * travelled, 1.0 - travelled);
            unsafe {
                let _ = KillTimer(Some(self.bar), HIDE_WINDOW_TIMER);
                if !IsWindowVisible(self.bar).as_bool() {
                    let _ = set_offset_opacity(&self.gfx, &visual, backdrop.as_ref(), hidden, 0.0);
                    let _ = ShowWindow(self.bar, SW_SHOWNOACTIVATE);
                }
            }
            if let Some(sh) = self.bar_shadow.as_mut() {
                sh.stay_under();
                sh.animate(from_y, 0.0, from_opacity, 1.0, theme::REVEAL_MS);
            }
            slide_y_both(&self.gfx, &visual, backdrop.as_ref(), from_y, 0.0, theme::REVEAL_MS)
                .and_then(|_| fade(&self.gfx, &visual, from_opacity, 1.0, theme::REVEAL_MS))
        } else {
            self.hide_started = Some(std::time::Instant::now());
            unsafe { SetTimer(Some(self.bar), HIDE_WINDOW_TIMER, theme::HIDE_MS + 20, None) };
            if let Some(sh) = self.bar_shadow.as_mut() {
                sh.animate(0.0, hidden, 1.0, 0.0, theme::HIDE_MS);
            }
            slide_y_both(&self.gfx, &visual, backdrop.as_ref(), 0.0, hidden, theme::HIDE_MS).and_then(|_| fade(&self.gfx, &visual, 1.0, 0.0, theme::HIDE_MS))
        };
        if let Err(e) = r {
            log::error!("bar animation failed: {e}");
        }
    }

    fn catcher_rect(&self, mode: Catcher) -> Option<RECT> {
        let m = self.monitor;
        let height = match mode {
            Catcher::Off => return None,
            Catcher::Strip => i32::from(self.config.bar.trigger_height_px),
            Catcher::Zone => self.bar_rect.bottom - m.top,
        };
        Some(RECT { bottom: m.top + height, ..m })
    }

    pub(super) fn set_catcher(&mut self, mode: Catcher) {
        self.catcher_mode = mode;
        unsafe {
            match self.catcher_rect(mode) {
                None => {
                    let _ = ShowWindow(self.catcher, SW_HIDE);
                }
                Some(r) => {
                    // Just below the bar, so the bar receives the pointer where they overlap.
                    let _ = SetWindowPos(
                        self.catcher,
                        Some(self.bar),
                        r.left,
                        r.top,
                        r.right - r.left,
                        r.bottom - r.top,
                        SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    );
                }
            }
        }
    }

    /// Pinned mode reserves the bar's band (margin + height); auto-hide reserves nothing.
    pub(super) fn reserve_space(&mut self) {
        let thickness = match self.config.bar.mode {
            BarMode::Pinned => scale_px(self.config.appearance.margin * 2 + self.config.appearance.height, self.dpi),
            BarMode::AutoHide => 0,
        };
        if let Some(ab) = &self.appbar {
            ab.set_top(self.monitor, thickness);
        }
    }

    fn pointer_in_zone(&self) -> bool {
        let mut p = POINT::default();
        unsafe {
            if GetCursorPos(&mut p).is_err() {
                return false;
            }
            let on_bar = IsWindowVisible(self.bar).as_bool() && contains(self.bar_rect, p);
            let inside = on_bar || self.catcher_rect(self.catcher_mode).is_some_and(|r| contains(r, p));
            let under = WindowFromPoint(p);
            still_in_zone(inside, under == self.bar || under == self.catcher)
        }
    }

    pub(super) fn on_pointer_move(&mut self, hwnd: HWND) {
        let i = usize::from(hwnd == self.catcher);
        if !self.tracking[i] {
            let mut tme = TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
            self.tracking[i] = unsafe { TrackMouseEvent(&mut tme) }.is_ok();
        }
        if !self.in_zone {
            self.in_zone = true;
            self.feed(VisInput::ZoneEnter);
        }
    }

    pub(super) fn on_pointer_leave(&mut self, hwnd: HWND) {
        self.tracking[usize::from(hwnd == self.catcher)] = false;
        log::debug!("mouse leave from {} in_zone={} still_inside={}", if hwnd == self.catcher { "catcher" } else { "bar" }, self.in_zone, self.pointer_in_zone());
        if self.in_zone && !self.pointer_in_zone() {
            self.in_zone = false;
            self.feed(VisInput::ZoneLeave);
        }
    }

    pub(super) fn on_appbar(&mut self, wp: WPARAM, lp: LPARAM) {
        match parse_appbar_notify(wp, lp) {
            AppBarNotify::PosChanged => self.reserve_space(),
            AppBarNotify::FullScreen(fs) => {
                let fs = fs && foreground_is_fullscreen(self.monitor);
                log::info!("fullscreen app: {fs}");
                self.fullscreen = fs;
                if self.config.bar.mode == BarMode::Pinned {
                    // Like the native taskbar: drop below a full-screen app, come back after.
                    unsafe {
                        let after = if fs { HWND_NOTOPMOST } else { HWND_TOPMOST };
                        let _ = SetWindowPos(self.bar, Some(after), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                        if fs {
                            let _ = SetWindowPos(self.bar, Some(HWND_BOTTOM), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                        }
                    }
                    // The shadow goes with the bar, under a full-screen app too.
                    if let Some(sh) = &self.bar_shadow {
                        sh.stay_under();
                    }
                }
                self.feed(VisInput::Fullscreen(fs));
            }
            AppBarNotify::Other => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::still_in_zone;

    #[test]
    fn a_window_covering_the_bar_counts_as_leaving() {
        // Screen Snipping over the bar: pointer inside the zone rect, but not over our window.
        assert!(!still_in_zone(true, false));
        assert!(still_in_zone(true, true));
        assert!(!still_in_zone(false, true));
        assert!(!still_in_zone(false, false));
    }
}
