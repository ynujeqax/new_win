# M3: Spotlight — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A tap of Win (or `Ctrl+Esc`, or the bar's magnifier) opens a glass Spotlight that finds and launches every Start-menu app and the system commands, with a text-only row menu (Open · Run as administrator · Open file location), wrong-layout search and launch counts — while native Start never appears and idle CPU stays 0 %.

**Architecture:** Matching, ranking, layout swap, the double-Enter automaton, line editing and the Win-tap automaton are pure functions in `newwin-core` with unit tests. `newwin-config` gets `usage.toml`. `newwin-platform` gets the `shell:AppsFolder` index with change notifications (thread pool + WinRT `PackageCatalog`), launch helpers and the Win/`Ctrl+Esc` branch of the existing low-level hook. `newwin-ui` gets a `SpotlightView` content for the existing glass `Panel<C>`. `apps/shell` wires them through `PostMessage`, exactly like M2.

**Tech Stack:** Rust 2024, `windows` 0.62 (Win32 Shell/COM, `Windows.ApplicationModel` WinRT, DirectWrite/Direct2D/DirectComposition), WARP rendering.

**Spec:** `docs/superpowers/specs/2026-10-22-newwin-m3-spotlight-design.md` (extends `docs/superpowers/specs/2026-10-21-newwin-design.md` §2, §9 and `docs/superpowers/specs/2026-10-22-newwin-liquid-glass-design.md`).

## Global Constraints

- Work on branch `m3-spotlight` (exists; the spec commit `67cc023` is on it).
- Build tools: `$env:Path="$env:USERPROFILE\.cargo\bin;$env:Path"`. Stop NewWin before release builds: `Get-Process NewWin -ErrorAction SilentlyContinue | Stop-Process -Force`, then `.\target\release\NewWin.exe --restore-taskbar`.
- `cargo test --workspace` and `cargo clippy --workspace --all-targets` stay green after every task. TDD: every behaviour test is written and seen failing first.
- Release builds use `panic = "abort"`: nothing from the shell namespace, other processes or files may panic (names, paths, properties, `usage.toml`, clipboard). `Option`/`Result`, no unchecked indexing or slicing of strings.
- No polling. Allowed timers: one-shot UI timers (`APPS_DEBOUNCE_TIMER = 9`, 2000 ms; `USAGE_SAVE_TIMER = 10`, 2000 ms), the panel's 16 ms frame timer only while a tween runs, and the panel's new one-shot `WAKE_TIMER` (caret blink, confirm expiry) only while Spotlight is open.
- No new threads of ours. Index builds and `usage.toml` writes run on `newwin-worker`; change notifications arrive on Windows' thread pool / WinRT threads and only `PostMessage`.
- Messages: `WM_APP + 14` Spotlight toggle (hook → ping), `WM_APP + 15` apps changed (watch → ping), `WM_APP + 16` apps indexed (worker → ping), `WM_APP + 17` Spotlight panel (panel → bar). `WM_APP + 1…13` are taken.
- Our own injected input carries `dwExtraInfo = NEWWIN_INPUT_TAG = 0x4E57_4E57`; the hook skips exactly those events and nothing else.
- Spotlight opening never reveals the bar (OLED): it is not a `ShellPanel` and feeds no `VisInput`.
- Look: existing liquid glass (`Panel`, `paint_glass`) plus `SPOTLIGHT_SHADE`; monochrome, text only. New `theme` constants: `SPOTLIGHT_WIDTH = 560.0`, `SPOTLIGHT_FIELD_H = 52.0`, `SPOTLIGHT_ROW_H = 34.0`, `SPOTLIGHT_HINTS_H = 28.0`, `SPOTLIGHT_PAD = 8.0`, `SPOTLIGHT_QUERY_TEXT = 20.0`, `SPOTLIGHT_HINT_TEXT = 11.0`, `SPOTLIGHT_TOP = 0.22`, `SPOTLIGHT_SHADE = Color::rgba(0, 0, 0, 0.22)`, `CARET_W = 1.35`, `ROW_FADE_MS = 120`.
- Strings go through `newwin_core::strings::Strings` (`en` and `uk`).
- Budget (spec §8): Win → first frame < 16 ms; ranking one keystroke over 1000 apps < 1 ms (release); idle 10 min = 0 % CPU, no new wakeups; shell private WS grows ≤ 5 MB and stays < 20 MB.
- Never press Shut down / Restart / Sign out / Sleep in tests or live checks (check only the confirmation text). Never launch Screen Snipping. Glass windows are invisible to DXGI screenshots — the user checks looks.
- Commit messages through a file (`git commit -F`), ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **Win held down long** (auto-repeat sends repeated Win downs) and then released → still one tap, Spotlight opens once. Test in Task 6 (`held_win_with_autorepeat_is_one_tap`).
2. **Odd app names** — empty, 300 characters, emoji (surrogate pairs), combining marks, `İ` whose lowercase is longer → match ranges stay inside the name's UTF-16 length, no panic. Test in Task 2 (`ranges_stay_in_bounds_for_odd_names`).
3. **The index rebuilds while Spotlight is open** (an app installs mid-search) → rows refresh, the selection stays in range, the next Enter launches what the row shows. Test in Task 10 (`new_catalog_keeps_selection_in_range`).
4. **`usage.toml` corrupt, huge or stale** (garbage text, counts at `u32::MAX`, keys of uninstalled apps) → loads empty with an error message, bumps saturate, stale keys are dropped on save. Tests in Task 5.
5. **Win pressed again while Spotlight is closing** (fast double tap) → a fresh Spotlight opens instead of the tap being lost. Test in Task 11 (`toggle_while_closing_opens_again`).

---

### Task 1: Risk probes

**Files:**
- Create: `crates/platform/examples/spotlight_win_probe.rs`, `crates/platform/examples/spotlight_focus_probe.rs`, `crates/platform/examples/apps_folder_probe.rs` (each starts with `//! Temporary M3 probe — remove after M3.`)
- Create: `docs/superpowers/notes/2026-10-22-m3-probes.md`

