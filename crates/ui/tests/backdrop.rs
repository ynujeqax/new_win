//! Live tests: they read the real screen, so they put a solid-color topmost window on it.
//! They share one on-screen spot, so a lock runs them one at a time.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use newwin_core::glass::PxRect;
use newwin_ui::backdrop::{Backdrops, FRAME, Frame, Region, take_frame};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, InvalidateRect, PAINTSTRUCT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows::Win32::Graphics::Dwm::{DWMWA_TRANSITIONS_FORCEDISABLED, DwmSetWindowAttribute};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, w};

const MSG_GLASS: u32 = WM_APP + 6;
const RED: u32 = 0x0000_00FF;
const GREEN: u32 = 0x0000_FF00;
static COLOR: AtomicU32 = AtomicU32::new(RED);
static SCREEN: Mutex<()> = Mutex::new(());

unsafe extern "system" fn paint_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_PAINT {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(h, &mut ps);
            let mut rc = RECT::default();
            let _ = GetClientRect(h, &mut rc);
            let b = CreateSolidBrush(COLORREF(COLOR.load(Ordering::Relaxed)));
            FillRect(dc, &rc, b);
            let _ = DeleteObject(b.into());
            let _ = EndPaint(h, &ps);
            return LRESULT(0);
        }
        DefWindowProcW(h, msg, wp, lp)
    }
}

struct Scene {
    painted: HWND,
    notify: HWND,
    _screen: MutexGuard<'static, ()>,
}

impl Scene {
    /// A 440x140 topmost window at (280, 280), covering the whole capture area of [`Scene::region`]
    /// (296..704 x 296..404) so nothing else on screen shows in a frame, and a message-only window
    /// receiving frames. DWM transitions are off: a window fading in or out would change the frames.
    fn new() -> Scene {
        let screen = SCREEN.lock().unwrap_or_else(|e| e.into_inner());
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let inst = GetModuleHandleW(None).unwrap();
            let wc = WNDCLASSW { lpfnWndProc: Some(paint_proc), hInstance: inst.into(), lpszClassName: w!("NewWinBackdropTest"), ..Default::default() };
            RegisterClassW(&wc);
            COLOR.store(RED, Ordering::Relaxed);
            let painted = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w!("NewWinBackdropTest"), w!("red"), WS_POPUP,
                280, 280, 440, 140, None, None, Some(inst.into()), None,
            )
            .unwrap();
            let off = BOOL::from(true);
            let _ = DwmSetWindowAttribute(painted, DWMWA_TRANSITIONS_FORCEDISABLED, (&raw const off).cast(), size_of::<BOOL>() as u32);
            let _ = ShowWindow(painted, SW_SHOWNA);
            let notify = CreateWindowExW(Default::default(), w!("STATIC"), w!("notify"), WINDOW_STYLE(0), 0, 0, 0, 0, Some(HWND_MESSAGE), None, Some(inst.into()), None).unwrap();
            let s = Scene { painted, notify, _screen: screen };
            s.pump(Duration::from_millis(300));
            s
        }
    }

    fn region() -> Region {
        Region { id: 7, window: PxRect { left: 320, top: 320, right: 680, bottom: 380 }, radius: 12.0 }
    }

    fn repaint(&self, color: u32) {
        COLOR.store(color, Ordering::Relaxed);
        unsafe {
            let _ = InvalidateRect(Some(self.painted), None, true);
        }
    }

    /// Pumps the thread's messages for `d`; returns the frames that arrived.
    fn pump(&self, d: Duration) -> Vec<Frame> {
        let end = Instant::now() + d;
        let mut frames = Vec::new();
        let mut msg = MSG::default();
        while Instant::now() < end {
            unsafe {
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    if msg.hwnd == self.notify && msg.message == MSG_GLASS && msg.wParam.0 == FRAME {
                        frames.push(*take_frame(msg.lParam));
                        continue;
                    }
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        frames
    }

    /// Waits up to `d` for the first frame.
    fn first_frame(&self, d: Duration) -> Option<Frame> {
        let end = Instant::now() + d;
        while Instant::now() < end {
            if let Some(f) = self.pump(Duration::from_millis(50)).into_iter().next() {
                return Some(f);
            }
        }
        None
    }
}

impl Drop for Scene {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.painted);
            let _ = DestroyWindow(self.notify);
        }
    }
}

/// BGR of the image's center pixel.
fn center(f: &Frame) -> [u8; 3] {
    let i = ((f.image.height / 2) * f.image.width + f.image.width / 2) * 4;
    [f.image.bgra[i], f.image.bgra[i + 1], f.image.bgra[i + 2]]
}

