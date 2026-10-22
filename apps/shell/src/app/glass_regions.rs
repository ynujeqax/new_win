//! Which glass windows want a backdrop right now (spec: liquid glass §3, "when capture is on").

use newwin_core::glass::PxRect;
use newwin_ui::backdrop::{FRAME, Region, UNAVAILABLE, take_frame};
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::IsWindowVisible;

use super::App;

fn px(r: RECT) -> PxRect {
    PxRect { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
}

fn id(h: HWND) -> u64 {
    h.0 as usize as u64
}

impl App {
    /// Tells the glass thread which windows want a backdrop, when that set changed.
    pub(super) fn sync_glass(&mut self) {
        let Some(backdrops) = &self.backdrops else { return };
        // Under a full-screen app the bar is covered: no point reading the screen for it.
        let bar_visible = unsafe { IsWindowVisible(self.bar) }.as_bool() && !self.fullscreen;
        let popups: Vec<(u64, PxRect)> =
            self.popup.as_ref().map(|(p, _)| p.windows().into_iter().map(|(h, r)| (id(h), px(r))).collect()).unwrap_or_default();
        let wanted = wanted_regions(Some((id(self.bar), px(self.bar_rect))), bar_visible, self.in_zone, &popups);
        if wanted != self.glass_sent {
            log::debug!("glass regions {wanted:?}");
            backdrops.set_regions(wanted.clone());
            self.glass_sent = wanted;
        }
    }

    pub(super) fn on_glass(&mut self, wp: WPARAM, lp: LPARAM) {
        match wp.0 {
            FRAME => {
                let frame = unsafe { take_frame(lp) };
                if !frame_target(frame.id, &self.glass_sent) {
                    return;
                }
                if frame.id == id(self.bar) {
                    let origin = (self.bar_rect.left, self.bar_rect.top);
                    let Some(layer) = self.bar_layer.as_mut() else { return };
                    if let Err(e) = layer.set_backdrop(&frame, origin) {
                        log::error!("bar backdrop failed: {e}");
                    } else if !self.bar_glass {
                        self.bar_glass = true;
                        self.paint();
                    }
                } else if let Some((p, _)) = &self.popup {
                    p.set_backdrop(HWND(frame.id as usize as *mut _), &frame);
                }
            }
            UNAVAILABLE => {
                log::warn!("glass: screen capture unavailable, using the plain tint");
                if let Some(layer) = self.bar_layer.as_mut() {
                    let _ = layer.clear_backdrop();
                }
                self.bar_glass = false;
                self.paint();
                if let Some((p, _)) = &self.popup {
                    p.set_glass_unavailable();
                }
            }
            _ => {}
        }
    }
}

/// The bar wants a backdrop while it is shown or the pointer is in its hot zone (capture setup
/// takes ~110 ms, the reveal waits 150 ms); every open popup window always does.
pub(super) fn wanted_regions(bar: Option<(u64, PxRect)>, bar_visible: bool, in_zone: bool, popups: &[(u64, PxRect)]) -> Vec<Region> {
    let bar = bar.filter(|_| bar_visible || in_zone);
    bar.into_iter().chain(popups.iter().copied()).map(|(id, window)| Region { id, window }).collect()
}

/// A frame is applied only while its window is still registered; frames in flight for a hidden
/// bar or a closed popup are dropped.
pub(super) fn frame_target(frame_id: u64, regions: &[Region]) -> bool {
    regions.iter().any(|r| r.id == frame_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BAR: PxRect = PxRect { left: 6, top: 6, right: 2554, bottom: 36 };
    const MENU: PxRect = PxRect { left: 100, top: 40, right: 300, bottom: 300 };
    const SUB: PxRect = PxRect { left: 290, top: 60, right: 500, bottom: 200 };

    #[test]
    fn bar_region_follows_visibility_or_zone() {
        assert!(wanted_regions(Some((1, BAR)), false, false, &[]).is_empty());
        assert_eq!(wanted_regions(Some((1, BAR)), false, true, &[]), vec![Region { id: 1, window: BAR }]);
        assert_eq!(wanted_regions(Some((1, BAR)), true, false, &[]), vec![Region { id: 1, window: BAR }]);
        assert!(wanted_regions(None, true, true, &[]).is_empty());
    }

    #[test]
    fn popups_are_always_included() {
        let popups = [(2, MENU), (3, SUB)];
        assert_eq!(wanted_regions(Some((1, BAR)), false, false, &popups), vec![Region { id: 2, window: MENU }, Region { id: 3, window: SUB }]);
        assert_eq!(wanted_regions(Some((1, BAR)), true, false, &popups).len(), 3);
    }

    #[test]
    fn stale_frames_are_dropped() {
        assert!(!frame_target(9, &[]));
        assert!(frame_target(9, &[Region { id: 9, window: MENU }]));
        assert!(!frame_target(9, &[Region { id: 1, window: BAR }]));
    }
}
