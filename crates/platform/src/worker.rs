//! Background thread for slow lookups (app names); results come back as posted messages.

use std::sync::mpsc;

use newwin_core::apps::GroupKey;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::appinfo::{ComGuard, resolve_name};

pub struct NameJob {
    pub key: GroupKey,
    pub exe: Option<String>,
    pub relaunch_name: Option<String>,
}

pub struct ResolvedName {
    pub key: GroupKey,
    pub name: String,
}

pub struct Worker {
    tx: mpsc::Sender<NameJob>,
}

impl Worker {
    /// Results are posted to `notify` as `msg` with a `Box<ResolvedName>` in LPARAM; take them with
    /// [`take_resolved`].
    pub fn spawn(notify: HWND, msg: u32) -> Self {
        let (tx, rx) = mpsc::channel::<NameJob>();
        let notify = notify.0 as isize;
        std::thread::Builder::new()
            .name("newwin-worker".into())
            .spawn(move || {
                let _com = ComGuard::init();
                for job in rx {
                    let name = resolve_name(&job.key, job.exe.as_deref(), job.relaunch_name.as_deref());
                    let boxed = Box::into_raw(Box::new(ResolvedName { key: job.key, name }));
                    let posted = unsafe { PostMessageW(Some(HWND(notify as *mut _)), msg, WPARAM(0), LPARAM(boxed as isize)) };
                    if posted.is_err() {
                        drop(unsafe { Box::from_raw(boxed) });
                    }
                }
            })
            .expect("cannot spawn worker thread");
        Worker { tx }
    }

    pub fn submit(&self, job: NameJob) {
        let _ = self.tx.send(job);
    }
}

/// # Safety
/// `lp` must be the LPARAM of a message posted by [`Worker`], taken exactly once.
pub unsafe fn take_resolved(lp: LPARAM) -> Box<ResolvedName> {
    unsafe { Box::from_raw(lp.0 as *mut ResolvedName) }
}