- [ ] **Step 1: Win probe.** Install a `WH_KEYBOARD_LL` hook for 60 s. Track Win down / other key / Win up; on a lone Win up, swallow it (return 1) and `SendInput` `[VK_NONAME(0xFC) down, VK_NONAME up, VK_LWIN up]`, all with `dwExtraInfo = 0x4E57_4E57`; skip events carrying that tag. Log every decision. Run `cargo run -p newwin-platform --example spotlight_win_probe` with NewWin stopped and the native taskbar restored, and ask the user to tap Win 5×, then Win+E, Win+R, Win+Shift+S (Esc out), Win+N, Win+Space, Alt+Shift.
  Expected: Start never opens; every combination works. **If Start still opens, stop and report to the user before Task 6.**
- [ ] **Step 2: Focus probe.** Same hook; on a lone Win tap post to a hidden `WS_EX_TOPMOST | WS_EX_TOOLWINDOW` popup that then shows itself and calls `SetForegroundWindow` + `SetFocus`, and logs `GetForegroundWindow() == hwnd` and the first `WM_CHAR` it receives. Also handle `Ctrl+Esc` (swallow Esc down/up, inject `VK_NONAME` down/up, post). Ask the user to tap Win and type a letter over File Explorer, a browser and a borderless full-screen app; then `Ctrl+Esc`.
  Expected: foreground true and the letter arrives each time. If not, retry with `AttachThreadInput(GetWindowThreadProcessId(GetForegroundWindow()), GetCurrentThreadId(), true)` around `SetForegroundWindow` and record which works.
- [ ] **Step 3: AppsFolder probe.** Enumerate `shell:AppsFolder` (`SHGetKnownFolderItem(FOLDERID_AppsFolder)` → `BindToHandler(BHID_EnumItems)` → `IEnumShellItems`), time it, and for each item print `SIGDN_NORMALDISPLAY`, `SIGDN_PARENTRELATIVEPARSING` and, via `IShellItem2::GetString`, `PKEY_Link_TargetParsingPath`, `PKEY_ParsingPath`, `PKEY_AppUserModel_PackageFullName`. Then `ShellExecuteExW` verb `runas` on `shell:AppsFolder\<parsing name of Command Prompt>` (the user confirms the UAC prompt opens an elevated cmd), and the same on its link target path.
  Expected: total time and count recorded; which property gives the file path for classic apps (Command Prompt → `...\cmd.exe`) and which marks UWP (Settings); which `runas` form elevates.
- [ ] **Step 4: Write `notes/2026-10-22-m3-probes.md`** with one section per probe: what was run, numbers, the decision (path property, UWP property, `runas` form, focus method). Later tasks use these decisions where marked "per probe".
- [ ] **Step 5: Commit** the probes and the note.

---

### Task 2: Matching and ranking (`newwin-core`)

**Files:**
- Create: `crates/core/src/spotlight.rs`; Modify: `crates/core/src/lib.rs` (`pub mod spotlight;`)
- Test: `crates/core/tests/spotlight.rs`

**Interfaces:**
- Produces (`newwin_core::spotlight`):
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)] pub enum MatchLevel { Prefix, WordStart, Substring, Subsequence }`
  - `#[derive(Clone, Debug, PartialEq, Eq)] pub struct Match { pub level: MatchLevel, pub ranges: Vec<(usize, usize)> }` — UTF-16 `(start, len)` in the name
  - `pub fn match_name(name: &str, query: &str) -> Option<Match>`
  - `pub fn swap_layout(query: &str) -> Option<String>`
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)] pub enum SystemCommand { Settings, ShutDown, Restart, Sleep, Lock, SignOut }`
  - `#[derive(Clone, Debug, PartialEq, Eq)] pub enum Kind { App, System(SystemCommand) }`
  - `#[derive(Clone, Debug, PartialEq, Eq)] pub struct Candidate { pub name: String, pub id: String, pub kind: Kind, pub file_path: Option<String>, pub packaged: bool }` with `pub fn usage_key(&self) -> String` (apps: `id`; system: `system:settings`, `system:shut_down`, `system:restart`, `system:sleep`, `system:lock`, `system:sign_out`) and `pub fn can_run_as_admin(&self) -> bool` (`Kind::App && !packaged && file_path.is_some()`)
  - `#[derive(Clone, Debug, PartialEq, Eq)] pub struct Hit { pub index: usize, pub ranges: Vec<(usize, usize)> }`
  - `pub fn rank(query: &str, cands: &[Candidate], launches: &dyn Fn(&str) -> u32, max: usize) -> Vec<Hit>` — `launches` takes a usage key

**Rules** (spec §2.2): query trimmed; empty → no hits. Folding is per char, `c.to_lowercase().next()`, on both sides, so each name char keeps its own UTF-16 offset and length (`İ` stays one char). Word starts: index 0, after whitespace / `-` / `_` / `.` / `(`, and a capital after a lowercase letter. Levels in order: name starts with query → `Prefix`; query occurs at a word start → `WordStart`; anywhere → `Substring` (first occurrence); else subsequence — first try placing every query char on word starts only (greedy left), else greedy leftmost chars → `Subsequence`, one range per char (adjacent chars merged). Sort: level, launches desc, `Kind::App` before `Kind::System`, folded name, then index. Then `swap_layout(query)` (if `Some` and different) is ranked the same way and its hits not already present are appended; the total is cut to `max`.

`swap_layout`: physical keys of the Ukrainian layout (lower and upper case):
`` ` ``↔`'`, `q w e r t y u i o p [ ]` ↔ `й ц у к е н г ш щ з х ї`, `a s d f g h j k l ; '` ↔ `ф і в а п р о л д ж є`, `z x c v b n m , . /` ↔ `я ч с м и т ь б ю .`, `\`↔`ґ`. Direction: Cyrillic → Latin if the first mapped letter is Cyrillic, else Latin → Cyrillic; unmapped chars stay. `None` when the query has no letter in the table.

- [ ] **Step 1: Write the failing tests**

```rust
fn app(name: &str) -> Candidate { Candidate { name: name.into(), id: name.into(), kind: Kind::App, file_path: None, packaged: false } }
fn none(_: &str) -> u32 { 0 }

