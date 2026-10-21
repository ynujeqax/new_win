# NewWin Milestone 1 — Shell Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A daily-usable NewWin: the native taskbar is hidden and safely restored, a floating glass top bar shows running apps (grouped, clickable) on the left and date/time on the right, in pinned or auto-hide mode, supervised by a watchdog.

**Architecture:** One Rust Cargo workspace. Pure logic (window model, grouping, visibility state machine, label fitting, crash policy, strings) lives in `crates/core` and is unit-tested. `crates/config` reads `config.toml`. `crates/platform` wraps Win32 (taskbar, appbar, window watching, activation, clock formatting, logging). `crates/ui` is a minimal Direct2D/DirectWrite/DirectComposition toolkit. `apps/shell` builds `NewWin.exe`, which runs as watchdog (default) or shell (`--shell`).

**Tech Stack:** Rust stable (edition 2024), `windows` crate (Microsoft), `serde` + `toml`, `log`, `embed-manifest` (build-dep). No other runtime dependencies without a reason written in the task.

**Spec:** `docs/superpowers/specs/2026-10-21-newwin-design.md` (milestone 1 = §15 item 1; relevant sections §2, §3, §4, §5, §6, §7 date/time only, §10 read-only, §11, §12, §14).

## Global Constraints

- Platform: Windows 11 x64, single (primary) monitor; DPI awareness Per-Monitor V2 via embedded manifest.
- `NewWin.exe` subsystem `windows` (no console). Release profile: `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip = true`.
- No polling timers. The only periodic wakeups in the shell are the minute-aligned clock timer and the watchdog ping (every 5 s).
- Idle CPU 0.0% (10 min); shell private working set < 20 MB; watchdog < 2 MB; cold start to visible bar < 300 ms; reveal animation ~150 ms.
- Bar geometry (logical px at 100% DPI, scale by DPI): height 30, margin 6 (top/left/right), corner radius 11.
- Colors: bar overlay `rgba(255,255,255,0.065)`, border 1 px `rgba(255,255,255,0.08)`, text `#ececec`, inactive `#9a9a9a`, active pill `rgba(255,255,255,0.11)` radius 7, hover pill `rgba(255,255,255,0.07)`, separator `rgba(255,255,255,0.12)`. Font `Segoe UI Variable Text` 13 px; active app weight 600, time weight 600.
- **Plan decision (addition to spec §2):** under the white overlay the bar paints a dark tint base `rgba(18,18,20,0.88)`, because in auto-hide mode the bar overlays app content (a white web page would make white text unreadable). Over a black wallpaper this looks the same as the mockup.
- Popover/menus: glass `rgba(28,28,30,0.78)` over the same tint, radius 12, item height 30, item radius 8, hover `rgba(255,255,255,0.10)`.
- Animations 120–180 ms ease-out, executed by DirectComposition (no app-side frame loop).
- Text antialiasing: grayscale (never ClearType).
- UI strings come only from `core::strings` (en, uk; chosen by `GetUserDefaultUILanguage`; fallback en).
- User data: `%APPDATA%\NewWin\config.toml`; logs `%LOCALAPPDATA%\NewWin\logs\newwin.log` (rotate at 1 MB, keep 3); taskbar state `%LOCALAPPDATA%\NewWin\taskbar-state.toml`.
- Process roles: `NewWin.exe` or `NewWin.exe --watchdog` → watchdog; `NewWin.exe --shell` → shell. Shell exit codes: `0` = quit (watchdog exits too), `3` = restart requested (not counted as a crash), anything else = crash.
- Single instance: named mutexes `Local\NewWin.Watchdog` and `Local\NewWin.Shell`.
- Out of scope for M1: tray (`⌃` button not rendered), layout/network/volume/search items, Spotlight, Win key, settings window, config hot reload, installer, autostart.

## Review Focus

1. **explorer.exe restarts while NewWin runs** → native taskbar is hidden again within 1 s; bar keeps working; no duplicate windows. (Task 6 checklist + Task 13 checklist)
2. **Shell is hard-killed (Task Manager → End task) or hangs** → watchdog restarts it; after 3 crashes in 60 s the native taskbar is restored and the watchdog exits. (Task 4 unit tests, Task 7 checklist)
3. **Pointer jitters on the top edge / leaves during the reveal animation / moves into the 6 px gap between strip and bar** → bar never gets stuck shown or hidden. (Task 3 unit tests)
4. **Windows that already exist when NewWin starts, UWP apps (ApplicationFrameHost), cloaked windows on other virtual desktops, untitled windows, elevated windows** → correct list: existing and UWP apps shown with proper names, cloaked hidden, elevated apps activatable. (Task 2 unit tests, Task 10 checklist)
5. **Sleep/resume, time-zone or clock change, DPI or resolution change** → clock correct immediately, bar repositioned and crisp. (Task 12 checklist, Task 9 checklist)

---

### Task 1: Toolchain and workspace skeleton

**Files:**
- Create: `Cargo.toml` (workspace), `rust-toolchain.toml`, `crates/core/Cargo.toml`, `crates/core/src/lib.rs`, `crates/config/Cargo.toml`, `crates/config/src/lib.rs`, `crates/platform/Cargo.toml`, `crates/platform/src/lib.rs`, `crates/ui/Cargo.toml`, `crates/ui/src/lib.rs`, `apps/shell/Cargo.toml`, `apps/shell/build.rs`, `apps/shell/src/main.rs`
- Modify: `.gitignore` (already contains `/target/` and `.superpowers/`)

**Interfaces:**
- Produces: workspace members `newwin-core`, `newwin-config`, `newwin-platform`, `newwin-ui`, `newwin-shell` (binary name `NewWin`). Crates are imported as `newwin_core` etc.

- [ ] **Step 1: Ask the user before installing anything.** Installing downloads software; get an explicit yes for: Git (`winget install --id Git.Git -e`), rustup (`winget install --id Rustlang.Rustup -e`), Visual Studio 2022 Build Tools with C++ workload and Windows 11 SDK (`winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"`).
- [ ] **Step 2: Install, open a new shell, verify.** Run `git --version; rustup --version; cargo --version`. Expected: three version lines. Run `rustup default stable`.
- [ ] **Step 3: `git init`, commit the existing spec** (`docs/`, `.gitignore`) with message `docs: NewWin design spec`.
- [ ] **Step 4: Create the workspace.** `rust-toolchain.toml` pins `channel = "stable"`. Root `Cargo.toml`: `resolver = "3"`, members as above, `[workspace.package] edition = "2024"`, `[workspace.dependencies]` with `windows` pinned to the latest 0.x release on crates.io at setup time, `serde = { version = "1", features = ["derive"] }`, `toml = "0.8"` (or latest), `log = "0.4"`; release profile from Global Constraints. `apps/shell/build.rs` embeds a manifest via `embed-manifest` with `dpi_awareness(PerMonitorV2)` and `long_path_aware`. `main.rs` starts with `#![windows_subsystem = "windows"]` and an empty `fn main() {}`.
- [ ] **Step 5: Verify.** Run `cargo build --release` and `cargo test --workspace`. Expected: both succeed; `target\release\NewWin.exe` exists.
- [ ] **Step 6: Commit** `chore: cargo workspace skeleton`.

---

### Task 2: Window model, grouping and click actions (`core`)

**Files:**
- Create: `crates/core/src/apps.rs`, `crates/core/tests/apps.rs`
- Modify: `crates/core/src/lib.rs` (`pub mod apps;`)

