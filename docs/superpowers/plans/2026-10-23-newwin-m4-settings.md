# M4: Settings window — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `NewWinSettings.exe` — a normal glass window with six sections (Bar · Appearance · Items · Spotlight · Tray · About) that writes `config.toml`; the shell watches the file and applies every change live, without a restart.

**Architecture:** `config.toml` is the only channel. `newwin-config` gets the new fields, atomic save, serialization and a pure `diff::changes(old, new)`. `newwin-platform` gets a directory watch (thread-pool wait, same pattern as `AppsWatch`) and the Run-key helper. The shell reloads on change (50 ms debounce) and runs only the actions `changes` returns. `newwin-ui` gets the controls (toggle, segmented, range slider, text field, reorder list, scroll, nav) and a top-level glass `AppWindow`. `apps/settings` assembles the sections.

**Tech Stack:** Rust 2024, `windows` 0.62 (Win32, DirectWrite/Direct2D/DirectComposition, DXGI duplication for glass), `serde` + `toml`.

**Spec:** `docs/superpowers/specs/2026-10-23-newwin-m4-settings-design.md` (extends `docs/superpowers/specs/2026-10-21-newwin-design.md` §2, §10 and `docs/superpowers/specs/2026-10-22-newwin-liquid-glass-design.md`).

## Global Constraints

