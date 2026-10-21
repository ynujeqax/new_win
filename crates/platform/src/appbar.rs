//! Registering the bar as a shell AppBar: reserves screen space (pinned mode) and receives
//! full-screen notifications (both modes).

use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::UI::Shell::{ABE_TOP, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE, ABM_SETPOS, APPBARDATA, SHAppBarMessage};
use windows::core::Result;

const ABN_POSCHANGED: usize = 1;
const ABN_FULLSCREENAPP: usize = 2;

#[derive(Debug, PartialEq, Eq)]
pub enum AppBarNotify {
    PosChanged,
    FullScreen(bool),
    Other,
}

/// Decodes the AppBar callback message (`wParam` = notification, `lParam` = argument).
pub fn parse_appbar_notify(wp: WPARAM, lp: LPARAM) -> AppBarNotify {
    match wp.0 {
        ABN_POSCHANGED => AppBarNotify::PosChanged,
        ABN_FULLSCREENAPP => AppBarNotify::FullScreen(lp.0 != 0),
        _ => AppBarNotify::Other,
    }
}

pub struct AppBar {
    hwnd: HWND,
    callback_msg: u32,
}

impl AppBar {
    fn data(&self) -> APPBARDATA {
        APPBARDATA { cbSize: size_of::<APPBARDATA>() as u32, hWnd: self.hwnd, ..Default::default() }
    }

    /// Registers without reserving space; notifications arrive as `callback_msg`.
    pub fn register(hwnd: HWND, callback_msg: u32) -> Result<Self> {
        let bar = AppBar { hwnd, callback_msg };
        bar.add()?;
        Ok(bar)
    }

    fn add(&self) -> Result<()> {
        let mut abd = self.data();
        abd.uCallbackMessage = self.callback_msg;
        if unsafe { SHAppBarMessage(ABM_NEW, &mut abd) } == 0 {
            return Err(windows::core::Error::new(windows::core::HRESULT(-1), "ABM_NEW refused"));
        }
        Ok(())
    }

    /// Registers again after explorer restarted (`TaskbarCreated`): registrations live in explorer
    /// and die with it. Removing first makes it work whether or not explorer still knows us.
    /// Reserve the space again with [`Self::set_top`] afterwards.
    pub fn reregister(&mut self) -> Result<()> {
        let mut abd = self.data();
        unsafe { SHAppBarMessage(ABM_REMOVE, &mut abd) };
        self.add()
    }

    /// Reserves a band of `thickness_px` along the top of `monitor`; returns the granted band.
    /// A thickness of 0 releases the reservation.
    pub fn set_top(&self, monitor: RECT, thickness_px: i32) -> RECT {
        let mut abd = self.data();
        abd.uEdge = ABE_TOP;
        abd.rc = RECT { bottom: monitor.top + thickness_px, ..monitor };
        unsafe {
            SHAppBarMessage(ABM_QUERYPOS, &mut abd);
            abd.rc.bottom = abd.rc.top + thickness_px;
            SHAppBarMessage(ABM_SETPOS, &mut abd);
        }
        abd.rc
    }
}

impl Drop for AppBar {
    fn drop(&mut self) {
        let mut abd = self.data();
        unsafe { SHAppBarMessage(ABM_REMOVE, &mut abd) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::{LPARAM, WPARAM};

    #[test]
    fn notifications() {
        assert_eq!(parse_appbar_notify(WPARAM(1), LPARAM(0)), AppBarNotify::PosChanged);
        assert_eq!(parse_appbar_notify(WPARAM(2), LPARAM(1)), AppBarNotify::FullScreen(true));
        assert_eq!(parse_appbar_notify(WPARAM(2), LPARAM(0)), AppBarNotify::FullScreen(false));
        assert_eq!(parse_appbar_notify(WPARAM(0), LPARAM(0)), AppBarNotify::Other);
    }
}