**Interfaces:**
- Produces:
```rust
pub type Hwnd = u64;
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum GroupKey { AppId(String), ExePath(String) }   // ExePath stored lowercased
impl GroupKey { pub fn exe(path: &str) -> Self; }
#[derive(Clone, Debug, PartialEq)]
pub struct WindowInfo {
    pub hwnd: Hwnd, pub has_owner: bool, pub visible: bool, pub cloaked: bool,
    pub tool_window: bool, pub app_window: bool, pub own_process: bool,
    pub minimized: bool, pub title: String, pub group_key: GroupKey, pub app_name: String,
}
pub fn is_taskbar_window(w: &WindowInfo) -> bool;
#[derive(Clone, Debug, PartialEq)]
pub struct AppGroup { pub key: GroupKey, pub name: String, pub windows: Vec<Hwnd> }
#[derive(Default)]
pub struct AppModel { /* private */ }
impl AppModel {
    pub fn upsert(&mut self, w: WindowInfo) -> bool;          // true if anything visible changed
    pub fn remove(&mut self, hwnd: Hwnd) -> bool;
    pub fn set_active(&mut self, hwnd: Option<Hwnd>) -> bool;
    pub fn set_group_name(&mut self, key: &GroupKey, name: String) -> bool;
    pub fn groups(&self) -> &[AppGroup];
    pub fn active_group(&self) -> Option<usize>;
    pub fn window(&self, hwnd: Hwnd) -> Option<&WindowInfo>;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClickAction { Activate(Hwnd), Minimize(Hwnd), Restore(Hwnd), Close(Hwnd), ShowWindowList(usize), Nothing }
pub fn left_click(m: &AppModel, group: usize) -> ClickAction;
pub fn middle_click(m: &AppModel, group: usize) -> ClickAction;
```

- [ ] **Step 1: Write failing tests** in `crates/core/tests/apps.rs` (helper `fn win(hwnd, key, name) -> WindowInfo` returning a visible, unowned, uncloaked, non-tool, non-minimized window):
```rust
#[test] fn filter_rules() {
    assert!(is_taskbar_window(&win(1, "a", "A")));
    assert!(!is_taskbar_window(&WindowInfo { visible: false, ..win(1,"a","A") }));
    assert!(!is_taskbar_window(&WindowInfo { cloaked: true, ..win(1,"a","A") }));
    assert!(!is_taskbar_window(&WindowInfo { own_process: true, ..win(1,"a","A") }));
    assert!(!is_taskbar_window(&WindowInfo { tool_window: true, ..win(1,"a","A") }));
    assert!(!is_taskbar_window(&WindowInfo { has_owner: true, ..win(1,"a","A") }));
    assert!(is_taskbar_window(&WindowInfo { has_owner: true, app_window: true, ..win(1,"a","A") }));
    assert!(is_taskbar_window(&WindowInfo { tool_window: true, app_window: true, ..win(1,"a","A") }));
}
#[test] fn groups_by_key_in_first_seen_order() {
    let mut m = AppModel::default();
    m.upsert(win(1, "chrome", "Chrome")); m.upsert(win(2, "code", "VS Code")); m.upsert(win(3, "chrome", "Chrome"));
    let g = m.groups();
    assert_eq!(g.len(), 2);
    assert_eq!((g[0].name.as_str(), g[0].windows.clone()), ("Chrome", vec![1, 3]));
    assert_eq!(g[1].name, "VS Code");
}
#[test] fn exe_key_is_case_insensitive() { assert_eq!(GroupKey::exe(r"C:\A\B.EXE"), GroupKey::exe(r"c:\a\b.exe")); }
#[test] fn removing_last_window_drops_group_and_upsert_of_filtered_window_removes_it() {
    let mut m = AppModel::default();
    m.upsert(win(1, "a", "A")); m.upsert(win(2, "b", "B"));
    assert!(m.remove(1)); assert_eq!(m.groups().len(), 1);
    assert!(m.upsert(WindowInfo { cloaked: true, ..win(2, "b", "B") }));
    assert!(m.groups().is_empty());
    assert!(!m.remove(99));
}
#[test] fn active_group_tracks_active_window() {
    let mut m = AppModel::default();
    m.upsert(win(1, "a", "A")); m.upsert(win(2, "b", "B"));
    m.set_active(Some(2)); assert_eq!(m.active_group(), Some(1));
    m.set_active(Some(77)); assert_eq!(m.active_group(), None);
}
#[test] fn rename_group() {
    let mut m = AppModel::default(); m.upsert(win(1, "a", "a.exe"));
    assert!(m.set_group_name(&key("a"), "Alpha".into())); assert_eq!(m.groups()[0].name, "Alpha");
}
#[test] fn left_click_rules() {
    let mut m = AppModel::default(); m.upsert(win(1, "a", "A"));
    assert_eq!(left_click(&m, 0), ClickAction::Activate(1));
    m.set_active(Some(1)); assert_eq!(left_click(&m, 0), ClickAction::Minimize(1));
    m.upsert(WindowInfo { minimized: true, ..win(1, "a", "A") }); assert_eq!(left_click(&m, 0), ClickAction::Restore(1));
    m.upsert(win(2, "a", "A")); assert_eq!(left_click(&m, 0), ClickAction::ShowWindowList(0));
}
#[test] fn middle_click_rules() {
    let mut m = AppModel::default(); m.upsert(win(1, "a", "A"));
    assert_eq!(middle_click(&m, 0), ClickAction::Close(1));
    m.upsert(win(2, "a", "A")); assert_eq!(middle_click(&m, 0), ClickAction::ShowWindowList(0));
    assert_eq!(middle_click(&m, 5), ClickAction::Nothing);
}
```
(`key("a")` = `GroupKey::AppId("a".into())`; `win` uses `key`.)
- [ ] **Step 2: Run** `cargo test -p newwin-core --test apps`. Expected: FAIL (module missing).
- [ ] **Step 3: Implement `crates/core/src/apps.rs`.** Filter rule: `visible && !cloaked && !own_process && (app_window || (!has_owner && !tool_window))`. A minimized window counts as active only if it is the active hwnd and not minimized (minimized → `Restore`). Moving a window whose `group_key` changed removes it from the old group first.
- [ ] **Step 4: Run** the same command. Expected: PASS (8 tests).
- [ ] **Step 5: Commit** `feat(core): window model, grouping, click actions`.

---

### Task 3: Visibility state machine (`core`)

**Files:**
- Create: `crates/core/src/visibility.rs`, `crates/core/tests/visibility.rs`
- Modify: `crates/core/src/lib.rs`