- Work on a new branch `m4-settings` from `main` (`a27a4dc` has the spec).
- Build tools: `$env:Path="$env:USERPROFILE\.cargo\bin;$env:Path"`. Stop NewWin before release builds: `Get-Process NewWin, NewWinSettings -ErrorAction SilentlyContinue | Stop-Process -Force`, then `.\target\release\NewWin.exe --restore-taskbar`.
- `cargo test --workspace` and `cargo clippy --workspace --all-targets` stay green after every task. TDD: every behaviour test is written and seen failing first.
- Release builds use `panic = "abort"`: nothing read from files (config, `tray_seen.toml`, `usage.toml`), the font collection or other processes may panic. `Option`/`Result`, no unchecked indexing or string slicing.
- No polling, no new periodic wakeups in the shell. New shell message: `WM_APP + 19` config dir changed (watch → ping); `WM_APP + 20` quit request (Settings → the shell's `NewWin.Ping` window, same as *Quit NewWin*). New bar timer: `CONFIG_DEBOUNCE_TIMER = 11`, one-shot 50 ms. `WM_APP + 1…18` and bar timers 1–10 are taken.
- Config bounds (spec §3, also enforced by `Config::sanitized`): `show_delay_ms` 0–1000, `hide_delay_ms` 0–2000, `trigger_height_px` 1–2, `height` 28–48, `margin` 0–16, `corner_radius` 0–24, `font_size` 10–18, `max_results` 3–15. Sliders step 10 ms for delays, 1 for the rest.
- Item names in `items.right` / `items.hidden`: `layout`, `network`, `volume`, `search`, `date`, `time`. Tray keys: exe file names, compared case-insensitively.
- Files: `%APPDATA%\NewWin\config.toml`, `usage.toml`, `tray_seen.toml` (≤ 200 entries). Config header line: `# NewWin settings. NewWinSettings.exe rewrites this file.`
- Names: mutex `Local\NewWin.Settings`, window class `NewWin.Settings`; Run value `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` → `NewWin` = `"<full path>\NewWin.exe"`.
- Settings window: 760×560 logical px, fixed size, own caption (`NewWin`, `–`, `×`), glass via its own `Backdrops`. Look: monochrome, animations 120–180 ms ease-out (toggle 150 ms).
- Strings through `newwin_core::strings::Strings` (`en` and `uk`); copy for banners and buttons exactly as in spec §3.
- Budget (spec §6–7): shell idle private WS ≤ 10.7 MB with Settings closed, CPU ≈ 0 %; slider move → bar frame < 100 ms; Settings first frame < 200 ms.
- Deviation from spec §4.2, by design: `changes` lives in `newwin-config` (`crates/config/src/diff.rs`), not `newwin-core` — the config crate is already pure (no Win32) and owns the types.
- Never press Shut down / Restart / Sign out / Sleep. Glass windows are invisible to DXGI screenshots — the user checks looks.
- Commit messages through a file (`git commit -F`), ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **A hand-edited `config.toml` with a syntax error, a wrong type or a value out of range** → the shell keeps the last good config (error) or clamps (range); never panics, never rewrites the file. Tests in Task 1 (`out_of_range_values_are_clamped_to_the_m4_bounds`) and Task 4 (`a_broken_file_keeps_the_last_good_config`).
2. **The file is deleted or replaced while NewWin runs** (Notepad "save as", cleaners) → defaults apply, the watch keeps working (a rename over the file is seen). Test in Task 3 (`a_rename_over_the_file_is_seen`) and Task 4 (`a_deleted_file_means_defaults`).
3. **Unknown or duplicated names in `items.right` / `items.hidden` / `tray.order`** (`["time", "time", "clock"]`) → unknown dropped, duplicates kept once, missing known items appended in default order. Test in Task 1 (`item_lists_are_normalized`).
4. **A slider dragged fast** (dozens of writes per second) → Settings writes at most once per 100 ms and the last value always lands; the shell applies the last file state. Tests in Task 9 (`debounce_writes_the_last_value_once`) and Task 4 (`debounce_restarts_on_each_event` via the timer id).
5. **Settings' own write coming back as a file event** → it does not show a banner or reload its controls under the user's finger. Test in Task 9 (`own_write_is_not_a_change`).

---

### Task 1: Config fields, bounds, save (`newwin-config`)

**Files:**
- Modify: `crates/config/src/lib.rs`, `crates/config/src/usage.rs` (move `save_atomic` to `lib.rs`, re-export from `usage` for existing callers)
- Test: `crates/config/tests/config.rs` (create if absent; existing tests stay where they are)

**Interfaces:**
- Produces: `Items { right: Vec<String>, hidden: Vec<String> }`; `TrayConfig { order: Vec<String>, hidden: Vec<String> }` and `Config.tray: TrayConfig` (`[tray]`); `pub const ITEM_NAMES: [&str; 6]`; `Config::sanitized` with the M4 bounds and list normalization; `pub fn to_toml(c: &Config) -> String` (header line + pretty TOML); `pub fn save_atomic(path: &Path, text: &str) -> std::io::Result<()>`; `pub fn tray_seen_path() -> PathBuf`; `ConfigError::Parse { line: Option<usize>, message: String }` (line 1-based, from `toml::de::Error::span` mapped to a line).

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn out_of_range_values_are_clamped_to_the_m4_bounds() {
    let c: Config = toml::from_str("[bar]\nshow_delay_ms = 5000\n[appearance]\nheight = 20\n[spotlight]\nmax_results = 99").unwrap();
    let c = c.sanitized();
    assert_eq!((c.bar.show_delay_ms, c.appearance.height, c.spotlight.max_results), (1000, 28, 15));
    let c: Config = toml::from_str("[spotlight]\nmax_results = 0").unwrap();
    assert_eq!(c.sanitized().spotlight.max_results, 3);
}

#[test]
fn item_lists_are_normalized() {
    let c: Config = toml::from_str("[items]\nright = [\"time\", \"time\", \"clock\", \"date\"]\nhidden = [\"clock\", \"date\"]").unwrap();
    let c = c.sanitized();
    assert_eq!(c.items.right, ["time", "date", "layout", "network", "volume", "search"]);
    assert_eq!(c.items.hidden, ["date"]);
}

#[test]
fn tray_lists_drop_duplicates_ignoring_case() {
    let c: Config = toml::from_str("[tray]\norder = [\"Discord.exe\", \"discord.EXE\", \"\"]\nhidden = [\"A.exe\"]").unwrap();
    assert_eq!(c.sanitized().tray.order, ["Discord.exe"]);
}

#[test]
fn round_trip_keeps_every_field() {
    let mut c = Config::default();
    c.items.hidden = vec!["search".into()];
    c.tray.hidden = vec!["MSI Afterburner.exe".into()];
    let text = to_toml(&c);
    assert!(text.starts_with("# NewWin settings. NewWinSettings.exe rewrites this file.\n"));
    assert_eq!(toml::from_str::<Config>(&text).unwrap().sanitized(), c);
}

#[test]
fn a_parse_error_names_its_line() {
    let dir = tempdir_in_target("parse_line");
    let p = dir.join("config.toml");
    std::fs::write(&p, "version = 1\n[bar]\nmode = \n").unwrap();
    match load(&p) { Err(ConfigError::Parse { line, .. }) => assert_eq!(line, Some(3)), r => panic!("{r:?}") }
}

#[test]
fn save_atomic_replaces_the_file_and_leaves_no_temp() {
    let dir = tempdir_in_target("save_atomic");
    let p = dir.join("config.toml");
    save_atomic(&p, "a").unwrap();
    save_atomic(&p, "b").unwrap();
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "b");
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
}
```
(`tempdir_in_target(name)` = a test helper creating `target/test-tmp/<name>-<pid>`, fresh each run.)

- [ ] **Step 2: Run** `cargo test -p newwin-config` — Expected: FAIL (missing fields/functions).
- [ ] **Step 3: Implement.** Normalization: keep the first occurrence of each known name (exact, lowercase names), drop unknown, append missing `ITEM_NAMES` in default order; `hidden` keeps only names present in `right`. Tray lists: trim, drop empty, dedupe case-insensitively keeping the first spelling. `write_default_if_missing` uses `to_toml`. `default_path` unchanged.
- [ ] **Step 4: Run** `cargo test -p newwin-config` and `cargo test --workspace` — Expected: PASS.
- [ ] **Step 5: Commit** `feat: M4 config fields, bounds, atomic save`.

---

### Task 2: What changed (`newwin-config::diff`)

**Files:**
- Create: `crates/config/src/diff.rs`; Modify: `crates/config/src/lib.rs` (`pub mod diff;`)
- Test: `crates/config/tests/diff.rs`

**Interfaces:**
- Produces:
```rust
pub struct Changes {
    pub geometry: bool,      // appearance.height | margin | corner_radius
    pub visibility: bool,    // bar.mode | show/hide delays | trigger_height_px | ignore_fullscreen
    pub mode: bool,          // bar.mode alone (appbar register/unregister)
    pub fonts: bool,         // appearance.font_family | font_size
    pub glass: bool,         // appearance.glass
    pub clock: bool,         // appearance.date_format | show_seconds
    pub items: bool,         // items.right | items.hidden
    pub win_key: bool,       // spotlight.win_key
    pub spotlight: bool,     // spotlight.max_results (applies on next open; nothing to do now)
    pub tray: bool,          // tray.order | tray.hidden
    pub autostart: bool,     // system.start_with_windows
}
pub fn changes(old: &Config, new: &Config) -> Changes;   // #[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
impl Changes { pub fn any(&self) -> bool; }
```

- [ ] **Step 1: Write the failing tests** — one per row of spec §4.2, each flipping one field from `Config::default()` and asserting the exact `Changes` (all other flags false), e.g.:

```rust
#[test]
fn mode_change_is_mode_and_visibility() {
    let mut new = Config::default();
    new.bar.mode = BarMode::Pinned;
    assert_eq!(changes(&Config::default(), &new), Changes { mode: true, visibility: true, ..Default::default() });
}
#[test]
fn identical_configs_change_nothing() { assert!(!changes(&Config::default(), &Config::default()).any()); }
```
Plus `height_is_geometry`, `font_size_is_fonts`, `glass_is_glass`, `seconds_is_clock`, `hidden_item_is_items`, `win_key_is_win_key`, `max_results_is_spotlight`, `tray_hidden_is_tray`, `autostart_is_autostart`, `delay_is_visibility_only`.
- [ ] **Step 2: Run** `cargo test -p newwin-config --test diff` — Expected: FAIL.
- [ ] **Step 3: Implement** field-by-field comparisons; `version` is ignored.
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** `feat: config diff for live apply`.

---

### Task 3: Directory watch and Run key (`newwin-platform`)

**Files:**
- Create: `crates/platform/src/dir_watch.rs`, `crates/platform/src/autostart.rs`; Modify: `crates/platform/src/lib.rs`, `crates/platform/src/apps_index.rs` (`AppsWatch` reuses the `Wait` plumbing from `dir_watch` instead of its own copy)
- Test: `crates/platform/tests/dir_watch.rs`, `crates/platform/tests/autostart.rs`

**Interfaces:**
- Produces: `pub struct DirWatch` with `pub fn start(dir: &Path, notify: HWND, msg: u32) -> windows::core::Result<DirWatch>` (filter `FILE_NOTIFY_CHANGE_LAST_WRITE | FILE_NOTIFY_CHANGE_FILE_NAME`, not recursive; posts `msg` with `WPARAM(0)`; `Drop` unregisters the wait and closes the handle, waiting for a running callback); `pub fn autostart_command() -> Option<String>` (quoted current exe path); `pub fn autostart_get() -> Option<String>`; `pub fn autostart_set(on: bool) -> windows::core::Result<()>` (writes only when the value differs; `false` deletes the value, missing value is `Ok`). Tests use a value name override: `autostart_set_named(name: &str, on: bool, command: &str)` / `autostart_get_named(name)`, which the public pair wraps with `"NewWin"`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_write_in_the_dir_posts_the_message() { /* create dir, start watch to a test window, write a file, pump ≤ 2 s, expect msg */ }
#[test]
fn a_rename_over_the_file_is_seen() { /* save_atomic over an existing file → msg */ }
#[test]
fn dropping_the_watch_stops_messages() { /* drop, write, pump 300 ms, expect none */ }
#[test]
fn autostart_value_is_written_once_and_removed() {
    autostart_set_named("NewWinTest", true, r#""C:\x\NewWin.exe""#).unwrap();
    assert_eq!(autostart_get_named("NewWinTest").as_deref(), Some(r#""C:\x\NewWin.exe""#));
    autostart_set_named("NewWinTest", false, "").unwrap();
    assert_eq!(autostart_get_named("NewWinTest"), None);
    autostart_set_named("NewWinTest", false, "").unwrap(); // already absent: Ok
}
```
- [ ] **Step 2: Run** `cargo test -p newwin-platform --test dir_watch --test autostart` — Expected: FAIL.
- [ ] **Step 3: Implement** (`FindFirstChangeNotificationW` + `RegisterWaitForSingleObject(WT_EXECUTEDEFAULT)`, callback posts and calls `FindNextChangeNotification`, as `apps_index.rs` does today; registry via `RegSetKeyValueW` / `RegDeleteKeyValueW` / `RegGetValueW` on `HKEY_CURRENT_USER`).
- [ ] **Step 4: Run** the two tests and `cargo test -p newwin-platform --test apps_index` — Expected: PASS.
- [ ] **Step 5: Commit** `feat: directory watch and Run-key helpers`.

