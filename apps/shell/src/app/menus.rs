//! Popup menus of the bar: window list, app menu, overflow, and the system (Win+X) menu.

use newwin_core::apps::{GroupKey, Hwnd, left_click};
use newwin_core::visibility::VisInput;
use newwin_platform::activate;
use newwin_platform::dpi::scale;
use newwin_platform::launch::{self, SystemTarget};
use newwin_ui::popup::{CLOSED, MenuItem, PICKED, Popup, PopupStyle, REGIONS};
use newwin_ui::{PointF, RectF};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostQuitMessage, WM_APP};

use super::App;
use super::apps_view::Target;
use crate::app::APPS_EXIT_RESTART;

pub(super) const WM_APP_POPUP: u32 = WM_APP + 5;

const NEW_WINDOW: u32 = 1;
const CLOSE_GROUP: u32 = 2;
const TARGET_BASE: u32 = 100;
const SEARCH: u32 = 150;
const NEWWIN_SETTINGS: u32 = 200;
const RESTART_NEWWIN: u32 = 201;
const QUIT: u32 = 202;

/// Order of the Win+X menu.
const SYSTEM_TARGETS: [SystemTarget; 19] = [
    SystemTarget::InstalledApps,
    SystemTarget::PowerOptions,
    SystemTarget::EventViewer,
    SystemTarget::System,
    SystemTarget::DeviceManager,
    SystemTarget::NetworkConnections,
    SystemTarget::DiskManagement,
    SystemTarget::ComputerManagement,
    SystemTarget::PowerShell,
    SystemTarget::PowerShellAdmin,
    SystemTarget::TaskManager,
    SystemTarget::Settings,
    SystemTarget::FileExplorer,
    SystemTarget::Run,
    SystemTarget::SignOut,
    SystemTarget::Sleep,
    SystemTarget::ShutDown,
    SystemTarget::Restart,
    SystemTarget::Desktop,
];

fn target_id(t: SystemTarget) -> u32 {
    TARGET_BASE + SYSTEM_TARGETS.iter().position(|&x| x == t).expect("listed target") as u32
}

pub(super) enum MenuCtx {
    WindowList { group: usize, windows: Vec<Hwnd> },
    AppMenu { group: usize, key: GroupKey, windows: Vec<Hwnd> },
    Overflow { groups: Vec<usize> },
    BarMenu,
    /// Right click on the NewWin row of the tray.
    NewWinMenu,
    /// The row menu of a Spotlight result (`index` into the catalog it shows).
    SpotlightRow { index: usize },
}

impl MenuCtx {
    /// The bar item a menu hangs from (to toggle it with a second click).
    pub(super) fn anchor_target(&self) -> Option<Target> {
        match self {
            MenuCtx::WindowList { group, .. } | MenuCtx::AppMenu { group, .. } => Some(Target::Group(*group)),
            MenuCtx::Overflow { .. } => Some(Target::Overflow),
            MenuCtx::BarMenu | MenuCtx::NewWinMenu | MenuCtx::SpotlightRow { .. } => None,
        }
    }
}

/// CLOSED carries the root popup's HWND; only the menu that is still open may be dropped.
fn closes_current(current: Option<isize>, closed: isize) -> bool {
    current == Some(closed)
}

impl App {
    fn popup_style(&self) -> PopupStyle {
        let a = &self.config.appearance;
        PopupStyle { font_family: a.font_family.clone(), font_size_px: scale(a.font_size, self.dpi), dpi: self.dpi }
    }

    /// Screen position just under a bar item (or under the pointer for the bar menu).
    fn anchor_below(&self, x_in_bar: f32) -> PointF {
        PointF { x: self.bar_rect.left as f32 + x_in_bar, y: self.bar_rect.bottom as f32 + scale(4.0, self.dpi).round() }
    }

    pub(super) fn open(&mut self, items: Vec<MenuItem>, anchor: PointF, ctx: MenuCtx) {
        self.close_panel();
        match Popup::show(&self.gfx, items, anchor, &self.popup_style(), self.bar, WM_APP_POPUP) {
            Ok(p) => {
                // Spotlight's row menu is not over the bar: the bar stays hidden (OLED).
                let over_bar = !matches!(ctx, MenuCtx::SpotlightRow { .. });
                self.popup = Some((p, ctx));
                if over_bar {
                    self.feed(VisInput::OverlayOpened);
                }
            }
            Err(e) => log::error!("cannot open menu: {e}"),
        }
    }

    pub(super) fn close_popup(&mut self) {
        match self.popup.take() {
            Some((_, MenuCtx::SpotlightRow { .. })) => self.spotlight_menu_closed(),
            Some(_) => self.feed(VisInput::OverlayClosed),
            None => {}
        }
    }

    pub(super) fn open_window_list(&mut self, group: usize, rect: RectF) {
        let Some(g) = self.model.groups().get(group) else { return };
        let windows = g.windows.clone();
        let name = g.name.clone();
        let items = windows
            .iter()
            .enumerate()
            .map(|(i, &h)| {
                let title = self.window_info(h).map(|w| w.title.clone()).filter(|t| !t.is_empty()).unwrap_or_else(|| name.clone());
                MenuItem::new(i as u32, title)
            })
            .collect();
        self.open(items, self.anchor_below(rect.x), MenuCtx::WindowList { group, windows });
    }

    pub(super) fn open_app_menu(&mut self, group: usize, rect: RectF) {
        let Some(g) = self.model.groups().get(group) else { return };
        let (key, windows) = (g.key.clone(), g.windows.clone());
        let s = self.strings;
        let close_label = if windows.len() > 1 { s.close_all_windows } else { s.close };
        let items = vec![MenuItem::new(NEW_WINDOW, s.new_window), MenuItem::new(CLOSE_GROUP, close_label).separator()];
        self.open(items, self.anchor_below(rect.x), MenuCtx::AppMenu { group, key, windows });
    }

