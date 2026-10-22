use newwin_core::glass::{Image, PxRect};
use newwin_ui::backdrop::Frame;
use newwin_ui::{Backdrop, Gfx, Layer, glass_tint};
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::WindowsAndMessaging::*;

fn glass_window() -> windows::Win32::Foundation::HWND {
    newwin_platform::window::create_test_window(
        WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        WS_POPUP | WS_VISIBLE,
        RECT { left: 100, top: 100, right: 300, bottom: 130 },
        "NewWin glass layer test",
    )
    .unwrap()
}

fn frame(w: usize, h: usize) -> Frame {
    Frame { id: 1, area: PxRect { left: 76, top: 76, right: 76 + 4 * w as i32, bottom: 76 + 4 * h as i32 }, image: Image { width: w, height: h, bgra: vec![40; w * h * 4] } }
}

#[test]
fn glass_layer_is_excluded_from_capture() {
    let gfx = Gfx::new().unwrap();
    let hwnd = glass_window();
    let _layer = Layer::new(&gfx, hwnd, Backdrop::Glass, 200, 30).unwrap();
    let mut affinity = 0u32;
    unsafe { GetWindowDisplayAffinity(hwnd, &mut affinity).unwrap() };
    assert_eq!(affinity, WDA_EXCLUDEFROMCAPTURE.0);
    unsafe { DestroyWindow(hwnd).unwrap() };
}

#[test]
fn backdrop_round_trip() {
    let gfx = Gfx::new().unwrap();
    let hwnd = glass_window();
    let mut layer = Layer::new(&gfx, hwnd, Backdrop::Glass, 200, 30).unwrap();
    assert!(!layer.has_backdrop());
    layer.set_backdrop(&frame(62, 19), (100, 100)).unwrap();
    assert!(layer.has_backdrop());
    assert!(layer.backdrop_visual().unwrap().is_some());
    layer.clear_backdrop().unwrap();
    assert!(!layer.has_backdrop());
    layer.set_backdrop(&frame(30, 10), (100, 100)).unwrap();
    assert!(layer.has_backdrop());
    unsafe { DestroyWindow(hwnd).unwrap() };
}

#[test]
fn tint_only_layers_have_no_backdrop_visual() {
    let gfx = Gfx::new().unwrap();
    let hwnd = glass_window();
    let mut layer = Layer::new(&gfx, hwnd, Backdrop::Tint, 200, 30).unwrap();
    assert!(layer.backdrop_visual().unwrap().is_none());
    unsafe { DestroyWindow(hwnd).unwrap() };
}

#[test]
fn glass_tint_depends_on_backdrop() {
    assert_eq!(glass_tint(true).a, 0.55);
    assert_eq!(glass_tint(false).a, 0.88);
}