---

### Task 4: Live config in the shell

**Files:**
- Create: `apps/shell/src/app/config_view.rs`; Modify: `apps/shell/src/app.rs` (field `config_watch: Option<DirWatch>`, start after the bar is up, route `WM_APP_CONFIG` and `CONFIG_DEBOUNCE_TIMER`), `apps/shell/src/app/visibility.rs`, `apps/shell/src/app/right_view.rs`, `apps/shell/src/app/clock_view.rs`, `crates/core/src/visibility.rs`, `crates/platform/src/input_hook.rs`
- Test: `crates/core/tests/visibility.rs`, `crates/platform/tests/keyboard.rs` (existing files), `apps/shell/src/app/config_view.rs` (unit tests of the pure helpers)

**Interfaces:**
- Consumes: `newwin_config::{load, diff::changes, Changes}`, `DirWatch` (Task 3), `autostart_set` (Task 3).
- Produces: `Visibility::set_config(&mut self, cfg: VisConfig) -> Vec<VisCmd>` (keeps state; mode switch goes through the existing `set_mode`); `InputHook::set_win_key(&self, on: bool)` (atomic read by the hook thread); `App::apply_config(&mut self, new: Config)`; `pub(super) const WM_APP_CONFIG: u32 = WM_APP + 19`; `App::reset_usage_if_deleted()`.

- [ ] **Step 1: Write the failing tests**