**Interfaces:**
- Produces:
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Mode { Pinned, AutoHide }
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum VisState { Hidden, PendingShow, Shown, PendingHide }
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Catcher { Off, Strip, Zone }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisInput { ZoneEnter, ZoneLeave, OverlayOpened, OverlayClosed, Timer, Fullscreen(bool) }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisCmd { StartTimer(u32), CancelTimer, AnimateIn, AnimateOut, SetCatcher(Catcher) }
pub struct VisConfig { pub mode: Mode, pub show_delay_ms: u32, pub hide_delay_ms: u32, pub ignore_fullscreen: bool }
pub struct Visibility { /* private */ }
impl Visibility {
    pub fn new(cfg: VisConfig) -> (Self, Vec<VisCmd>);   // initial commands
    pub fn handle(&mut self, input: VisInput) -> Vec<VisCmd>;
    pub fn set_mode(&mut self, mode: Mode) -> Vec<VisCmd>;
    pub fn state(&self) -> VisState;
}
```
"Zone" = the whole hot rectangle (full width × (margin + height)). The shell reports `ZoneEnter`/`ZoneLeave` for the union of the catcher window and the bar window. While shown in auto-hide, the catcher window grows to the whole zone (`Catcher::Zone`) so the 6 px gaps never lose the pointer.

- [ ] **Step 1: Write failing tests** (`cfg(mode)` = show 150, hide 400, ignore_fullscreen true):
```rust
#[test] fn autohide_starts_hidden_with_strip() {
    let (v, c) = Visibility::new(cfg(Mode::AutoHide));
    assert_eq!(v.state(), VisState::Hidden); assert_eq!(c, vec![VisCmd::SetCatcher(Catcher::Strip)]);
}
#[test] fn pinned_starts_shown_without_catcher() {
    let (v, c) = Visibility::new(cfg(Mode::Pinned));
    assert_eq!(v.state(), VisState::Shown); assert_eq!(c, vec![VisCmd::SetCatcher(Catcher::Off), VisCmd::AnimateIn]);
}
#[test] fn reveal_after_delay() {
    let (mut v, _) = Visibility::new(cfg(Mode::AutoHide));
    assert_eq!(v.handle(VisInput::ZoneEnter), vec![VisCmd::StartTimer(150)]);
    assert_eq!(v.handle(VisInput::Timer), vec![VisCmd::SetCatcher(Catcher::Zone), VisCmd::AnimateIn]);
    assert_eq!(v.state(), VisState::Shown);
}
#[test] fn leaving_before_delay_cancels() {
    let (mut v, _) = Visibility::new(cfg(Mode::AutoHide));
    v.handle(VisInput::ZoneEnter);
    assert_eq!(v.handle(VisInput::ZoneLeave), vec![VisCmd::CancelTimer]);
    assert_eq!(v.state(), VisState::Hidden);
    assert_eq!(v.handle(VisInput::Timer), vec![]);           // stale timer is ignored
}
#[test] fn hide_after_delay_and_reenter_cancels() {
    let mut v = shown();
    assert_eq!(v.handle(VisInput::ZoneLeave), vec![VisCmd::StartTimer(400)]);
    assert_eq!(v.handle(VisInput::ZoneEnter), vec![VisCmd::CancelTimer]);
    v.handle(VisInput::ZoneLeave);
    assert_eq!(v.handle(VisInput::Timer), vec![VisCmd::AnimateOut, VisCmd::SetCatcher(Catcher::Strip)]);
    assert_eq!(v.state(), VisState::Hidden);
}
#[test] fn overlay_blocks_hiding() {
    let mut v = shown();
    v.handle(VisInput::OverlayOpened);
    assert_eq!(v.handle(VisInput::ZoneLeave), vec![]);
    assert_eq!(v.state(), VisState::Shown);
    assert_eq!(v.handle(VisInput::OverlayClosed), vec![VisCmd::StartTimer(400)]);
}
#[test] fn overlay_closed_while_pointer_inside_keeps_shown() {
    let mut v = shown(); v.handle(VisInput::OverlayOpened);
    assert_eq!(v.handle(VisInput::OverlayClosed), vec![]);
}
#[test] fn jitter_never_sticks() {
    let (mut v, _) = Visibility::new(cfg(Mode::AutoHide));
    for _ in 0..50 { v.handle(VisInput::ZoneEnter); v.handle(VisInput::ZoneLeave); }
    assert_eq!(v.state(), VisState::Hidden);
    v.handle(VisInput::ZoneEnter); v.handle(VisInput::Timer); v.handle(VisInput::ZoneLeave); v.handle(VisInput::ZoneEnter);
    assert_eq!(v.state(), VisState::Shown);
}
#[test] fn fullscreen_disables_trigger_only_when_hidden_and_enabled() {
    let (mut v, _) = Visibility::new(cfg(Mode::AutoHide));
    assert_eq!(v.handle(VisInput::Fullscreen(true)), vec![VisCmd::SetCatcher(Catcher::Off)]);
    assert_eq!(v.handle(VisInput::ZoneEnter), vec![]);
    assert_eq!(v.handle(VisInput::Fullscreen(false)), vec![VisCmd::SetCatcher(Catcher::Strip)]);
    let (mut w, _) = Visibility::new(VisConfig { ignore_fullscreen: false, ..cfg(Mode::AutoHide) });
    assert_eq!(w.handle(VisInput::Fullscreen(true)), vec![]);
}
#[test] fn switching_modes() {
    let (mut v, _) = Visibility::new(cfg(Mode::AutoHide));
    assert_eq!(v.set_mode(Mode::Pinned), vec![VisCmd::CancelTimer, VisCmd::SetCatcher(Catcher::Off), VisCmd::AnimateIn]);
    assert_eq!(v.handle(VisInput::ZoneLeave), vec![]);
    assert_eq!(v.set_mode(Mode::AutoHide), vec![VisCmd::SetCatcher(Catcher::Zone), VisCmd::StartTimer(400)]);
}
```
(`shown()` = AutoHide, then `ZoneEnter`, `Timer`.) Note `set_mode(AutoHide)` assumes the pointer is outside the zone; a following `ZoneEnter` cancels the timer.
- [ ] **Step 2: Run** `cargo test -p newwin-core --test visibility`. Expected: FAIL.
- [ ] **Step 3: Implement** as a `match (state, input)` with private fields `pointer_in_zone: bool`, `overlays: u32`, `fullscreen: bool`. `Timer` acts only in `PendingShow`/`PendingHide`. In `Pinned` every input except `Fullscreen` and overlay counting returns `vec![]`. While `fullscreen && ignore_fullscreen`, `ZoneEnter` in `Hidden` is ignored, and any transition back to `Hidden` sets `Catcher::Off` instead of `Strip`.
- [ ] **Step 4: Run** the same command. Expected: PASS (10 tests).
- [ ] **Step 5: Commit** `feat(core): bar visibility state machine`.

---

### Task 4: Label fitting, crash policy, clock math, strings (`core`)

**Files:**
- Create: `crates/core/src/layout.rs`, `crates/core/src/watchdog.rs`, `crates/core/src/clock.rs`, `crates/core/src/strings.rs`, `crates/core/tests/misc.rs`
- Modify: `crates/core/src/lib.rs`

**Interfaces:**
- Produces:
```rust
// layout.rs
#[derive(Debug, PartialEq)] pub struct LabelFit { pub widths: Vec<f32>, pub overflow_from: Option<usize> }
pub fn fit_labels(natural: &[f32], available: f32, min_width: f32, gap: f32, overflow_button: f32) -> LabelFit;
// watchdog.rs
#[derive(Debug, PartialEq, Eq)] pub enum ExitDecision { Restart, GiveUp, Stop }
pub struct CrashPolicy { /* window 60 s, max 3 */ }
impl CrashPolicy { pub fn new() -> Self; pub fn on_exit(&mut self, exit_code: u32, now: std::time::Instant) -> ExitDecision; }
#[derive(Debug, PartialEq, Eq)] pub enum PingDecision { Healthy, Kill }
pub struct HangMonitor { /* 3 consecutive failures */ }
impl HangMonitor { pub fn new() -> Self; pub fn on_ping(&mut self, responded: bool) -> PingDecision; }
// clock.rs
pub fn ms_until_next_minute(second: u16, millis: u16) -> u32;
// strings.rs
pub struct Strings { pub new_window: &'static str, pub close: &'static str, pub close_all_windows: &'static str,
                     pub settings: &'static str, pub restart: &'static str, pub quit: &'static str }
pub fn strings_for(lang_tag: &str) -> &'static Strings;
```
- [ ] **Step 1: Write failing tests:**
```rust
#[test] fn labels_fit_naturally() {
    assert_eq!(fit_labels(&[50.,60.], 200., 40., 4., 24.), LabelFit { widths: vec![50.,60.], overflow_from: None });
}
#[test] fn longest_labels_shrink_first() {
    // available 150 = gaps 4 + widths 146 → cap c with 30 + min(100,c) + min(80,c) = 146 → c = 58
    assert_eq!(fit_labels(&[30.,100.,80.], 150., 40., 2., 24.), LabelFit { widths: vec![30.,58.,58.], overflow_from: None });
}
#[test] fn overflow_when_min_widths_do_not_fit() {
    // all at min 40 + gaps need > 120; reserve 24 for the » button: 40+4+40 = 84 ≤ 120-24-4 → 2 fit
    assert_eq!(fit_labels(&[90.,90.,90.,90.], 120., 40., 4., 24.), LabelFit { widths: vec![40.,40.], overflow_from: Some(2) });
}
#[test] fn three_crashes_in_a_minute_give_up() {
    let mut p = CrashPolicy::new(); let t = Instant::now();
    assert_eq!(p.on_exit(1, t), ExitDecision::Restart);
    assert_eq!(p.on_exit(1, t + Duration::from_secs(10)), ExitDecision::Restart);
    assert_eq!(p.on_exit(1, t + Duration::from_secs(20)), ExitDecision::GiveUp);
}
#[test] fn old_crashes_expire_and_codes_0_and_3_are_not_crashes() {
    let mut p = CrashPolicy::new(); let t = Instant::now();
    p.on_exit(1, t); p.on_exit(1, t + Duration::from_secs(5));
    assert_eq!(p.on_exit(1, t + Duration::from_secs(70)), ExitDecision::Restart);
    assert_eq!(p.on_exit(3, t + Duration::from_secs(71)), ExitDecision::Restart);
    assert_eq!(p.on_exit(0, t + Duration::from_secs(72)), ExitDecision::Stop);
}
#[test] fn hang_after_three_missed_pings() {
    let mut h = HangMonitor::new();
    assert_eq!(h.on_ping(false), PingDecision::Healthy); assert_eq!(h.on_ping(false), PingDecision::Healthy);
    assert_eq!(h.on_ping(true), PingDecision::Healthy);
    h.on_ping(false); h.on_ping(false); assert_eq!(h.on_ping(false), PingDecision::Kill);
}
#[test] fn minute_alignment() {
    assert_eq!(ms_until_next_minute(0, 0), 60_050);
    assert_eq!(ms_until_next_minute(59, 990), 60);
}
#[test] fn strings_fallback() {
    assert_eq!(strings_for("uk-UA").quit, "Вийти з NewWin");
    assert_eq!(strings_for("en-US").quit, "Quit NewWin");
    assert_eq!(strings_for("de-DE").quit, "Quit NewWin");
}
```
- [ ] **Step 2: Run** `cargo test -p newwin-core --test misc`. Expected: FAIL.
- [ ] **Step 3: Implement.** `fit_labels`: if natural sum + gaps fits, return natural. Else binary-search (or water-fill) the cap `c ≥ min_width` such that `Σ min(w, c)` fills the space exactly. If even all-at-`min_width` does not fit, take the largest prefix `k` that fits in `available - overflow_button - gap` at `min(w, min_width)` each and set `overflow_from = Some(k)`. `ms_until_next_minute` = `60_000 - (second*1000 + millis) + 50`. Strings: en `New window`, `Close`, `Close all windows`, `Settings…`, `Restart`, `Quit NewWin`; uk `Нове вікно`, `Закрити`, `Закрити всі вікна`, `Налаштування…`, `Перезапустити`, `Вийти з NewWin`. Match on the language subtag before `-`.
- [ ] **Step 4: Run** the same command. Expected: PASS (8 tests).
- [ ] **Step 5: Commit** `feat(core): label fitting, crash policy, clock math, strings`.

---

### Task 5: Config loading (`config`)

**Files:**
- Create: `crates/config/src/lib.rs` (replace stub), `crates/config/tests/config.rs`

**Interfaces:**
- Produces:
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)] #[serde(rename_all = "kebab-case")]
pub enum BarMode { Pinned, AutoHide }
pub struct BarConfig { pub mode: BarMode, pub show_delay_ms: u32, pub hide_delay_ms: u32, pub trigger_height_px: u8, pub ignore_fullscreen: bool }
pub struct Appearance { pub height: u32, pub margin: u32, pub corner_radius: f32, pub background_opacity: f32,
                        pub font_family: String, pub font_size: f32, pub date_format: String, pub show_seconds: bool }
pub struct Items { pub right: Vec<String> }
pub struct SpotlightConfig { pub win_key: bool, pub max_results: u32 }
pub struct SystemConfig { pub start_with_windows: bool }
pub struct Config { pub version: u32, pub bar: BarConfig, pub appearance: Appearance, pub items: Items,
                    pub spotlight: SpotlightConfig, pub system: SystemConfig }
#[derive(Debug)] pub enum ConfigError { Io(std::io::Error), Parse(String) }
impl Config { pub fn sanitized(self) -> Self; }
pub fn load(path: &Path) -> Result<Config, ConfigError>;                 // missing file → Ok(default)
pub fn default_path() -> PathBuf;                                       // %APPDATA%\NewWin\config.toml
```
All structs `#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)] #[serde(default)]`; `Default` impls carry exactly the defaults of spec §10.
- [ ] **Step 1: Write failing tests:**
```rust
#[test] fn missing_file_gives_defaults() {
    let c = load(Path::new("Z:/definitely/missing.toml")).unwrap();
    assert_eq!(c.bar.mode, BarMode::AutoHide); assert_eq!(c.bar.show_delay_ms, 150); assert_eq!(c.bar.hide_delay_ms, 400);
    assert_eq!(c.appearance.height, 30); assert_eq!(c.appearance.margin, 6); assert_eq!(c.appearance.corner_radius, 11.0);
    assert_eq!(c.appearance.font_family, "Segoe UI Variable Text"); assert_eq!(c.appearance.date_format, "system");
    assert_eq!(c.items.right, ["layout","network","volume","search","date","time"]);
}
#[test] fn partial_file_keeps_other_defaults_and_ignores_unknown() {
    let c = parse("[bar]\nmode = \"pinned\"\nfuture_key = 1\n").unwrap();
    assert_eq!(c.bar.mode, BarMode::Pinned); assert_eq!(c.bar.hide_delay_ms, 400);
}
#[test] fn invalid_toml_is_error() { assert!(matches!(parse("[bar\nmode="), Err(ConfigError::Parse(_)))); }
#[test] fn sanitize_clamps() {
    let c = parse("[bar]\ntrigger_height_px = 9\nshow_delay_ms = 99999\n[appearance]\nheight = 3\n").unwrap();
    assert_eq!(c.bar.trigger_height_px, 2); assert_eq!(c.bar.show_delay_ms, 2000); assert_eq!(c.appearance.height, 24);
}
```
(`parse(s)` = write `s` to a temp file under `std::env::temp_dir()` and `load` it.) Clamp ranges: `trigger_height_px` 1..=2, delays 0..=2000, `height` 24..=48, `margin` 0..=16, `corner_radius` 0..=16, `background_opacity` 0..=1, `font_size` 10..=18. `load` returns `sanitized()` values.
- [ ] **Step 2: Run** `cargo test -p newwin-config`. Expected: FAIL.
- [ ] **Step 3: Implement** with `serde` + `toml`; `default_path` via `std::env::var_os("APPDATA")`.
- [ ] **Step 4: Run** the same command. Expected: PASS (4 tests).
- [ ] **Step 5: Commit** `feat(config): load config.toml with defaults and clamping`.

