//! What [`crate::panel::Panel`] and [`crate::app_window::AppWindow`] share: client mouse and
//! keyboard messages translated to [`PanelInput`], the frame / wake timers of a drawn animation,
//! and staying clear until the first glass frame.

use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent, VK_CONTROL};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::Result;

use crate::panel::PanelInput;
use crate::{Gfx, Layer, PointF, set_offset_opacity};

/// Not exported by the `windows` crate.
pub(crate) const WM_MOUSELEAVE: u32 = 0x02A3;
pub(crate) const FRAME_TIMER: usize = 1;
pub(crate) const WAKE_TIMER: usize = 3;
pub(crate) const REVEAL_TIMER: usize = 4;
pub(crate) const FRAME_MS: u32 = 16;
/// The longest a glass-waiting window stays clear.
pub(crate) const REVEAL_MAX_MS: u32 = 250;

/// Where a press that lost the mouse capture is released: outside any window.
pub(crate) const CANCEL_POINT: PointF = PointF { x: -1.0e6, y: -1.0e6 };

/// A client point from a mouse message's `lParam`.
pub(crate) fn point(lp: LPARAM) -> PointF {
    PointF { x: (lp.0 & 0xFFFF) as i16 as f32, y: ((lp.0 >> 16) & 0xFFFF) as i16 as f32 }
}

/// Mouse tracking and capture of one window.
#[derive(Default)]
pub(crate) struct Mouse {
    tracking: bool,
    capturing: bool,
}

impl Mouse {
    /// `None`: not an input message. `Some(None)`: handled, nothing for the content (a control
    /// character). Tracks the mouse leaving and captures it while the left button is down.
    pub(crate) fn translate(&mut self, hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<Option<PanelInput>> {
        let ev = match msg {
            WM_MOUSEMOVE => {
                if !self.tracking {
                    let mut tme = TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                    self.tracking = unsafe { TrackMouseEvent(&mut tme) }.is_ok();
                }
                PanelInput::Move(point(lp))
            }
            WM_MOUSELEAVE => {
                self.tracking = false;
                PanelInput::Leave
            }
            WM_LBUTTONDOWN => {
                unsafe { SetCapture(hwnd) };
                self.capturing = true;
                PanelInput::Press(point(lp))
            }
            WM_LBUTTONUP => {
                self.release();
                PanelInput::Release(point(lp))
            }
            // Another window took the mouse while the button was down (the Win key, a toast, UAC):
            // the button-up never comes, so the press ends here. The point is far outside, so it
            // cancels like a Windows button does: nothing under the pointer is clicked, a dragged
            // row lands in the slot it was last shown in.
            WM_CAPTURECHANGED => {
                if !self.capturing {
                    return Some(None);
                }
                self.capturing = false;
                PanelInput::Release(CANCEL_POINT)
            }
            WM_RBUTTONUP => PanelInput::RightRelease(point(lp)),
            WM_MOUSEWHEEL => PanelInput::Wheel(((wp.0 >> 16) & 0xFFFF) as i16),
            WM_KEYDOWN => {
                let vk = wp.0 as u16;
                if unsafe { GetKeyState(i32::from(VK_CONTROL.0)) } < 0 { PanelInput::CtrlKey(vk) } else { PanelInput::Key(vk) }
            }
            WM_CHAR => return Some(char::from_u32(wp.0 as u32).filter(|c| !c.is_control()).map(PanelInput::Char)),
            WM_CONTEXTMENU => {
                // -1: from the keyboard (the menu key, Shift+F10); else a screen point.
                let at = (lp.0 != -1).then(|| {
                    let mut pt = POINT { x: (lp.0 & 0xFFFF) as i16 as i32, y: ((lp.0 >> 16) & 0xFFFF) as i16 as i32 };
                    unsafe {
                        let _ = ScreenToClient(hwnd, &mut pt);
                    }
                    PointF { x: pt.x as f32, y: pt.y as f32 }
                });
                PanelInput::ContextMenu(at)
            }
            _ => return None,
        };
        Some(Some(ev))
    }

    /// Lets go of the mouse capture, if held.
    pub(crate) fn release(&mut self) {
        if self.capturing {
            self.capturing = false;
            unsafe {
                let _ = ReleaseCapture();
            }
        }
    }
}

/// The frame timer of a drawn animation and the one-shot wake for changes without one.
#[derive(Default)]
pub(crate) struct Frames {
    animating: bool,
}

impl Frames {
    /// A [`FRAME_MS`] timer while `animating`; otherwise a one-shot [`WAKE_TIMER`] at `wake_at`
    /// (the caret blink), if any.
    pub(crate) fn schedule(&mut self, hwnd: HWND, animating: bool, wake_at: Option<u64>, now: u64) {
        if animating {
            unsafe { SetTimer(Some(hwnd), FRAME_TIMER, FRAME_MS, None) };
        } else if self.animating {
            unsafe {
                let _ = KillTimer(Some(hwnd), FRAME_TIMER);
            }
        }
        self.animating = animating;
        match wake_at.filter(|_| !animating) {
            Some(at) => unsafe {
                SetTimer(Some(hwnd), WAKE_TIMER, at.saturating_sub(now).clamp(1, u64::from(u32::MAX)) as u32, None);
            },
            None => unsafe {
                let _ = KillTimer(Some(hwnd), WAKE_TIMER);
            },
        }
    }

    /// `WM_TIMER` with `id`: true when it is the frame or the wake timer (repaint now).
    pub(crate) fn on_timer(hwnd: HWND, id: usize) -> bool {
        match id {
            FRAME_TIMER => true,
            WAKE_TIMER => {
                unsafe {
                    let _ = KillTimer(Some(hwnd), WAKE_TIMER);
                }
                true
            }
            _ => false,
        }
    }
}

/// Makes the window clear at vertical offset `y` (where its show animation starts) until
/// [`end_clear`], at most [`REVEAL_MAX_MS`] ([`REVEAL_TIMER`] fires then).
pub(crate) fn start_clear(gfx: &Gfx, layer: &mut Layer, hwnd: HWND, y: f32) -> Result<()> {
    unsafe { SetTimer(Some(hwnd), REVEAL_TIMER, REVEAL_MAX_MS, None) };
    let root = layer.visual()?;
    let inner = layer.backdrop_visual()?;
    set_offset_opacity(gfx, &root, inner.as_ref(), y, 0.0)
}

/// The glass came (or will not): stop waiting for it.
pub(crate) fn end_clear(hwnd: HWND) {
    unsafe {
        let _ = KillTimer(Some(hwnd), REVEAL_TIMER);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lost_capture_ends_the_press_like_a_release() {
        let mut m = Mouse { tracking: false, capturing: true };
        let ev = m.translate(HWND::default(), WM_CAPTURECHANGED, WPARAM(0), LPARAM(0));
        assert_eq!(ev, Some(Some(PanelInput::Release(CANCEL_POINT))), "released outside: no click");
        assert!(!m.capturing);
        // Not holding the capture (our own ReleaseCapture already ran): nothing more.
        assert_eq!(m.translate(HWND::default(), WM_CAPTURECHANGED, WPARAM(0), LPARAM(0)), Some(None));
    }
}