#[test] fn levels_in_order() {
    assert_eq!(match_name("Steam", "ste").unwrap(), Match { level: MatchLevel::Prefix, ranges: vec![(0, 3)] });
    assert_eq!(match_name("Visual Studio Code", "code").unwrap(), Match { level: MatchLevel::WordStart, ranges: vec![(14, 4)] });
    assert_eq!(match_name("Windows PowerShell", "shell").unwrap().level, MatchLevel::WordStart); // CamelCase
    assert_eq!(match_name("Discord", "ord").unwrap(), Match { level: MatchLevel::Substring, ranges: vec![(4, 3)] });
    assert_eq!(match_name("Visual Studio Code", "vsc").unwrap(), Match { level: MatchLevel::Subsequence, ranges: vec![(0, 1), (7, 1), (14, 1)] });
    assert_eq!(match_name("Steam", "xyz"), None);
}
#[test] fn case_and_cyrillic() {
    assert_eq!(match_name("Блокнот", "БЛОК").unwrap().level, MatchLevel::Prefix);
    assert_eq!(match_name("STEAM", "steam").unwrap().ranges, vec![(0, 5)]);
}
#[test] fn order_level_then_launches_then_kind_then_name() {
    let c = vec![app("Notepad++"), app("Notepad"), app("Snote"), Candidate { kind: Kind::System(SystemCommand::Settings), ..app("Note sys") }];
    let launches = |k: &str| if k == "Notepad++" { 5 } else { 0 };
    let names: Vec<_> = rank("note", &c, &launches, 8).iter().map(|h| c[h.index].name.clone()).collect();
    assert_eq!(names, ["Notepad++", "Notepad", "Note sys", "Snote"]);
}
#[test] fn max_results() { let c: Vec<_> = (0..20).map(|i| app(&format!("App {i}"))).collect(); assert_eq!(rank("app", &c, &none, 8).len(), 8); }
#[test] fn empty_query_no_hits() { assert!(rank("  ", &[app("Steam")], &none, 8).is_empty()); }
#[test] fn swap_layout_both_ways() {
    assert_eq!(swap_layout("іеуфь").as_deref(), Some("steam"));
    assert_eq!(swap_layout("ВШІСЩКВ").as_deref(), Some("DISCORD"));
    assert_eq!(swap_layout("steam").as_deref(), Some("іеуфь"));
    assert_eq!(swap_layout("123"), None);
}
#[test] fn wrong_layout_hits_come_after_direct_ones_without_duplicates() {
    let c = vec![app("Steam"), app("іеуфь notes")];
    let hits = rank("іеуфь", &c, &none, 8);
    assert_eq!(hits.iter().map(|h| h.index).collect::<Vec<_>>(), [1, 0]);
}
#[test] fn ranges_stay_in_bounds_for_odd_names() {
    for name in ["", &"x".repeat(300), "🎮 Game 🎮", "Cafe\u{301} Bar", "İstanbul"] {
        let units = name.encode_utf16().count();
        for q in ["a", "g", "e", "i", "🎮", "ist"] {
            if let Some(m) = match_name(name, q) { assert!(m.ranges.iter().all(|&(s, l)| s + l <= units), "{name:?} {q:?}"); }
        }
    }
}
#[test] #[ignore] fn rank_1000_apps_under_1ms() { // cargo test -p newwin-core --release -- --ignored
    let c: Vec<_> = (0..1000).map(|i| app(&format!("Application Number {i} Studio"))).collect();
    let t = std::time::Instant::now(); for q in ["a", "ap", "stu", "ans"] { rank(q, &c, &none, 8); }
    assert!(t.elapsed().as_micros() < 4000, "{:?}", t.elapsed());
}
```

- [ ] **Step 2: Run** `cargo test -p newwin-core --test spotlight` — Expected: FAIL (module missing).
- [ ] **Step 3: Implement** the interfaces above in `crates/core/src/spotlight.rs` per the rules.
- [ ] **Step 4: Run** `cargo test -p newwin-core --test spotlight` and `cargo test -p newwin-core --release --test spotlight -- --ignored` — Expected: PASS.
- [ ] **Step 5: Commit** `feat: Spotlight matching, ranking and wrong-layout search`.

---

### Task 3: System commands, confirmation, listing filter, strings (`newwin-core`)

**Files:**
- Modify: `crates/core/src/spotlight.rs`, `crates/core/src/strings.rs`
- Test: `crates/core/tests/spotlight.rs`

**Interfaces:**
- Consumes: Task 2 types.
- Produces:
  - `pub fn system_candidates(s: &Strings) -> Vec<Candidate>` — order Settings, Shut down, Restart, Sleep, Lock, Sign out; names `s.settings`, `s.shut_down`, `s.restart`, `s.sleep`, `s.lock`, `s.sign_out`; `id` = usage key; `file_path: None`, `packaged: false`
  - `impl SystemCommand { pub fn needs_confirm(self) -> bool }` (ShutDown, Restart)
  - `pub const CONFIRM_MS: u64 = 3000;`
  - `#[derive(Default, Debug)] pub struct Confirm` with `press(&mut self, index: usize, needs_confirm: bool, now_ms: u64) -> bool` (true = run now), `armed(&self, now_ms: u64) -> Option<usize>`, `expires_at(&self) -> Option<u64>`, `reset(&mut self)`
  - `pub fn is_listed(name: &str, file_path: Option<&str>) -> bool`
  - New `Strings` fields (en / uk): `lock` ("Lock" / "Заблокувати"), `open` ("Open" / "Відкрити"), `run_as_admin` ("Run as administrator" / "Запустити від імені адміністратора"), `open_file_location` ("Open file location" / "Відкрити розташування файлу"), `no_results` ("No results" / "Нічого не знайдено"), `kind_app` ("App" / "Програма"), `kind_system` ("System" / "Система"), `press_again_shut_down` ("Press ↵ again to shut down" / "Натисніть ↵ ще раз, щоб завершити роботу"), `press_again_restart` ("Press ↵ again to restart" / "Натисніть ↵ ще раз, щоб перезавантажити"), `spotlight_keys` ("↑↓ select · ↵ open · Ctrl ↵ run as administrator · Esc close" / "↑↓ вибір · ↵ відкрити · Ctrl ↵ від імені адміністратора · Esc закрити"). The field hint reuses `search`.

