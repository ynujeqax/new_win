//! The tray host on its own test class (never `Shell_TrayWnd`, so the real tray is not touched).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use newwin_core::tray::*;
use newwin_platform::tray_host::{Forward, HostOptions, TrayHost};
use newwin_platform::window::{Handler, WindowSpec, create_test_window, create_window};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::UI::WindowsAndMessaging::*;

const MSG: u32 = WM_APP + 7;

#[allow(clippy::too_many_arguments)]
fn nid(msg: u32, hwnd: u32, id: u32, flags: u32, tip: &str) -> Vec<u8> {
    let mut b = vec![0u8; 8 + NID_FULL_SIZE];
    let put = |b: &mut Vec<u8>, at: usize, v: u32| b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    put(&mut b, 0, TRAY_SIGNATURE);
    put(&mut b, 4, msg);
    put(&mut b, 8, NID_FULL_SIZE as u32);
    put(&mut b, 12, hwnd);
    put(&mut b, 16, id);
    put(&mut b, 20, flags);
    for (i, u) in tip.encode_utf16().enumerate() {
        b[32 + 2 * i..34 + 2 * i].copy_from_slice(&u.to_le_bytes());
    }
    b
}

fn copydata(to: HWND, data: usize, bytes: &[u8]) -> isize {
    let cds = COPYDATASTRUCT { dwData: data, cbData: bytes.len() as u32, lpData: bytes.as_ptr() as *mut _ };
    unsafe { SendMessageW(to, WM_COPYDATA, Some(WPARAM(0)), Some(LPARAM(&cds as *const _ as isize))) }.0
}

/// Lets sent messages from other threads (the forwarder) arrive.
fn pump(ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
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

/// Changes the host posted to `notify` so far.
fn posted(notify: HWND) -> Vec<usize> {
    let mut out = Vec::new();
    let mut m = MSG::default();
    unsafe {
        while PeekMessageW(&mut m, Some(notify), MSG, MSG, PM_REMOVE).as_bool() {
            out.push(m.wParam.0);
        }
    }
    out
}

/// Stands in for explorer's tray: records copies, answers AppBar messages with 0x55.
type CopyLog = Rc<RefCell<Vec<(usize, Vec<u8>)>>>;

struct FakeExplorer(CopyLog);

impl Handler for FakeExplorer {
    fn on_message(&mut self, _: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
        if msg == WM_COMMAND || msg == WM_USER + 5 {
            self.0.borrow_mut().push((msg as usize | 0x1_0000, wp.0.to_le_bytes().to_vec()));
            return Some(LRESULT(0x77));
        }
        if msg != WM_COPYDATA {
            return None;
        }
        let cds = unsafe { &*(lp.0 as *const COPYDATASTRUCT) };
        let bytes = unsafe { std::slice::from_raw_parts(cds.lpData as *const u8, cds.cbData as usize) }.to_vec();
        self.0.borrow_mut().push((cds.dwData, bytes));
        Some(LRESULT(if cds.dwData == 0 { 0x55 } else { 1 }))
    }
}

fn fake_explorer() -> (HWND, CopyLog) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let spec = WindowSpec { class: "NewWin.TestExplorer", ex_style: WS_EX_TOOLWINDOW, style: WS_POPUP, rect: RECT::default(), layered_alpha: None };
    (create_window(&spec, Rc::new(RefCell::new(FakeExplorer(log.clone())))).unwrap(), log)
}

fn host(forward: Forward) -> (TrayHost, HWND) {
    let notify = create_test_window(WS_EX_TOOLWINDOW, WS_POPUP, RECT::default(), "notify").unwrap();
    let h = TrayHost::create(HostOptions { class: "NewWin.TestTray", topmost: false, forward }, notify, MSG).unwrap();
    (h, notify)
}