---

### Task 6: Native taskbar hide/restore and logging (`platform`)

**Files:**
- Create: `crates/platform/src/taskbar.rs`, `crates/platform/src/log.rs`, `crates/platform/src/paths.rs`, `crates/platform/tests/taskbar.rs`
- Modify: `crates/platform/src/lib.rs`, `crates/platform/Cargo.toml` (windows features: `Win32_Foundation`, `Win32_UI_WindowsAndMessaging`, `Win32_UI_Shell`, `Win32_UI_Accessibility`, `Win32_System_Threading`)

**Interfaces:**
- Produces:
```rust
// paths.rs
pub fn local_data_dir() -> PathBuf;          // %LOCALAPPDATA%\NewWin (created on demand)
pub fn taskbar_state_file() -> PathBuf;      // local_data_dir()\taskbar-state.toml
// log.rs
pub fn init_logging() -> std::io::Result<()>; // `log` facade → newwin.log, rotate at 1 MB keep 3
// taskbar.rs
pub struct NativeTaskbar { /* saved ABM state */ }
impl NativeTaskbar {
    pub fn hide(state_file: &Path) -> windows::core::Result<Self>;
    pub fn reapply(&self) -> windows::core::Result<()>;   // after TaskbarCreated or the taskbar showing itself
    pub fn restore(self) -> windows::core::Result<()>;
}
pub fn restore_from_state_file(state_file: &Path) -> windows::core::Result<bool>; // true if something was restored
pub fn taskbar_created_message() -> u32;                 // RegisterWindowMessageW("TaskbarCreated")
pub fn watch_taskbar_show(notify: HWND, msg: u32) -> windows::core::Result<WinEventHookGuard>;
```
- [ ] **Step 1: Write an ignored integration test** `hide_then_restore_roundtrip` (`#[ignore]`, run manually): `hide` → assert `IsWindowVisible(Shell_TrayWnd) == false` and the state file exists → `restore` → assert visible again and the state file is gone. A second test `restore_from_missing_state_file_is_noop` (not ignored) asserts `Ok(false)` for a nonexistent path.
- [ ] **Step 2: Run** `cargo test -p newwin-platform --test taskbar`. Expected: FAIL to compile.
- [ ] **Step 3: Implement.** `hide`: find `Shell_TrayWnd` and every `Shell_SecondaryTrayWnd` → read `ABM_GETSTATE` → write `autohide_was_on = <bool>` to the state file **before** changing anything → `ABM_SETSTATE` with `ABS_AUTOHIDE` → `ShowWindow(SW_HIDE)`. `restore`: `ABM_SETSTATE` back to saved state → `ShowWindow(SW_SHOWNA)` → delete the file. `watch_taskbar_show`: `SetWinEventHook(EVENT_OBJECT_SHOW, …, WINEVENT_OUTOFCONTEXT)` on explorer's PID, filtered to `Shell_TrayWnd` / `OBJID_WINDOW`, posting `msg` to `notify`; the guard unhooks on drop.
- [ ] **Step 4: Run** `cargo test -p newwin-platform --test taskbar -- --include-ignored`. Expected: PASS (2 tests), with the taskbar visible again at the end.
- [ ] **Step 5: Commit** `feat(platform): hide/restore native taskbar, logging`.

