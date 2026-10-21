//! Shell role: owns the bar and keeps the native taskbar hidden.

use std::cell::RefCell;
use std::rc::Rc;

use newwin_config::Config;
use newwin_platform::dpi::{primary_monitor, scale};
use newwin_platform::paths::taskbar_state_file;
use newwin_platform::taskbar::{NativeTaskbar, WinEventHookGuard, restore_from_state_file, taskbar_created_message, watch_taskbar_show};
use newwin_platform::window::{Handler, WindowSpec, create_window, run_message_loop};
use newwin_ui::{Backdrop, Gfx, Layer, TextCache, theme};
use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, FILETIME, GetLastError, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::SystemInformation::GetSystemTimePreciseAsFileTime;
use windows::Win32::System::Threading::{CreateMutexW, GetCurrentProcess, GetProcessTimes};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{Result, w};

use crate::bar::{bar_rect, paint_background, text_origin, text_style};
use crate::watchdog::PING_WINDOW_CLASS;

const WM_APP_TASKBAR_SHOWN: u32 = WM_APP + 1;

pub struct App {
    gfx: Rc<Gfx>,
    config: Config,
    text: TextCache,
    taskbar: Option<NativeTaskbar>,
    show_watch: Option<WinEventHookGuard>,
    taskbar_created: u32,
    ping: HWND,
    bar: HWND,
    bar_layer: Option<Layer>,
    bar_rect: RECT,
    dpi: u32,
}

fn ms_since_process_start() -> u64 {
    let filetime = |f: FILETIME| (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime);
    unsafe {
        let (mut created, mut exit, mut kernel, mut user) = Default::default();
        if GetProcessTimes(GetCurrentProcess(), &mut created, &mut exit, &mut kernel, &mut user).is_err() {
            return 0;
        }
        (filetime(GetSystemTimePreciseAsFileTime()).saturating_sub(filetime(created))) / 10_000
    }
}

impl App {
    fn reposition(&mut self) {
        let (monitor, dpi) = primary_monitor();
        self.dpi = dpi;
        self.bar_rect = bar_rect(monitor, dpi, &self.config.appearance);
        let r = self.bar_rect;
        unsafe {
            let _ = SetWindowPos(self.bar, Some(HWND_TOPMOST), r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOACTIVATE);
        }
        if let Some(layer) = self.bar_layer.as_mut() {
            let _ = layer.resize((r.right - r.left) as u32, (r.bottom - r.top) as u32);
        }
        self.paint();
    }

    fn paint(&mut self) {
        let Some(layer) = self.bar_layer.as_mut() else { return };
        let (w, h) = layer.size();
        let (w, h) = (w as f32, h as f32);
        let a = &self.config.appearance;
        let pad = scale(14.0, self.dpi);
        let regular = text_style(a, self.dpi, theme::WEIGHT_REGULAR);
        let strong = text_style(a, self.dpi, theme::WEIGHT_SEMIBOLD);
        let (Ok(left), Ok(right)) = (self.text.layout("NewWin", &strong, w / 2.0), self.text.layout("Milestone 1", &regular, w / 2.0))
        else {
            return;
        };
        let dpi = self.dpi;
        let r = layer.draw(None, |p| {
            paint_background(p, w, h, dpi, a);
            p.text(&left, text_origin(pad, h, &left), theme::TEXT_STRONG);
            p.text(&right, text_origin(w - pad - right.width(), h, &right), theme::TEXT);
        });
        if let Err(e) = r.and_then(|_| self.gfx.commit()) {
            log::error!("bar paint failed: {e}");
        }
    }

    fn reapply_taskbar(&mut self, reinstall_watch: bool) {
        if let Some(tb) = &self.taskbar
            && let Err(e) = tb.reapply()
        {
            log::error!("re-hiding native taskbar failed: {e}");
        }
        if reinstall_watch {
            // Bound to the old explorer process; must be recreated for the new one.
            self.show_watch = None;
            self.show_watch = watch_taskbar_show(self.ping, WM_APP_TASKBAR_SHOWN).map_err(|e| log::error!("taskbar watch: {e}")).ok();
        }
    }