#[test]
fn add_then_delete_updates_model_and_notifies() {
    let (fake, _) = fake_explorer();
    let (h, notify) = host(Forward::Window(fake));
    assert_eq!(copydata(h.hwnd(), 1, &nid(0, 11, 1, NIF_TIP, "Telegram")), 1);
    assert_eq!(h.icons().len(), 1);
    assert_eq!(h.icons()[0].tip, "Telegram");
    assert_eq!(posted(notify), vec![Change::Added as usize]);
    assert_eq!(copydata(h.hwnd(), 1, &nid(2, 11, 1, 0, "")), 1);
    assert!(h.icons().is_empty());
}

#[test]
fn add_of_a_known_icon_still_answers_true() {
    let (h, _) = host(Forward::Nowhere);
    assert_eq!(copydata(h.hwnd(), 1, &nid(0, 11, 1, NIF_TIP, "A")), 1);
    assert_eq!(copydata(h.hwnd(), 1, &nid(0, 11, 1, NIF_TIP, "A")), 1, "apps retry NIM_ADD forever on FALSE");
}

#[test]
fn notify_messages_are_forwarded_in_order() {
    let (fake, log) = fake_explorer();
    let (h, _) = host(Forward::Window(fake));
    for tip in ["1", "2", "3"] {
        copydata(h.hwnd(), 1, &nid(0, 11, tip.parse().unwrap(), NIF_TIP, tip));
    }
    pump(300);
    let tips: Vec<String> = log.borrow().iter().filter(|(d, _)| *d == 1).map(|(_, b)| parse_notify(b).unwrap().tip.unwrap()).collect();
    assert_eq!(tips, ["1", "2", "3"]);
}

/// Sends an AppBar message to the host from inside its own (busy) handler, like `App` does.
struct Busy {
    host: HWND,
    result: Rc<RefCell<isize>>,
}

impl Handler for Busy {
    fn on_message(&mut self, _: HWND, msg: u32, _: WPARAM, _: LPARAM) -> Option<LRESULT> {
        if msg == WM_APP + 99 {
            *self.result.borrow_mut() = copydata(self.host, 0, &[1, 2, 3, 4]);
            return Some(LRESULT(0));
        }
        None
    }
}

#[test]
fn appbar_message_forwarded_while_caller_is_busy() {
    let (fake, log) = fake_explorer();
    let (h, _) = host(Forward::Window(fake));
    let result = Rc::new(RefCell::new(0));
    let spec = WindowSpec { class: "NewWin.TestBusy", ex_style: WS_EX_TOOLWINDOW, style: WS_POPUP, rect: RECT::default(), layered_alpha: None };
    let busy = create_window(&spec, Rc::new(RefCell::new(Busy { host: h.hwnd(), result: result.clone() }))).unwrap();
    unsafe { SendMessageW(busy, WM_APP + 99, None, None) };
    assert_eq!(*result.borrow(), 0x55);
    assert_eq!(log.borrow().iter().filter(|(d, _)| *d == 0).count(), 1);
}

#[test]
fn garbage_copydata_returns_false() {
    let (h, notify) = host(Forward::Nowhere);
    assert_eq!(copydata(h.hwnd(), 1, &[1, 2, 3]), 0);
    let cds = COPYDATASTRUCT { dwData: 1, cbData: 100, lpData: std::ptr::null_mut() };
    assert_eq!(unsafe { SendMessageW(h.hwnd(), WM_COPYDATA, None, Some(LPARAM(&cds as *const _ as isize))) }.0, 0);
    assert!(h.icons().is_empty());
    assert!(posted(notify).is_empty());
}

#[test]
fn forward_to_dead_window_does_not_block() {
    let (fake, _) = fake_explorer();
    unsafe {
        let _ = DestroyWindow(fake);
    }
    let (h, _) = host(Forward::Window(fake));
    let start = Instant::now();
    assert_eq!(copydata(h.hwnd(), 1, &nid(0, 11, 1, NIF_TIP, "A")), 1);
    assert!(start.elapsed() < Duration::from_millis(50));
    assert_eq!(copydata(h.hwnd(), 0, &[0; 8]), 0, "AppBar message without a live target");
    assert_eq!(h.icons().len(), 1);
}

