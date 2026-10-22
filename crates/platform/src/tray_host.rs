//! NewWin's own notification-area window (spec: M2 §5).
//!
//! Apps find the first `Shell_TrayWnd` in z-order and send it `Shell_NotifyIcon` data through
//! `WM_COPYDATA`. The host keeps its own icon model, answers from it, and forwards a copy of every
//! message to explorer's tray on a separate thread, so explorer keeps the icons (toasts, fallback
//! after a crash). AppBar messages and anything else it does not own go to explorer synchronously.
//!
//! The window has its own window procedure with state in a thread-local, borrowed only around
//! model updates and never while calling out: `WM_COPYDATA` is sent, so it may arrive while
//! `App` or this very procedure is in the middle of something (our own `SHAppBarMessage`, or a
//! re-entrant message during a synchronous forward).

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::sync::mpsc;

use newwin_core::tray::{Change, IconKey, TrayIcon, TrayModel, parse_notify};
use windows::Win32::Foundation::{ERROR_CLASS_ALREADY_EXISTS, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{HSTRING, PCWSTR, Result};

use crate::taskbar::{WinEventHookGuard, explorer_tray, taskbar_created_message};

/// Larger buffers are not tray messages; refuse them before copying.
const MAX_COPYDATA: u32 = 65_536;
const NIN_SELECT: u32 = WM_USER;
const NIN_KEYSELECT: u32 = WM_USER + 1;
const GETRECT_TOP_LEFT: u32 = 1;
const GETRECT_BOTTOM_RIGHT: u32 = 2;

pub enum Forward {
    /// Explorer's own `Shell_TrayWnd` (production).
    Explorer,
    Window(HWND),
    Nowhere,
}

pub struct HostOptions {
    pub class: &'static str,
    /// The system tray host: kept first in z-order, announces itself with `TaskbarCreated`.
    pub topmost: bool,
    pub forward: Forward,
}

struct HostState {
    hwnd: HWND,
    model: TrayModel,
    notify: HWND,
    msg: u32,
    anchor: RECT,
    copies: mpsc::Sender<Vec<u8>>,
    target: Arc<AtomicIsize>,
}

thread_local! {
    static HOSTS: RefCell<Vec<HostState>> = const { RefCell::new(Vec::new()) };
}

fn with_host<R>(hwnd: HWND, f: impl FnOnce(&mut HostState) -> R) -> Option<R> {
    HOSTS.with(|h| {
        let mut hosts = h.try_borrow_mut().ok()?;
        hosts.iter_mut().find(|s| s.hwnd == hwnd).map(f)
    })
}

fn pid_of(hwnd: HWND) -> u32 {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

fn makelong(lo: i32, hi: i32) -> isize {
    ((lo as u16 as u32) | ((hi as u16 as u32) << 16)) as i32 as isize
}

/// `dwData == 1`: update the model, queue a copy for explorer, answer from the model.
fn on_notify(hwnd: HWND, bytes: &[u8]) -> isize {
    let Ok(d) = parse_notify(bytes) else { return 0 };
    let pid = pid_of(HWND(d.hwnd as usize as *mut _));
    let applied = with_host(hwnd, |s| {
        let change = s.model.apply(&d, pid);
        let _ = s.copies.send(bytes.to_vec());
        (change, s.notify, s.msg)
    });
    let Some((change, notify, msg)) = applied else { return 0 };
    log::debug!("tray {:?} {change:?} key={:?} tip={:?} pid={pid}", d.command, d.key, d.tip);
    if change == Change::Ignored {
        return 0;
    }
    unsafe {
        let _ = PostMessageW(Some(notify), msg, WPARAM(change as usize), LPARAM(0));
    }
    1
}

/// `dwData == 3` (`Shell_NotifyIconGetRect`): where the icon is — our tray button.
fn on_get_rect(hwnd: HWND, bytes: &[u8]) -> isize {
    let Some(message) = bytes.get(4..8).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])) else { return 0 };
    let Some(r) = with_host(hwnd, |s| s.anchor) else { return 0 };
    match message {
        GETRECT_TOP_LEFT => makelong(r.left, r.top),
        GETRECT_BOTTOM_RIGHT => makelong(r.right, r.bottom),
        _ => 0,
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COPYDATA => {
            if lp.0 == 0 {
                return LRESULT(0);
            }
            let cds = unsafe { &*(lp.0 as *const COPYDATASTRUCT) };
            if cds.lpData.is_null() || cds.cbData > MAX_COPYDATA {
                return LRESULT(0);
            }
            let bytes = unsafe { std::slice::from_raw_parts(cds.lpData as *const u8, cds.cbData as usize) };
            LRESULT(match cds.dwData {
                1 => on_notify(hwnd, bytes),
                3 => on_get_rect(hwnd, bytes),
                _ => {
                    // AppBar and the rest belong to explorer; no borrow is held while it answers.
                    let target = with_host(hwnd, |s| s.target.load(Ordering::Relaxed)).unwrap_or(0);
                    let target = HWND(target as *mut _);
                    if target.is_invalid() || target == hwnd {
                        return LRESULT(0);
                    }
                    let mut result = 0usize;
                    let ok = unsafe { SendMessageTimeoutW(target, WM_COPYDATA, wp, lp, SMTO_ABORTIFHUNG, 1000, Some(&mut result)) };
                    if ok.0 == 0 { 0 } else { result as isize }
                }
            })
        }
        WM_NCDESTROY => {
            HOSTS.with(|h| {
                if let Ok(mut hosts) = h.try_borrow_mut() {
                    hosts.retain(|s| s.hwnd != hwnd);
                }
            });
            unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

fn register(class: &str, proc_: WNDPROC) -> Result<()> {
    let name = HSTRING::from(class);
    let wc = WNDCLASSW { lpfnWndProc: proc_, hInstance: unsafe { GetModuleHandleW(None)? }.into(), lpszClassName: PCWSTR(name.as_ptr()), ..Default::default() };
    if unsafe { RegisterClassW(&wc) } == 0 {
        let e = windows::core::Error::from_thread();
        if e.code() != ERROR_CLASS_ALREADY_EXISTS.to_hresult() {
            return Err(e);
        }
    }
    Ok(())
}

unsafe extern "system" fn plain_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

fn resolve(forward: &Forward) -> isize {
    match forward {
        Forward::Explorer => explorer_tray().map_or(0, |h| h.0 as isize),
        Forward::Window(h) => h.0 as isize,
        Forward::Nowhere => 0,
    }
}

/// Sends queued copies to explorer in order; sleeps on the channel otherwise.
fn spawn_forwarder(host: HWND, target: Arc<AtomicIsize>) -> mpsc::Sender<Vec<u8>> {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let host = host.0 as usize;
    std::thread::Builder::new()
        .name("newwin-tray-fwd".into())
        .spawn(move || {
            for bytes in rx {
                let to = HWND(target.load(Ordering::Relaxed) as *mut _);
                if to.is_invalid() {
                    continue;
                }
                let cds = COPYDATASTRUCT { dwData: 1, cbData: bytes.len() as u32, lpData: bytes.as_ptr() as *mut _ };
                unsafe {
                    SendMessageTimeoutW(to, WM_COPYDATA, WPARAM(host), LPARAM(&cds as *const _ as isize), SMTO_ABORTIFHUNG | SMTO_BLOCK, 500, None);
                }
            }
        })
        .expect("cannot spawn tray forwarder");
    tx
}

pub struct TrayHost {
    hwnd: HWND,
    class: &'static str,
    topmost: bool,
    forward: Forward,
    target: Arc<AtomicIsize>,
}

impl TrayHost {
    /// After every model change other than `Ignored`, posts `msg` to `notify` with the `Change`
    /// (as `usize`) in `wParam`.
    pub fn create(opts: HostOptions, notify: HWND, msg: u32) -> Result<TrayHost> {
        register(opts.class, Some(wndproc))?;
        register("TrayNotifyWnd", Some(plain_proc))?;
        let ex = if opts.topmost { WS_EX_TOOLWINDOW | WS_EX_TOPMOST } else { WS_EX_TOOLWINDOW };
        let hinst = unsafe { GetModuleHandleW(None)? };
        let class = HSTRING::from(opts.class);
        let hwnd = unsafe { CreateWindowExW(ex, &class, PCWSTR::null(), WS_POPUP, 0, 0, 0, 0, None, None, Some(hinst.into()), None)? };
        // Old apps look for this child under Shell_TrayWnd.
        let _ = unsafe { CreateWindowExW(Default::default(), &HSTRING::from("TrayNotifyWnd"), PCWSTR::null(), WS_CHILD, 0, 0, 0, 0, Some(hwnd), None, Some(hinst.into()), None) };
        let target = Arc::new(AtomicIsize::new(resolve(&opts.forward)));
        let copies = spawn_forwarder(hwnd, target.clone());
        HOSTS.with(|h| h.borrow_mut().push(HostState { hwnd, model: TrayModel::default(), notify, msg, anchor: RECT::default(), copies, target: target.clone() }));
        let host = TrayHost { hwnd, class: opts.class, topmost: opts.topmost, forward: opts.forward, target };
        if host.topmost {
            log::info!("tray host first in z-order: {}", host.ensure_on_top());
        }
        Ok(host)
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    pub fn icons(&self) -> Vec<TrayIcon> {
        with_host(self.hwnd, |s| s.model.icons().to_vec()).unwrap_or_default()
    }

    pub fn get(&self, key: &IconKey) -> Option<TrayIcon> {
        with_host(self.hwnd, |s| s.model.get(key).cloned()).flatten()
    }

    /// Drops icons whose window is gone; true if any.
    pub fn prune(&self) -> bool {
        with_host(self.hwnd, |s| s.model.prune(|h| unsafe { IsWindow(Some(HWND(h as usize as *mut _))) }.as_bool())).unwrap_or(false)
    }

    /// The rectangle answered to `Shell_NotifyIconGetRect` (screen px).
    pub fn set_anchor(&self, r: RECT) {
        with_host(self.hwnd, |s| s.anchor = r);
    }

    /// Puts our window first for `FindWindow` again if explorer raised its own; true if first.
    pub fn ensure_on_top(&self) -> bool {
        if !self.topmost {
            return false;
        }
        let class = HSTRING::from(self.class);
        let first = || unsafe { FindWindowW(&class, None) }.is_ok_and(|h| h == self.hwnd);
        if first() {
            return true;
        }
        unsafe {
            let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
        let ok = first();
        log::info!("tray host raised above explorer's: first={ok}");
        ok
    }

    /// Asks every app to register its icons again (they find us first).
    pub fn announce(&self) {
        unsafe {
            let _ = PostMessageW(Some(HWND_BROADCAST), taskbar_created_message(), WPARAM(0), LPARAM(0));
        }
    }

    /// Explorer restarted: its tray is a new window, and apps will register again.
    pub fn explorer_restarted(&self) {
        self.target.store(resolve(&self.forward), Ordering::Relaxed);
        with_host(self.hwnd, |s| s.model.clear());
        self.ensure_on_top();
        self.announce();
    }
}

impl Drop for TrayHost {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
        // Apps go back to explorer.
        if self.topmost {
            self.announce();
        }
    }
}

pub enum TrayClick {
    Left,
    Right,
    Key,
}

fn makelparam(lo: u32, hi: u32) -> LPARAM {
    LPARAM(((lo & 0xFFFF) | ((hi & 0xFFFF) << 16)) as isize)
}

/// Tells the app its icon was clicked (`at` = screen px); the app opens its window or menu.
pub fn send_click(icon: &TrayIcon, click: TrayClick, at: POINT) {
    let hwnd = HWND(icon.hwnd as usize as *mut _);
    let id = match icon.key {
        IconKey::Id { id, .. } => id,
        IconKey::Guid(_) => 0,
    };
    let cb = icon.callback_message;
    if cb == 0 {
        return;
    }
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(icon.pid);
        if icon.version >= 4 {
            let event = match click {
                TrayClick::Left => NIN_SELECT,
                TrayClick::Right => WM_CONTEXTMENU,
                TrayClick::Key => NIN_KEYSELECT,
            };
            let wp = WPARAM(((at.x as u16 as u32) | ((at.y as u16 as u32) << 16)) as usize);
            let _ = PostMessageW(Some(hwnd), cb, wp, makelparam(event, id));
        } else {
            let (down, up) = match click {
                TrayClick::Right => (WM_RBUTTONDOWN, WM_RBUTTONUP),
                TrayClick::Left | TrayClick::Key => (WM_LBUTTONDOWN, WM_LBUTTONUP),
            };
            let _ = PostMessageW(Some(hwnd), cb, WPARAM(id as usize), LPARAM(down as isize));
            let _ = PostMessageW(Some(hwnd), cb, WPARAM(id as usize), LPARAM(up as isize));
        }
    }
}

static REORDER_NOTIFY: AtomicIsize = AtomicIsize::new(0);
static REORDER_MSG: AtomicU32 = AtomicU32::new(0);

/// Posts `msg` to `notify` whenever explorer reorders its windows (it may raise its tray above
/// ours); bound to the current explorer process — call again after `TaskbarCreated`.
pub fn watch_reorder(notify: HWND, msg: u32) -> Result<WinEventHookGuard> {
    unsafe extern "system" fn on_reorder(_: HWINEVENTHOOK, _: u32, _: HWND, _: i32, _: i32, _: u32, _: u32) {
        let notify = HWND(REORDER_NOTIFY.load(Ordering::Relaxed) as *mut _);
        unsafe {
            let _ = PostMessageW(Some(notify), REORDER_MSG.load(Ordering::Relaxed), WPARAM(0), LPARAM(0));
        }
    }
    REORDER_NOTIFY.store(notify.0 as isize, Ordering::Relaxed);
    REORDER_MSG.store(msg, Ordering::Relaxed);
    let explorer = explorer_tray().ok_or_else(|| windows::core::Error::new(windows::core::HRESULT(-1), "explorer taskbar not found"))?;
    let hook = unsafe { SetWinEventHook(EVENT_OBJECT_REORDER, EVENT_OBJECT_REORDER, None, Some(on_reorder), pid_of(explorer), 0, WINEVENT_OUTOFCONTEXT) };
    if hook.is_invalid() {
        return Err(windows::core::Error::from_thread());
    }
    Ok(WinEventHookGuard(hook))
}
