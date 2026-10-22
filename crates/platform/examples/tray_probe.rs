//! Throwaway probe for M2 risk §13.1; not used by NewWin.
//!
//! Creates a hidden top-level `Shell_TrayWnd`, checks that `FindWindow` finds it before explorer's,
//! broadcasts `TaskbarCreated` and for `--secs` (default 60) logs every `WM_COPYDATA`. Every
//! message is also forwarded to explorer's own tray and explorer's reply is logged, so explorer
//! keeps its icons. On exit the window is destroyed and `TaskbarCreated` broadcast again, so apps
//! go back to explorer.

use std::time::Instant;

use newwin_core::tray::parse_notify;
use newwin_platform::appinfo::process_image_path;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, PCWSTR, w};

static mut OURS: HWND = HWND(std::ptr::null_mut());
static mut EXPLORER: HWND = HWND(std::ptr::null_mut());
static mut START: Option<Instant> = None;

fn t() -> f32 {
    unsafe { START.map_or(0.0, |s| s.elapsed().as_secs_f32()) }
}

fn class_of(h: HWND) -> String {
    let mut buf = [0u16; 64];
    let n = unsafe { GetClassNameW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn pid_of(h: HWND) -> u32 {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
    pid
}

fn exe_of(pid: u32) -> String {
    process_image_path(pid).and_then(|p| p.rsplit('\\').next().map(str::to_string)).unwrap_or_else(|| format!("pid {pid}"))
}

fn explorer_tray() -> Option<HWND> {
    unsafe extern "system" fn each(h: HWND, lp: LPARAM) -> BOOL {
        let out = unsafe { &mut *(lp.0 as *mut Option<HWND>) };
        if class_of(h) == "Shell_TrayWnd" && pid_of(h) != unsafe { GetCurrentProcessId() } {
            *out = Some(h);
            return false.into();
        }
        true.into()
    }
    let mut found = None;
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut found as *mut _ as isize));
    }
    found
}

fn forward(data: usize, bytes: &[u8]) -> Option<usize> {
    let target = unsafe { EXPLORER };
    if target.is_invalid() {
        return None;
    }
    let cds = COPYDATASTRUCT { dwData: data, cbData: bytes.len() as u32, lpData: bytes.as_ptr() as *mut _ };
    let mut result = 0usize;
    let ok = unsafe {
        SendMessageTimeoutW(target, WM_COPYDATA, WPARAM(OURS.0 as usize), LPARAM(&cds as *const _ as isize), SMTO_ABORTIFHUNG, 1000, Some(&mut result))
    };
    (ok.0 != 0).then_some(result)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ")
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_COPYDATA => {
                let cds = &*(lp.0 as *const COPYDATASTRUCT);
                let bytes: &[u8] = if cds.lpData.is_null() { &[] } else { std::slice::from_raw_parts(cds.lpData as *const u8, cds.cbData as usize) };
                let sender = HWND(wp.0 as *mut _);
                let reply = forward(cds.dwData, bytes);
                match cds.dwData {
                    1 => match parse_notify(bytes) {
                        Ok(d) => {
                            let cb = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
                            println!(
                                "{:7.2}s NOTIFY {:?} cb={cb} len={} key={:?} tip={:?} cbmsg={:#x} ver={} exe={} explorer_reply={reply:?}",
                                t(),
                                d.command,
                                bytes.len(),
                                d.key,
                                d.tip,
                                d.callback_message,
                                d.version,
                                exe_of(pid_of(HWND(d.hwnd as usize as *mut _)))
                            );
                        }
                        Err(e) => println!("{:7.2}s NOTIFY parse error {e:?} len={}", t(), bytes.len()),
                    },
                    3 => println!("{:7.2}s GETRECT len={} raw={} explorer_reply={reply:?}", t(), bytes.len(), hex(bytes)),
                    d => println!(
                        "{:7.2}s COPYDATA dwData={d} len={} sender={} explorer_reply={reply:?}",
                        t(),
                        bytes.len(),
                        exe_of(pid_of(sender))
                    ),
                }
                LRESULT(reply.unwrap_or(0) as isize)
            }
            WM_TIMER => {
                let first = FindWindowW(w!("Shell_TrayWnd"), None).unwrap_or_default();
                println!("{:7.2}s end: FindWindow still ours: {}", t(), first == hwnd);
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

fn main() -> windows::core::Result<()> {
    let secs: u32 = std::env::args().skip_while(|a| a != "--secs").nth(1).and_then(|s| s.parse().ok()).unwrap_or(60);
    unsafe {
        START = Some(Instant::now());
        let explorer = explorer_tray().unwrap_or_default();
        EXPLORER = explorer;
        println!("explorer tray {explorer:?} (pid {})", pid_of(explorer));
        let hinst = GetModuleHandleW(None)?;
        let wc = WNDCLASSW { lpfnWndProc: Some(wndproc), hInstance: hinst.into(), lpszClassName: w!("Shell_TrayWnd"), ..Default::default() };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            w!("Shell_TrayWnd"),
            PCWSTR::null(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinst.into()),
            None,
        )?;
        OURS = hwnd;
        let _ = CreateWindowExW(Default::default(), w!("Shell_TrayWnd"), w!("TrayNotifyWnd"), WS_CHILD, 0, 0, 0, 0, Some(hwnd), None, Some(hinst.into()), None);
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        let first = FindWindowW(w!("Shell_TrayWnd"), None).unwrap_or_default();
        println!("ours {hwnd:?}; FindWindow -> {first:?} (ours first: {})", first == hwnd);
        let created = RegisterWindowMessageW(w!("TaskbarCreated"));
        let _ = PostMessageW(Some(HWND_BROADCAST), created, WPARAM(0), LPARAM(0));
        println!("TaskbarCreated broadcast; listening {secs} s");
        SetTimer(Some(hwnd), 1, secs * 1000, None);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            DispatchMessageW(&msg);
        }
        let _ = PostMessageW(Some(HWND_BROADCAST), created, WPARAM(0), LPARAM(0));
        println!("TaskbarCreated broadcast again; done");
    }
    Ok(())
}