---

### Task 7: Watchdog and process roles (`apps/shell`)

**Files:**
- Create: `apps/shell/src/watchdog.rs`, `apps/shell/src/roles.rs`
- Modify: `apps/shell/src/main.rs`

**Interfaces:**
- Consumes: `core::watchdog::{CrashPolicy, HangMonitor, ExitDecision, PingDecision}`, `platform::taskbar::restore_from_state_file`, `platform::paths::taskbar_state_file`, `platform::log::init_logging`.
- Produces: `pub const PING_WINDOW_CLASS: &str = "NewWin.Ping";` (the shell creates a hidden top-level window of this class in Task 9); `pub enum Role { Watchdog, Shell }`; `pub fn role_from_args(args: &[String]) -> Role`; `pub fn run_watchdog() -> i32`.

- [ ] **Step 1: Write a failing unit test** in `roles.rs`: `role_from_args(&[])` and `&["--watchdog"]` → `Watchdog`; `&["--shell"]` → `Shell`; unknown flags → `Watchdog`.
- [ ] **Step 2: Run** `cargo test -p newwin-shell`. Expected: FAIL.
- [ ] **Step 3: Implement.** `main`: init logging, install a panic hook and a `SetUnhandledExceptionFilter` handler (catches access violations in FFI code) that both call `restore_from_state_file` and log the cause, then dispatch by role. Watchdog: take the `Local\NewWin.Watchdog` mutex (exit 0 if already held); call `restore_from_state_file` once at startup (heals a previous hard kill); loop { `CreateProcessW(self, "--shell")`; wait on the process handle with a 5000 ms timeout; on timeout `FindWindowW(PING_WINDOW_CLASS)` + `SendMessageTimeoutW(WM_NULL, SMTO_ABORTIFHUNG, 2000)` → `HangMonitor::on_ping`; on `Kill`, `TerminateProcess(h, 1)`; on exit, `CrashPolicy::on_exit(code)` → `Restart` loops, `Stop` returns 0, `GiveUp` → `restore_from_state_file` + `MessageBoxW` "NewWin stopped after repeated crashes. The Windows taskbar has been restored." + return 1 }. Watchdog is a plain loop without windows, so it allocates nothing beyond the process handle.
- [ ] **Step 4: Run** `cargo test -p newwin-shell`. Expected: PASS.
- [ ] **Step 5: Manual check** with a temporary `--shell` stub that sleeps 3 s and exits with code 1: run `target\debug\NewWin.exe`; the log shows 3 starts, then GiveUp and the message box. Delete the stub behavior after the check (Task 9 replaces it).
- [ ] **Step 6: Commit** `feat(shell): watchdog with crash and hang policy`.

---

### Task 8: Spike — glass window backdrop and corners

**Files:**
- Create: `crates/ui/examples/glass_probe.rs`, `docs/superpowers/notes/2026-10-21-glass-probe.md`

**Interfaces:**
- Produces: a decision recorded in the notes file and used by Task 9: `Backdrop::SystemAcrylic` (DWM blur, DWM corners) or `Backdrop::Tint` (no blur, D2D-drawn rounded rect with exact radius 11 on a transparent `WS_EX_NOREDIRECTIONBITMAP` window).

- [ ] **Step 1: Build the probe.** A 600×30 `WS_POPUP` window with `WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_NOREDIRECTIONBITMAP`, a DirectComposition target, and a D2D rect filled with the tint base + white overlay. Variant A: `DwmSetWindowAttribute(DWMWA_SYSTEMBACKDROP_TYPE, DWMSBT_TRANSIENTWINDOW)`, `DWMWA_USE_IMMERSIVE_DARK_MODE = TRUE`, `DWMWA_WINDOW_CORNER_PREFERENCE = DWMWCP_ROUND`, `DwmExtendFrameIntoClientArea(-1)`. Variant B: no DWM attributes, rounded rect drawn by D2D, corners transparent. Command-line flag selects the variant.
- [ ] **Step 2: Run** `cargo run -p newwin-ui --example glass_probe -- a`, then `-- b`, over a white window and over the black desktop. Record in the notes file: does A blur, are its corners round, does it show a frame or shadow artefact, does text stay readable; is B crisp with transparent corners.
- [ ] **Step 3: Decide.** Prefer A only if blur and rounded corners both work without artefacts; otherwise B. Write the decision line `DECISION: SystemAcrylic` or `DECISION: Tint` in the notes file.
- [ ] **Step 4: Commit** `spike(ui): glass backdrop probe and decision`.

