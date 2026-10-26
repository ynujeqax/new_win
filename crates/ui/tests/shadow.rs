use newwin_ui::Gfx;
use newwin_ui::shadow::{Shadow, shadow_rect};
use windows::Win32::Foundation::{POINT, RECT};
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
        let _ = DwmFlush();
        let _ = DwmFlush();
    }
}

fn brightness(p: POINT) -> u32 {
    unsafe {
        let dc = GetDC(None);
        let c = GetPixel(dc, p.x, p.y).0;
        ReleaseDC(None, dc);
        (c & 0xff) + ((c >> 8) & 0xff) + ((c >> 16) & 0xff)
    }
}

#[test]
fn the_shadow_window_surrounds_its_owner() {
    let r = RECT { left: 100, top: 100, right: 300, bottom: 200 };
    assert_eq!(shadow_rect(r, 34), RECT { left: 66, top: 66, right: 334, bottom: 234 });
}

#[test]
fn a_shadow_darkens_below_its_owner_and_lets_clicks_through() {
    let gfx = Gfx::new().unwrap();
    let rect = RECT { left: 300, top: 300, right: 600, bottom: 450 };
    let owner = newwin_platform::window::create_test_window(WS_EX_TOOLWINDOW | WS_EX_TOPMOST, WS_POPUP | WS_VISIBLE, rect, "owner").unwrap();
    // A bright window under everything, so the shadow has something to darken.
    let under = RECT { left: 200, top: 200, right: 700, bottom: 600 };
    let back = newwin_platform::window::create_test_window(WS_EX_TOOLWINDOW | WS_EX_TOPMOST, WS_POPUP | WS_VISIBLE, under, "back").unwrap();
    unsafe {
        let _ = SetWindowPos(owner, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
    }
    pump();
    let below = POINT { x: 450, y: 462 };
    let before = brightness(below);
    let mut shadow = Shadow::new(&gfx, owner, rect, 12.0, 96).unwrap();
    shadow.set_capturable(true);
    shadow.set_offset_opacity(0.0, 1.0);
    pump();
    let after = brightness(below);
    assert!(after < before, "darker under the owner's bottom edge: {before} -> {after}");
    let hit = unsafe { WindowFromPoint(below) };
    assert_ne!(hit, shadow.hwnd(), "the shadow never takes the pointer");
    assert_eq!(hit, back);
    drop(shadow);
    unsafe {
        let _ = DestroyWindow(owner);
        let _ = DestroyWindow(back);
    }
}