**Rules:** `Confirm::press` — `!needs_confirm` → reset, true; armed on the same index and `now < armed_at + CONFIRM_MS` → reset, true; otherwise arm `(index, now)`, false. `armed` is `None` after expiry. `is_listed` is false for an empty/blank name, a name starting with `uninstall` (folded), a file name starting with `unins`, and file extensions `txt chm pdf htm html url rtf md log ini` (folded).

- [ ] **Step 1: Write the failing tests** `system_commands_in_order_with_usage_keys` (en names, `usage_key()` values above, `can_run_as_admin()` false), `confirm_needs_second_enter` (press(2,true,0)=false, armed(100)=Some(2), press(2,true,100)=true, armed=None), `confirm_resets_on_other_row_and_after_3s` (press(2,true,0); press(3,true,10)=false and armed=Some(3); press(3,true,3010)=false re-arms), `no_confirm_runs_at_once`, `uninstallers_and_documents_are_not_listed` (`"Uninstall Steam"`, `Some(r"C:\G\unins000.exe")`, `"Readme"` with `.txt`, `.url` → false; `"Steam"` with `.exe`, `"Settings"` with `None` → true), `uk_strings_present` (`strings_for("uk-UA").lock == "Заблокувати"`).
- [ ] **Step 2: Run** `cargo test -p newwin-core --test spotlight` — Expected: FAIL.
- [ ] **Step 3: Implement** in `spotlight.rs` and `strings.rs`.
- [ ] **Step 4: Run** `cargo test -p newwin-core` — Expected: PASS.
- [ ] **Step 5: Commit** `feat: Spotlight system commands, double-Enter confirmation, listing filter`.

---

### Task 4: Line editing (`newwin-core`)

**Files:**
- Create: `crates/core/src/text_edit.rs`; Modify: `crates/core/src/lib.rs`
- Test: `crates/core/tests/text_edit.rs`

**Interfaces:**
- Produces (`newwin_core::text_edit`):
  - `pub const MAX_CHARS: usize = 200;`
  - `#[derive(Clone, Debug, PartialEq)] pub enum Edit { Insert(String), Backspace, BackspaceWord, Delete, Left, Right, Home, End, SelectAll }`
  - `#[derive(Clone, Debug, Default, PartialEq)] pub struct TextEdit` with `apply(&mut self, e: Edit) -> bool` (true = text changed), `text(&self) -> &str`, `caret_utf16(&self) -> usize`, `all_selected(&self) -> bool`, `clear(&mut self)`

**Rules:** the caret is a char index. `Insert` keeps only the first line and drops control chars, then inserts at the caret up to `MAX_CHARS` total. `SelectAll` sets a flag that the next `Insert`/`Backspace`/`BackspaceWord`/`Delete` acts on the whole text (replace / clear) and any movement clears. `BackspaceWord` deletes spaces left of the caret, then the run of non-spaces before them.

- [ ] **Step 1: Write the failing tests** `insert_and_move` (Insert "stam", Left×2, Insert "e" → "steam", caret 3), `backspace_word` ("visual studio  " + BackspaceWord → "visual "), `select_all_then_type_replaces`, `paste_takes_first_line_and_caps_length` (Insert of `"a\nb"` → "a"; 250 × "x" → 200 chars), `caret_counts_utf16` (Insert "🎮a" → caret_utf16 3), `edges_do_not_panic` (Left/Backspace/BackspaceWord at 0, Right/Delete at end → false).
- [ ] **Step 2: Run** `cargo test -p newwin-core --test text_edit` — Expected: FAIL.
- [ ] **Step 3: Implement** `text_edit.rs`.
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** `feat: single-line text editing for Spotlight`.

---

### Task 5: Launch counts (`newwin-config`)

**Files:**
- Create: `crates/config/src/usage.rs`; Modify: `crates/config/src/lib.rs` (`pub mod usage;`, `pub fn usage_path() -> PathBuf` = `%APPDATA%\NewWin\usage.toml`, next to `default_path`)
- Test: `crates/config/tests/usage.rs`

**Interfaces:**
- Produces (`newwin_config::usage`): `#[derive(Clone, Debug, Default, PartialEq)] pub struct Usage` with `load(path: &Path) -> (Usage, Option<String>)` (missing → empty, `None`; unreadable/invalid → empty, `Some(error)`), `get(&self, key: &str) -> u32`, `bump(&mut self, key: &str)` (saturating), `retain(&mut self, keep: impl Fn(&str) -> bool)`, `to_toml(&self) -> String`; `pub fn save_atomic(path: &Path, text: &str) -> std::io::Result<()>` (creates the dir, writes `usage.toml.tmp`, renames over).
- File format: `[launches]` table, keys quoted by `toml`, values integers (`BTreeMap<String, u32>`; values above `u32::MAX` or negative → that entry dropped, not the file).

- [ ] **Step 1: Write the failing tests** `missing_file_is_empty`, `round_trip` (bump `"Microsoft.WindowsTerminal_8wekyb3d8bbwe!App"` twice and `system:lock` once → save_atomic → load → 2 and 1), `corrupt_file_is_empty_with_error` (`"[[[ nope"`), `bump_saturates` (`"[launches]\n\"a\" = 4294967295"` → bump → `u32::MAX`), `out_of_range_entry_dropped` (`"\"a\" = -1"` and `"\"b\" = 5"` → a 0, b 5), `retain_drops_stale_keys`. Use a temp dir under `std::env::temp_dir()` with a unique name.
- [ ] **Step 2: Run** `cargo test -p newwin-config --test usage` — Expected: FAIL.
- [ ] **Step 3: Implement** `usage.rs` (parse into `toml::Table`, read `launches` entries one by one so one bad value drops only itself).
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** `feat: usage.toml launch counts`.