---

### Task 9: UI toolkit core and the static bar (`ui`, `apps/shell`)

**Files:**
- Create: `crates/ui/src/gfx.rs`, `crates/ui/src/layer.rs`, `crates/ui/src/paint.rs`, `crates/ui/src/text.rs`, `crates/ui/src/anim.rs`, `crates/ui/src/theme.rs`, `crates/platform/src/window.rs`, `crates/platform/src/dpi.rs`, `apps/shell/src/app.rs`, `apps/shell/src/bar.rs`
- Modify: `crates/ui/src/lib.rs`, `crates/platform/src/lib.rs`, `apps/shell/src/main.rs`, Cargo features (`Win32_Graphics_Direct3D`, `Win32_Graphics_Direct3D11`, `Win32_Graphics_Dxgi`, `Win32_Graphics_Dxgi_Common`, `Win32_Graphics_Direct2D`, `Win32_Graphics_Direct2D_Common`, `Win32_Graphics_DirectWrite`, `Win32_Graphics_DirectComposition`, `Win32_Graphics_Dwm`, `Win32_Graphics_Gdi`, `Win32_UI_HiDpi`, `Win32_System_Com`)

**Interfaces:**
- Consumes: Task 8 decision, `config::Config`, `platform::taskbar::*`.
- Produces:
```rust
// platform::window
pub trait Handler { fn on_message(&mut self, hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT>; }
pub struct WindowSpec { pub class: &'static str, pub ex_style: WINDOW_EX_STYLE, pub style: WINDOW_STYLE, pub rect: RECT, pub layered_alpha: Option<u8> }
pub fn create_window(spec: &WindowSpec, handler: Rc<RefCell<dyn Handler>>) -> windows::core::Result<HWND>;
pub fn run_message_loop() -> i32;
// platform::dpi
pub fn primary_monitor() -> (RECT /*monitor*/, u32 /*dpi*/);
pub fn scale(px: f32, dpi: u32) -> f32;
// ui
#[derive(Clone, Copy, Debug, PartialEq)] pub struct PointF { pub x: f32, pub y: f32 }
#[derive(Clone, Copy, Debug, PartialEq)] pub struct RectF { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }
#[derive(Clone, Copy, Debug, PartialEq)] pub struct Color { pub r: u8, pub g: u8, pub b: u8, pub a: f32 }
impl Color { pub const fn rgba(r: u8, g: u8, b: u8, a: f32) -> Self; }
pub struct Gfx { /* d3d, d2d device, dwrite factory, dcomp desktop device */ }
impl Gfx { pub fn new() -> windows::core::Result<Rc<Gfx>>; pub fn commit(&self) -> windows::core::Result<()>; }
pub enum Backdrop { SystemAcrylic, Tint }
pub struct Layer { /* hwnd target, root visual, virtual surface */ }
impl Layer {
    pub fn new(gfx: &Rc<Gfx>, hwnd: HWND, backdrop: Backdrop) -> windows::core::Result<Self>;
    pub fn resize(&mut self, w: u32, h: u32) -> windows::core::Result<()>;
    pub fn draw(&mut self, dirty: Option<RectF>, f: impl FnOnce(&mut Painter)) -> windows::core::Result<()>;
    pub fn visual(&self) -> &IDCompositionVisual2;
}
pub struct Painter<'a> { /* d2d device context, brushes cache */ }
impl Painter<'_> {
    pub fn clear(&mut self);
    pub fn fill_round_rect(&mut self, r: RectF, radius: f32, c: Color);
    pub fn stroke_round_rect(&mut self, r: RectF, radius: f32, c: Color, width: f32);
    pub fn line(&mut self, a: PointF, b: PointF, c: Color, width: f32);
    pub fn text(&mut self, t: &TextLayout, origin: PointF, c: Color);
}
pub struct TextStyle { pub family: String, pub size: f32, pub weight: u16 }
pub struct TextCache { /* IDWriteTextFormat per style */ }
impl TextCache { pub fn layout(&mut self, s: &str, style: &TextStyle, max_width: f32) -> TextLayout; } // ellipsis trimming
impl TextLayout { pub fn width(&self) -> f32; pub fn height(&self) -> f32; }
pub fn slide_y(gfx: &Gfx, v: &IDCompositionVisual2, from: f32, to: f32, ms: u32) -> windows::core::Result<()>;
pub fn fade(gfx: &Gfx, v: &IDCompositionVisual2, from: f32, to: f32, ms: u32) -> windows::core::Result<()>;
pub mod theme { /* every color/size constant from Global Constraints as `pub const` */ }
```
- [ ] **Step 1: Implement `platform::window`** (class registration once per class name; handler stored in `GWLP_USERDATA`; `DefWindowProcW` when the handler returns `None`) and `platform::dpi`.
- [ ] **Step 2: Implement `ui`** per the interfaces. One `Gfx` per process (D3D11 `BGRA_SUPPORT`, D2D device context, `IDCompositionDesktopDevice`). `Layer` uses `IDCompositionVirtualSurface` with premultiplied alpha. Text antialias mode grayscale. On `D2DERR_RECREATE_TARGET` or `DXGI_ERROR_DEVICE_REMOVED`, `Gfx` rebuilds devices and every `Layer` recreates its surface on the next `draw`. `slide_y`/`fade` build an `IDCompositionAnimation` with a cubic ease-out approximation (`AddCubic`) and call `Gfx::commit`.
- [ ] **Step 3: Implement the shell skeleton** in `app.rs` (`struct App` owning `Gfx`, `Config`, `NativeTaskbar`, bar state) and `bar.rs` (bar window: `WS_POPUP`, `WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_NOREDIRECTIONBITMAP`, rect = monitor width minus 2×margin, at y = margin, height = height, all DPI-scaled). It paints the background, a sample left label and a sample right label. Shell startup order: take the `Local\NewWin.Shell` mutex → `restore_from_state_file` → load config → `NativeTaskbar::hide` → create the hidden `NewWin.Ping` top-level window → create the bar → first `commit` → log `first-frame-ms=<ms since process creation>` (via `GetProcessTimes`). Handle `TaskbarCreated` (→ `reapply`), the show-watch message (→ `reapply`), `WM_DPICHANGED` and `WM_DISPLAYCHANGE` (→ reposition + resize + redraw). On `WM_ENDSESSION` and on quit → `NativeTaskbar::restore`.
- [ ] **Step 4: Verify.** `cargo build --release`; run `target\release\NewWin.exe`. Expected: the native taskbar disappears, a crisp glass bar appears 6 px from the top, `first-frame-ms` < 300 in the log. Restart explorer (`taskkill /f /im explorer.exe; start explorer`): the taskbar is hidden again within 1 s. Change display scale 100% → 150%: the bar stays crisp and repositioned. End `NewWin.exe --shell` in Task Manager: the watchdog restarts it. End both processes: the next `NewWin.exe` launch first restores, then re-hides (the log shows both).
- [ ] **Step 5: Commit** `feat(ui,shell): rendering toolkit and static glass bar`.

---

### Task 10: Window watching and app names (`platform`)

