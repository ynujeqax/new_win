//! Is the foreground app full-screen (game, video, presentation)?

use windows::Win32::Foundation::RECT;
use windows::Win32::UI::Shell::{
    QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState,
};
use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetForegroundWindow, GetWindowRect};

/// True when `window` covers the whole `monitor` (borders may overhang).
pub fn covers(window: RECT, monitor: RECT) -> bool {
    window.left <= monitor.left && window.top <= monitor.top && window.right >= monitor.right && window.bottom >= monitor.bottom
}

pub fn foreground_is_fullscreen(monitor: RECT) -> bool {
    unsafe {
        if let Ok(state) = SHQueryUserNotificationState()
            && (state == QUNS_BUSY || state == QUNS_RUNNING_D3D_FULL_SCREEN || state == QUNS_PRESENTATION_MODE)
        {
            return true;
        }
        let fg = GetForegroundWindow();
        if fg.is_invalid() {
            return false;
        }
        let mut buf = [0u16; 32];
        let n = GetClassNameW(fg, &mut buf);
        let class = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
        if class == "Progman" || class == "WorkerW" {
            return false;
        }
        let mut r = RECT::default();
        GetWindowRect(fg, &mut r).is_ok() && covers(r, monitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::RECT;

    #[test]
    fn covering() {
        let mon = RECT { left: 0, top: 0, right: 2560, bottom: 1440 };
        assert!(covers(mon, mon));
        assert!(covers(RECT { left: -8, top: -8, right: 2568, bottom: 1448 }, mon));
        assert!(!covers(RECT { left: 0, top: 0, right: 2560, bottom: 1392 }, mon));
        assert!(!covers(RECT { left: 0, top: 42, right: 2560, bottom: 1440 }, mon));
    }
}
