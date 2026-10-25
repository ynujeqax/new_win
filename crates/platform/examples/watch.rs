//! Prints the bar's app groups whenever they change. Usage: `watch [seconds]`

use std::cell::RefCell;
use std::rc::Rc;

use newwin_core::apps::AppModel;
use newwin_platform::appinfo::ComGuard;
use newwin_platform::window::{Handler, WindowSpec, create_window, run_message_loop};
use newwin_platform::windows_watch::{AppNames, WindowEvent, WindowWatcher, describe, enumerate_top_level};
use newwin_platform::worker::{Worker, take_resolved};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;

const WM_NAME: u32 = WM_APP + 10;
const QUIT_TIMER: usize = 1;

struct Watch {
    model: AppModel,
    names: Option<AppNames>,
    watcher: Option<WindowWatcher>,
}

impl Watch {
    fn print(&self) {
        let active = self.model.active_group();
        let line: Vec<String> = self
            .model
            .groups()
            .iter()
            .enumerate()
            .map(|(i, g)| format!("{}{}({})", if Some(i) == active { "*" } else { "" }, g.name, g.windows.len()))
            .collect();
        println!("{}", line.join(" | "));
    }

    fn refresh(&mut self, h: u64) -> bool {
        match describe(h, self.names.as_mut().unwrap()) {
            Some(w) => self.model.upsert(w),
            None => self.model.remove(h),
        }
    }
}

impl Handler for Watch {
    fn on_message(&mut self, _hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
        if msg == WM_TIMER && wp.0 == QUIT_TIMER {
            unsafe { PostQuitMessage(0) };
            return Some(LRESULT(0));
        }
        if msg == WM_NAME {
            let r = unsafe { take_resolved(lp) };
            self.names.as_mut().unwrap().store(&r);
            if self.model.set_group_name(&r.key, r.name.clone()) {
                self.print();
            }
            return Some(LRESULT(0));
        }
        let ev = self.watcher.as_ref()?.translate(msg, wp, lp)?;
        let changed = match ev {
            WindowEvent::Changed(h) => self.refresh(h),
            WindowEvent::Destroyed(h) => self.model.remove(h),
            WindowEvent::Activated(h) => {
                let a = h.map(|h| self.refresh(h)).unwrap_or(false);
                self.model.set_active(h) | a
            }
        };
        if changed {
            self.print();
        }
        Some(LRESULT(0))
    }
}

fn main() -> windows::core::Result<()> {
    let _com = ComGuard::init();
    let secs: u32 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(20);
    let app = Rc::new(RefCell::new(Watch { model: AppModel::default(), names: None, watcher: None }));
    let notify = create_window(
        &WindowSpec { class: "NewWin.WatchProbe", ex_style: WS_EX_TOOLWINDOW, style: WS_POPUP, rect: RECT::default(), layered_alpha: None },
        app.clone(),
    )?;
    {
        let mut a = app.borrow_mut();
        a.names = Some(AppNames::new(Worker::spawn(notify, WM_NAME, WM_NAME + 1, WM_NAME + 2)));
        a.watcher = Some(WindowWatcher::start(notify)?);
        for h in enumerate_top_level() {
            a.refresh(h);
        }
        let fg = unsafe { GetForegroundWindow() };
        a.model.set_active(Some(fg.0 as usize as u64));
        a.print();
    }
    unsafe { SetTimer(Some(notify), QUIT_TIMER, secs * 1000, None) };
    run_message_loop();
    Ok(())
}