**Files:**
- Create: `crates/platform/src/windows_watch.rs`, `crates/platform/src/appinfo.rs`, `crates/platform/src/worker.rs`
- Modify: `crates/platform/src/lib.rs`, features (`Win32_UI_Shell_PropertiesSystem`, `Win32_Storage_EnhancedStorage`, `Win32_Storage_FileSystem`, `Win32_System_ProcessStatus`, `Win32_Storage_Packaging_Appx`)

**Interfaces:**
- Consumes: `core::apps::{WindowInfo, GroupKey, Hwnd}`.
- Produces:
```rust
pub enum WindowEvent { Changed(Hwnd), Destroyed(Hwnd), Activated(Option<Hwnd>) }
pub struct WindowWatcher { /* shell hook registration + WinEvent hook guards */ }
impl WindowWatcher {
    pub fn start(notify: HWND) -> windows::core::Result<Self>;
    pub fn translate(&self, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<WindowEvent>; // call from notify's wndproc
}
pub fn enumerate_top_level() -> Vec<Hwnd>;
pub fn describe(hwnd: Hwnd, names: &mut AppNames) -> Option<WindowInfo>;  // None if the window is gone
pub struct AppNames { /* GroupKey → name cache */ }
impl AppNames {
    pub fn new(worker: Worker) -> Self;
    pub fn name_or_placeholder(&mut self, key: &GroupKey) -> String;  // placeholder = exe stem / AUMID tail; queues a resolve
}
pub struct Worker; impl Worker { pub fn spawn(notify: HWND, msg: u32) -> Self; }
pub struct ResolvedName { pub key: GroupKey, pub name: String }   // boxed in LPARAM of `msg`
pub unsafe fn take_resolved(lp: LPARAM) -> Box<ResolvedName>;
```
- [ ] **Step 1: Implement `describe`.** Fields come from `IsWindowVisible`, `DwmGetWindowAttribute(DWMWA_CLOAKED)`, `GetWindow(GW_OWNER)`, `GetWindowLongPtrW(GWL_EXSTYLE)`, `IsIconic`, `GetWindowTextW`, and own PID comparison. Group key: `SHGetPropertyStoreForWindow` → `PKEY_AppUserModel_ID`; else the process AUMID via `GetApplicationUserModelId`. For `ApplicationFrameHost.exe` windows, use the process of the child `Windows.UI.Core.CoreWindow`. Otherwise `GroupKey::exe(QueryFullProcessImageNameW)` with `PROCESS_QUERY_LIMITED_INFORMATION` (works for elevated processes).
- [ ] **Step 2: Implement name resolution on the worker thread.** For `AppId` keys, read the display name of `shell:AppsFolder\<AUMID>` (`SHCreateItemInKnownFolder(FOLDERID_AppsFolder)` → `GetDisplayName(SIGDN_NORMALDISPLAY)`). For `ExePath` keys, read `FileDescription` via `GetFileVersionInfoW`/`VerQueryValueW` (first translation). Fallback: file stem. The worker initializes COM (STA) on its own thread and posts `Box<ResolvedName>` via `PostMessageW`.
- [ ] **Step 3: Implement `WindowWatcher`.** `RegisterShellHookWindow(notify)` + `RegisterWindowMessageW("SHELLHOOK")`: `HSHELL_WINDOWCREATED`/`REDRAW` → `Changed`, `HSHELL_WINDOWDESTROYED` → `Destroyed`, `HSHELL_WINDOWACTIVATED`/`HSHELL_RUDEAPPACTIVATED` → `Activated`. `SetWinEventHook` out-of-context for `EVENT_OBJECT_NAMECHANGE`, `EVENT_OBJECT_SHOW`/`HIDE`, `EVENT_OBJECT_CLOAKED`/`UNCLOAKED`, `EVENT_SYSTEM_MINIMIZESTART`/`END`, `EVENT_SYSTEM_FOREGROUND`, filtered to `idObject == OBJID_WINDOW && idChild == CHILDID_SELF` and top-level windows, re-posted to `notify` as `Changed`/`Activated`.
- [ ] **Step 4: Verify with an example** `crates/platform/examples/watch.rs` that prints `groups()` of an `AppModel` on each change. Run it, then open and close Notepad and Chrome (2 windows), Calculator (UWP), an elevated PowerShell, and switch to another virtual desktop. Expected: Notepad, Chrome (2 windows, name "Google Chrome"), Calculator (name "Calculator", not "ApplicationFrameHost"), and the elevated PowerShell appear and disappear correctly. Windows on the other desktop disappear while it is not current. Windows already open at start are listed.
- [ ] **Step 5: Commit** `feat(platform): window watcher and app name resolution`.

---

### Task 11: Visibility wiring: auto-hide, pinned AppBar, fullscreen (`platform`, `apps/shell`)

**Files:**
- Create: `crates/platform/src/appbar.rs`, `crates/platform/src/fullscreen.rs`, `apps/shell/src/catcher.rs`, `apps/shell/src/visibility.rs`
- Modify: `apps/shell/src/app.rs`, `apps/shell/src/bar.rs`

**Interfaces:**
- Consumes: `core::visibility::*`, `ui::{slide_y, fade}`, `config::BarConfig`.
- Produces:
```rust
// platform::appbar
pub struct AppBar { /* hwnd, callback msg */ }
impl AppBar {
    pub fn register(hwnd: HWND, callback_msg: u32) -> windows::core::Result<Self>;
    pub fn set_top(&self, monitor: RECT, thickness_px: i32) -> RECT;   // ABM_QUERYPOS + ABM_SETPOS, returns granted rect
}
impl Drop for AppBar { /* ABM_REMOVE */ }
pub enum AppBarNotify { PosChanged, FullScreen(bool), Other }
pub fn parse_appbar_notify(wp: WPARAM, lp: LPARAM) -> AppBarNotify;
// platform::fullscreen
pub fn foreground_is_fullscreen(monitor: RECT) -> bool;   // SHQueryUserNotificationState busy/D3D/presentation, or foreground rect covers monitor (ignoring Progman/WorkerW)
```
- [ ] **Step 1: Implement the catcher window** (`catcher.rs`): `WS_POPUP`, `WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_NOACTIVATE`, `SetLayeredWindowAttributes(alpha = 1)`, z-ordered just below the bar. `Catcher::Strip` = full width × `trigger_height_px`; `Zone` = full width × (margin + height); `Off` = hidden.
- [ ] **Step 2: Implement zone tracking** in `visibility.rs`. `WM_MOUSEMOVE` on the catcher or the bar arms `TrackMouseEvent(TME_LEAVE)` and sends `ZoneEnter` once. `WM_MOUSELEAVE` from either window sends `ZoneLeave` only if `GetCursorPos` is outside the zone rect. Execute `VisCmd`s: `StartTimer` = `SetTimer(id 1, ms)` (one-shot: kill on fire), `CancelTimer` = `KillTimer`, `AnimateIn` = `slide_y(-(margin+height) → 0, 150)` + `fade(0 → 1, 150)` after showing the bar window, `AnimateOut` = reverse with 120 ms, then hide the window, `SetCatcher` = resize/hide the catcher.
- [ ] **Step 3: Implement pinned mode.** `Mode::Pinned` → `AppBar::register` + `set_top(monitor, margin + height)`; re-call `set_top` on `PosChanged`, `WM_DISPLAYCHANGE`, `WM_DPICHANGED`. `FullScreen(true)` → `SetWindowPos(HWND_NOTOPMOST)` + `HWND_BOTTOM`; `false` → back to `HWND_TOPMOST`. Auto-hide mode registers no AppBar.
- [ ] **Step 4: Fullscreen in auto-hide.** On `Activated` window events and `EVENT_SYSTEM_MOVESIZEEND`, compute `foreground_is_fullscreen` and feed `VisInput::Fullscreen` only when the value changes.
- [ ] **Step 5: Verify.** Auto-hide (default): no pixels are visible at the top; a maximized window fills 100% of the screen. Hovering the top edge for 150 ms slides the bar in. Moving down through the 6 px gap does not hide it; leaving below hides it after 400 ms. Fast jiggling on the edge leaves no stuck state. In a fullscreen YouTube video or game, the edge does nothing. Pinned (`mode = "pinned"` in config, restart): a maximized window starts below the bar (y = 42 at 100%). A fullscreen video covers the bar.
- [ ] **Step 6: Commit** `feat(shell): auto-hide, pinned appbar, fullscreen handling`.

