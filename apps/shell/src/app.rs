//! Shell role: owns the bar and keeps the native taskbar hidden.

use std::cell::RefCell;
use std::rc::Rc;

use newwin_config::Config;
use newwin_core::apps::{AppModel, SnipTracker};
use newwin_core::strings::{Strings, strings_for};
use newwin_core::visibility::{Catcher, VisInput, Visibility};
use newwin_core::watchdog::EXIT_RESTART;
use newwin_platform::appbar::AppBar;
use newwin_platform::appinfo::ComGuard;
use newwin_platform::clock::ui_language_tag;
use newwin_platform::dpi::{primary_monitor, scale};
use newwin_platform::paths::taskbar_state_file;
use newwin_platform::taskbar::{NativeTaskbar, WinEventHookGuard, restore_from_state_file, taskbar_created_message, watch_taskbar_show};
use newwin_platform::tray_host::{Forward, HostOptions, TrayHost, watch_reorder};
use newwin_platform::window::{Handler, WindowSpec, create_window, run_message_loop};
use newwin_platform::windows_watch::{AppNames, WindowWatcher, enumerate_top_level};
use newwin_platform::worker::{Worker, take_resolved};
use newwin_ui::backdrop::{Backdrops, Region};
use newwin_ui::popup::Popup;
use newwin_ui::tooltip::Tooltip;
use newwin_ui::{Backdrop, Gfx, Layer, PointF, TextCache};
use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, FILETIME, GetLastError, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::SystemInformation::GetSystemTimePreciseAsFileTime;
use windows::Win32::System::Threading::{CreateMutexW, GetCurrentProcess, GetProcessTimes};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, Result, w};

use crate::bar::bar_rect;
use crate::watchdog::PING_WINDOW_CLASS;

mod apps_view;
mod clock_view;
mod config_view;
mod glass_regions;
mod icons_view;
mod layout_view;
mod menus;
mod network_view;
mod panels;
mod right_view;
mod spotlight_view;
mod tray_view;
mod volume_view;
mod visibility;

use apps_view::{BarItem, Button, Target};
use menus::{MenuCtx, WM_APP_POPUP};
use panels::{ShellPanel, WM_APP_PANEL};
use right_view::RightView;

/// Not exported by the `windows` crate.
const WM_MOUSELEAVE: u32 = 0x02A3;
const PBT_APMRESUMEAUTOMATIC: usize = 0x12;

const WM_APP_TASKBAR_SHOWN: u32 = WM_APP + 1;
const WM_APP_APPBAR: u32 = WM_APP + 3;
const WM_APP_NAME: u32 = WM_APP + 4;
const WM_APP_GLASS: u32 = WM_APP + 6;
const WM_APP_TRAY: u32 = WM_APP + 7;
const WM_APP_REORDER: u32 = WM_APP + 12;
/// Settings' *Restore Windows taskbar*: the same as *Quit NewWin*.
pub(super) const WM_APP_QUIT: u32 = WM_APP + 20;
/// An app icon from the worker (`IconReady`).
pub(super) const WM_APP_ICON: u32 = WM_APP + 21;

