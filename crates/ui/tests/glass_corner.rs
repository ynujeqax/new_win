//! The glass's rounded corners must be smooth: the clip that shapes the blurred backdrop is
//! antialiased, so pixels on the curve blend instead of stepping.

use newwin_core::glass::{Image, PxRect};
use newwin_ui::backdrop::Frame;
use newwin_ui::{Backdrop, Gfx, Layer};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::Graphics::Gdi::{GetDC, GetPixel, ReleaseDC};
use windows::Win32::UI::WindowsAndMessaging::*;

fn pump() {
    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        for _ in 0..3 {
            let _ = DwmFlush();
        }
    }
}

fn grey(x: i32, y: i32) -> u8 {
    unsafe {
        let dc = GetDC(None);
        let c = GetPixel(dc, x, y).0;
        ReleaseDC(None, dc);
        (((c & 0xff) + ((c >> 8) & 0xff) + ((c >> 16) & 0xff)) / 3) as u8
    }
}

#[test]
fn rounded_glass_corners_are_antialiased() {
    let gfx = Gfx::new().unwrap();
    let (x0, y0, w, h) = (400, 400, 200, 80);
    // Black behind, white glass: any grey on the curve is antialiasing.
    let back = newwin_platform::window::create_test_window(WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOREDIRECTIONBITMAP, WS_POPUP | WS_VISIBLE, RECT { left: 350, top: 350, right: 650, bottom: 530 }, "back").unwrap();
    let mut back_layer = Layer::new(&gfx, back, Backdrop::Tint, 300, 180).unwrap();
    back_layer.draw(None, |p| p.fill_round_rect(newwin_ui::RectF::new(0.0, 0.0, 300.0, 180.0), 0.0, newwin_ui::Color::rgba(0, 0, 0, 1.0))).unwrap();
    let hwnd = newwin_platform::window::create_test_window(WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOREDIRECTIONBITMAP, WS_POPUP | WS_VISIBLE, RECT { left: x0, top: y0, right: x0 + w, bottom: y0 + h }, "glass").unwrap();
    let mut layer = Layer::new(&gfx, hwnd, Backdrop::Glass, w as u32, h as u32).unwrap();
    layer.set_capturable(true);
    layer.set_corner_radius(24.0).unwrap();
    let (fw, fh) = (w as usize / 4 + 2, h as usize / 4 + 2);
    let frame = Frame { id: 1, area: PxRect { left: x0 - 4, top: y0 - 4, right: x0 - 4 + 4 * fw as i32, bottom: y0 - 4 + 4 * fh as i32 }, image: Image { width: fw, height: fh, bgra: vec![255; fw * fh * 4] } };
    layer.set_backdrop(&frame, (x0, y0)).unwrap();
    layer.draw(None, |p| p.clear()).unwrap();
    gfx.commit().unwrap();
    pump();
    let mut greys = 0;
    let mut rows = String::new();
    for y in 0..14 {
        for x in 0..14 {
            let g = grey(x0 + x, y0 + y);
            if g > 30 && g < 225 {
                greys += 1;
            }
            rows.push(char::from(b'0' + (g / 26).min(9)));
        }
        rows.push('\n');
    }
    unsafe {
        let _ = DestroyWindow(hwnd);
        let _ = DestroyWindow(back);
    }
    assert!(greys >= 6, "the curve blends ({greys} grey pixels):\n{rows}");
}
