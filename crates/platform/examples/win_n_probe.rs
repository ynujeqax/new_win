//! Throwaway probe for M2 risk §13.2; not used by NewWin.
//!
//! Sends Win+N twice — with the native taskbar shown, then hidden — and logs which window is in
//! the foreground 1.5 s later, then closes it with Esc and restores the taskbar.

use std::time::Duration;

use newwin_platform::taskbar::NativeTaskbar;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetForegroundWindow, GetWindowTextW};

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, dwFlags: if up { KEYEVENTF_KEYUP } else { Default::default() }, ..Default::default() } },
    }
}

fn send(keys: &[INPUT]) {
    unsafe { SendInput(keys, size_of::<INPUT>() as i32) };
}

fn foreground() -> String {
    unsafe {
        let h = GetForegroundWindow();
        let mut c = [0u16; 128];
        let mut t = [0u16; 128];
        let nc = GetClassNameW(h, &mut c);
        let nt = GetWindowTextW(h, &mut t);
        format!("class={:?} title={:?}", String::from_utf16_lossy(&c[..nc.max(0) as usize]), String::from_utf16_lossy(&t[..nt.max(0) as usize]))
    }
}

fn try_win_n(label: &str) {
    println!("{label}: before {}", foreground());
    send(&[key(VK_LWIN, false), key(VK_N, false), key(VK_N, true), key(VK_LWIN, true)]);
    std::thread::sleep(Duration::from_millis(1500));
    println!("{label}: after Win+N {}", foreground());
    send(&[key(VK_ESCAPE, false), key(VK_ESCAPE, true)]);
    std::thread::sleep(Duration::from_millis(800));
}

fn main() -> windows::core::Result<()> {
    try_win_n("taskbar shown");
    let state = std::env::temp_dir().join("newwin-win-n-probe.state");
    let tb = NativeTaskbar::hide(&state)?;
    std::thread::sleep(Duration::from_millis(1000));
    try_win_n("taskbar hidden");
    tb.restore()?;
    println!("taskbar restored");
    Ok(())
}