---

### Task 6: Win and Ctrl+Esc in the keyboard hook

**Files:**
- Modify: `crates/core/src/input_lang.rs`, `crates/platform/src/input_hook.rs`, `crates/platform/src/launch.rs` (Win+N tagged), `apps/shell/src/app/layout_view.rs` (new `start` arguments)
- Test: `crates/core/tests/input_lang.rs`

**Interfaces:**
- Produces (`newwin_core::input_lang`): `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum TapAction { Pass, WinTap, CtrlEsc, Swallow }`; `pub struct WinKeyTap` with `new(enabled: bool) -> Self` and `key(&mut self, vk: u16, down: bool) -> TapAction`.
- Produces (`newwin_platform::input_hook`): `pub const NEWWIN_INPUT_TAG: usize = 0x4E57_4E57;`, `pub fn send_tagged(keys: &[(u16, bool)])` (vk, key-up) via `SendInput` with the tag; `InputHook::start(notify: HWND, layout_msg: u32, spotlight_msg: u32, win_key: bool) -> Result<InputHook>`.

**Rules (automaton):** disabled → always `Pass`. Win (`0x5B`/`0x5C`) down → held, not spoiled (repeats keep state). Any other key down while Win is held → spoiled. Win up: held and not spoiled → `WinTap`, else `Pass`; either way released. `Ctrl` (`0x11/0xA2/0xA3`) tracked; `Esc` (`0x1B`) down with Ctrl held and Win not held → `CtrlEsc` and remember to swallow the next `Esc` up → `Swallow`. Everything else `Pass`.

**Hook:** skip (pass on) events whose `dwExtraInfo == NEWWIN_INPUT_TAG`; otherwise feed both `LayoutHotkeys` and `WinKeyTap` (replace the `LLKHF_INJECTED` filter — this fixes deferred M9). `WinTap` → return `LRESULT(1)`, `send_tagged(&[(0xFC,false),(0xFC,true),(vk_of_the_win_key,true)])`, `PostMessage(spotlight_msg)`. `CtrlEsc` → return 1, `send_tagged(&[(0xFC,false),(0xFC,true)])`, post. `Swallow` → return 1. `open_notification_center` uses `send_tagged`.

- [ ] **Step 1: Write the failing tests** in `crates/core/tests/input_lang.rs`: `lone_win_is_a_tap` (down, up → WinTap), `held_win_with_autorepeat_is_one_tap` (down×5, up → one WinTap), `win_combination_is_not_a_tap` (Win down, `E` down → Pass, `E` up, Win up → Pass), `right_win_taps_too`, `ctrl_esc_is_swallowed` (Ctrl down, Esc down → CtrlEsc, Esc up → Swallow, Ctrl up → Pass), `win_ctrl_esc_is_not_ctrl_esc`, `disabled_passes_everything`, `next_tap_after_combination_works`.
- [ ] **Step 2: Run** `cargo test -p newwin-core --test input_lang` — Expected: FAIL.
- [ ] **Step 3: Implement** `WinKeyTap`, then the hook changes, `send_tagged`, the Win+N change and the new `InputHook::start` call in `layout_view.rs` (passing `WM_APP + 14` and `self.config.spotlight.win_key`; the ping handler for `WM_APP + 14` is added in Task 11 — until then it logs `spotlight toggle` at debug).
- [ ] **Step 4: Run** `cargo test --workspace` — Expected: PASS. Live (release build, NewWin running): tap Win → Start does not open, log shows `spotlight toggle`; Win+E, Win+R, Win+N, Win+Space, Alt+Shift still work.
- [ ] **Step 5: Commit** `feat: the keyboard hook takes Win taps and Ctrl+Esc for Spotlight`.

---

### Task 7: App index and change notifications (`newwin-platform`)

**Files:**
- Create: `crates/platform/src/apps_index.rs`; Modify: `crates/platform/src/lib.rs`, `crates/platform/src/worker.rs`, `crates/platform/Cargo.toml` (features `ApplicationModel`, `Foundation`), `apps/shell/src/app.rs` (`Worker::spawn` call)
- Test: `crates/platform/tests/apps_index.rs`

**Interfaces:**
- Consumes: `newwin_core::spotlight::{Candidate, Kind, is_listed}`.
- Produces:
  - `pub fn enumerate_apps() -> windows::core::Result<Vec<Candidate>>` — COM must be initialised on the caller; `Kind::App`; `name` = `SIGDN_NORMALDISPLAY`; `id` = `SIGDN_PARENTRELATIVEPARSING`; `file_path` and `packaged` from the properties chosen in Task 1 (`file_path` only if the file exists); filtered by `is_listed`; duplicate ids dropped (first wins).
  - `pub struct AppsWatch`; `AppsWatch::start(notify: HWND, msg: u32) -> windows::core::Result<AppsWatch>`: `FindFirstChangeNotificationW(dir, TRUE, FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_DIR_NAME)` on `%APPDATA%\Microsoft\Windows\Start Menu\Programs` and `%ProgramData%\Microsoft\Windows\Start Menu\Programs` (a missing dir is skipped), each waited with `RegisterWaitForSingleObject(..., INFINITE, WT_EXECUTEDEFAULT)` whose callback posts `msg` and calls `FindNextChangeNotification`; plus `PackageCatalog::OpenForCurrentUser()` with `PackageInstalling`, `PackageUninstalling`, `PackageUpdating` handlers that post `msg` when `IsComplete()`. `Drop`: `UnregisterWaitEx(wait, INVALID_HANDLE_VALUE)` (blocks until callbacks end), `FindCloseChangeNotification`, remove the event tokens.
  - Worker: `pub enum Job { Name(NameJob), Apps, Run(Box<dyn FnOnce() + Send>) }`; `Worker::spawn(notify: HWND, names_msg: u32, apps_msg: u32)`; `submit(NameJob)` stays; `pub fn submit_job(&self, job: Job)`; `pub struct AppsIndexed { pub apps: Result<Vec<Candidate>, String>, pub ms: u32 }`; `pub unsafe fn take_apps(lp: LPARAM) -> Box<AppsIndexed>`. `Apps` runs `enumerate_apps` and posts `Box<AppsIndexed>` with `apps_msg`.