```rust
// crates/core/tests/visibility.rs
#[test]
fn new_delays_apply_to_the_next_timer() { /* AutoHide, set_config(show 0) → ZoneEnter yields StartTimer(0) */ }
#[test]
fn switching_to_pinned_shows_and_stops_the_catcher() { /* set_config(mode Pinned) → [SetCatcher(Off), AnimateIn] */ }

// crates/platform/tests/keyboard.rs
#[test]
fn win_key_can_be_turned_off_live() { /* start(win_key true), set_win_key(false), real untagged Win tap → no spotlight msg (same pattern as win_taps_post_spotlight_but_tagged_ones_do_not) */ }

// apps/shell/src/app/config_view.rs (pure helper)
#[test]
fn a_broken_file_keeps_the_last_good_config() { assert_eq!(next_config(&good, Err(parse_err)), None); }
#[test]
fn a_deleted_file_means_defaults() { assert_eq!(next_config(&good, Ok(Config::default())), Some(Config::default())); }
#[test]
fn an_unchanged_file_does_nothing() { assert_eq!(next_config(&good, Ok(good.clone())), None); }
#[test]
fn debounce_restarts_on_each_event() { assert_eq!(CONFIG_DEBOUNCE_TIMER, 11); assert_eq!(CONFIG_DEBOUNCE_MS, 50); }
```
`fn next_config(current: &Config, loaded: Result<Config, ConfigError>) -> Option<Config>` — `None` means "nothing to apply" (error logged by the caller).
- [ ] **Step 2: Run** the three test targets — Expected: FAIL.
- [ ] **Step 3: Implement.** On `WM_APP_CONFIG`: `SetTimer(bar, 11, 50)` (re-arming restarts it). On the timer: `KillTimer`, `load`, `next_config`; then also `reset_usage_if_deleted()` (if `usage.toml` is missing and in-memory usage is not empty → clear it, do not save). `apply_config` stores the new config, then in this order: `geometry`/`fonts`/`items` → `reposition()` (+ rebuild text formats and `RightView::new(&visible_items)` where `visible_items` = `right` minus `hidden`; items that stay visible keep their pill state so they glide to the new slots like a newly shown item, spec §4.3); `glass` → spawn or drop `Backdrops`, `bar_glass = false`, `sync_glass()`; `clock` → `tick_clock()` (it re-arms `CLOCK_TIMER` for minutes or seconds); `visibility` → `vis.set_config(..)` + `exec`, `set_catcher`, and on `mode` `reserve_space()`; `win_key` → `input_hook.set_win_key`; `tray` → nothing yet (Task 5 wires `refresh_tray()` here); `autostart` → `autostart_set`. One `paint()` at the end. At startup call `autostart_set(config.system.start_with_windows)` once.
- [ ] **Step 4: Run** `cargo test --workspace` — Expected: PASS. Then live, no user needed: release build, start NewWin with `NEWWIN_DEBUG=1`, edit `%APPDATA%\NewWin\config.toml` from PowerShell (`height = 44`, then `mode = "pinned"`, then a syntax error, then delete the file) and read the log: `config applied: Changes { geometry: true, .. }`, `config error at line N, keeping the last good one`, `config file missing: defaults`. Restore the user's file afterwards.
- [ ] **Step 5: Commit** `feat: the shell applies config.toml changes live`.

---

### Task 5: Tray order, hiding and `tray_seen.toml`

**Files:**
- Create: `crates/config/src/tray_seen.rs`; Modify: `crates/config/src/lib.rs`, `crates/core/src/tray.rs`, `apps/shell/src/app/tray_view.rs`
- Test: `crates/config/tests/tray_seen.rs`, `crates/core/tests/tray.rs`