    pub(super) fn open_overflow_menu(&mut self, rect: RectF) {
        let Some(from) = self.overflow_from else { return };
        let groups: Vec<usize> = (from..self.model.groups().len()).collect();
        let items = groups.iter().map(|&g| MenuItem::new(g as u32, self.model.groups()[g].name.clone())).collect();
        self.open(items, self.anchor_below(rect.x), MenuCtx::Overflow { groups });
    }

    pub(super) fn open_bar_menu(&mut self, at: PointF) {
        let s = self.strings;
        let t = |target: SystemTarget, label: &str| MenuItem::new(target_id(target), label);
        let items = vec![
            t(SystemTarget::InstalledApps, s.installed_apps),
            t(SystemTarget::PowerOptions, s.power_options),
            t(SystemTarget::EventViewer, s.event_viewer),
            t(SystemTarget::System, s.system),
            t(SystemTarget::DeviceManager, s.device_manager),
            t(SystemTarget::NetworkConnections, s.network_connections),
            t(SystemTarget::DiskManagement, s.disk_management),
            t(SystemTarget::ComputerManagement, s.computer_management),
            t(SystemTarget::PowerShell, s.powershell),
            t(SystemTarget::PowerShellAdmin, s.powershell_admin),
            t(SystemTarget::TaskManager, s.task_manager).separator(),
            t(SystemTarget::Settings, s.settings),
            t(SystemTarget::FileExplorer, s.file_explorer),
            // Opens Spotlight, which arrives in milestone 3.
            MenuItem::new(SEARCH, s.search),
            t(SystemTarget::Run, s.run),
            MenuItem::new(0, s.shut_down_or_sign_out).separator().submenu(vec![
                t(SystemTarget::SignOut, s.sign_out),
                t(SystemTarget::Sleep, s.sleep),
                t(SystemTarget::ShutDown, s.shut_down),
                t(SystemTarget::Restart, s.restart),
            ]),
            t(SystemTarget::Desktop, s.desktop),
            // The settings window arrives in milestone 4.
            MenuItem::new(NEWWIN_SETTINGS, s.newwin_settings).separator().enabled(false),
            MenuItem::new(RESTART_NEWWIN, s.restart_newwin),
            MenuItem::new(QUIT, s.quit),
        ];
        self.open(items, self.anchor_below(at.x), MenuCtx::BarMenu);
    }

    /// Settings… · Restart NewWin · Quit NewWin, at `at` (screen px).
    pub(super) fn open_newwin_menu(&mut self, at: PointF) {
        let s = self.strings;
        let items = vec![
            MenuItem::new(NEWWIN_SETTINGS, s.settings_ellipsis),
            MenuItem::new(RESTART_NEWWIN, s.restart_newwin).separator(),
            MenuItem::new(QUIT, s.quit),
        ];
        self.open(items, at, MenuCtx::NewWinMenu);
    }

    pub(super) fn on_popup(&mut self, wp: WPARAM, lp: LPARAM) {
        match lp.0 {
            PICKED => self.on_pick(wp.0 as u32),
            REGIONS => self.sync_glass(),
            CLOSED if closes_current(self.popup.as_ref().map(|(p, _)| p.hwnd().0 as isize), wp.0 as isize) => self.close_popup(),
            _ => {}
        }
    }

    fn on_pick(&mut self, id: u32) {
        let Some((_, ctx)) = self.popup.as_ref() else { return };
        match ctx {
            MenuCtx::WindowList { windows, .. } => {
                if let Some(&h) = windows.get(id as usize) {
                    activate::activate(h);
                }
            }
            MenuCtx::AppMenu { key, windows, .. } => match id {
                NEW_WINDOW => activate::launch_new_instance(key, windows.first().copied()),
                CLOSE_GROUP => windows.iter().for_each(|&h| activate::close(h)),
                _ => {}
            },
            MenuCtx::Overflow { groups } => {
                let g = id as usize;
                if groups.contains(&g) {
                    let action = left_click(&self.model, g);
                    // A group with several windows opens its list where the » button is.
                    let rect = self.items.iter().find(|it| it.target == Target::Overflow).map_or(RectF::default(), |it| it.rect);
                    self.close_popup();
                    self.run_click(action, rect);
                }
            }
            &MenuCtx::SpotlightRow { index } => self.on_spotlight_menu_pick(index, id),
            MenuCtx::BarMenu | MenuCtx::NewWinMenu => match id {
                NEWWIN_SETTINGS => self.open_settings_file(),
                SEARCH => self.toggle_spotlight(),
                RESTART_NEWWIN => {
                    log::info!("restart requested from the menu");
                    unsafe { PostQuitMessage(APPS_EXIT_RESTART) };
                }
                QUIT => {
                    log::info!("quit requested from the menu");
                    self.restore_taskbar();
                    unsafe { PostQuitMessage(0) };
                }
                _ if id >= TARGET_BASE && ((id - TARGET_BASE) as usize) < SYSTEM_TARGETS.len() => {
                    let target = SYSTEM_TARGETS[(id - TARGET_BASE) as usize];
                    if let Err(e) = launch::run(target) {
                        log::error!("{target:?} failed: {e}");
                    }
                }
                _ => {}
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_menu_that_closed_is_dropped() {
        // A late CLOSED from a menu we already replaced must not close the new one.
        assert!(closes_current(Some(100), 100));
        assert!(!closes_current(Some(200), 100));
        assert!(!closes_current(None, 100));
    }
}