- [ ] **Step 1: Write the failing tests** `index_has_known_apps` (with `ComGuard::init()`: non-empty; contains a name `Notepad` or `Блокнот`; `Settings`/`Параметри`/`Настройки` with `packaged` true and `file_path` None; at least one candidate with `file_path` ending `.exe` that exists; no duplicate ids; logs the count), `watch_starts_and_stops` (start on a test window, drop — no hang), `worker_posts_apps` (spawn on a test window, `submit_job(Job::Apps)`, pump messages ≤ 10 s, `take_apps` → `Ok`, non-empty).
- [ ] **Step 2: Run** `cargo test -p newwin-platform --test apps_index` — Expected: FAIL.
- [ ] **Step 3: Implement** `apps_index.rs`, the worker jobs, and update the shell's `Worker::spawn(ping, WM_APP_NAME, WM_APP + 16)` (the handler comes in Task 11).
- [ ] **Step 4: Run** `cargo test --workspace` — Expected: PASS.
- [ ] **Step 5: Commit** `feat: Start-menu app index with change notifications`.

---

### Task 8: Launch helpers and clipboard (`newwin-platform`)

**Files:**
- Modify: `crates/platform/src/launch.rs`; Create: `crates/platform/src/clipboard.rs`; Modify: `crates/platform/src/lib.rs`, `crates/platform/Cargo.toml` (feature `Win32_System_Memory`)
- Test: `crates/platform/tests/clock_launch.rs`

**Interfaces:**
- Produces:
  - `pub fn apps_folder_path(id: &str) -> String` → `shell:AppsFolder\<id>`
  - `pub fn open_app(id: &str) -> windows::core::Result<()>` — `ShellExecuteExW`, verb `open`, `SEE_MASK_FLAG_NO_UI`
  - `pub fn run_as_admin(c: &Candidate) -> windows::core::Result<()>` — verb `runas` on the form chosen in Task 1; a declined UAC prompt is an `Err` (`ERROR_CANCELLED`)
  - `pub fn open_file_location(path: &str) -> windows::core::Result<()>` — `SHParseDisplayName` + `SHOpenFolderAndSelectItems`
  - `SystemTarget::Lock` → `Action::Lock` → `LockWorkStation`
  - `newwin_platform::clipboard::text() -> Option<String>` — `CF_UNICODETEXT`, `None` on any failure

- [ ] **Step 1: Write the failing tests** `apps_folder_path_form` (`apps_folder_path("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App") == r"shell:AppsFolder\Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"`), `lock_is_lock_workstation` (`action(SystemTarget::Lock) == Action::Lock`), `open_file_location_of_missing_file_errs` (path in a non-existent temp dir → `Err`, no window).
- [ ] **Step 2: Run** `cargo test -p newwin-platform --test clock_launch` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** `feat: open, run as administrator, open file location, lock`.

---

### Task 9: UI foundations: panel, text, icon, theme (`newwin-ui`)

**Files:**
- Modify: `crates/ui/src/panel.rs`, `crates/ui/src/text.rs`, `crates/ui/src/icons.rs`, `crates/ui/src/theme.rs`
- Test: `crates/ui/tests/panel.rs`, `crates/ui/tests/text.rs`, `crates/ui/tests/icons.rs`

**Interfaces:**
- Produces:
  - `PanelInput::CtrlKey(u16)` — `WM_KEYDOWN` while Ctrl is down (instead of `Key`); `PanelInput::ContextMenu(Option<PointF>)` — `WM_CONTEXTMENU` (client point, `None` for keyboard: lParam `-1`)
  - `PanelContent::wake_at(&self, now_ms: u64) -> Option<u64>` with default `None`; after every paint the panel sets one-shot `WAKE_TIMER = 3` for `wake_at - now` (min 1 ms) or kills it; on fire it repaints
  - `Panel::read<R>(&self, f: impl FnOnce(&C) -> R) -> Option<R>`
  - `TextCache::layout_ranges(&mut self, s: &str, style: &TextStyle, max_width: f32, bold: &[(usize, usize)]) -> Result<TextLayout>` (`layout_emphasis` delegates to it); `TextLayout::caret_x(&self, utf16_pos: usize) -> f32` (`HitTestTextPosition`, position clamped)
  - `icons::magnifier(p: &mut Painter, c: PointF, s: f32, color: Color, w: f32)` — circle of diameter `0.62·s` centred at `c + (-0.1·s, -0.1·s)`, handle to `c + (0.38·s, 0.38·s)`, round caps
  - the theme constants from Global Constraints

- [ ] **Step 1: Write the failing tests** `ctrl_key_and_context_menu_reach_content` (panel with a recording content; `SendMessageW(WM_CONTEXTMENU, hwnd, -1)` → `ContextMenu(None)`), `wake_at_repaints_once` (content counts paints and returns `wake_at = Some(start + 30)` until then, `None` after; after 200 ms of pumping exactly 2 paints, no frame timer), `read_returns_content_state`; `ranges_bold_several` (`layout_ranges("Visual Studio Code", .., &[(0,1),(7,1),(14,1)])` wider than plain; out-of-range clamped), `caret_x_grows` (`caret_x(0) < caret_x(3) < caret_x(len)`, `caret_x(999)` == `caret_x(len)`); `magnifier_stays_in_box` (in the style of the existing icon tests).
- [ ] **Step 2: Run** `cargo test -p newwin-ui --test panel --test text --test icons` — Expected: FAIL.
- [ ] **Step 3: Implement.** `TrayList` and `VolumePanel` ignore `CtrlKey`/`ContextMenu`.
- [ ] **Step 4: Run** `cargo test --workspace` — Expected: PASS.
- [ ] **Step 5: Commit** `feat: panel context menu, wake timer and read access; multi-range bold, caret position, magnifier`.

