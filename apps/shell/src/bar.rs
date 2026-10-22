//! The bar window: geometry and painting.

use newwin_config::Appearance;
use newwin_platform::dpi::{scale, scale_px};
use newwin_ui::{Painter, RectF, TextStyle, paint_glass};
use windows::Win32::Foundation::RECT;

pub fn text_style(a: &Appearance, dpi: u32, weight: u16) -> TextStyle {
    TextStyle { family: a.font_family.clone(), size: scale(a.font_size, dpi), weight }
}

/// Liquid glass over the blurred backdrop (`with_backdrop`) or the plain fallback tint.
pub fn paint_background(p: &mut Painter, w: f32, h: f32, dpi: u32, a: &Appearance, with_backdrop: bool) {
    paint_glass(p, RectF::new(0.0, 0.0, w, h), scale(a.corner_radius, dpi), with_backdrop);
}

/// Floating bar: `margin` from the top, left and right monitor edges, `height` tall (physical px).
pub fn bar_rect(monitor: RECT, dpi: u32, a: &Appearance) -> RECT {
    let m = scale_px(a.margin, dpi);
    let h = scale_px(a.height, dpi);
    RECT { left: monitor.left + m, top: monitor.top + m, right: monitor.right - m, bottom: monitor.top + m + h }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floating_bar_rect() {
        let mon = RECT { left: 0, top: 0, right: 2560, bottom: 1440 };
        let a = Appearance::default();
        assert_eq!(bar_rect(mon, 96, &a), RECT { left: 6, top: 6, right: 2554, bottom: 42 });
        assert_eq!(bar_rect(mon, 144, &a), RECT { left: 9, top: 9, right: 2551, bottom: 63 });
        let offset = RECT { left: 100, top: 50, right: 1100, bottom: 850 };
        assert_eq!(bar_rect(offset, 96, &a), RECT { left: 106, top: 56, right: 1094, bottom: 92 });
    }
}