/// `TaskbarCreated` also comes from our own tray host (start, quit, restart handling). Explorer
/// restarted only if its tray (`current`) is a different window than the one we forward to
/// (`known`).
fn is_explorer_restart(current: Option<isize>, known: Option<isize>) -> bool {
    match known {
        // No tray host to compare with: M1's behaviour, every TaskbarCreated is a restart.
        None => true,
        Some(k) => current.is_some_and(|c| c != k),
    }
}

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
    // Tray host (M2)
    tray_host: Option<TrayHost>,
    reorder_watch: Option<WinEventHookGuard>,
    /// The icon behind each app row of the open tray panel.
    tray_keys: Vec<newwin_core::tray::IconKey>,
    /// Fallback row names by process id.
    tray_names: std::collections::HashMap<u32, String>,
    /// Exe path per pid (empty: unreadable), like `tray_names`.
    tray_exes: std::collections::HashMap<u32, String>,
    /// While the tray is held open under an app's menu: that app, and a foreground watch.
    held_pid: u32,
    foreground_watch: Option<WinEventHookGuard>,
    // Volume (M2)
    audio: Option<newwin_platform::audio::AudioService>,
    /// (level, muted) of the default output device.
    volume: Option<(u8, bool)>,
    volume_device: String,
    volume_wheel: newwin_core::audio::Wheel,
    audio_switch_warned: bool,
    // Network (M2)
    network: Option<newwin_platform::network::NetworkWatch>,
    net_info: Option<newwin_core::network::NetInfo>,
    // Keyboard layout (M2)
    input_hook: Option<newwin_platform::input_hook::InputHook>,
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
    panel: Option<ShellPanel>,
    tooltip: Option<Tooltip>,
    // Liquid glass
    backdrops: Option<Backdrops>,
    /// The regions last sent to the glass thread.
    glass_sent: Vec<Region>,
    /// The bar shows a blurred backdrop (else the fallback tint).
    bar_glass: bool,
    /// A full-screen app covers the bar.
    fullscreen: bool,
    /// Screen Snipping is open: glass is capturable and its backdrop frozen.
    snips: SnipTracker,
    // Clock
    date_text: String,
    time_text: String,
    // Spotlight (M3)
    apps: Vec<newwin_core::spotlight::Candidate>,
    /// System commands, then the indexed apps.
    catalog: Rc<Vec<newwin_core::spotlight::Candidate>>,
    usage: newwin_config::usage::Usage,
    /// `usage.toml` is known to be on disk (loaded, or written by the worker): only then does its
    /// absence mean it was deleted from outside.
    usage_on_disk: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Apps that have had a tray icon (`tray_seen.toml`); loaded once, saved when it changes.
    tray_seen: newwin_config::tray_seen::TraySeen,
    /// A name changed since `tray_seen.toml` was last written (saved with the next add or at quit).
    tray_seen_dirty: bool,
    /// `config.toml` was there at the last read: "missing" is logged once, on the change.
    config_on_disk: bool,
    // App icons (1.0.1)
    icons: newwin_core::app_icon::IconCache<Rc<newwin_core::app_icon::IconImage>>,
    icon_bitmaps: newwin_ui::icon_bitmaps::IconBitmaps,
    /// When each icon arrived, while it fades in.
    icon_arrived: std::collections::HashMap<newwin_core::app_icon::IconKey, u64>,
    spotlight: Option<newwin_ui::panel::Panel<newwin_ui::spotlight::SpotlightView>>,
    apps_watch: Option<newwin_platform::apps_index::AppsWatch>,
    // Tray button and right side (M2)
    right: RightView,
    // Live config (M4)
    config_watch: Option<newwin_platform::dir_watch::DirWatch>,
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
        // The output's size or position may have changed: the glass capture re-reads it.
        if let Some(b) = &self.backdrops {
            b.refresh();
        }
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

    /// AppBar registrations live in explorer and die with it: register again and reserve the
    /// space again (pinned mode), and forget a full-screen state whose end will never be reported.
    fn on_explorer_restarted(&mut self) {
        let r = match self.appbar.as_mut() {
            Some(ab) => ab.reregister(),
            None => AppBar::register(self.bar, WM_APP_APPBAR).map(|ab| self.appbar = Some(ab)),
        };
        if let Err(e) = r {
            log::error!("appbar after explorer restart: {e}");
        }
        self.reserve_space();
        if self.fullscreen {
            self.fullscreen = false;
            self.feed(VisInput::Fullscreen(false));
        }
    }

    /// Our `Shell_TrayWnd`: apps register their icons with it (it is first for `FindWindow`).
    fn start_tray_host(&mut self) {
        let opts = HostOptions { class: "Shell_TrayWnd", topmost: true, forward: Forward::Explorer };
        self.tray_host = TrayHost::create(opts, self.ping, WM_APP_TRAY).map_err(|e| log::error!("tray host: {e}")).ok();
        self.reorder_watch = watch_reorder(self.ping, WM_APP_REORDER).map_err(|e| log::error!("tray reorder watch: {e}")).ok();
        if let Some(h) = &self.tray_host {
            h.announce();
            log::info!("tray host up");
        }
    }

    /// Forwards to the new explorer, which apps then register with again; done first, so the
    /// AppBar calls that re-hide the taskbar reach the new explorer and not the dead one.
    fn restart_tray_host(&mut self) {
        let Some(h) = &self.tray_host else { return };
        h.explorer_restarted();
        // Bound to the old explorer process.
        self.reorder_watch = None;
        self.reorder_watch = watch_reorder(self.ping, WM_APP_REORDER).map_err(|e| log::error!("tray reorder watch: {e}")).ok();
    }

    /// Saves usage and changed tray names, gives the taskbar back and ends the message loop. Exit
    /// code 0 makes the watchdog exit as well.
    pub(super) fn quit(&mut self) {
        log::info!("quit requested");
        self.save_usage_now();
        self.save_tray_seen_if_dirty();
        self.restore_taskbar();
        unsafe { PostQuitMessage(0) };
    }

    fn restore_taskbar(&mut self) {
        // Apps go back to explorer, and restoring talks to explorer directly.
        self.reorder_watch = None;
        self.tray_host = None;
        self.show_watch = None;
        if let Some(tb) = self.taskbar.take()
            && let Err(e) = tb.restore()
        {
            log::error!("restoring native taskbar failed: {e}");
        }
    }

    fn on_ping_message(&mut self, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
        if msg == self.taskbar_created {
            // After restore_taskbar (quit, sign-out) the host's own goodbye arrives here.
            let current = newwin_platform::taskbar::explorer_tray().map(|h| h.0 as isize);
            let known = self.tray_host.as_ref().map(|h| h.target().0 as isize);
            if self.taskbar.is_none() || !is_explorer_restart(current, known) {
                return Some(LRESULT(0));
            }
            log::info!("explorer restarted, hiding the taskbar again");
            self.restart_tray_host();
            self.reapply_taskbar(true);
            self.on_explorer_restarted();
            return Some(LRESULT(0));
        }
        if msg == tray_view::WM_APP_FOREGROUND {
            self.on_foreground(HWND(wp.0 as *mut _));
            return Some(LRESULT(0));
        }
        if msg == spotlight_view::WM_APP_APPS_CHANGED {
            self.on_apps_changed();
            return Some(LRESULT(0));
        }
        if msg == spotlight_view::WM_APP_APPS {
            let r = unsafe { newwin_platform::worker::take_apps(lp) };
            self.on_apps_indexed(*r);
            return Some(LRESULT(0));
        }
        if msg == spotlight_view::WM_APP_LAUNCHED {
            let key = unsafe { Box::from_raw(lp.0 as *mut String) };
            self.on_launched(*key);
            return Some(LRESULT(0));
        }
        if msg == spotlight_view::WM_APP_SPOTLIGHT {
            if wp.0 == newwin_platform::input_hook::SPOTLIGHT_PREWARM {
                // Win went down: a tap may follow, and Spotlight then needs the glass device.
                if let Some(b) = &self.backdrops {
                    b.warm();
                }
            } else {
                self.toggle_spotlight();
            }
            return Some(LRESULT(0));
        }
        if msg == layout_view::WM_APP_LAYOUT {
            self.on_layout_hotkey();
            return Some(LRESULT(0));
        }
        if msg == network_view::WM_APP_NET {
            self.on_network_event();
            return Some(LRESULT(0));
        }
        if msg == volume_view::WM_APP_AUDIO {
            self.on_audio(wp, lp);
            return Some(LRESULT(0));
        }
        if msg == WM_APP_TRAY {
            self.on_tray_changed(wp.0);
            return Some(LRESULT(0));
        }
        if msg == WM_APP_REORDER {
            if let Some(h) = &self.tray_host {
                h.ensure_on_top();
            }
            return Some(LRESULT(0));
        }
        if msg == WM_APP_QUIT {
            self.quit();
            return Some(LRESULT(0));
        }
        if msg == config_view::WM_APP_CONFIG {
            self.on_config_dir_changed();
            return Some(LRESULT(0));
        }
        if msg == WM_APP_GLASS {
            self.on_glass(wp, lp);
            return Some(LRESULT(0));
        }
        if msg == WM_APP_ICON {
            let r = unsafe { newwin_platform::worker::take_icon(lp) };
            self.on_icon_ready(*r);
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
            WM_MOUSEWHEEL => {
                let delta = ((wp.0 >> 16) & 0xFFFF) as i16;
                let screen = client_point(lp);
                let p = PointF { x: screen.x - self.bar_rect.left as f32, y: screen.y - self.bar_rect.top as f32 };
                self.on_wheel(delta, p);
                Some(LRESULT(0))
            }
            spotlight_view::WM_APP_SPOTLIGHT_PANEL => {
                self.on_spotlight_panel(wp, lp);
                Some(LRESULT(0))
            }
            WM_APP_PANEL => {
                self.on_panel(wp, lp);
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
                // A launch counted in the last two seconds is still waiting for its save timer.
                self.save_usage_now();
                self.save_tray_seen_if_dirty();
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
    if let Some(dir) = newwin_config::default_path().parent() {
        let n = newwin_config::clean_stale_tmp(dir, std::time::Duration::from_secs(60));
        if n > 0 {
            log::info!("removed {n} stale temp files");
        }
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

    let right = RightView::new(&config_view::visible_items(&config.items));
    let gfx = Gfx::new()?;
    let app = Rc::new(RefCell::new(App {
        text: TextCache::new(&gfx),
        gfx,
        config,
        strings: strings_for(&ui_language_tag()),
        taskbar: None,
        show_watch: None,
        taskbar_created: taskbar_created_message(),
        tray_host: None,
        reorder_watch: None,
        tray_keys: Vec::new(),
        tray_names: std::collections::HashMap::new(),
        tray_exes: std::collections::HashMap::new(),
        held_pid: 0,
        foreground_watch: None,
        audio: None,
        volume: None,
        volume_device: String::new(),
        volume_wheel: newwin_core::audio::Wheel::default(),
        audio_switch_warned: false,
        network: None,
        net_info: None,
        input_hook: None,
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
        panel: None,
        tooltip: None,
        backdrops: None,
        glass_sent: Vec::new(),
        bar_glass: false,
        fullscreen: false,
        snips: SnipTracker::default(),
        right,
        config_watch: None,
        apps: Vec::new(),
        catalog: Rc::new(Vec::new()),
        usage: Default::default(),
        usage_on_disk: Default::default(),
        tray_seen: Default::default(),
        tray_seen_dirty: false,
        config_on_disk: config_path.exists(),
        icons: Default::default(),
        icon_bitmaps: Default::default(),
        icon_arrived: Default::default(),
        spotlight: None,
        apps_watch: None,
        date_text: String::new(),
        time_text: String::new(),
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
        a.start_tray_host();
        a.appbar = AppBar::register(bar, WM_APP_APPBAR).map_err(|e| log::error!("appbar: {e}")).ok();
        a.bar_layer = Some(Layer::new(&a.gfx, bar, Backdrop::Glass, 1, 1)?);
        if a.config.appearance.glass {
            a.backdrops = Some(Backdrops::spawn(ping, WM_APP_GLASS));
        }
        a.names = Some(AppNames::new(Worker::spawn_with(ping, WM_APP_NAME, spotlight_view::WM_APP_APPS, WM_APP_ICON, icon_fetch())));
        a.watcher = Some(WindowWatcher::start(ping)?);
        let foreground = unsafe { GetForegroundWindow() }.0 as usize as u64;
        a.load_windows(enumerate_top_level(), foreground);
        a.start_audio();
        a.start_network();
        a.start_layout();
        a.load_tray_seen();
        a.start_spotlight();
        a.tick_clock();
        a.reposition();
        a.start_visibility();
        // After the bar is up: a change from now on applies live.
        a.start_config_watch();
        config_view::sync_autostart(a.config.system.start_with_windows);
    }
    log::info!("first-frame-ms={}", ms_since_process_start());
    Ok(app)
}

/// Shell icons come from `NewWin.exe --icon-helper`, so the components Windows loads for them
/// stay out of the shell (1.0.1); without our own path, here.
fn icon_fetch() -> newwin_platform::worker::IconFetch {
    match std::env::current_exe() {
        Ok(exe) => newwin_platform::worker::IconFetch::Helper(exe),
        Err(_) => newwin_platform::worker::IconFetch::InProcess,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explorer_restart_is_a_new_explorer_tray() {
        // Our own TaskbarCreated (start, quit, explorer restart handling): explorer's tray is the same.
        assert!(!is_explorer_restart(Some(0x100d2), Some(0x100d2)));
        // A new explorer made a new tray window.
        assert!(is_explorer_restart(Some(0x2040a), Some(0x100d2)));
        // Explorer gone: nothing to re-hide or register with.
        assert!(!is_explorer_restart(None, Some(0x100d2)));
        // Without a tray host there is nothing to compare: treat it as a restart (M1 behaviour).
        assert!(is_explorer_restart(Some(0x100d2), None));
    }
}