---

### Task 12: Running apps on the bar, popovers, clock, bar menu (`apps/shell`, `ui`, `platform`)

**Files:**
- Create: `crates/ui/src/popup.rs`, `crates/platform/src/activate.rs`, `crates/platform/src/clock.rs`, `apps/shell/src/apps_view.rs`, `apps/shell/src/clock_view.rs`, `apps/shell/src/menus.rs`
- Modify: `apps/shell/src/app.rs`, `apps/shell/src/bar.rs`, features (`Win32_Globalization`, `Win32_System_SystemInformation`, `Win32_System_Power`)

**Interfaces:**
- Consumes: `core::apps::*`, `core::layout::fit_labels`, `core::strings::strings_for`, `core::clock::ms_until_next_minute`, `platform::windows_watch::*`, `ui::*`.
- Produces:
```rust
// ui::popup
pub struct MenuItem { pub label: String, pub enabled: bool, pub bold: bool }
pub struct Popup { /* glass window, item list */ }
impl Popup {
    pub fn show(gfx: &Rc<Gfx>, items: Vec<MenuItem>, anchor: PointF /* top-left, screen px */, on_pick: Box<dyn FnOnce(usize)>, on_close: Box<dyn FnOnce()>) -> windows::core::Result<Self>;
    pub fn close(self);
}
// platform::activate
pub fn activate(hwnd: Hwnd); pub fn minimize(hwnd: Hwnd); pub fn restore(hwnd: Hwnd); pub fn close(hwnd: Hwnd);
pub fn launch_new_instance(key: &GroupKey);   // ShellExecuteExW exe path, or shell:AppsFolder\<AUMID>
// platform::clock
pub fn format_bar_date(date_format: &str) -> String;   // "system" → GetDateFormatEx(LOCALE_NAME_USER_DEFAULT, "ddd MMM d"); else the pattern
pub fn format_bar_time(show_seconds: bool) -> String;  // GetTimeFormatEx(LOCALE_NAME_USER_DEFAULT, TIME_NOSECONDS unless show_seconds)
pub fn local_second_and_millis() -> (u16, u16);
```
- [ ] **Step 1: Write a failing test** for `platform::clock`: with any locale, `format_bar_time(false)` contains no seconds (`assert!(s.matches(':').count() <= 1)`) and `format_bar_date("yyyy")` returns the current year as 4 digits. Run `cargo test -p newwin-platform clock`, expect FAIL; implement; expect PASS.
- [ ] **Step 2: Implement `apps_view.rs`.** Feed the `WindowWatcher` events and `ResolvedName` messages into `AppModel` (initial snapshot = `enumerate_top_level` + `describe`). On every change returning `true`, rebuild label layouts (`TextCache`, weight 600 for the active group, 400 otherwise) → `fit_labels(natural, left_area_width, 40.0, 2.0, 24.0)` → pill rects (padding 9 px horizontal, height 22, radius 7) → repaint only the left area. Hover pill on `WM_MOUSEMOVE` over an item. Overflow `»` item opens a `Popup` with the remaining group names.
- [ ] **Step 3: Implement clicks.** Left → `core::apps::left_click`; middle → `middle_click`; execute `ClickAction` via `platform::activate` **synchronously inside the click handler** (foreground rights). `ShowWindowList(g)` → `Popup` with window titles (empty title → group name) under the item; pick → `activate`. Right-click on an item → `Popup` [`New window`, `Close` (1 window) or `Close all windows` (more)]. Right-click on empty bar space → [`Settings…` (disabled until M4), `Restart` (exit 3), `Quit NewWin` (restore taskbar, exit 0)]. Every popup sends `OverlayOpened`/`OverlayClosed` to the visibility machine. Popups close on `WM_ACTIVATE(WA_INACTIVE)` and `Esc`.
- [ ] **Step 4: Implement `clock_view.rs`.** Right area: date (`#ececec`, weight 400), a 2 px gap, then time (`#ffffff`, weight 600), separator before them per spec §2. Use a one-shot `SetTimer(id 2, ms_until_next_minute(...))` re-armed on fire. Refresh and re-arm on `WM_TIMECHANGE`, `WM_POWERBROADCAST(PBT_APMRESUMEAUTOMATIC)`, and `WM_SETTINGCHANGE` with `lParam == "intl"`. These are delivered to top-level windows only, so handle them in the bar window, not a message-only window.
- [ ] **Step 5: Verify.** Open 3 Chrome windows + VS Code + Explorer. Expected: `Google Chrome`, `Visual Studio Code`, `File Explorer` labels in launch order, active one bold in a pill. Clicking the active app minimizes it; clicking again restores it. Chrome opens a list of 3 titles. Middle-click on Explorer closes it. Open 30 apps: labels shrink, then `»` appears. Right-click empty space → Quit restores the native taskbar and both processes exit. Date shows like `Wed Oct 21`, time like `2:32 PM` (en-US). Switch Windows display language or region format → the bar updates without restart. Sleep and wake → time correct within 1 s.
- [ ] **Step 6: Commit** `feat(shell): running apps, popovers, clock, bar menu`.

---

### Task 13: Performance measurement and milestone acceptance

**Files:**
- Create: `scripts/measure.ps1`, `docs/superpowers/notes/2026-10-21-m1-acceptance.md`

**Interfaces:**
- Consumes: the release build, the `first-frame-ms` log line.

- [ ] **Step 1: Write `scripts/measure.ps1`** with parameters `-Minutes 10`. Every 5 s it samples the `\Process(NewWin*)\% Processor Time` and `\Process(NewWin*)\Working Set - Private` counters for both processes. It prints the average CPU % per process, max private working set in MB, and `PASS`/`FAIL` against: shell CPU avg ≤ 0.05%, shell private WS < 20 MB, watchdog private WS < 2 MB. It also reads the last `first-frame-ms` from the log and checks `< 300`.
- [ ] **Step 2: Run** `cargo build --release`, start `target\release\NewWin.exe`, leave the PC idle, then run `powershell -File scripts/measure.ps1 -Minutes 10`. Expected: all PASS. If CPU fails, record a WPR trace (`wpr -start CPU; … ; wpr -stop m1.etl`) and find the wakeup source before continuing.
- [ ] **Step 3: Run the acceptance checklist** and record each result (pass/fail + note) in the acceptance notes file: the Review Focus items 1–5; Task 9, 11 and 12 verification steps; logging off and on with NewWin running (native taskbar visible after logon if NewWin is not started; hidden once it is); uninstall path not applicable (M5).
- [ ] **Step 4: Commit** `test: M1 performance script and acceptance results`.
