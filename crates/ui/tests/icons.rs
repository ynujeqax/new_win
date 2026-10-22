use newwin_ui::{Backdrop, Gfx, Layer, PointF, icons, theme};
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::WindowsAndMessaging::*;

/// Every icon, at rest and mid-animation, draws through Direct2D without an error.
#[test]
fn icons_draw() {
    let gfx = Gfx::new().unwrap();
    let hwnd = newwin_platform::window::create_test_window(
        WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP,
        WS_POPUP,
        RECT { left: 0, top: 0, right: 200, bottom: 40 },
        "icons",
    )
    .unwrap();
    let mut layer = Layer::new(&gfx, hwnd, Backdrop::Tint, 200, 40).unwrap();
    let (s, w, col) = (26.0, theme::ICON_STROKE, theme::TEXT);
    layer
        .draw(None, |p| {
            let c = |x: f32| PointF { x, y: 20.0 };
            icons::chevron(p, c(13.0), s, 0.5, col, w);
            icons::ethernet(p, c(39.0), s, col, w);
            icons::wifi(p, c(65.0), s, 1.5, col, w);
            icons::speaker(p, c(91.0), s, 2.3, 0.4, col, w);
            icons::slash(p, c(117.0), s, 1.0, col.with_alpha(0.5), w);
        })
        .unwrap();
}