---

### Task 10: The Spotlight view (`newwin-ui`)

**Files:**
- Create: `crates/ui/src/spotlight.rs`; Modify: `crates/ui/src/lib.rs`
- Test: `crates/ui/tests/spotlight.rs`

**Interfaces:**
- Consumes: Tasks 2–4, 9; `PopupStyle`.
- Produces (`newwin_ui::spotlight`):
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum SpotlightOp { Open = 0, Admin = 1, Menu = 2 }`; `pub fn action_id(index: usize, op: SpotlightOp) -> u32` (`index << 2 | op`); `pub fn decode_action(id: u32) -> (usize, SpotlightOp)` — `index` is into the catalog
  - `pub struct SpotlightLabels { pub hint: String, pub no_results: String, pub kind_app: String, pub kind_system: String, pub press_again_shut_down: String, pub press_again_restart: String, pub keys: String }`
  - `pub struct SpotlightView` implementing `PanelContent`; `new(catalog: Rc<Vec<Candidate>>, launches: HashMap<String, u32>, labels: SpotlightLabels, style: &PopupStyle, max_results: usize, blink_ms: Option<u32>, gfx: &Rc<Gfx>) -> SpotlightView`; `for_test(catalog: Rc<Vec<Candidate>>, max_results: usize) -> SpotlightView` (no text, 96 DPI); `set_catalog(&mut self, c: Rc<Vec<Candidate>>)` (re-ranks, keeps the selection on the same catalog id if still shown, else clamps); `query(&self) -> &str`; `hits(&self) -> &[Hit]`; `selected(&self) -> Option<usize>` (row); `catalog(&self) -> &Rc<Vec<Candidate>>`; `row_rect(&self, row: usize) -> RectF`

**Behaviour** (spec §2.2–2.4, §5): size = (`SPOTLIGHT_WIDTH`, `FIELD_H + rows_h + HINTS_H`) scaled, where `rows_h` = 0 for an empty query, `PAD + n·ROW_H + PAD` for n hits, one row for "No results". Typing (`Char`, `Key` Back/Delete/Left/Right/Home/End, `CtrlKey` Back/`A`/`V` via `clipboard::text()`) edits the query; a text change re-ranks with `rank(query, catalog, launches, max)`, selects row 0 (None when no hits), resets `Confirm`, returns `resize`. `Up`/`Down` wrap. `Key(VK_RETURN)` / left `Release` on a row: system commands go through `Confirm::press` (armed → repaint only, the row shows the "press again" label); otherwise `action = Open`. `CtrlKey(VK_RETURN)` → `Admin` only if `can_run_as_admin`. `ContextMenu`, `RightRelease` on a row (selects it first) → `Menu` only for `Kind::App`. `Move` → hover pill (tweened like `TrayList`). Paint: `SPOTLIGHT_SHADE` over the panel; magnifier + query (`SPOTLIGHT_QUERY_TEXT`) or muted hint; caret at `caret_x`, drawn while the blink phase is on (always on if `blink_ms` is `None`; phase restarts on every edit); separator under the field when rows show; rows: selected pill (tweened `y`), name via `layout_ranges` with hit ranges, kind right-aligned in `TEXT_INACTIVE`, new rows fade in over `ROW_FADE_MS`; hints line centred, `SPOTLIGHT_HINT_TEXT`, `TEXT_INACTIVE`. `wake_at` = min(next blink flip, `Confirm::expires_at`).

- [ ] **Step 1: Write the failing tests** (with `for_test` and a catalog of `Steam`, `Visual Studio Code` (file `.exe`), `Settings` (packaged) and the six system commands):
  - `empty_query_is_just_the_field` (size h == `FIELD_H + HINTS_H`, no hits)
  - `typing_selects_first_row_and_grows` (`Char('s')`… → selected `Some(0)`, `resize` true, height grew)
  - `arrows_wrap`
  - `enter_opens_the_selected_app` (`"ste"` + Return → `action == Some(action_id(index_of_steam, Open))`)
  - `shut_down_needs_two_enters` (`"shut"` + Return → no action, + Return → action Open on Shut down)
  - `ctrl_enter_only_for_classic_apps` (`"settings"` App row + `CtrlKey(Return)` → None; `"visual"` → Admin)
  - `context_menu_only_for_apps` (on Lock → None; on Steam → Menu)
  - `no_results_row` (`"zzzz"` → one row, Return → None)
  - `new_catalog_keeps_selection_in_range` (select row 2 of 3, `set_catalog` with one app → selected `Some(0)`, Return opens that app's new index)
  - `wake_at_follows_confirm` (armed → `wake_at(now) <= now + CONFIRM_MS`)
- [ ] **Step 2: Run** `cargo test -p newwin-ui --test spotlight` — Expected: FAIL.
- [ ] **Step 3: Implement** `spotlight.rs`.
- [ ] **Step 4: Run** `cargo test --workspace` — Expected: PASS.
- [ ] **Step 5: Commit** `feat: Spotlight view`.

---

### Task 11: Wiring Spotlight into the shell (`apps/shell`)

**Files:**
- Create: `apps/shell/src/app/spotlight_view.rs`; Modify: `apps/shell/src/app.rs` (fields, ping and bar handlers, startup), `apps/shell/src/app/menus.rs` (`MenuCtx::SpotlightRow`), `apps/shell/src/app/panels.rs` (`glass_windows`), `apps/shell/src/app/visibility.rs` (`on_timer`)

**Interfaces:**
- Consumes: everything above.
- Produces: `App` fields `apps: Vec<Candidate>`, `catalog: Rc<Vec<Candidate>>` (system + apps), `usage: Usage`, `spotlight: Option<Panel<SpotlightView>>`, `apps_watch: Option<AppsWatch>`; methods `toggle_spotlight()`, `on_spotlight_panel(wp, lp)`, `on_apps_indexed(Box<AppsIndexed>)`, `on_apps_changed()`, `launch_candidate(index: usize, admin: bool)`; pure `fn toggle_opens(open: bool, closing: bool) -> bool` (true when nothing is open or the open one is closing).

**Behaviour:**
- Startup: `Usage::load(usage_path())` (log a warning on `Some(err)`); `catalog` = `system_candidates(strings)`; `worker.submit_job(Job::Apps)`; `AppsWatch::start(ping, WM_APP + 15)` (log on error).
- `WM_APP + 14` → `toggle_spotlight`: if `toggle_opens` → drop any old Spotlight, `close_popup`, `close_panel`, then `Panel::show(SpotlightView::new(catalog.clone(), launches snapshot, labels from strings, popup style, config.spotlight.max_results, GetCaretBlinkTime (None for INFINITE)), anchor = (monitor centre x, work.top + SPOTLIGHT_TOP·work height) with the work area from `SystemParametersInfoW(SPI_GETWORKAREA)`, dpi, bar, WM_APP + 17)`; else `close()`. `sync_glass` after both. No `VisInput`.
- `WM_APP + 17`: `PICKED` → `decode_action` → `Open` → `launch_candidate(i, false)`; `Admin` → `launch_candidate(i, true)`; `Menu` → `Popup` with Open · Run as administrator (if `can_run_as_admin`) · Open file location (if `file_path`), at the cursor for a right click or under the selected row (`row_rect`) for the keyboard, `MenuCtx::SpotlightRow { index }`, Spotlight `hold()`; `CLOSED` of the current one → drop.
- `MenuCtx::SpotlightRow` opens and closes without feeding `VisInput`; a pick runs the item; closing without a pick calls `SetForegroundWindow` on the Spotlight so typing continues.
- `launch_candidate`: App → `open_app` / `run_as_admin`; System → Settings `open_settings_file()`, others `launch::run(SystemTarget::{ShutDown, Restart, Sleep, Lock, SignOut})`. On `Ok`: `usage.bump(usage_key)`, `SetTimer(USAGE_SAVE_TIMER, 2000)`. Either way the Spotlight closes; errors `log::warn!`.
- `USAGE_SAVE_TIMER` → `usage.retain(key in catalog or starts with "system:")` (only once `apps` is non-empty), `worker.submit_job(Job::Run(save_atomic(usage_path(), to_toml())))`.
- `WM_APP + 15` → `SetTimer(APPS_DEBOUNCE_TIMER, 2000)`; on fire → `submit_job(Job::Apps)`.
- `WM_APP + 16` → `Ok(apps)` → rebuild `catalog`, `log::info!("apps indexed: {} in {} ms")`, an open Spotlight gets `set_catalog`; `Err` → `log::warn!`, keep the old list.

- [ ] **Step 1: Write the failing test** `toggle_while_closing_opens_again` (in `spotlight_view.rs` tests: `toggle_opens(false,false)`, `toggle_opens(true,true)` true; `toggle_opens(true,false)` false).
- [ ] **Step 2: Run** `cargo test -p newwin-shell` — Expected: FAIL.
- [ ] **Step 3: Implement** the wiring.
- [ ] **Step 4: Run** `cargo test --workspace`, `cargo clippy --workspace --all-targets` — Expected: PASS. Live (release, `NEWWIN_DEBUG=1`): Win opens and closes Spotlight, the bar stays hidden; `ste` + Enter launches Steam; log shows `apps indexed: N in M ms` and `panel first-frame-ms` < 16.
- [ ] **Step 5: Commit** `feat: Spotlight opens on Win and launches apps and system commands`.

---

### Task 12: Magnifier on the bar

**Files:**
- Modify: `crates/core/src/items.rs` (`RightItem::Search`, parsed from `search`), `apps/shell/src/app/right_view.rs` (width = pill height, paint `icons::magnifier` at `s(16.0)`, left click → `toggle_spotlight`, tooltip `strings.search`)
- Test: `crates/core/tests/items.rs`

- [ ] **Step 1: Update the failing test**: the existing test that expects `search` to be skipped now expects `RightItem::Search` between Volume and Date for the default `items.right`, and `place_right` puts the separator after Search.
- [ ] **Step 2: Run** `cargo test -p newwin-core --test items` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --workspace` — Expected: PASS. Live: the magnifier shows, hover pill and tooltip work, a click opens Spotlight and a second click closes it.
- [ ] **Step 5: Commit** `feat: magnifier on the bar opens Spotlight`.