fn near(a: [u8; 3], b: [u8; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 8)
}

#[test]
fn frames_show_what_is_under_the_region() {
    let s = Scene::new();
    let b = Backdrops::spawn(s.notify, MSG_GLASS);
    b.set_regions(vec![Scene::region()]);
    let f = s.first_frame(Duration::from_secs(2)).expect("no frame within 2 s");
    assert_eq!(f.id, 7);
    assert_eq!(f.area, PxRect { left: 296, top: 296, right: 704, bottom: 404 });
    assert!(near(center(&f), [0, 0, 255]), "center {:?}", center(&f));
}

#[test]
fn an_unchanged_screen_posts_nothing_more() {
    let s = Scene::new();
    let b = Backdrops::spawn(s.notify, MSG_GLASS);
    b.set_regions(vec![Scene::region()]);
    assert!(s.first_frame(Duration::from_secs(2)).is_some());
    s.pump(Duration::from_millis(200));
    assert!(s.pump(Duration::from_secs(1)).is_empty());
}

#[test]
fn empty_regions_stop_frames() {
    let s = Scene::new();
    let b = Backdrops::spawn(s.notify, MSG_GLASS);
    b.set_regions(vec![Scene::region()]);
    assert!(s.first_frame(Duration::from_secs(2)).is_some());
    b.set_regions(vec![]);
    s.pump(Duration::from_millis(200));
    s.repaint(GREEN);
    assert!(s.pump(Duration::from_secs(1)).is_empty());
}

#[test]
fn capture_survives_access_lost() {
    let s = Scene::new();
    let b = Backdrops::spawn(s.notify, MSG_GLASS);
    b.set_regions(vec![Scene::region()]);
    assert!(s.first_frame(Duration::from_secs(2)).is_some());
    b.simulate_access_lost();
    s.repaint(GREEN);
    let frames = s.pump(Duration::from_secs(2));
    assert!(frames.iter().any(|f| near(center(f), [0, 255, 0])), "no green frame after access lost");
}

#[test]
fn refresh_rebuilds_the_capture_on_display_changes() {
    // WM_DISPLAYCHANGE: the output bounds may have changed, so the capture is rebuilt from scratch.
    let s = Scene::new();
    let b = Backdrops::spawn(s.notify, MSG_GLASS);
    b.set_regions(vec![Scene::region()]);
    assert!(s.first_frame(Duration::from_secs(2)).is_some());
    b.refresh();
    s.repaint(GREEN);
    let frames = s.pump(Duration::from_secs(2));
    assert!(frames.iter().any(|f| near(center(f), [0, 255, 0])), "no green frame after refresh");
}

/// Spotlight opens with the bar hidden (no regions): building a D3D device takes 75–130 ms,
/// longer than its show animation, so the device outlives an empty set and only the duplication
/// is rebuilt.
#[test]
fn a_reopened_region_reuses_the_device() {
    let s = Scene::new();
    let b = Backdrops::spawn(s.notify, MSG_GLASS);
    b.set_regions(vec![Scene::region()]);
    assert!(s.first_frame(Duration::from_secs(3)).is_some());
    b.set_regions(vec![]);
    s.pump(Duration::from_millis(200));
    b.set_regions(vec![Scene::region()]);
    let reopened = Instant::now();
    assert!(s.first_frame(Duration::from_secs(3)).is_some(), "a frame after reopening");
    eprintln!("reopened frame after {} ms", reopened.elapsed().as_millis());
    assert_eq!(b.devices_built(), 1);
}

#[test]
fn warm_builds_the_device_ahead() {
    let s = Scene::new();
    let b = Backdrops::spawn(s.notify, MSG_GLASS);
    assert_eq!(b.devices_built(), 0, "nothing is built while no glass is wanted");
    b.warm();
    s.pump(Duration::from_millis(400));
    assert_eq!(b.devices_built(), 1, "built on warm, before any region");
    b.set_regions(vec![Scene::region()]);
    assert!(s.first_frame(Duration::from_secs(3)).is_some());
    assert_eq!(b.devices_built(), 1, "the region reuses it");
}

#[test]
fn an_idle_device_is_released() {
    let s = Scene::new();
    let b = Backdrops::spawn_with_keep(s.notify, MSG_GLASS, Duration::from_millis(300));
    b.set_regions(vec![Scene::region()]);
    assert!(s.first_frame(Duration::from_secs(3)).is_some());
    b.set_regions(vec![]);
    s.pump(Duration::from_millis(800));
    b.set_regions(vec![Scene::region()]);
    assert!(s.first_frame(Duration::from_secs(3)).is_some());
    assert_eq!(b.devices_built(), 2, "released after the keep time, built again");
}