#[test]
fn prune_removes_icons_of_destroyed_windows() {
    let (h, _) = host(Forward::Nowhere);
    let app = create_test_window(WS_EX_TOOLWINDOW, WS_POPUP, RECT::default(), "app").unwrap();
    copydata(h.hwnd(), 1, &nid(0, app.0 as usize as u32, 1, NIF_TIP, "A"));
    assert!(!h.prune());
    unsafe {
        let _ = DestroyWindow(app);
    }
    assert!(h.prune());
    assert!(h.icons().is_empty());
}

/// Shell.Application's FileRun / ToggleDesktop post WM_COMMAND to the tray window, which is ours now.
#[test]
fn shell_commands_reach_explorer() {
    let (fake, log) = fake_explorer();
    let (h, _) = host(Forward::Window(fake));
    unsafe {
        let _ = PostMessageW(Some(h.hwnd()), WM_COMMAND, WPARAM(407), LPARAM(0));
    }
    pump(100);
    // Real senders are other threads (and processes).
    let to = h.hwnd().0 as usize;
    let other = std::thread::spawn(move || unsafe { SendMessageW(HWND(to as *mut _), WM_USER + 5, Some(WPARAM(9)), Some(LPARAM(0))) }.0);
    while !other.is_finished() {
        pump(10);
    }
    assert_eq!(other.join().unwrap(), 0x77, "a sent message returns explorer's answer");
    let got: Vec<(usize, usize)> = log
        .borrow()
        .iter()
        .filter(|(d, _)| *d & 0x1_0000 != 0)
        .map(|(d, b)| (d & 0xFFFF, usize::from_le_bytes(b[..8].try_into().unwrap())))
        .collect();
    assert_eq!(got, vec![(WM_COMMAND as usize, 407), ((WM_USER + 5) as usize, 9)]);
}

#[test]
fn clicks_follow_explorers_sequence() {
    use newwin_platform::tray_host::{TrayClick, click_messages};
    use windows::Win32::Foundation::POINT;
    let at = POINT { x: 100, y: 50 };
    let xy = (50 << 16) | 100;
    let lp = |m: u32| ((7u32 << 16) | m) as isize;
    let v4 = |c| click_messages(4, false, c, 7, at).iter().map(|(w, l)| (w.0, l.0)).collect::<Vec<_>>();
    assert_eq!(v4(TrayClick::Left), vec![(xy, lp(WM_LBUTTONDOWN)), (xy, lp(WM_LBUTTONUP)), (xy, lp(WM_USER))]);
    assert_eq!(v4(TrayClick::Right), vec![(xy, lp(WM_RBUTTONDOWN)), (xy, lp(WM_RBUTTONUP)), (xy, lp(WM_CONTEXTMENU))]);
    assert_eq!(v4(TrayClick::Key), vec![(xy, lp(WM_USER + 1))]);
    let old: Vec<(usize, isize)> = click_messages(0, false, TrayClick::Left, 7, at).iter().map(|(w, l)| (w.0, l.0)).collect();
    assert_eq!(old, vec![(7, WM_LBUTTONDOWN as isize), (7, WM_LBUTTONUP as isize)]);
}

/// An icon taken over from an update has an unknown version: right click in both formats.
#[test]
fn adopted_icons_get_both_right_click_formats() {
    use newwin_platform::tray_host::{TrayClick, click_messages};
    use windows::Win32::Foundation::POINT;
    let at = POINT { x: 100, y: 50 };
    let got: Vec<(usize, isize)> = click_messages(0, true, TrayClick::Right, 7, at).iter().map(|(w, l)| (w.0, l.0)).collect();
    assert_eq!(got, vec![(7, WM_RBUTTONDOWN as isize), (7, WM_RBUTTONUP as isize), ((50 << 16) | 100, ((7 << 16) | WM_CONTEXTMENU) as isize)]);
    let left: Vec<(usize, isize)> = click_messages(0, true, TrayClick::Left, 7, at).iter().map(|(w, l)| (w.0, l.0)).collect();
    assert_eq!(left, vec![(7, WM_LBUTTONDOWN as isize), (7, WM_LBUTTONUP as isize)], "one left click, never two");
}
