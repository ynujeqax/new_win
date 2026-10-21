//! Monitor geometry and DPI.

use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};

pub const BASE_DPI: u32 = 96;

/// Logical (96-DPI) length → physical pixels.
pub fn scale(px: f32, dpi: u32) -> f32 {
    px * dpi as f32 / BASE_DPI as f32
}

pub fn scale_px(px: u32, dpi: u32) -> i32 {
    scale(px as f32, dpi).round() as i32
}

/// Primary monitor rectangle in physical pixels and its effective DPI.
pub fn primary_monitor() -> (RECT, u32) {
    unsafe {
        let mon = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(mon, &mut info);
        let (mut dx, mut dy) = (BASE_DPI, BASE_DPI);
        if GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy).is_err() {
            dx = BASE_DPI;
        }
        (info.rcMonitor, dx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_by_dpi() {
        assert_eq!(scale(30.0, 96), 30.0);
        assert_eq!(scale(30.0, 144), 45.0);
        assert_eq!(scale(6.0, 120), 7.5);
        assert_eq!(scale_px(6, 120), 8); // rounded
    }
}
