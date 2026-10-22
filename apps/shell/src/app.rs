//! Shell role: owns the bar and keeps the native taskbar hidden.

use std::cell::RefCell;
use std::rc::Rc;

use newwin_config::Config;
use newwin_core::apps::AppModel;
use newwin_core::strings::{Strings, strings_for};
use newwin_core::visibility::{Catcher, Visibility};
use newwin_core::watchdog::EXIT_RESTART;
use newwin_platform::appbar::AppBar;
use newwin_platform::appinfo::ComGuard;
use newwin_platform::clock::ui_language_tag;
use newwin_platform::dpi::{primary_monitor, scale};
use newwin_platform::paths::taskbar_state_file;
use newwin_platform::taskbar::{NativeTaskbar, WinEventHookGuard, restore_from_state_file, taskbar_created_message, watch_taskbar_show};
use newwin_platform::window::{Handler, WindowSpec, create_window, run_message_loop};
use newwin_platform::windows_watch::{AppNames, WindowWatcher, enumerate_top_level};
use newwin_platform::worker::{Worker, take_resolved};
use newwin_ui::backdrop::{Backdrops, Region};
use newwin_ui::popup::Popup;
use newwin_ui::{Backdrop, Gfx, Layer, PointF, TextCache, TextLayout};
use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, FILETIME, GetLastError, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::SystemInformation::GetSystemTimePreciseAsFileTime;
use windows::Win32::System::Threading::{CreateMutexW, GetCurrentProcess, GetProcessTimes};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, Result, w};

use crate::bar::bar_rect;
use crate::watchdog::PING_WINDOW_CLASS;

mod apps_view;
mod clock_view;
mod glass_regions;
mod menus;
mod visibility;

use apps_view::{BarItem, Button, Target};
use menus::{MenuCtx, WM_APP_POPUP};

/// Not exported by the `windows` crate.
const WM_MOUSELEAVE: u32 = 0x02A3;
const PBT_APMRESUMEAUTOMATIC: usize = 0x12;

const WM_APP_TASKBAR_SHOWN: u32 = WM_APP + 1;
const WM_APP_APPBAR: u32 = WM_APP + 3;
const WM_APP_NAME: u32 = WM_APP + 4;
const WM_APP_GLASS: u32 = WM_APP + 6;

/// `PostQuitMessage` code that makes the watchdog start a fresh shell.
pub const APPS_EXIT_RESTART: i32 = EXIT_RESTART as i32;

