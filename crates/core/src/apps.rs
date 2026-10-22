//! Running-app model: which windows belong on the bar, how they group, what a click does.

use std::collections::HashMap;

pub type Hwnd = u64;

/// Identity of an app group: an AppUserModelID when the window has one, else the exe path.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum GroupKey {
    AppId(String),
    ExePath(String),
}

impl GroupKey {
    /// Paths on Windows are case-insensitive, so the key is stored lowercased.
    pub fn exe(path: &str) -> Self {
        GroupKey::ExePath(path.to_lowercase())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowInfo {
    pub hwnd: Hwnd,
    pub has_owner: bool,
    pub visible: bool,
    pub cloaked: bool,
    pub tool_window: bool,
    pub app_window: bool,
    /// `WS_EX_NOACTIVATE`: helper windows that never take focus (explorer's PopupHost, OSDs).
    pub no_activate: bool,
    /// Bare top-level `Windows.UI.Core.CoreWindow`: system UI such as Screen Snipping or Search.
    /// Real UWP apps are hosted in an `ApplicationFrameWindow` instead.
    pub shell_ui: bool,
    pub own_process: bool,
    pub minimized: bool,
    pub title: String,
    pub group_key: GroupKey,
    pub app_name: String,
}

/// Native taskbar rules: an explicit `WS_EX_APPWINDOW` wins, otherwise unowned windows that are
/// neither tool windows nor never-activated helpers; system UI windows are never apps.
pub fn is_taskbar_window(w: &WindowInfo) -> bool {
    w.visible
        && !w.cloaked
        && !w.own_process
        && !w.shell_ui
        && (w.app_window || (!w.has_owner && !w.tool_window && !w.no_activate))
}

/// A window of Screen Snipping (Win+Shift+S / PrtScn), cloaked or not: its overlay is created
/// cloaked and the screenshot is taken before it is shown.
pub fn is_screen_snip(w: &WindowInfo) -> bool {
    matches!(&w.group_key, GroupKey::AppId(id) if id.ends_with("!ScreenClipping"))
}

/// Tracks the windows of a running screen snip; reports when the first appears and the last goes.
#[derive(Default)]
pub struct SnipTracker {
    windows: std::collections::HashSet<Hwnd>,
}

impl SnipTracker {
    /// `is_snip`: whether `hwnd` (still exists and) is a snip window. Returns `Some(true)` when a
    /// snip starts, `Some(false)` when it ends.
    pub fn update(&mut self, hwnd: Hwnd, is_snip: bool) -> Option<bool> {
        let was = self.active();
        if is_snip {
            self.windows.insert(hwnd);
        } else {
            self.windows.remove(&hwnd);
        }
        (was != self.active()).then_some(self.active())
    }

    pub fn active(&self) -> bool {
        !self.windows.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AppGroup {
    pub key: GroupKey,
    pub name: String,
    pub windows: Vec<Hwnd>,
}

#[derive(Default)]
pub struct AppModel {
    groups: Vec<AppGroup>,
    windows: HashMap<Hwnd, WindowInfo>,
    active: Option<Hwnd>,
}

impl AppModel {
    /// Adds or refreshes a window; a window that no longer qualifies is removed.
    /// Returns true if anything the bar shows changed.
    pub fn upsert(&mut self, w: WindowInfo) -> bool {
        if !is_taskbar_window(&w) {
            return self.remove(w.hwnd);
        }
        let hwnd = w.hwnd;
        let mut changed = false;
        if let Some(old) = self.windows.get(&hwnd) {
            if old == &w {
                return false;
            }
            if old.group_key != w.group_key {
                self.detach(hwnd);
                changed = true;
            }
        }
        match self.groups.iter_mut().find(|g| g.key == w.group_key) {
            Some(g) => {
                if !g.windows.contains(&hwnd) {
                    g.windows.push(hwnd);
                    changed = true;
                }
            }
            None => {
                self.groups.push(AppGroup { key: w.group_key.clone(), name: w.app_name.clone(), windows: vec![hwnd] });
                changed = true;
            }
        }
        let old = self.windows.insert(hwnd, w);
        // Title and minimized state are visible in the list and in click behavior.
        changed || old.is_none() || old.as_ref() != self.windows.get(&hwnd)
    }

    pub fn remove(&mut self, hwnd: Hwnd) -> bool {
        if self.windows.remove(&hwnd).is_none() {
            return false;
        }
        self.detach(hwnd);
        if self.active == Some(hwnd) {
            self.active = None;
        }
        true
    }

    fn detach(&mut self, hwnd: Hwnd) {
        for g in &mut self.groups {
            g.windows.retain(|&h| h != hwnd);
        }
        self.groups.retain(|g| !g.windows.is_empty());
    }

    pub fn set_active(&mut self, hwnd: Option<Hwnd>) -> bool {
        let changed = self.active != hwnd;
        self.active = hwnd;
        changed
    }

    pub fn set_group_name(&mut self, key: &GroupKey, name: String) -> bool {
        match self.groups.iter_mut().find(|g| &g.key == key) {
            Some(g) if g.name != name => {
                g.name = name;
                true
            }
            _ => false,
        }
    }

    pub fn groups(&self) -> &[AppGroup] {
        &self.groups
    }

    pub fn active_group(&self) -> Option<usize> {
        let active = self.active?;
        self.groups.iter().position(|g| g.windows.contains(&active))
    }

    pub fn window(&self, hwnd: Hwnd) -> Option<&WindowInfo> {
        self.windows.get(&hwnd)
    }

    fn is_active(&self, hwnd: Hwnd) -> bool {
        self.active == Some(hwnd)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClickAction {
    Activate(Hwnd),
    Minimize(Hwnd),
    Restore(Hwnd),
    Close(Hwnd),
    ShowWindowList(usize),
    Nothing,
}

pub fn left_click(m: &AppModel, group: usize) -> ClickAction {
    let Some(g) = m.groups().get(group) else { return ClickAction::Nothing };
    match g.windows.as_slice() {
        [hwnd] => {
            let hwnd = *hwnd;
            if m.window(hwnd).is_some_and(|w| w.minimized) {
                ClickAction::Restore(hwnd)
            } else if m.is_active(hwnd) {
                ClickAction::Minimize(hwnd)
            } else {
                ClickAction::Activate(hwnd)
            }
        }
        _ => ClickAction::ShowWindowList(group),
    }
}

pub fn middle_click(m: &AppModel, group: usize) -> ClickAction {
    let Some(g) = m.groups().get(group) else { return ClickAction::Nothing };
    match g.windows.as_slice() {
        [hwnd] => ClickAction::Close(*hwnd),
        _ => ClickAction::ShowWindowList(group),
    }
}
