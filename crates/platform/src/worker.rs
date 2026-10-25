//! Background thread for slow work (app names, the Spotlight index, file writes); results come
//! back as posted messages.

use std::sync::mpsc;
use std::time::Instant;

use newwin_core::app_icon::{IconImage, IconKey};
use newwin_core::apps::GroupKey;
use newwin_core::spotlight::Candidate;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::appinfo::{ComGuard, resolve_name};
use crate::app_icon::shell_icon;
use crate::apps_index::enumerate_apps;

pub struct NameJob {
    pub key: GroupKey,
    pub exe: Option<String>,
    pub relaunch_name: Option<String>,
}

pub struct ResolvedName {
    pub key: GroupKey,
    pub name: String,
}

pub enum Job {
    Name(NameJob),
    /// Rebuild the Spotlight index; posts [`AppsIndexed`].
    Apps,
    /// An app icon as Explorer shows it; posts [`IconReady`].
    /// `fallback` is tried when `source` has none; the result keeps `key` either way.
    Icon { key: IconKey, source: String, fallback: Option<String>, px: u32 },
    /// Anything else that must not block the UI (writing `usage.toml`).
    Run(Box<dyn FnOnce() + Send>),
}

pub struct IconReady {
    pub key: IconKey,
    pub image: Option<IconImage>,
}

pub struct AppsIndexed {
    pub apps: Result<Vec<Candidate>, String>,
    pub ms: u32,
}

pub struct Worker {
    tx: mpsc::Sender<Job>,
}

fn post<T>(notify: isize, msg: u32, value: T) {
    let boxed = Box::into_raw(Box::new(value));
    let posted = unsafe { PostMessageW(Some(HWND(notify as *mut _)), msg, WPARAM(0), LPARAM(boxed as isize)) };
    if posted.is_err() {
        drop(unsafe { Box::from_raw(boxed) });
    }
}

impl Worker {
    /// Names are posted to `notify` as `names_msg` with a `Box<ResolvedName>` in LPARAM (take them
    /// with [`take_resolved`]); the index as `apps_msg` with a `Box<AppsIndexed>` ([`take_apps`]); icons as `icon_msg`
    /// with a `Box<IconReady>` ([`take_icon`]).
    pub fn spawn(notify: HWND, names_msg: u32, apps_msg: u32, icon_msg: u32) -> Self {
        let (tx, rx) = mpsc::channel::<Job>();
        let notify = notify.0 as isize;
        std::thread::Builder::new()
            .name("newwin-worker".into())
            .spawn(move || {
                let _com = ComGuard::init();
                for job in rx {
                    match job {
                        Job::Name(job) => {
                            let name = resolve_name(&job.key, job.exe.as_deref(), job.relaunch_name.as_deref());
                            post(notify, names_msg, ResolvedName { key: job.key, name });
                        }
                        Job::Apps => {
                            let started = Instant::now();
                            let apps = enumerate_apps().map_err(|e| e.to_string());
                            post(notify, apps_msg, AppsIndexed { apps, ms: started.elapsed().as_millis() as u32 });
                        }
                        Job::Icon { key, source, fallback, px } => {
                            let image = shell_icon(&source, px).or_else(|| fallback.and_then(|f| shell_icon(&f, px)));
                            post(notify, icon_msg, IconReady { key, image });
                        }
                        Job::Run(f) => f(),
                    }
                }
            })
            .expect("cannot spawn worker thread");
        Worker { tx }
    }

    pub fn submit(&self, job: NameJob) {
        self.submit_job(Job::Name(job));
    }

    pub fn submit_job(&self, job: Job) {
        let _ = self.tx.send(job);
    }
}

/// # Safety
/// `lp` must be the LPARAM of a names message posted by [`Worker`], taken exactly once.
pub unsafe fn take_resolved(lp: LPARAM) -> Box<ResolvedName> {
    unsafe { Box::from_raw(lp.0 as *mut ResolvedName) }
}

/// # Safety
/// `lp` must be the LPARAM of an apps message posted by [`Worker`], taken exactly once.
pub unsafe fn take_apps(lp: LPARAM) -> Box<AppsIndexed> {
    unsafe { Box::from_raw(lp.0 as *mut AppsIndexed) }
}

/// # Safety
/// `lp` must be the LPARAM of an icon message posted by [`Worker`], taken exactly once.
pub unsafe fn take_icon(lp: LPARAM) -> Box<IconReady> {
    unsafe { Box::from_raw(lp.0 as *mut IconReady) }
}