**Interfaces:**
- Produces: `TraySeen { apps: Vec<SeenApp> }`, `SeenApp { exe: String, name: String }`; `TraySeen::load(path) -> (TraySeen, Option<String>)`; `TraySeen::note(&mut self, exe: &str, name: &str) -> bool` (true when something changed; new entries at the end; >200 drops the oldest); `TraySeen::to_toml(&self) -> String`. Core: `pub fn arrange_rows<T>(rows: Vec<(String /*exe*/, T)>, order: &[String], hidden: &[String]) -> Vec<T>` (hidden removed; `order` first in its order; the rest keep their incoming order; exe compared case-insensitively).

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn arrange_puts_ordered_first_and_drops_hidden() {
    let rows = vec![("a.exe".into(), 1), ("B.exe".into(), 2), ("c.exe".into(), 3)];
    assert_eq!(arrange_rows(rows, &["b.EXE".into()], &["C.exe".into()]), vec![2, 1]);
}
#[test]
fn note_adds_once_and_updates_the_name() {
    let mut s = TraySeen::default();
    assert!(s.note("Discord.exe", "Discord"));
    assert!(!s.note("discord.exe", "Discord"));
    assert!(s.note("Discord.exe", "Discord (3)"));
    assert_eq!(s.apps.len(), 1);
}
#[test]
fn seen_list_is_capped_at_200() { /* 201 notes → 200, first one gone */ }
#[test]
fn garbage_tray_seen_loads_empty_with_an_error() { /* "[[app]]\nexe = 5" → empty, Some(err) */ }
```
- [ ] **Step 2: Run** — Expected: FAIL.
- [ ] **Step 3: Implement.** `tray_rows` builds `(exe file name of icon.pid, row)` pairs, applies `arrange_rows` with `config.tray`, keeps the NewWin header first, and calls `seen.note` for every non-explorer icon (exe from `process_image_path`, skip `explorer.exe` and our own pid); when `note` returns true, save `tray_seen.toml` on the worker (`Job::Run`) via `save_atomic`. `refresh_tray()` = rebuild rows if the tray is open (the existing update path).
- [ ] **Step 4: Run** `cargo test --workspace` — Expected: PASS.
- [ ] **Step 5: Commit** `feat: tray order and hiding from config; tray_seen.toml`.

---

### Task 6: Toggle, segmented, range slider (`newwin-ui`)

**Files:**
- Create: `crates/ui/src/toggle.rs`, `crates/ui/src/segmented.rs`; Modify: `crates/ui/src/slider.rs`, `crates/ui/src/lib.rs`, `crates/ui/src/theme.rs`, existing slider callers (`volume_panel.rs`)
- Test: `crates/ui/tests/controls.rs`

**Interfaces:**
- Produces:
  - `pub struct Toggle { pub on: bool, anim: Tween }`; `Toggle::rect(origin: PointF, dpi: u32) -> RectF` (36×20 logical); `Toggle::flip(&mut self, now_ms: u64)`; `Toggle::paint(&self, p: &mut Painter, r: RectF, now_ms: u64)`; `Toggle::animating(&self, now_ms: u64) -> bool`.
  - `pub struct Segmented { pub labels: Vec<String>, pub selected: usize, pill: Tween }`; `Segmented::hit(&self, layout: &[RectF], p: PointF) -> Option<usize>`; `Segmented::layout(&self, origin: PointF, widths: &[f32], dpi: u32) -> Vec<RectF>`; `select(&mut self, i: usize, now_ms: u64)`; `paint(..)`.
  - `pub struct Range { pub min: i32, pub max: i32, pub step: i32 }`; `pub fn value_at(x: f32, track: RectF, r: Range) -> i32` (snapped to `step`, clamped); `pub fn thumb_x_for(v: i32, track: RectF, r: Range) -> f32`. Existing `level_at`/`thumb_x` become wrappers with `Range { min: 0, max: 100, step: 1 }`.
  - `theme`: `TOGGLE_W = 36.0`, `TOGGLE_H = 20.0`, `TOGGLE_MS = 150`, `SEGMENT_PAD_X = 12.0`, `SEGMENT_H = 26.0`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn value_at_snaps_to_step_and_clamps() {
    let track = RectF::new(0.0, 0.0, 100.0, 4.0);
    let r = Range { min: 0, max: 1000, step: 10 };
    assert_eq!(value_at(14.6, track, r), 150);
    assert_eq!(value_at(-5.0, track, r), 0);
    assert_eq!(value_at(500.0, track, r), 1000);
}
#[test]
fn thumb_round_trips_through_value() { /* for v in [3, 9, 15] with Range{3,15,1}: value_at(thumb_x_for(v)) == v */ }
#[test]
fn segmented_hit_picks_the_segment_under_the_pointer() { /* two segments, point in the second → Some(1); outside → None */ }
#[test]
fn toggle_animates_for_150_ms_after_a_flip() { /* flip at t=0 → animating(100) true, animating(151) false; on flipped */ }
#[test]
fn volume_slider_levels_are_unchanged() { /* level_at / thumb_x keep the existing test values */ }
```
- [ ] **Step 2: Run** `cargo test -p newwin-ui --test controls` — Expected: FAIL.
- [ ] **Step 3: Implement** (paint with the existing `Painter` primitives; knob on/off colours = monochrome white on dark as in the approved mockup).
- [ ] **Step 4: Run** `cargo test --workspace` — Expected: PASS.
- [ ] **Step 5: Commit** `feat: toggle, segmented control and ranged slider`.

---

### Task 7: Text field, reorder list, scroll, nav (`newwin-ui`)

**Files:**
- Create: `crates/ui/src/text_field.rs`, `crates/ui/src/reorder_list.rs`, `crates/ui/src/scroll.rs`, `crates/ui/src/nav.rs`; Modify: `crates/core/src/items.rs` (add `reorder`), `crates/ui/src/lib.rs`
- Test: `crates/core/tests/items.rs`, `crates/ui/tests/controls.rs`

