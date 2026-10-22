//! Tracking top-level windows: snapshot, per-window description and change events.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicIsize, Ordering};

use newwin_core::apps::{GroupKey, Hwnd, WindowInfo};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, Result, w};

use crate::appinfo::{
    process_creation_time,
    implicit_app_id, placeholder_name, process_app_id, process_image_path, window_app_id, window_relaunch_name,
};
use crate::worker::{NameJob, ResolvedName, Worker};

fn hwnd(h: Hwnd) -> HWND {
    HWND(h as isize as *mut _)
}

/// Group-name cache in front of the worker thread.
pub struct AppNames {
    names: HashMap<GroupKey, String>,
    pending: HashSet<GroupKey>,
    exe_by_pid: ExeCache,
    worker: Option<Worker>,
}

impl AppNames {
    pub fn new(worker: Worker) -> Self {
        Self { names: HashMap::new(), pending: HashSet::new(), exe_by_pid: ExeCache::default(), worker: Some(worker) }
    }

    /// Placeholders only; for tests and tools.
    pub fn without_worker() -> Self {
        Self { names: HashMap::new(), pending: HashSet::new(), exe_by_pid: ExeCache::default(), worker: None }
    }

    /// The background thread, for other slow work (the Spotlight index).
    pub fn worker(&self) -> Option<&Worker> {
        self.worker.as_ref()
    }

    /// The resolved name, or a placeholder while the worker resolves it. `relaunch_name` is read
    /// only when a lookup is submitted.
    pub fn name_or_placeholder(
        &mut self,
        key: &GroupKey,
        exe: Option<&str>,
        relaunch_name: impl FnOnce() -> Option<String>,
    ) -> String {
        if let Some(n) = self.names.get(key) {
            return n.clone();
        }
        if let Some(w) = &self.worker
            && self.pending.insert(key.clone())
        {
            w.submit(NameJob { key: key.clone(), exe: exe.map(str::to_string), relaunch_name: relaunch_name() });
        }
        placeholder_name(key, exe)
    }

    pub fn store(&mut self, r: &ResolvedName) {
        self.pending.remove(&r.key);
        self.names.insert(r.key.clone(), r.name.clone());
    }

    fn exe_for_pid(&mut self, pid: u32) -> Option<String> {
        self.exe_by_pid.get(pid, process_creation_time(pid), || process_image_path(pid))
    }
}

/// Exe path per process. Windows reuses process ids, so an entry is valid only for the process
/// that had the pid at that creation time.
#[derive(Default)]
pub struct ExeCache {
    by_pid: HashMap<u32, (u64, Option<String>)>,
}

impl ExeCache {
    /// `created`: the process creation time, `None` if the process cannot be opened (then
    /// nothing is cached).
    pub fn get(&mut self, pid: u32, created: Option<u64>, resolve: impl FnOnce() -> Option<String>) -> Option<String> {
        let Some(created) = created else { return resolve() };
        match self.by_pid.get(&pid) {
            Some((t, exe)) if *t == created => exe.clone(),
            _ => {
                let exe = resolve();
                self.by_pid.insert(pid, (created, exe.clone()));
                exe
            }
        }
    }
}

pub fn enumerate_top_level() -> Vec<Hwnd> {
    unsafe extern "system" fn collect(h: HWND, lp: LPARAM) -> BOOL {
        unsafe { (*(lp.0 as *mut Vec<Hwnd>)).push(h.0 as usize as Hwnd) };
        true.into()
    }
    let mut out: Vec<Hwnd> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut out as *mut _ as isize));
    }
    out
}

fn window_pid(h: HWND) -> u32 {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
    pid
}

/// UWP windows are hosted by ApplicationFrameHost; the app is the process of the inner CoreWindow.
fn uwp_core_pid(frame: HWND) -> Option<u32> {
    let core = unsafe { FindWindowExW(Some(frame), None, w!("Windows.UI.Core.CoreWindow"), None) }.ok()?;
    Some(window_pid(core))
}

