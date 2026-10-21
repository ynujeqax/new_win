//! Win32 windows driven by a Rust handler.
//!
//! The handler is shared (`Rc<RefCell<…>>`) so one app object can own several windows.
//!
//! A message can arrive while the handler is still running: our own `SetWindowPos` sends
//! `WM_WINDOWPOSCHANGING`, and calls such as `ShellExecute` or COM run a nested message loop that
//! dispatches whatever was posted meanwhile. Sent messages that need an answer go to
//! `DefWindowProcW`; posted messages and the notifications we act on are queued and delivered as
//! soon as the handler is free, so nothing is lost.

use std::cell::RefCell;
use std::collections::{HashSet, VecDeque};
use std::rc::Rc;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{HSTRING, Result};

pub trait Handler {
    fn on_message(&mut self, hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT>;
}

pub type SharedHandler = Rc<RefCell<dyn Handler>>;

pub struct WindowSpec {
    pub class: &'static str,
    pub ex_style: WINDOW_EX_STYLE,
    pub style: WINDOW_STYLE,
    pub rect: RECT,
    /// For `WS_EX_LAYERED` windows: constant alpha set right after creation.
    pub layered_alpha: Option<u8>,
}

type Deferred = (HWND, u32, WPARAM, LPARAM);

thread_local! {
    static REGISTERED: RefCell<HashSet<&'static str>> = RefCell::new(HashSet::new());
    static DEFERRED: RefCell<VecDeque<Deferred>> = const { RefCell::new(VecDeque::new()) };
}

/// Broadcast notifications we act on; they arrive sent, but their result does not matter.
const DEFERRABLE_SENT: [u32; 6] = [WM_DISPLAYCHANGE, WM_DPICHANGED, WM_SETTINGCHANGE, WM_TIMECHANGE, WM_POWERBROADCAST, WM_ENDSESSION];

fn can_defer(msg: u32) -> bool {
    if msg == WM_PAINT {
        return false;
    }
    let sent = unsafe { InSendMessage() }.as_bool();
    !sent || DEFERRABLE_SENT.contains(&msg)
}

fn handler_of(hwnd: HWND) -> Option<&'static SharedHandler> {
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const SharedHandler;
    // Valid until WM_NCDESTROY clears the slot; only used for the duration of one call.
    unsafe { ptr.as_ref() }
}

/// Delivers queued messages whose handler is free again, in arrival order.
fn drain_deferred() {
    loop {
        let Some((hwnd, msg, wp, lp)) = DEFERRED.with(|q| q.borrow_mut().pop_front()) else { return };
        if !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
            continue;
        }
        let Some(handler) = handler_of(hwnd) else { continue };
        let Ok(mut h) = handler.try_borrow_mut() else {
            // Still busy further up the stack: retry when that call returns.
            DEFERRED.with(|q| q.borrow_mut().push_front((hwnd, msg, wp, lp)));
            return;
        };
        h.on_message(hwnd, msg, wp, lp);
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_NCCREATE {
            let cs = &*(lp.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
        }
        if msg == WM_NCDESTROY {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut SharedHandler;
            if !ptr.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(ptr));
            }
            return DefWindowProcW(hwnd, msg, wp, lp);
        }
        if let Some(handler) = handler_of(hwnd) {
            match handler.try_borrow_mut() {
                Ok(mut h) => {
                    let r = h.on_message(hwnd, msg, wp, lp);
                    drop(h);
                    drain_deferred();
                    if let Some(r) = r {
                        return r;
                    }
                }
                Err(_) if can_defer(msg) => {
                    DEFERRED.with(|q| q.borrow_mut().push_back((hwnd, msg, wp, lp)));
                    return LRESULT(0);
                }
                Err(_) => {}
            }
        }
        DefWindowProcW(hwnd, msg, wp, lp)
    }
}

fn register(class: &'static str) -> Result<()> {
    if REGISTERED.with(|r| r.borrow().contains(class)) {
        return Ok(());
    }
    unsafe {
        let name = HSTRING::from(class);
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: GetModuleHandleW(None)?.into(),
            lpszClassName: windows::core::PCWSTR(name.as_ptr()),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0 {
            return Err(windows::core::Error::from_thread());
        }
    }
    REGISTERED.with(|r| r.borrow_mut().insert(class));
    Ok(())
}

pub fn create_window(spec: &WindowSpec, handler: SharedHandler) -> Result<HWND> {
    register(spec.class)?;
    let boxed = Box::into_raw(Box::new(handler));
    let r = spec.rect;
    let hwnd = unsafe {
        CreateWindowExW(
            spec.ex_style,
            &HSTRING::from(spec.class),
            &HSTRING::from(spec.class),
            spec.style,
            r.left,
            r.top,
            r.right - r.left,
            r.bottom - r.top,
            None,
            None,
            Some(GetModuleHandleW(None)?.into()),
            Some(boxed as *const _),
        )
    };
    let hwnd = match hwnd {
        Ok(h) => h,
        Err(e) => {
            // WM_NCCREATE never stored the pointer, so we still own it.
            drop(unsafe { Box::from_raw(boxed) });
            return Err(e);
        }
    };
    if let Some(alpha) = spec.layered_alpha {
        unsafe { SetLayeredWindowAttributes(hwnd, Default::default(), alpha, LWA_ALPHA)? };
    }
    Ok(hwnd)
}

struct NoopHandler;

impl Handler for NoopHandler {
    fn on_message(&mut self, _: HWND, _: u32, _: WPARAM, _: LPARAM) -> Option<LRESULT> {
        None
    }
}

/// A plain window with default behavior; for tests and tools.
pub fn create_test_window(ex_style: WINDOW_EX_STYLE, style: WINDOW_STYLE, rect: RECT, title: &str) -> Result<HWND> {
    let hwnd = create_window(&WindowSpec { class: "NewWin.Test", ex_style, style, rect, layered_alpha: None }, Rc::new(RefCell::new(NoopHandler)))?;
    unsafe { SetWindowTextW(hwnd, &HSTRING::from(title))? };
    Ok(hwnd)
}

/// Runs until `WM_QUIT`; returns its exit code.
pub fn run_message_loop() -> i32 {
    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    msg.wParam.0 as i32
}