pub struct App {
    gfx: Rc<Gfx>,
    config: Config,
    strings: &'static Strings,
    text: TextCache,
    taskbar: Option<NativeTaskbar>,
    show_watch: Option<WinEventHookGuard>,
    taskbar_created: u32,
    ping: HWND,
    bar: HWND,
    bar_layer: Option<Layer>,
    bar_rect: RECT,
    monitor: RECT,
    dpi: u32,
    // Visibility
    catcher: HWND,
    catcher_mode: Catcher,
    vis: Option<Visibility>,
    appbar: Option<AppBar>,
    /// Pointer is inside the bar or the catcher (the hot zone).
    in_zone: bool,
    /// `TrackMouseEvent(TME_LEAVE)` armed for [bar, catcher].
    tracking: [bool; 2],
    /// When the running hide animation started (to resume a reveal from its position).
    hide_started: Option<std::time::Instant>,
    // Running apps
    model: AppModel,
    names: Option<AppNames>,
    watcher: Option<WindowWatcher>,
    last_signature: Vec<(String, bool)>,
    items: Vec<BarItem>,
    overflow_from: Option<usize>,
    hover: Option<Target>,
    popup: Option<(Popup, MenuCtx)>,
    // Liquid glass
    backdrops: Option<Backdrops>,
    /// The regions last sent to the glass thread.
    glass_sent: Vec<Region>,
    /// The bar shows a blurred backdrop (else the fallback tint).
    bar_glass: bool,
    /// A full-screen app covers the bar.
    fullscreen: bool,
    // Clock
    date_text: String,
    time_text: String,
    date_layout: Option<TextLayout>,
    time_layout: Option<TextLayout>,
    date_x: f32,
    time_x: f32,
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

fn client_point(lp: LPARAM) -> PointF {
    PointF { x: (lp.0 & 0xFFFF) as i16 as f32, y: ((lp.0 >> 16) & 0xFFFF) as i16 as f32 }
}

/// `WM_SETTINGCHANGE` with lParam "intl": regional format changed.
fn is_intl_change(lp: LPARAM) -> bool {
    lp.0 != 0 && unsafe { PCWSTR(lp.0 as *const u16).to_string() }.is_ok_and(|s| s == "intl")
}

impl App {
    fn reposition(&mut self) {
        let (monitor, dpi) = primary_monitor();
        self.monitor = monitor;
        self.dpi = dpi;
        self.bar_rect = bar_rect(monitor, dpi, &self.config.appearance);
        let r = self.bar_rect;
        unsafe {
            let _ = SetWindowPos(self.bar, Some(HWND_TOPMOST), r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOACTIVATE);
        }
        if let Some(layer) = self.bar_layer.as_mut() {
            let _ = layer.resize((r.right - r.left) as u32, (r.bottom - r.top) as u32);
            let _ = layer.set_corner_radius(scale(self.config.appearance.corner_radius, dpi));
        }
        self.set_catcher(self.catcher_mode);
        self.reserve_space();
        self.refresh();
        self.sync_glass();
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

    fn on_ping_message(&mut self, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
        if msg == self.taskbar_created {
            log::info!("explorer restarted, hiding the taskbar again");
            self.reapply_taskbar(true);
            return Some(LRESULT(0));
        }
        if msg == WM_APP_GLASS {
            self.on_glass(wp, lp);
            return Some(LRESULT(0));
        }
        if msg == WM_APP_NAME {
            let r = unsafe { take_resolved(lp) };
            self.on_name_resolved(&r);
            return Some(LRESULT(0));
        }
        if let Some(ev) = self.watcher.as_ref().and_then(|w| w.translate(msg, wp, lp)) {
            self.on_window_event(ev);
            return Some(LRESULT(0));
        }
        if msg == WM_APP_TASKBAR_SHOWN {
            self.reapply_taskbar(false);
            return Some(LRESULT(0));
        }
        None
    }

    fn on_bar_message(&mut self, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
        match msg {
            WM_MOUSEMOVE => {
                self.on_pointer_move(self.bar);
                self.update_hover(Some(client_point(lp)));
                Some(LRESULT(0))
            }
            WM_MOUSELEAVE => {
                self.on_pointer_leave(self.bar);
                self.update_hover(None);
                Some(LRESULT(0))
            }
            WM_LBUTTONUP | WM_MBUTTONUP | WM_RBUTTONUP => {
                let button = match msg {
                    WM_LBUTTONUP => Button::Left,
                    WM_MBUTTONUP => Button::Middle,
                    _ => Button::Right,
                };
                self.on_click(button, client_point(lp));
                Some(LRESULT(0))
            }
            WM_TIMER => {
                self.on_timer(wp.0);
                Some(LRESULT(0))
            }
            WM_APP_POPUP => {
                self.on_popup(wp, lp);
                Some(LRESULT(0))
            }
            WM_APP_APPBAR => {
                self.on_appbar(wp, lp);
                Some(LRESULT(0))
            }
            WM_DISPLAYCHANGE | WM_DPICHANGED => {
                self.reposition();
                Some(LRESULT(0))
            }
            // Clock: time zone/clock change, wake from sleep, regional format change.
            WM_TIMECHANGE => {
                self.tick_clock();
                Some(LRESULT(0))
            }
            WM_POWERBROADCAST if wp.0 == PBT_APMRESUMEAUTOMATIC => {
                self.tick_clock();
                Some(LRESULT(1))
            }
            WM_SETTINGCHANGE if is_intl_change(lp) => {
                self.tick_clock();
                Some(LRESULT(0))
            }
            WM_MOUSEACTIVATE => Some(LRESULT(MA_NOACTIVATE as isize)),
            WM_WINDOWPOSCHANGED => {
                let pos = unsafe { &*(lp.0 as *const WINDOWPOS) };
                if pos.flags.contains(SWP_SHOWWINDOW) || pos.flags.contains(SWP_HIDEWINDOW) {
                    log::debug!(
                        "bar window {} (state {:?})",
                        if pos.flags.contains(SWP_SHOWWINDOW) { "shown" } else { "hidden" },
                        self.vis.as_ref().map(|v| v.state())
                    );
                }
                None
            }
            WM_ENDSESSION if wp.0 != 0 => {
                self.restore_taskbar();
                Some(LRESULT(0))
            }
            _ => None,
        }
    }
}

impl Handler for App {
    fn on_message(&mut self, hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
        if hwnd == self.ping {
            return self.on_ping_message(msg, wp, lp);
        }
        if hwnd == self.bar {
            return self.on_bar_message(msg, wp, lp);
        }
        if hwnd == self.catcher {
            return match msg {
                WM_MOUSEACTIVATE => Some(LRESULT(MA_NOACTIVATE as isize)),
                WM_MOUSEMOVE => {
                    self.on_pointer_move(hwnd);
                    Some(LRESULT(0))
                }
                WM_MOUSELEAVE => {
                    self.on_pointer_leave(hwnd);
                    Some(LRESULT(0))
                }
                _ => None,
            };
        }
        None
    }
}

pub fn run_shell() -> i32 {
    let _mutex = unsafe { CreateMutexW(None, true, w!("Local\\NewWin.Shell")) };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        log::info!("shell already running, exiting");
        return 0;
    }
    // Shell APIs (window AppUserModelIDs, Run dialog, show desktop) need COM on the UI thread.
    let _com = ComGuard::init();
    match start() {
        Ok(app) => {
            let code = run_message_loop();
            let mut a = app.borrow_mut();
            a.close_popup();
            a.restore_taskbar();
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
        strings: strings_for(&ui_language_tag()),
        taskbar: None,
        show_watch: None,
        taskbar_created: taskbar_created_message(),
        ping: HWND::default(),
        bar: HWND::default(),
        bar_layer: None,
        bar_rect: RECT::default(),
        monitor: RECT::default(),
        dpi: 96,
        catcher: HWND::default(),
        catcher_mode: Catcher::Off,
        vis: None,
        appbar: None,
        in_zone: false,
        tracking: [false; 2],
        hide_started: None,
        model: AppModel::default(),
        names: None,
        watcher: None,
        last_signature: Vec::new(),
        items: Vec::new(),
        overflow_from: None,
        hover: None,
        popup: None,
        backdrops: None,
        glass_sent: Vec::new(),
        bar_glass: false,
        fullscreen: false,
        date_text: String::new(),
        time_text: String::new(),
        date_layout: None,
        time_layout: None,
        date_x: 0.0,
        time_x: 0.0,
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
    // Invisible (alpha 1/255) but hit-testable: reveals the hidden bar on hover.
    let catcher = create_window(
        &WindowSpec {
            class: "NewWin.Catcher",
            ex_style: WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_LAYERED,
            style: WS_POPUP,
            rect: RECT { left: 0, top: 0, right: 1, bottom: 1 },
            layered_alpha: Some(1),
        },
        app.clone(),
    )?;

    {
        let mut a = app.borrow_mut();
        a.ping = ping;
        a.bar = bar;
        a.catcher = catcher;
        a.taskbar = Some(NativeTaskbar::hide(&state)?);
        a.show_watch = watch_taskbar_show(ping, WM_APP_TASKBAR_SHOWN).map_err(|e| log::error!("taskbar watch: {e}")).ok();
        a.appbar = AppBar::register(bar, WM_APP_APPBAR).map_err(|e| log::error!("appbar: {e}")).ok();
        a.bar_layer = Some(Layer::new(&a.gfx, bar, Backdrop::Glass, 1, 1)?);
        if a.config.appearance.glass {
            a.backdrops = Some(Backdrops::spawn(ping, WM_APP_GLASS));
        }
        a.names = Some(AppNames::new(Worker::spawn(ping, WM_APP_NAME)));
        a.watcher = Some(WindowWatcher::start(ping)?);
        let foreground = unsafe { GetForegroundWindow() }.0 as usize as u64;
        a.load_windows(enumerate_top_level(), foreground);
        a.tick_clock();
        a.reposition();
        a.start_visibility();
    }
    log::info!("first-frame-ms={}", ms_since_process_start());
    Ok(app)
}