**Interfaces:**
- Produces:
  - core: `pub fn reorder<T>(v: &mut Vec<T>, from: usize, to: usize)` (out-of-range indices do nothing); `pub fn drop_index(row_tops: &[f32], row_h: f32, y: f32) -> usize`.
  - `TextField { edit: newwin_core::text_edit::TextEdit, focused: bool }` with `input(&mut self, ev: PanelInput) -> bool` (true = text changed) and `paint(..)` — reuses `TextEdit` and the caret/blink from `SpotlightView`.
  - `ReorderList { rows: Vec<ReorderRow>, drag: Option<Drag> }`, `ReorderRow { label: String, detail: Option<String>, on: Toggle }`; `press(p) / drag_to(p) / release() -> Option<(usize, usize)>` (from, to); rows glide to their new slots over 150 ms.
  - `Scroll { offset: f32, content_h: f32, view_h: f32 }`: `wheel(&mut self, delta: i16)` (120 = 48 logical px), `drag_thumb(..)`, `clamp()`, `thumb_rect(view: RectF) -> Option<RectF>` (`None` when content fits).
  - `Nav { items: Vec<String>, selected: usize, pill: Tween }`: `hit`, `select`, `paint` (pill glides as the bar's active pill).

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn reorder_moves_one_item() { let mut v = vec![1, 2, 3, 4]; reorder(&mut v, 0, 2); assert_eq!(v, [2, 3, 1, 4]); reorder(&mut v, 9, 0); assert_eq!(v, [2, 3, 1, 4]); }
#[test]
fn drop_index_uses_row_midpoints() { assert_eq!(drop_index(&[0.0, 30.0, 60.0], 30.0, 44.0), 1); assert_eq!(drop_index(&[0.0, 30.0, 60.0], 30.0, 200.0), 2); }
#[test]
fn scroll_clamps_and_hides_thumb_when_content_fits() { /* content 300, view 400 → thumb None, wheel keeps 0; content 900 → wheel(-120) offset 48 */ }
#[test]
fn reorder_list_drag_reports_from_and_to() { /* press row 0, drag below row 2, release → Some((0, 2)) */ }
#[test]
fn nav_hit_selects_the_row() { /* six items, point in the third → Some(2) */ }
```
- [ ] **Step 2: Run** — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --workspace` — Expected: PASS.
- [ ] **Step 5: Commit** `feat: text field, reorder list, scroll and nav controls`.

---

### Task 8: Top-level glass window (`newwin-ui::app_window`)

**Files:**
- Create: `crates/ui/src/app_window.rs`; Modify: `crates/ui/src/lib.rs`
- Test: `crates/ui/tests/app_window.rs`

**Interfaces:**
- Consumes: `Layer` (glass), `Backdrops`/`Region` (`crates/ui/src/backdrop.rs`), `create_window` (`newwin_platform::window`).
- Produces: `pub trait AppContent { fn paint(&mut self, p: &mut Painter, now_ms: u64); fn input(&mut self, ev: PanelInput, now_ms: u64) -> PanelReply; fn animating(&self, now_ms: u64) -> bool; fn wake_at(&self, now_ms: u64) -> Option<u64> { None } }` (reuses `PanelInput`/`PanelReply`); `pub struct AppWindow<C>`; `AppWindow::create(gfx: &Rc<Gfx>, content: C, class: &str, title: &str, size_logical: (f32, f32)) -> Result<AppWindow<C>>` (centered on the primary monitor's work area); `pub fn caption_hit(p: PointF, size: (f32, f32), dpi: u32) -> u32` (pure: `HTCAPTION` for the top 40 logical px except the buttons, `HTMINBUTTON` / `HTCLOSE` for the two 46×40 boxes at the right, else `HTCLIENT`); `AppWindow::hwnd()`, `update(f)`, `read(f)`. Glass: own `Backdrops` thread; region = window rect, re-sent on `WM_WINDOWPOSCHANGED` (moves) and `WM_ACTIVATE`; while minimized no region. `WM_CLOSE` → `DestroyWindow` → `PostQuitMessage(0)`. Styles: `WS_POPUP | WS_MINIMIZEBOX | WS_SYSMENU` + `WS_EX_APPWINDOW` (on the bar, Alt+Tab, Snap via `HTCAPTION`).

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn caption_hit_regions() {
    let s = (760.0, 560.0);
    assert_eq!(caption_hit(PointF { x: 300.0, y: 10.0 }, s, 96), HTCAPTION);
    assert_eq!(caption_hit(PointF { x: 750.0, y: 10.0 }, s, 96), HTCLOSE);
    assert_eq!(caption_hit(PointF { x: 700.0, y: 10.0 }, s, 96), HTMINBUTTON);
    assert_eq!(caption_hit(PointF { x: 300.0, y: 100.0 }, s, 96), HTCLIENT);
    assert_eq!(caption_hit(PointF { x: 1500.0, y: 20.0 }, (760.0, 560.0), 192), HTCLOSE);
}
#[test]
fn the_window_is_an_app_window_of_the_given_size() { /* create at 96 dpi test monitor; GetWindowRect size == 760×560 scaled; WS_EX_APPWINDOW set */ }
#[test]
fn moving_the_window_resends_its_glass_region() { /* SetWindowPos → the regions sent to Backdrops (test hook) contain the new rect */ }
```
- [ ] **Step 2: Run** `cargo test -p newwin-ui --test app_window` — Expected: FAIL.
- [ ] **Step 3: Implement** (paint loop and frame timer as `Panel`; `WM_NCHITTEST` uses `caption_hit`; `HTMINBUTTON`/`HTCLOSE` clicks handled in `WM_NCLBUTTONUP` → `ShowWindow(SW_MINIMIZE)` / `WM_CLOSE`; caption text and buttons painted by the window, hover highlight on the buttons).
- [ ] **Step 4: Run** `cargo test --workspace` — Expected: PASS.
- [ ] **Step 5: Commit** `feat: top-level glass window with its own caption`.

---

### Task 9: `NewWinSettings.exe` skeleton: one instance, nav, writer, banners

**Files:**
- Create: `apps/settings/Cargo.toml`, `apps/settings/src/main.rs`, `apps/settings/src/writer.rs`, `apps/settings/src/view.rs`; Modify: `Cargo.toml` (workspace member), `crates/core/src/strings.rs` (section names, banners, buttons — `en`, `uk`)
- Test: `apps/settings/src/writer.rs` (unit tests)

**Interfaces:**
- Consumes: `AppWindow` (Task 8), `Nav` (Task 7), `DirWatch` (Task 3), `newwin_config::{load, to_toml, save_atomic}` (Task 1).
- Produces: `Writer { pending: Option<Config>, last_written: Option<String> }` with `fn edit(&mut self, c: Config) -> bool` (true = arm the 100 ms timer), `fn flush(&mut self, path: &Path) -> std::io::Result<()>`, `fn is_own(&self, file_text: &str) -> bool`; `SettingsView` (implements `AppContent`) with `pub enum Section { Bar, Appearance, Items, Spotlight, Tray, About }`, `banner: Option<Banner>`, `pub enum Banner { ParseError { line: Option<usize> }, SaveFailed(String), NotRunning }`. Section bodies are stubs here (filled in Tasks 10–11).

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn debounce_writes_the_last_value_once() {
    let mut w = Writer::default();
    let (mut a, mut b) = (Config::default(), Config::default());
    a.appearance.height = 40; b.appearance.height = 44;
    assert!(w.edit(a)); assert!(w.edit(b)); // both arm (re-arming restarts the timer)
    w.flush(&path).unwrap();
    assert!(std::fs::read_to_string(&path).unwrap().contains("height = 44"));
    w.flush(&path).unwrap(); // nothing pending: no second write (mtime unchanged)
}
#[test]
fn own_write_is_not_a_change() { /* after flush, is_own(file text) == true; after an external edit, false */ }
#[test]
fn a_broken_file_is_not_overwritten_until_an_edit() { /* start with broken file: no flush happens without edit() */ }
#[test]
fn keyboard_focus_moves_and_wraps() {
    // pure: focus_next(current: Option<usize>, count: usize, back: bool) -> Option<usize>
    assert_eq!(focus_next(None, 3, false), Some(0));
    assert_eq!(focus_next(Some(2), 3, false), Some(0));
    assert_eq!(focus_next(Some(0), 3, true), Some(2));
    assert_eq!(focus_next(None, 0, false), None);
}
```
- [ ] **Step 2: Run** `cargo test -p newwin-settings` — Expected: FAIL.
- [ ] **Step 3: Implement.** `main`: logger (`newwin_platform::log`, file `settings.log`), DPI awareness, `CreateMutexW(Local\NewWin.Settings)`; if it already existed → `FindWindowW(NewWin.Settings)`, `ShowWindow(SW_RESTORE)` if iconic, `SetForegroundWindow`, exit 0. Load config (`ParseError` → banner, keep defaults on screen, no write until an edit). `DirWatch` on the config dir → reload unless `is_own`; external change reloads the controls. `NotRunning` banner when `OpenMutexW(Local\NewWin.Shell)` fails (checked on start and on window activation); *Start NewWin* runs `NewWin.exe` next to our exe. Write timer 100 ms; `SaveFailed` banner on error, cleared on the next good write. Keyboard (spec §5): Tab / Shift+Tab move focus over the section's controls (`focus_next`, focus ring = 1 px white outline), Ctrl+Tab / Ctrl+Shift+Tab switch sections, Left/Right change a focused slider by one step or a segment by one, Space flips a focused toggle, Esc closes an open font list.
- [ ] **Step 4: Run** `cargo test --workspace`; then `cargo run -p newwin-settings`: the window appears with six nav items and glass; second launch focuses the first. Expected: PASS / as described.
- [ ] **Step 5: Commit** `feat: NewWinSettings.exe skeleton`.

---

### Task 10: Sections Bar, Appearance, Spotlight, About

**Files:**
- Create: `apps/settings/src/sections/{mod.rs,bar.rs,appearance.rs,spotlight.rs,about.rs}`, `apps/settings/src/fonts.rs`; Modify: `apps/settings/src/view.rs`, `crates/core/src/strings.rs`
- Test: unit tests in each section file (pure: control → config mapping), `apps/settings/src/fonts.rs`

**Interfaces:**
- Consumes: `Toggle`, `Segmented`, `Range`/`value_at`, `TextField`, `Scroll` (Tasks 6–7), `Writer` (Task 9), `launch` helpers from `newwin_platform::launch` (`shell_execute`).
- Produces: per section `fn rows(c: &Config) -> Vec<Row>` and `fn apply(c: &mut Config, id: RowId, v: RowValue)`; `pub enum RowValue { Bool(bool), Int(i32), Choice(usize), Text(String) }`; `fonts::families() -> Vec<String>` (system collection, localized names for the user's locale, sorted, deduped).

- [ ] **Step 1: Write the failing tests** — for every control in spec §3.1, §3.2, §3.4, §3.6 one mapping test, e.g.:

```rust
#[test]
fn mode_segment_maps_to_bar_mode() { let mut c = Config::default(); apply(&mut c, RowId::Mode, RowValue::Choice(1)); assert_eq!(c.bar.mode, BarMode::Pinned); }
#[test]
fn empty_custom_date_is_system() { let mut c = Config::default(); apply(&mut c, RowId::DateTemplate, RowValue::Text("  ".into())); assert_eq!(c.appearance.date_format, "system"); }
#[test]
fn delay_slider_range_is_0_to_1000_step_10() { assert_eq!(range_of(RowId::ShowDelay), Range { min: 0, max: 1000, step: 10 }); }
#[test]
fn families_include_segoe_ui_and_are_sorted() { let f = families(); assert!(f.iter().any(|n| n == "Segoe UI")); assert!(f.windows(2).all(|w| w[0].to_lowercase() <= w[1].to_lowercase())); }
#[test]
fn clear_history_needs_a_second_click_within_3_s() { /* confirm automaton: click → Armed; click at +2 s → Fire; click at +4 s → Armed again */ }
```
- [ ] **Step 2: Run** `cargo test -p newwin-settings` — Expected: FAIL.
- [ ] **Step 3: Implement.** Font row opens a glass list (existing `Popup` style, with `Scroll`, each family painted in itself, typing filters like the tray). Date *Custom* shows a `TextField` and a live sample (`format_bar_date(template)`). *Clear* deletes `usage.toml`. About: version `env!("CARGO_PKG_VERSION")`, *Open logs* (`%LOCALAPPDATA%\NewWin\logs`), *Open config file*, *Restore Windows taskbar* (two-click confirm; posts `WM_APP + 20` to the shell's `NewWin.Ping` window (Task 12), waits ≤ 3 s for `Local\NewWin.Shell` to disappear, else runs `NewWin.exe --restore-taskbar`). The confirm automaton is shared by *Clear* and *Restore* (`Confirm { armed_at: Option<u64> }`, 3000 ms).
- [ ] **Step 4: Run** `cargo test --workspace` — Expected: PASS.
- [ ] **Step 5: Commit** `feat: Settings sections Bar, Appearance, Spotlight, About`.

---

### Task 11: Sections Items and Tray

**Files:**
- Create: `apps/settings/src/sections/{items.rs,tray.rs}`; Modify: `apps/settings/src/view.rs`, `apps/settings/src/main.rs` (watch `tray_seen.toml` changes through the same `DirWatch`)
- Test: unit tests in both files

**Interfaces:**
- Consumes: `ReorderList`, `reorder` (Task 7), `TraySeen` (Task 5), `ITEM_NAMES` (Task 1).
- Produces: `items::rows(c) -> Vec<ReorderRow>`, `items::moved(c: &mut Config, from, to)`, `items::toggled(c, index, on)`; `tray::rows(c, seen: &TraySeen) -> Vec<(String /*exe*/, ReorderRow)>` (order: `tray.order`, then the rest by name), `tray::moved(c, rows_exe: &[String], from, to)` (writes the full current order into `tray.order`), `tray::toggled(c, exe, on)`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn hiding_an_item_keeps_its_place() { let mut c = Config::default(); toggled(&mut c, 2, false); assert_eq!(c.items.right[2], "volume"); assert_eq!(c.items.hidden, ["volume"]); }
#[test]
fn moving_an_item_rewrites_right() { let mut c = Config::default(); moved(&mut c, 5, 0); assert_eq!(c.items.right[0], "time"); }
#[test]
fn tray_rows_follow_order_then_name() { /* seen [Zed.exe, Alpha.exe, Discord.exe], order [Discord.exe] → Discord, Alpha, Zed */ }
#[test]
fn hiding_a_tray_app_adds_its_exe_once() { /* toggled twice off → hidden has it once; on → removed */ }
#[test]
fn empty_tray_list_shows_the_hint() { /* rows empty → view shows "Apps appear here after they put an icon in the tray." */ }
```
- [ ] **Step 2: Run** — Expected: FAIL.
- [ ] **Step 3: Implement** (detail line = exe name; hint copy exactly as spec §3.5).
- [ ] **Step 4: Run** `cargo test --workspace` — Expected: PASS.
- [ ] **Step 5: Commit** `feat: Settings sections Items and Tray`.

---

### Task 12: Opening Settings from the shell; quit request

**Files:**
- Modify: `apps/shell/src/app/menus.rs` (enable *NewWin settings…*), `apps/shell/src/app/tray_view.rs` (`open_settings_file` → `open_settings`), `apps/shell/src/app/spotlight_view.rs` (*Settings* command), `apps/shell/src/watchdog.rs`, `apps/shell/src/app.rs` (route `WM_APP_QUIT`), `crates/platform/src/launch.rs`
- Test: `crates/platform/tests/clock_launch.rs` (existing launch tests), `apps/shell/src/watchdog.rs` (unit test for the decision)

**Interfaces:**
- Produces: `newwin_platform::launch::settings_exe() -> Option<PathBuf>` (sibling of the current exe, if it exists); `App::open_settings(&self)` (runs it with `AllowSetForegroundWindow(ASFW_ANY)` first; falls back to `open_settings_file`); `pub(super) const WM_APP_QUIT: u32 = WM_APP + 20` on the ping window and `App::quit(&mut self)` (the menu's *Quit NewWin* body moved here: save usage, restore taskbar, `PostQuitMessage(0)`; exit code 0 makes the watchdog exit on its own); `fn second_launch_action(settings: Option<&Path>) -> SecondLaunch` (`OpenSettings(path)` | `Nothing`).

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn settings_exe_is_next_to_the_current_exe() { /* in tests: current exe dir + "NewWinSettings.exe" → Some only after touching that file */ }
#[test]
fn a_second_launch_opens_settings_when_present() { assert_eq!(second_launch_action(Some(p)), SecondLaunch::OpenSettings(p.into())); assert_eq!(second_launch_action(None), SecondLaunch::Nothing); }
```
- [ ] **Step 2: Run** — Expected: FAIL.
- [ ] **Step 3: Implement.** Watchdog: on mutex conflict run `second_launch_action` and exit. Shell: `WM_APP_QUIT` on the ping window → `App::quit()`; the menu's QUIT calls the same. (Deviation from spec §4.6, simpler: no watchdog window — the watchdog already exits when the shell exits with 0.)
- [ ] **Step 4: Run** `cargo test --workspace`; live: release build, NewWin running, open Settings from the bar menu, the tray NewWin row and Spotlight `settings`; launch `NewWin.exe` again → Settings opens. Expected: one Settings window each time.
- [ ] **Step 5: Commit** `feat: the shell opens NewWinSettings.exe and quits on request`.

---

### Task 13: Review, live checks, measurements

**Files:**
- Create: `docs/superpowers/notes/2026-10-23-m4-acceptance.md`, a handoff note for the next chat

- [ ] **Step 1: Whole-branch review** by a fresh reviewer (`superpowers:requesting-code-review`, diff `main..m4-settings`); fix Critical/Important findings with tests, list Minor ones as deferred in the acceptance note.
- [ ] **Step 2: Live checklist with the user** (spec §7), recording each result in the acceptance note.
- [ ] **Step 3: Measurements** — `powershell -File scripts\measure.ps1 -Minutes 10` with Settings closed and NewWin without `NEWWIN_DEBUG`, the user away (≤ 10.7 MB private WS, CPU ≈ 0 %); slider → bar frame from the debug log (`config applied` → next `bar frame` timestamps, 5 samples, < 100 ms); Settings first frame from its log (< 200 ms).
- [ ] **Step 4: Run** `cargo test --workspace` and `cargo clippy --workspace --all-targets` — Expected: PASS.
- [ ] **Step 5: Commit** the notes; ask the user before merging into `main`.
