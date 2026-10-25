use std::rc::Rc;

use newwin_core::app_icon::IconImage;
use newwin_ui::icon_bitmaps::{IconBitmaps, icon_rect};
use newwin_ui::{Backdrop, Gfx, Layer, RectF};
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::WindowsAndMessaging::*;

fn window() -> windows::Win32::Foundation::HWND {
    newwin_platform::window::create_test_window(
        WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        WS_POPUP | WS_VISIBLE,
        RECT { left: 100, top: 100, right: 132, bottom: 132 },
        "NewWin icon test",
    )
    .unwrap()
}

fn red() -> Rc<IconImage> {
    Rc::new(IconImage { width: 16, height: 16, bgra: [0, 0, 255, 255].repeat(256) })
}

#[test]
fn an_icon_is_drawn_and_its_bitmap_reused() {
    let gfx = Gfx::new().unwrap();
    let hwnd = window();
    let mut layer = Layer::new(&gfx, hwnd, Backdrop::Tint, 32, 32).unwrap();
    let mut cache = IconBitmaps::default();
    let img = red();
    for _ in 0..2 {
        layer.draw(None, |p| p.icon(&mut cache, &img, RectF::new(8.0, 8.0, 16.0, 16.0), 1.0)).unwrap();
    }
    assert_eq!(cache.len(), 1, "one bitmap for one image");
    unsafe { DestroyWindow(hwnd).unwrap() };
}

#[test]
fn a_dropped_image_leaves_the_cache() {
    let gfx = Gfx::new().unwrap();
    let hwnd = window();
    let mut layer = Layer::new(&gfx, hwnd, Backdrop::Tint, 32, 32).unwrap();
    let mut cache = IconBitmaps::default();
    let first = red();
    layer.draw(None, |p| p.icon(&mut cache, &first, RectF::new(0.0, 0.0, 16.0, 16.0), 1.0)).unwrap();
    drop(first);
    let second = red();
    layer.draw(None, |p| p.icon(&mut cache, &second, RectF::new(0.0, 0.0, 16.0, 16.0), 0.5)).unwrap();
    assert_eq!(cache.len(), 1, "the dropped image's bitmap went");
    unsafe { DestroyWindow(hwnd).unwrap() };
}

#[test]
fn icon_rect_is_16_px_at_96_dpi_and_24_at_144() {
    assert_eq!(icon_rect(10.0, 18.0, 96), RectF::new(10.0, 10.0, 16.0, 16.0));
    assert_eq!(icon_rect(10.0, 18.0, 144), RectF::new(10.0, 6.0, 24.0, 24.0));
}
