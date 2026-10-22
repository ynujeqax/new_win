//! Temporary M3 probe — remove after M3.
//!
//! Probes 1 and 2 of the M3 plan in one run (180 s): a lone Win tap is swallowed and replayed as
//! `VK_NONAME` down/up + Win up (tagged), so Start must not open; `Ctrl+Esc` is swallowed. Each
//! tap shows a small topmost window that takes the foreground and logs the first typed char.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::Instant;

use newwin_platform::window::{Handler, WindowSpec, create_window};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const TAG: usize = 0x4E57_4E57;
const SHOW: u32 = WM_APP + 1;
static POPUP: AtomicIsize = AtomicIsize::new(0);

#[derive(Default)]
struct Tap {
    win: bool,
    spoiled: bool,
    ctrl: bool,
    eat_esc_up: bool,
}

thread_local! {
    static TAP: RefCell<Tap> = RefCell::new(Tap::default());
    static START: Instant = Instant::now();
}

fn t() -> u128 {
    START.with(|s| s.elapsed().as_millis())
}

fn send(keys: &[(u16, bool)]) {
    let inputs: Vec<INPUT> = keys
        .iter()
        .map(|&(vk, up)| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk), dwFlags: if up { KEYEVENTF_KEYUP } else { Default::default() }, dwExtraInfo: TAG, ..Default::default() },
            },
        })
        .collect();
    let n = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    println!("{:>6} ms   SendInput {keys:?} -> {n}", t());
}

fn post_show(what: &str) {
    unsafe {
        let _ = PostMessageW(Some(HWND(POPUP.load(Ordering::Relaxed) as *mut _)), SHOW, WPARAM(0), LPARAM(0));
    }
    println!("{:>6} ms   {what}: show popup", t());
}

unsafe extern "system" fn on_key(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32
        && let Some(k) = unsafe { (lp.0 as *const KBDLLHOOKSTRUCT).as_ref() }
        && k.dwExtraInfo != TAG
    {
        let down = matches!(wp.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        let vk = k.vkCode as u16;
        let is_win = vk == VK_LWIN.0 || vk == VK_RWIN.0;
        let is_ctrl = matches!(vk, 0x11 | 0xA2 | 0xA3);
        let eat = TAP.with(|tap| {
            let mut s = tap.borrow_mut();
            if is_ctrl {
                s.ctrl = down;
            }
            if is_win && down {
                if !s.win {
                    s.spoiled = false;
                }
                s.win = true;
                return false;
            }
            if is_win && !down {
                let tap_now = s.win && !s.spoiled;
                s.win = false;
                if tap_now {
                    println!("{:>6} ms   lone Win up: swallowed", t());
                    send(&[(0xFC, false), (0xFC, true), (vk, true)]);
                    post_show("Win tap");
                    return true;
                }
                println!("{:>6} ms   Win up after a combination: passed", t());
                return false;
            }
            if vk == VK_ESCAPE.0 && down && s.ctrl && !s.win {
                s.eat_esc_up = true;
                println!("{:>6} ms   Ctrl+Esc: swallowed", t());
                send(&[(0xFC, false), (0xFC, true)]);
                post_show("Ctrl+Esc");
                return true;
            }
            if vk == VK_ESCAPE.0 && !down && s.eat_esc_up {
                s.eat_esc_up = false;
                return true;
            }
            if down && s.win {
                s.spoiled = true;
            }
            false
        });
        if eat {
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

struct Popup;

fn take_foreground(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(Some(hwnd));
        if GetForegroundWindow() == hwnd {
            println!("{:>6} ms   foreground: yes (plain SetForegroundWindow)", t());
            return;
        }
        let fg = GetForegroundWindow();
        let other = GetWindowThreadProcessId(fg, None);
        let me = GetCurrentThreadId();
        let _ = AttachThreadInput(other, me, true);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(Some(hwnd));
        let _ = AttachThreadInput(other, me, false);
        println!("{:>6} ms   foreground after AttachThreadInput: {}", t(), GetForegroundWindow() == hwnd);
    }
}

impl Handler for Popup {
    fn on_message(&mut self, hwnd: HWND, msg: u32, wp: WPARAM, _lp: LPARAM) -> Option<LRESULT> {
        match msg {
            SHOW => take_foreground(hwnd),
            WM_CHAR => {
                println!("{:>6} ms   popup got char {:?}", t(), char::from_u32(wp.0 as u32));
                unsafe {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
            }
            WM_TIMER => unsafe { PostQuitMessage(0) },
            _ => return None,
        }
        Some(LRESULT(0))
    }
}

fn main() -> windows::core::Result<()> {
    let rect = RECT { left: 1100, top: 300, right: 1460, bottom: 380 };
    let hwnd = create_window(
        &WindowSpec { class: "NewWin.Probe", ex_style: WS_EX_TOPMOST | WS_EX_TOOLWINDOW, style: WS_POPUP | WS_BORDER, rect, layered_alpha: None },
        Rc::new(RefCell::new(Popup)),
    )?;
    POPUP.store(hwnd.0 as isize, Ordering::Relaxed);
    let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(on_key), Some(GetModuleHandleW(None)?.into()), 0)? };
    unsafe { SetTimer(Some(hwnd), 1, 180_000, None) };
    println!("probe running for 180 s");
    let mut m = MSG::default();
    unsafe {
        while GetMessageW(&mut m, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&m);
            DispatchMessageW(&m);
        }
        let _ = UnhookWindowsHookEx(hook);
    }
    println!("probe done");
    Ok(())
}
