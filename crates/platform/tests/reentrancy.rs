use std::cell::RefCell;
use std::rc::Rc;

use newwin_platform::window::{Handler, WindowSpec, create_window};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;

const FIRST: u32 = WM_APP + 1;
const SECOND: u32 = WM_APP + 2;

fn pump(ms: u64) {
    let until = std::time::Instant::now() + std::time::Duration::from_millis(ms);
    let mut msg = MSG::default();
    while std::time::Instant::now() < until {
        unsafe {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                DispatchMessageW(&msg);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

struct Recorder {
    seen: Rc<RefCell<Vec<u32>>>,
}

impl Handler for Recorder {
    fn on_message(&mut self, hwnd: HWND, msg: u32, _: WPARAM, _: LPARAM) -> Option<LRESULT> {
        match msg {
            FIRST => {
                // Like ShellExecute or a COM call: something posts to us and a nested loop
                // dispatches it while this handler is still running.
                unsafe { PostMessageW(Some(hwnd), SECOND, WPARAM(0), LPARAM(0)).unwrap() };
                pump(50);
                self.seen.borrow_mut().push(FIRST);
                Some(LRESULT(0))
            }
            SECOND => {
                self.seen.borrow_mut().push(SECOND);
                Some(LRESULT(0))
            }
            _ => None,
        }
    }
}

#[test]
fn messages_posted_during_a_nested_loop_are_not_lost() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let hwnd = create_window(
        &WindowSpec { class: "NewWin.ReentrancyTest", ex_style: WS_EX_TOOLWINDOW, style: WS_POPUP, rect: RECT::default(), layered_alpha: None },
        Rc::new(RefCell::new(Recorder { seen: seen.clone() })),
    )
    .unwrap();
    unsafe { PostMessageW(Some(hwnd), FIRST, WPARAM(0), LPARAM(0)).unwrap() };
    pump(200);
    assert_eq!(*seen.borrow(), vec![FIRST, SECOND]);
    unsafe { DestroyWindow(hwnd).unwrap() };
}

struct DpiRecorder {
    lparams: Rc<RefCell<Vec<isize>>>,
}

impl Handler for DpiRecorder {
    fn on_message(&mut self, hwnd: HWND, msg: u32, _: WPARAM, lp: LPARAM) -> Option<LRESULT> {
        match msg {
            FIRST => {
                // Sent while this handler runs: queued, and its RECT pointer dies with this frame.
                let suggested = RECT { left: 1, top: 2, right: 3, bottom: 4 };
                unsafe { SendMessageW(hwnd, WM_DPICHANGED, Some(WPARAM(0x0090_0090)), Some(LPARAM(&suggested as *const RECT as isize))) };
                Some(LRESULT(0))
            }
            WM_DPICHANGED => {
                self.lparams.borrow_mut().push(lp.0);
                Some(LRESULT(0))
            }
            _ => None,
        }
    }
}

#[test]
fn a_queued_dpi_change_carries_no_dangling_rect() {
    let lparams = Rc::new(RefCell::new(Vec::new()));
    let hwnd = create_window(
        &WindowSpec { class: "NewWin.ReentrancyDpiTest", ex_style: WS_EX_TOOLWINDOW, style: WS_POPUP, rect: RECT::default(), layered_alpha: None },
        Rc::new(RefCell::new(DpiRecorder { lparams: lparams.clone() })),
    )
    .unwrap();
    unsafe { PostMessageW(Some(hwnd), FIRST, WPARAM(0), LPARAM(0)).unwrap() };
    pump(100);
    assert_eq!(*lparams.borrow(), vec![0]);
    unsafe { DestroyWindow(hwnd).unwrap() };
}
