use std::time::{Duration, Instant};

use newwin_platform::dpi::{primary_monitor, scale};
use newwin_ui::app_window::{AppContent, AppWindow, caption_hit};
use newwin_ui::panel::{PanelInput, PanelReply};
use newwin_ui::{Gfx, Painter, PointF};
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::WindowsAndMessaging::*;

struct Blank;

impl AppContent for Blank {
    fn paint(&mut self, _: &mut Painter, _: u64) {}
    fn input(&mut self, _: PanelInput, _: u64) -> PanelReply {
        PanelReply::default()
    }
    fn animating(&self, _: u64) -> bool {
        false
    }
}

fn pump(d: Duration) {
    let end = Instant::now() + d;
    let mut m = MSG::default();
    while Instant::now() < end {
        unsafe {
            while PeekMessageW(&mut m, None, 0, 0, PM_REMOVE).as_bool() {
                DispatchMessageW(&m);
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn window_rect(w: &AppWindow<Blank>) -> RECT {
    let mut r = RECT::default();
    unsafe { GetWindowRect(w.hwnd(), &mut r).unwrap() };
    r
}

#[test]
fn caption_hit_regions() {
    let s = (760.0, 560.0);
    assert_eq!(caption_hit(PointF { x: 300.0, y: 10.0 }, s, 96), HTCAPTION);
    assert_eq!(caption_hit(PointF { x: 750.0, y: 10.0 }, s, 96), HTCLOSE);
    assert_eq!(caption_hit(PointF { x: 700.0, y: 10.0 }, s, 96), HTMINBUTTON);
    assert_eq!(caption_hit(PointF { x: 300.0, y: 100.0 }, s, 96), HTCLIENT);
    assert_eq!(caption_hit(PointF { x: 1500.0, y: 20.0 }, (760.0, 560.0), 192), HTCLOSE);
}

#[test]
fn the_window_is_an_app_window_of_the_given_size() {
    let gfx = Gfx::new().unwrap();
    let w = AppWindow::create(&gfx, Blank, "NewWin.AppWindowTest", "NewWin", (760.0, 560.0)).unwrap();
    let (_, dpi) = primary_monitor();
    let r = window_rect(&w);
    assert_eq!(r.right - r.left, scale(760.0, dpi).round() as i32);
    assert_eq!(r.bottom - r.top, scale(560.0, dpi).round() as i32);
    let ex = unsafe { GetWindowLongPtrW(w.hwnd(), GWL_EXSTYLE) } as u32;
    assert_ne!(ex & WS_EX_APPWINDOW.0, 0);
    assert_eq!(ex & WS_EX_TOOLWINDOW.0, 0);
    let style = unsafe { GetWindowLongPtrW(w.hwnd(), GWL_STYLE) } as u32;
    assert_ne!(style & WS_MINIMIZEBOX.0, 0);
    let mut title = [0u16; 64];
    let n = unsafe { GetWindowTextW(w.hwnd(), &mut title) } as usize;
    assert_eq!(String::from_utf16_lossy(&title[..n]), "NewWin");
}

#[test]
fn moving_the_window_resends_its_glass_region() {
    let gfx = Gfx::new().unwrap();
    let w = AppWindow::create(&gfx, Blank, "NewWin.AppWindowTest", "NewWin", (760.0, 560.0)).unwrap();
    pump(Duration::from_millis(50));
    let id = w.hwnd().0 as usize as u64;
    let first = w.last_regions();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].id, id);
    let r = window_rect(&w);
    assert_eq!((first[0].window.left, first[0].window.top), (r.left, r.top));

    unsafe { SetWindowPos(w.hwnd(), None, r.left + 37, r.top + 21, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE).unwrap() };
    pump(Duration::from_millis(50));
    let moved = w.last_regions();
    assert_eq!(moved.len(), 1);
    let m = moved[0].window;
    assert_eq!((m.left, m.top, m.right - m.left, m.bottom - m.top), (r.left + 37, r.top + 21, r.right - r.left, r.bottom - r.top));

    // Minimized: no glass to read the screen for.
    unsafe {
        let _ = ShowWindow(w.hwnd(), SW_MINIMIZE);
    }
    pump(Duration::from_millis(50));
    assert!(w.last_regions().is_empty());
    unsafe {
        let _ = ShowWindow(w.hwnd(), SW_RESTORE);
    }
    pump(Duration::from_millis(50));
    assert_eq!(w.last_regions().len(), 1);
}
