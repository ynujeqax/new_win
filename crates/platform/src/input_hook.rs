//! The Input thread (spec: main §4, M2 §4.3): a low-level keyboard hook that only watches for
//! layout hotkeys and posts a signal. It decides nothing slow, never blocks, and always passes
//! the key on. Milestone 3 adds the Win key to this same hook.

use std::cell::RefCell;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;

use newwin_core::input_lang::LayoutHotkeys;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::Result;

static NOTIFY: AtomicIsize = AtomicIsize::new(0);
static MSG_ID: AtomicU32 = AtomicU32::new(0);

thread_local! {
    static HOTKEYS: RefCell<LayoutHotkeys> = RefCell::new(LayoutHotkeys::default());
}

unsafe extern "system" fn on_key(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32
        && let Some(k) = unsafe { (lp.0 as *const KBDLLHOOKSTRUCT).as_ref() }
        // Our own injected keys (Win+N) are not the user switching layouts.
        && !k.flags.contains(LLKHF_INJECTED)
    {
        let down = matches!(wp.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        let switched = HOTKEYS.with(|h| h.try_borrow_mut().is_ok_and(|mut h| h.key(k.vkCode as u16, down)));
        if switched {
            unsafe {
                let _ = PostMessageW(Some(HWND(NOTIFY.load(Ordering::Relaxed) as *mut _)), MSG_ID.load(Ordering::Relaxed), WPARAM(0), LPARAM(0));
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

pub struct InputHook {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl InputHook {
    /// Posts `msg` to `notify` when a layout hotkey may have switched the layout.
    pub fn start(notify: HWND, msg: u32) -> Result<InputHook> {
        NOTIFY.store(notify.0 as isize, Ordering::Relaxed);
        MSG_ID.store(msg, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel::<Result<u32>>();
        let thread = std::thread::Builder::new()
            .name("newwin-input".into())
            .spawn(move || {
                let hook = unsafe { GetModuleHandleW(None).and_then(|m| SetWindowsHookExW(WH_KEYBOARD_LL, Some(on_key), Some(m.into()), 0)) };
                let hook = match hook {
                    Ok(h) => h,
                    Err(e) => {
                        let _ = tx.send(Err(e));
                        return;
                    }
                };
                let _ = tx.send(Ok(unsafe { GetCurrentThreadId() }));
                let mut m = MSG::default();
                unsafe {
                    while GetMessageW(&mut m, None, 0, 0).as_bool() {
                        DispatchMessageW(&m);
                    }
                    let _ = UnhookWindowsHookEx(hook);
                }
            })
            .expect("cannot spawn the input thread");
        let thread_id = rx.recv().map_err(|_| windows::core::Error::from_thread())??;
        Ok(InputHook { thread_id, thread: Some(thread) })
    }
}

impl Drop for InputHook {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