---

### Task 13: Review, live checks, measurements

**Files:**
- Create: `docs/superpowers/notes/2026-10-22-m3-acceptance.md`, a handoff note for the next chat
- Delete: the three probe examples from Task 1

- [ ] **Step 1: Whole-branch review** by a fresh reviewer (`superpowers:requesting-code-review`, diff `main..m3-spotlight`); fix Critical/Important findings with tests, list Minor ones as deferred in the acceptance note.
- [ ] **Step 2: Live checklist with the user** (spec §8), recording each result in the acceptance note: Win opens/closes, Start never appears; Win+E/R/Shift+S/N/Space, Alt+Shift; `Ctrl+Esc`; magnifier; focus over Explorer, a browser, a full-screen app; `ste`, `code`, `vsc`, `іеуфь`; Enter, Ctrl+Enter (UAC), row menu — all three items, `cmd` → Open file location selects `cmd.exe`; Shut down / Restart show the "press again" text and reset (do not press the second Enter); Lock; install or remove a small app → it shows/vanishes in ~2 s.
- [ ] **Step 3: Measurements** — `powershell -File scripts\measure.ps1 -Minutes 10` with NewWin without `NEWWIN_DEBUG`, the user away; Win → first frame from the log (5 opens); ranking bench from Task 2 in release. Record against the budget.
- [ ] **Step 4: Run** `cargo test --workspace` and `cargo clippy --workspace --all-targets` — Expected: PASS.
- [ ] **Step 5: Commit** the notes and the probe removal; ask the user before merging into `main`.
