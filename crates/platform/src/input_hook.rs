//! The Input thread (spec: main §4, M2 §4.3, M3 §4): a low-level keyboard hook that watches for
//! layout hotkeys, a lone Win tap and `Ctrl+Esc`, and posts a signal. It decides nothing slow and
//! never blocks; it swallows only a lone Win up (replayed masked) and the Esc of `Ctrl+Esc`.

use std::cell::RefCell;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;

use newwin_core::input_lang::{LayoutHotkeys, TapAction, WinKeyTap};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::Result;

/// `dwExtraInfo` of every key NewWin injects; the hook passes exactly those through untouched.
pub const NEWWIN_INPUT_TAG: usize = 0x4E57_4E57;
/// A key with no meaning: between Win down and up it makes Windows see a combination.
const VK_NONAME: u16 = 0xFC;

static NOTIFY: AtomicIsize = AtomicIsize::new(0);
static MSG_ID: AtomicU32 = AtomicU32::new(0);
static SPOTLIGHT_MSG: AtomicU32 = AtomicU32::new(0);

thread_local! {
    static HOTKEYS: RefCell<LayoutHotkeys> = RefCell::new(LayoutHotkeys::default());
    static TAP: RefCell<WinKeyTap> = RefCell::new(WinKeyTap::new(false));
}

/// Injects `(vk, key up)` events tagged as NewWin's own.
pub fn send_tagged(keys: &[(u16, bool)]) {
    let inputs: Vec<INPUT> = keys
        .iter()
        .map(|&(vk, up)| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk), dwFlags: if up { KEYEVENTF_KEYUP } else { Default::default() }, dwExtraInfo: NEWWIN_INPUT_TAG, ..Default::default() },
            },
        })
        .collect();
    unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
}

fn post(msg: u32) {
    unsafe {
        let _ = PostMessageW(Some(HWND(NOTIFY.load(Ordering::Relaxed) as *mut _)), msg, WPARAM(0), LPARAM(0));
    }
}

unsafe extern "system" fn on_key(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32
        && let Some(k) = unsafe { (lp.0 as *const KBDLLHOOKSTRUCT).as_ref() }
        // Our own injected keys (Win+N, the Win mask) are neither hotkeys nor taps.
        && k.dwExtraInfo != NEWWIN_INPUT_TAG
    {
        let down = matches!(wp.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        let vk = k.vkCode as u16;
        let switched = HOTKEYS.with(|h| h.try_borrow_mut().is_ok_and(|mut h| h.key(vk, down)));
        if switched {
            post(MSG_ID.load(Ordering::Relaxed));
        }
        let tap = TAP.with(|t| t.try_borrow_mut().map_or(TapAction::Pass, |mut t| t.key(vk, down)));
        match tap {
            TapAction::Pass => {}
            TapAction::WinTap => {
                // Injected from inside the hook, these queue after the current event: so the
                // real Win up is swallowed and replayed after the mask.
                send_tagged(&[(VK_NONAME, false), (VK_NONAME, true), (vk, true)]);
                post(SPOTLIGHT_MSG.load(Ordering::Relaxed));
                return LRESULT(1);
            }
            TapAction::CtrlEsc => {
                // Our own input makes us the last input's source: Spotlight may take the foreground.
                send_tagged(&[(VK_NONAME, false), (VK_NONAME, true)]);
                post(SPOTLIGHT_MSG.load(Ordering::Relaxed));
                return LRESULT(1);
            }
            TapAction::Swallow => return LRESULT(1),
        }
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

pub struct InputHook {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl InputHook {
    /// Posts `layout_msg` to `notify` when a layout hotkey may have switched the layout, and
    /// `spotlight_msg` on a lone Win tap or `Ctrl+Esc` (only when `win_key`).
    pub fn start(notify: HWND, layout_msg: u32, spotlight_msg: u32, win_key: bool) -> Result<InputHook> {
        NOTIFY.store(notify.0 as isize, Ordering::Relaxed);
        MSG_ID.store(layout_msg, Ordering::Relaxed);
        SPOTLIGHT_MSG.store(spotlight_msg, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel::<Result<u32>>();
        let thread = std::thread::Builder::new()
            .name("newwin-input".into())
            .spawn(move || {
                TAP.with(|t| *t.borrow_mut() = WinKeyTap::new(win_key));
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