    fn restore_taskbar(&mut self) {
        self.show_watch = None;
        if let Some(tb) = self.taskbar.take()
            && let Err(e) = tb.restore()
        {
            log::error!("restoring native taskbar failed: {e}");
        }
    }
}

impl Handler for App {
    fn on_message(&mut self, hwnd: HWND, msg: u32, wp: WPARAM, _lp: LPARAM) -> Option<LRESULT> {
        if msg == self.taskbar_created && hwnd == self.ping {
            log::info!("explorer restarted, hiding the taskbar again");
            self.reapply_taskbar(true);
            return Some(LRESULT(0));
        }
        match msg {
            WM_APP_TASKBAR_SHOWN => {
                self.reapply_taskbar(false);
                Some(LRESULT(0))
            }
            WM_DISPLAYCHANGE | WM_DPICHANGED if hwnd == self.bar => {
                self.reposition();
                Some(LRESULT(0))
            }
            WM_MOUSEACTIVATE if hwnd == self.bar => Some(LRESULT(MA_NOACTIVATE as isize)),
            WM_ENDSESSION if wp.0 != 0 => {
                self.restore_taskbar();
                Some(LRESULT(0))
            }
            _ => None,
        }
    }
}

pub fn run_shell() -> i32 {
    let _mutex = unsafe { CreateMutexW(None, true, w!("Local\\NewWin.Shell")) };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        log::info!("shell already running, exiting");
        return 0;
    }
    match start() {
        Ok(app) => {
            let code = run_message_loop();
            app.borrow_mut().restore_taskbar();
            code
        }
        Err(e) => {
            log::error!("shell failed to start: {e}");
            let _ = restore_from_state_file(&taskbar_state_file());
            1
        }
    }
}

fn start() -> Result<Rc<RefCell<App>>> {
    let state = taskbar_state_file();
    if let Ok(true) = restore_from_state_file(&state) {
        log::warn!("restored a taskbar left hidden by a previous run");
    }
    let config_path = newwin_config::default_path();
    let config = newwin_config::load(&config_path).unwrap_or_else(|e| {
        log::error!("{e}; using defaults");
        Config::default()
    });

    let gfx = Gfx::new()?;
    let app = Rc::new(RefCell::new(App {
        text: TextCache::new(&gfx),
        gfx,
        config,
        taskbar: None,
        show_watch: None,
        taskbar_created: taskbar_created_message(),
        ping: HWND::default(),
        bar: HWND::default(),
        bar_layer: None,
        bar_rect: RECT::default(),
        dpi: 96,
    }));

    let ping = create_window(
        &WindowSpec { class: PING_WINDOW_CLASS, ex_style: WS_EX_TOOLWINDOW, style: WS_POPUP, rect: RECT::default(), layered_alpha: None },
        app.clone(),
    )?;
    let bar = create_window(
        &WindowSpec {
            class: "NewWin.Bar",
            ex_style: WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_NOREDIRECTIONBITMAP,
            style: WS_POPUP,
            rect: RECT { left: 0, top: 0, right: 1, bottom: 1 },
            layered_alpha: None,
        },
        app.clone(),
    )?;

    {
        let mut a = app.borrow_mut();
        a.ping = ping;
        a.bar = bar;
        a.taskbar = Some(NativeTaskbar::hide(&state)?);
        a.show_watch = watch_taskbar_show(ping, WM_APP_TASKBAR_SHOWN).map_err(|e| log::error!("taskbar watch: {e}")).ok();
        a.bar_layer = Some(Layer::new(&a.gfx, bar, Backdrop::Tint, 1, 1)?);
        a.reposition();
        unsafe {
            let _ = ShowWindow(bar, SW_SHOWNOACTIVATE);
        }
    }
    log::info!("first-frame-ms={}", ms_since_process_start());
    Ok(app)
}