/// Everything the bar needs to know about a window; `None` if it no longer exists.
pub fn describe(h: Hwnd, names: &mut AppNames) -> Option<WindowInfo> {
    let w = hwnd(h);
    unsafe {
        if !IsWindow(Some(w)).as_bool() {
            return None;
        }
        let pid = window_pid(w);
        let exe = names.exe_for_pid(pid);
        let is_frame_host = exe.as_deref().is_some_and(|e| e.to_lowercase().ends_with("\\applicationframehost.exe"));
        let app_id = window_app_id(w)
            .or_else(|| if is_frame_host { uwp_core_pid(w).and_then(process_app_id) } else { process_app_id(pid) })
            .or_else(|| exe.as_deref().and_then(implicit_app_id).map(str::to_string));
        let group_key = match (app_id, &exe) {
            (Some(id), _) => GroupKey::AppId(id),
            (None, Some(path)) => GroupKey::exe(path),
            (None, None) => GroupKey::ExePath(format!("pid:{pid}")),
        };
        // Original-case exe path (the key is lowercased) for placeholders and FileDescription.
        let app_exe = if is_frame_host { None } else { exe.as_deref() };
        let app_name = names.name_or_placeholder(&group_key, app_exe, || window_relaunch_name(w));

        let mut cloaked = 0u32;
        let _ = DwmGetWindowAttribute(w, DWMWA_CLOAKED, &mut cloaked as *mut _ as _, 4);
        let ex = WINDOW_EX_STYLE(GetWindowLongPtrW(w, GWL_EXSTYLE) as u32);
        let mut buf = [0u16; 512];
        let n = GetWindowTextW(w, &mut buf);
        let mut class_buf = [0u16; 128];
        let cn = GetClassNameW(w, &mut class_buf);
        let class = String::from_utf16_lossy(&class_buf[..cn.max(0) as usize]);
        log::debug!(
            "describe {h:#x} class={class} title={:?} ex={:#x} cloaked={cloaked} key={group_key:?}",
            String::from_utf16_lossy(&buf[..n.max(0) as usize]),
            ex.0
        );
        Some(WindowInfo {
            hwnd: h,
            has_owner: GetWindow(w, GW_OWNER).is_ok_and(|o| !o.is_invalid()),
            visible: IsWindowVisible(w).as_bool(),
            cloaked: cloaked != 0,
            tool_window: ex.contains(WS_EX_TOOLWINDOW),
            app_window: ex.contains(WS_EX_APPWINDOW),
            no_activate: ex.contains(WS_EX_NOACTIVATE),
            shell_ui: class == "Windows.UI.Core.CoreWindow",
            own_process: pid == std::process::id(),
            minimized: IsIconic(w).as_bool(),
            title: String::from_utf16_lossy(&buf[..n.max(0) as usize]),
            group_key,
            app_name,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum WindowEvent {
    Changed(Hwnd),
    Destroyed(Hwnd),
    Activated(Option<Hwnd>),
}

/// `HSHELL_WINDOWACTIVATED | HSHELL_HIGHBIT`; not exported by the `windows` crate.
const HSHELL_RUDEAPPACTIVATED: u32 = 0x8004;

/// WinEvent callbacks are re-posted to the notify window as this message (wp = event, lp = hwnd).
pub const WM_APP_WINEVENT: u32 = WM_APP + 2;

static NOTIFY: AtomicIsize = AtomicIsize::new(0);

unsafe extern "system" fn on_event(_: HWINEVENTHOOK, event: u32, h: HWND, id_object: i32, id_child: i32, _: u32, _: u32) {
    if id_object != OBJID_WINDOW.0 || id_child != CHILDID_SELF as i32 || h.is_invalid() {
        return;
    }
    unsafe {
        if GetAncestor(h, GA_ROOT) != h {
            return;
        }
        let notify = HWND(NOTIFY.load(Ordering::Relaxed) as *mut _);
        let _ = PostMessageW(Some(notify), WM_APP_WINEVENT, WPARAM(event as usize), LPARAM(h.0 as isize));
    }
}

pub struct WindowWatcher {
    notify: HWND,
    shellhook: u32,
    hooks: Vec<HWINEVENTHOOK>,
}

impl WindowWatcher {
    pub fn start(notify: HWND) -> Result<Self> {
        NOTIFY.store(notify.0 as isize, Ordering::Relaxed);
        unsafe {
            RegisterShellHookWindow(notify).ok()?;
            let shellhook = RegisterWindowMessageW(w!("SHELLHOOK"));
            let ranges = [
                (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
                (EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND),
                (EVENT_OBJECT_SHOW, EVENT_OBJECT_HIDE),
                (EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_NAMECHANGE),
                (EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED),
            ];
            let hooks = ranges
                .iter()
                .map(|&(a, b)| SetWinEventHook(a, b, None, Some(on_event), 0, 0, WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS))
                .filter(|h| !h.is_invalid())
                .collect();
            Ok(Self { notify, shellhook, hooks })
        }
    }

    /// Call from the notify window's procedure; `Some` for messages that belong to the watcher.
    pub fn translate(&self, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<WindowEvent> {
        let target = lp.0 as usize as Hwnd;
        if msg == self.shellhook {
            return match wp.0 as u32 {
                HSHELL_WINDOWCREATED | HSHELL_REDRAW => Some(WindowEvent::Changed(target)),
                HSHELL_WINDOWDESTROYED => Some(WindowEvent::Destroyed(target)),
                HSHELL_WINDOWACTIVATED | HSHELL_RUDEAPPACTIVATED => Some(WindowEvent::Activated((target != 0).then_some(target))),
                _ => None,
            };
        }
        if msg == WM_APP_WINEVENT {
            return Some(match wp.0 as u32 {
                EVENT_SYSTEM_FOREGROUND => WindowEvent::Activated(Some(target)),
                _ => WindowEvent::Changed(target),
            });
        }
        None
    }
}

impl Drop for WindowWatcher {
    fn drop(&mut self) {
        unsafe {
            for h in self.hooks.drain(..) {
                let _ = UnhookWinEvent(h);
            }
            let _ = DeregisterShellHookWindow(self.notify);
        }
    }
}
