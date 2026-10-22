# M2: Right Side of the Bar and the Tray — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The bar gains a tray (`⌃` → glass text list of background apps, backed by our own `Shell_TrayWnd` that also forwards every message to explorer) and a right side with keyboard layout, network, volume (with output-device switching) and a clickable date — all in liquid glass, animated, at 0 % idle CPU.

**Architecture:** Pure logic (tray protocol parsing and model, tweens, audio/network/layout mapping, right-side placement) lives in `newwin-core` with unit tests. `newwin-platform` gets the tray host window (own re-entrant wndproc, forwarding thread), Core Audio + `IPolicyConfig`, IP Helper network hints, keyboard layout and a low-level keyboard hook thread. `newwin-ui` gets vector icons, a generic glass `Panel<C>` popover, a tooltip and the panel contents (tray list, volume). `apps/shell` wires it together; every source is event-driven and reaches the UI thread through `PostMessage`.

**Tech Stack:** Rust 2024, `windows` 0.62 (Win32 Shell, Core Audio, IP Helper, WLAN, DirectComposition/Direct2D/DirectWrite), WARP rendering.

**Spec:** `docs/superpowers/specs/2026-10-22-newwin-m2-tray-design.md` (extends `docs/superpowers/specs/2026-10-21-newwin-design.md` §7–8 and `docs/superpowers/specs/2026-10-22-newwin-liquid-glass-design.md`). Reference implementation for the tray protocol: ManagedShell / RetroBar (C#, `TrayService`), read only for layouts — do not copy code.

## Global Constraints

- Work on branch `m2-tray` (already created from `main`; the spec commit `4ac8133` is on it).
- Build tools: `$env:Path="$env:USERPROFILE\.cargo\bin;$env:Path"`; git at `C:\Programs\Git\cmd\git.exe`. Stop NewWin before release builds: `Get-Process NewWin -ErrorAction SilentlyContinue | Stop-Process -Force`. If cargo skips a rebuild after a clock jump, `cargo clean`.
- `cargo test --workspace` and `cargo clippy --workspace --all-targets` stay green after every task. TDD: every behaviour test is written and seen failing first.
- Release builds use `panic = "abort"`: **nothing that comes from another process may panic** (`WM_COPYDATA`, tips, device names). Parse with `Result`/`Option`, never index unchecked.
- No polling. Allowed timers: one-shot timers (debounce, tooltip delay, layout re-check), and a 16 ms frame timer that runs only while a tween is running.
- Threads talk only through `PostMessage(WM_APP+n)` with an owned `Box` in LPARAM where data is needed (spec §4 of the main design). New messages: `WM_APP+7` tray changed, `WM_APP+8` audio, `WM_APP+9` network, `WM_APP+10` layout hotkey, `WM_APP+11` panel, `WM_APP+12` explorer reorder. New bar timers: `ANIM_TIMER = 5`, `TOOLTIP_TIMER = 6`, `NET_DEBOUNCE_TIMER = 7`, `LAYOUT_RECHECK_TIMER = 8`.
- Look: everything new uses `Backdrop::Glass` + `paint_glass`. New constants in `crates/ui/src/theme.rs`: `PANEL_RADIUS = 16.0`, `PANEL_SHOW_MS = 150`, `PANEL_HIDE_MS = 120`, `PANEL_DROP_PX = 6.0`, `HOVER_GLIDE_MS = 140`, `ICON_STROKE = 1.35`, `TOOLTIP_DELAY_MS = 500`, `TOOLTIP_TEXT = 12.0`. Hover pill colour is the existing `theme::HOVER_PILL`; text colours are the existing `TEXT`, `TEXT_STRONG`, `TEXT_INACTIVE`.
- Monochrome, text only: no colour icons anywhere; the tray never stores or draws `HICON`s.
- Strings go through `newwin_core::strings::Strings` (`en` and `uk`).
- Budget (spec §7): idle 10 min with the bar hidden = 0 % CPU and no wakeups beyond M1's; shell private working set < 20 MB; first frame of a panel < 16 ms after the click; idle new threads never wake.
- Never launch Screen Snipping, never press Shut down / Restart / Sign out / Sleep. The user checks looks; glass windows are invisible to DXGI screenshots.
- Commit messages through a file (`git commit -F`), ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **Our own pinned AppBar talks to our tray host.** `SHAppBarMessage` from inside `App`'s handler now lands in our `Shell_TrayWnd`; it must be forwarded to explorer synchronously and its reply returned, so pinned mode still reserves space. Test in Task 4 (`appbar_message_forwarded_while_caller_is_busy`) and live in Task 5.
2. **Hostile or truncated `WM_COPYDATA`** (wrong signature, `cbSize` larger than the payload, empty buffer, tip without NUL) → ignored or read only within bounds, no panic, model unchanged. Tests in Task 1 (`rejects_*`, `tip_without_nul_stops_at_field_end`) and Task 4 (`garbage_copydata_returns_false`).
3. **Explorer hung, restarted or gone** → forwarding times out on its own thread, the UI never blocks; after `TaskbarCreated` icons come back to us. Tests in Task 4 (`forward_to_dead_window_does_not_block`) and Task 5 live check.
4. **A tray app dies without `NIM_DELETE`** → no ghost row the next time the tray opens. Test in Task 1 (`prune_drops_dead_windows`), wired in Task 9.
5. **No audio device, default device unplugged, audio service restarted** → speaker icon "no sound", panel row `No output devices`, no crash, recovers on the next device event. Tests in Task 10 (`no_device_maps_to_unavailable`) and Task 11 live check.

---

### Task 1: Tray protocol and model (`newwin-core`)

**Files:**
- Create: `crates/core/src/tray.rs`
- Modify: `crates/core/src/lib.rs` (`pub mod tray;`)
- Test: `crates/core/tests/tray.rs`

**Interfaces:**
- Produces (`newwin_core::tray`):
  - consts `TRAY_SIGNATURE: u32 = 0x3475_3423`, `NIF_MESSAGE = 0x1`, `NIF_TIP = 0x4`, `NIF_STATE = 0x8`, `NIF_GUID = 0x20`, `NIS_HIDDEN = 0x1`, `NID_V1_SIZE: usize = 152`, `NID_V2_SIZE: usize = 936`, `NID_V3_SIZE: usize = 952`, `NID_FULL_SIZE: usize = 956`
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum TrayCommand { Add, Modify, Delete, SetFocus, SetVersion }` (NIM_ 0..=4)
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)] pub enum IconKey { Id { hwnd: u32, id: u32 }, Guid(u128) }`
  - `#[derive(Clone, Debug, PartialEq)] pub struct NotifyData { pub command: TrayCommand, pub hwnd: u32, pub key: IconKey, pub flags: u32, pub callback_message: u32, pub tip: Option<String>, pub state: u32, pub state_mask: u32, pub version: u32 }`
  - `#[derive(Debug, PartialEq, Eq)] pub enum ParseError { TooShort, BadSignature, BadCommand(u32), BadSize(u32) }`
  - `pub fn parse_notify(bytes: &[u8]) -> Result<NotifyData, ParseError>`
  - `#[derive(Clone, Debug, PartialEq)] pub struct TrayIcon { pub key: IconKey, pub hwnd: u32, pub pid: u32, pub callback_message: u32, pub tip: String, pub version: u32, pub hidden: bool, pub seq: u64 }`
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Change { Added, Updated, Removed, Ignored, Focus }`
  - `#[derive(Default)] pub struct TrayModel` with `apply(&mut self, d: &NotifyData, pid: u32) -> Change`, `icons(&self) -> &[TrayIcon]` (in `seq` order), `get(&self, key: &IconKey) -> Option<&TrayIcon>`, `prune(&mut self, alive: impl Fn(u32) -> bool) -> bool`, `clear(&mut self)`
  - `pub fn row_label(tip: &str, fallback: &str) -> String`
  - `pub fn match_range(label: &str, query: &str) -> Option<(usize, usize)>` — UTF-16 (start, len)

**Wire format** (the 32-bit `NOTIFYICONDATA` that shell32 puts after an 8-byte header; offsets from the start of the buffer):

| Offset | Field |
|---|---|
| 0 | `dwSignature` = `TRAY_SIGNATURE` |
| 4 | `dwMessage` (NIM_*) |
| 8 | `cbSize` |
| 12 | `hWnd` (u32) |
| 16 | `uID` |
| 20 | `uFlags` |
| 24 | `uCallbackMessage` |
| 28 | `hIcon` (ignored) |
| 32 | `szTip` — 64 UTF-16 units when `cbSize < NID_V2_SIZE`, else 128 |
| 288 | `dwState` (only if `cbSize >= NID_V2_SIZE`) |
| 292 | `dwStateMask` |
| 808 | `uVersion` |
| 944 | `guidItem` (16 bytes, read as `u128::from_le_bytes`; only if `cbSize >= NID_V3_SIZE`) |

Rules: `bytes.len() < 8 + NID_V1_SIZE` → `TooShort`; signature mismatch → `BadSignature`; NIM > 4 → `BadCommand`; the readable size is `min(cbSize, bytes.len() - 8)` and must be `>= NID_V1_SIZE` else `BadSize(cbSize)`; a field is read only when it lies wholly inside the readable size, otherwise it is 0/`None`. The tip is read when `NIF_TIP` is set: UTF-16 up to the first NUL or the field end, lossy-decoded. The key is `Guid` when `NIF_GUID` is set and the GUID was readable, else `Id { hwnd, id }`.

**Model rules:** `Add` of a new key creates the icon (`callback_message` if `NIF_MESSAGE`, tip if `NIF_TIP` else `""`, `hidden` from state if `NIF_STATE` and the mask has `NIS_HIDDEN`, `version` 0, next `seq`) → `Added`; `Add` of an existing key behaves like `Modify` → `Updated`. `Modify` of an unknown key → `Ignored`; otherwise updates only the flagged fields and `hwnd` → `Updated`. `Delete` → `Removed` / `Ignored`. `SetVersion` sets `version` on a known key → `Updated`, else `Ignored`. `SetFocus` → `Focus`.

**Labels:** `row_label` takes the first line of `tip` (split on `\r` or `\n`), cuts it at the first of `" - "`, `" – "`, `" — "`, trims; if that is empty, returns `fallback.trim()`. `match_range` is case-insensitive per `char` (`to_lowercase`), returns the first match in UTF-16 units; an empty query returns `Some((0, 0))`.

- [ ] **Step 1: Write the failing tests** in `crates/core/tests/tray.rs`:

```rust
use newwin_core::tray::*;

/// A TRAYNOTIFYDATA buffer as shell32 sends it.
fn nid(msg: u32, cb: u32, hwnd: u32, id: u32, flags: u32, cbmsg: u32, tip: &str, state: (u32, u32), version: u32, guid: Option<u128>) -> Vec<u8> {
    let mut b = vec![0u8; 8 + NID_FULL_SIZE];
    let put = |b: &mut Vec<u8>, at: usize, v: u32| b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    put(&mut b, 0, TRAY_SIGNATURE);
    put(&mut b, 4, msg);
    put(&mut b, 8, cb);
    put(&mut b, 12, hwnd);
    put(&mut b, 16, id);
    put(&mut b, 20, flags);
    put(&mut b, 24, cbmsg);
    for (i, u) in tip.encode_utf16().enumerate() {
        b[32 + 2 * i..34 + 2 * i].copy_from_slice(&u.to_le_bytes());
    }
    put(&mut b, 288, state.0);
    put(&mut b, 292, state.1);
    put(&mut b, 808, version);
    if let Some(g) = guid {
        b[944..960].copy_from_slice(&g.to_le_bytes());
    }
    b
}

const FULL: u32 = NID_FULL_SIZE as u32;

#[test]
fn parses_add_with_tip_and_message() {
    let d = parse_notify(&nid(0, FULL, 0x1234, 7, NIF_MESSAGE | NIF_TIP, 0x8001, "Telegram", (0, 0), 0, None)).unwrap();
    assert_eq!(d.command, TrayCommand::Add);
    assert_eq!(d.key, IconKey::Id { hwnd: 0x1234, id: 7 });
    assert_eq!(d.callback_message, 0x8001);
    assert_eq!(d.tip.as_deref(), Some("Telegram"));
}

#[test]
fn guid_icons_are_keyed_by_guid() {
    let g = 0x0011_2233_4455_6677_8899_aabb_ccdd_eeffu128;
    let d = parse_notify(&nid(1, FULL, 5, 1, NIF_GUID, 0, "", (0, 0), 0, Some(g))).unwrap();
    assert_eq!(d.key, IconKey::Guid(g));
}

#[test]
fn v1_size_reads_a_64_unit_tip_and_no_guid() {
    let long: String = "x".repeat(100);
    let mut b = nid(0, NID_V1_SIZE as u32, 5, 1, NIF_TIP | NIF_GUID, 0, &long, (0, 0), 0, Some(9));
    b.truncate(8 + NID_V1_SIZE);
    let d = parse_notify(&b).unwrap();
    assert_eq!(d.tip.as_deref().map(str::len), Some(64));
    assert_eq!(d.key, IconKey::Id { hwnd: 5, id: 1 });
}

#[test]
fn tip_without_nul_stops_at_field_end() {
    let d = parse_notify(&nid(0, FULL, 5, 1, NIF_TIP, 0, &"y".repeat(128), (0, 0), 0, None)).unwrap();
    assert_eq!(d.tip.unwrap().len(), 128);
}

#[test]
fn set_version_reads_uversion() {
    let d = parse_notify(&nid(4, FULL, 5, 1, 0, 0, "", (0, 0), 4, None)).unwrap();
    assert_eq!((d.command, d.version), (TrayCommand::SetVersion, 4));
}

#[test]
fn rejects_short_bad_signature_bad_command_and_lying_size() {
    assert_eq!(parse_notify(&[]), Err(ParseError::TooShort));
    let mut b = nid(0, FULL, 5, 1, 0, 0, "", (0, 0), 0, None);
    b[0] = 0;
    assert_eq!(parse_notify(&b), Err(ParseError::BadSignature));
    assert_eq!(parse_notify(&nid(9, FULL, 5, 1, 0, 0, "", (0, 0), 0, None)), Err(ParseError::BadCommand(9)));
    assert_eq!(parse_notify(&nid(0, 40, 5, 1, 0, 0, "", (0, 0), 0, None)), Err(ParseError::BadSize(40)));
    // cbSize claims more than was sent: fields past the end are simply absent.
    let mut cut = nid(0, FULL, 5, 1, NIF_GUID, 0, "", (0, 0), 0, Some(3));
    cut.truncate(8 + 600);
    assert_eq!(parse_notify(&cut).unwrap().key, IconKey::Id { hwnd: 5, id: 1 });
}

fn add(m: &mut TrayModel, hwnd: u32, id: u32, tip: &str) -> Change {
    m.apply(&parse_notify(&nid(0, FULL, hwnd, id, NIF_TIP | NIF_MESSAGE, 0x400, tip, (0, 0), 0, None)).unwrap(), 42)
}

#[test]
fn model_add_modify_delete_in_order() {
    let mut m = TrayModel::default();
    assert_eq!(add(&mut m, 1, 1, "A"), Change::Added);
    assert_eq!(add(&mut m, 2, 1, "B"), Change::Added);
    assert_eq!(add(&mut m, 1, 1, "A2"), Change::Updated);
    assert_eq!(m.icons().iter().map(|i| i.tip.as_str()).collect::<Vec<_>>(), ["A2", "B"]);
    assert_eq!(m.icons()[0].pid, 42);
    let modify_unknown = parse_notify(&nid(1, FULL, 9, 9, NIF_TIP, 0, "Z", (0, 0), 0, None)).unwrap();
    assert_eq!(m.apply(&modify_unknown, 1), Change::Ignored);
    let del = parse_notify(&nid(2, FULL, 1, 1, 0, 0, "", (0, 0), 0, None)).unwrap();
    assert_eq!(m.apply(&del, 1), Change::Removed);
    assert_eq!(m.apply(&del, 1), Change::Ignored);
    assert_eq!(m.icons().len(), 1);
}

#[test]
fn hidden_state_follows_mask() {
    let mut m = TrayModel::default();
    add(&mut m, 1, 1, "A");
    let hide = parse_notify(&nid(1, FULL, 1, 1, NIF_STATE, 0, "", (NIS_HIDDEN, NIS_HIDDEN), 0, None)).unwrap();
    m.apply(&hide, 42);
    assert!(m.icons()[0].hidden);
    let other_bit = parse_notify(&nid(1, FULL, 1, 1, NIF_STATE, 0, "", (0, 0x2), 0, None)).unwrap();
    m.apply(&other_bit, 42);
    assert!(m.icons()[0].hidden, "bits outside the mask are untouched");
}

#[test]
fn modify_without_tip_flag_keeps_tip() {
    let mut m = TrayModel::default();
    add(&mut m, 1, 1, "Keep");
    m.apply(&parse_notify(&nid(1, FULL, 1, 1, 0, 0, "Drop", (0, 0), 0, None)).unwrap(), 42);
    assert_eq!(m.icons()[0].tip, "Keep");
}

#[test]
fn prune_drops_dead_windows() {
    let mut m = TrayModel::default();
    add(&mut m, 1, 1, "A");
    add(&mut m, 2, 1, "B");
    assert!(m.prune(|h| h == 2));
    assert_eq!(m.icons().len(), 1);
    assert!(!m.prune(|_| true));
}

#[test]
fn labels() {
    assert_eq!(row_label("Steam - Online", "steam"), "Steam");
    assert_eq!(row_label("Telegram\n3 unread", ""), "Telegram");
    assert_eq!(row_label("NVIDIA Settings – Ready", ""), "NVIDIA Settings");
    assert_eq!(row_label("  ", " Discord "), "Discord");
    assert_eq!(row_label("", ""), "");
}

#[test]
fn match_ranges_are_utf16_and_case_insensitive() {
    assert_eq!(match_range("Telegram", "GRAM"), Some((4, 4)));
    assert_eq!(match_range("Телеграм", "теле"), Some((0, 4)));
    assert_eq!(match_range("Steam", ""), Some((0, 0)));
    assert_eq!(match_range("Steam", "x"), None);
    assert_eq!(match_range("😀 Discord", "dis"), Some((3, 3)));
}
```

- [ ] **Step 2: Run** `cargo test -p newwin-core --test tray` → FAIL (module `tray` not found).
- [ ] **Step 3: Implement `crates/core/src/tray.rs`** per the interfaces, format table and rules above. Read little-endian with a `fn u32_at(b: &[u8], at: usize) -> Option<u32>` helper (`b.get(at..at + 4)`), never slice-index.
- [ ] **Step 4: Run** `cargo test -p newwin-core --test tray` → PASS; `cargo clippy --workspace --all-targets` clean.
- [ ] **Step 5: Commit** `feat(core): tray protocol parsing, icon model, labels and filter`.

---

### Task 2: Risk probes (throwaway, user checkpoint)

**Files:**
- Create: `crates/platform/examples/tray_probe.rs`, `crates/platform/examples/win_n_probe.rs`, `crates/platform/examples/audio_switch_probe.rs`
- Create: `docs/superpowers/notes/2026-10-22-m2-probes.md`
- Modify: `crates/platform/Cargo.toml` (add the `windows` features the probes need: `Win32_Media_Audio`, `Win32_Media_Audio_Endpoints`, `Win32_Devices_FunctionDiscovery`, `Win32_System_DataExchange`)

**Interfaces:**
- Consumes: `newwin_core::tray::{parse_notify, TrayModel}` (Task 1).
- Produces: the notes file with a yes/no per question below. Task 14 runs only if 2b is "no". Tasks 4–5 change course if 2a is "no" (stop and report to the user).

Probes are labelled throwaway in their file header (`//! Throwaway probe for M2 risk …; not used by NewWin.`). They stay in `examples/` for later re-runs.

- [ ] **Step 1: 2a — tray host probe.** `tray_probe` hides nothing; it creates a hidden top-level `Shell_TrayWnd` window (`WS_POPUP`, `WS_EX_TOOLWINDOW | WS_EX_TOPMOST`) with a raw wndproc, checks `FindWindowW("Shell_TrayWnd") == ours` and logs it, broadcasts `TaskbarCreated`, and for 60 s logs every `WM_COPYDATA`: `dwData`, `cbData`, and for `dwData == 1` the parsed `NotifyData` (Task 1) plus `GetWindowThreadProcessId` exe name. Every `dwData == 1` buffer is also forwarded unchanged to explorer's own tray (the `Shell_TrayWnd` whose pid is not ours) with `SendMessageTimeoutW(SMTO_ABORTIFHUNG, 500)`; `dwData == 0` and `3` are forwarded the same way synchronously and explorer's reply is returned and logged. It also logs whether `FindWindowW` still returns ours at the end. On exit it destroys the window and broadcasts `TaskbarCreated` again.
- [ ] **Step 2: Run the probe with the user** (`cargo run -p newwin-platform --example tray_probe`), while NVIDIA, Telegram, Discord and Steam are running. Record in the notes: which apps registered, their `cbSize` values, whether explorer's tray still shows their icons afterwards (balloon/toast path intact), whether the `dwData == 3` buffer layout matches `{magic u32, message u32 (1 = top-left, 2 = bottom-right), cbSize u32, pad u32, hWnd u64, uID u32, guidItem 16 bytes}` (log the raw hex), and whether `FindWindow` flipped back to explorer during the minute.
- [ ] **Step 3: 2b — Win+N probe.** `win_n_probe` hides the native taskbar (`NativeTaskbar::hide` to a temp state file), waits 1 s, sends Win+N with `SendInput` (LWIN down, N down, N up, LWIN up), waits 5 s, restores the taskbar. The user reports whether the notification center with the calendar opened. Record yes/no.
- [ ] **Step 4: 2c — audio switch probe.** `audio_switch_probe` lists active render endpoints (`IMMDeviceEnumerator::EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)` + `PKEY_Device_FriendlyName`), switches the default to the next one through `IPolicyConfig` (declaration below) for `eConsole`, `eMultimedia`, `eCommunications`, waits 3 s, switches back. Record whether sound moved and back.

```rust
// CLSID CPolicyConfigClient
const CLSID_POLICY_CONFIG: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);
#[windows::core::interface("f8679f50-850a-41cf-9c72-430f290290c8")]
unsafe trait IPolicyConfig: IUnknown {
    // Slots before SetDefaultEndpoint are never called; they only keep the vtable order.
    fn GetMixFormat(&self, id: PCWSTR, fmt: *mut *mut c_void) -> HRESULT;
    fn GetDeviceFormat(&self, id: PCWSTR, default: i32, fmt: *mut *mut c_void) -> HRESULT;
    fn ResetDeviceFormat(&self, id: PCWSTR) -> HRESULT;
    fn SetDeviceFormat(&self, id: PCWSTR, endpoint: *mut c_void, mix: *mut c_void) -> HRESULT;
    fn GetProcessingPeriod(&self, id: PCWSTR, default: i32, def: *mut i64, min: *mut i64) -> HRESULT;
    fn SetProcessingPeriod(&self, id: PCWSTR, period: *mut i64) -> HRESULT;
    fn GetShareMode(&self, id: PCWSTR, mode: *mut c_void) -> HRESULT;
    fn SetShareMode(&self, id: PCWSTR, mode: *mut c_void) -> HRESULT;
    fn GetPropertyValue(&self, id: PCWSTR, key: *const c_void, value: *mut c_void) -> HRESULT;
    fn SetPropertyValue(&self, id: PCWSTR, key: *const c_void, value: *mut c_void) -> HRESULT;
    fn SetDefaultEndpoint(&self, id: PCWSTR, role: ERole) -> HRESULT;
    fn SetEndpointVisibility(&self, id: PCWSTR, visible: i32) -> HRESULT;
}
```

- [ ] **Step 5: Write `docs/superpowers/notes/2026-10-22-m2-probes.md`** with the three answers and the raw facts above. If 2a failed (apps do not find our window, or forwarding breaks explorer's tray), **stop and report to the user** before Task 3.
- [ ] **Step 6: Commit** `chore: M2 risk probes and their results`.

---

### Task 3: Find explorer's own taskbar, never ours

**Files:**
- Modify: `crates/platform/src/taskbar.rs` (`primary_tray`, `all_trays`, `watch_taskbar_show`)
- Test: `crates/platform/tests/taskbar.rs` (add a test)

**Interfaces:**
- Produces: `pub fn explorer_tray() -> Option<HWND>` in `newwin_platform::taskbar` — the top-level `Shell_TrayWnd` whose owning process is not the current one. `primary_tray()` returns it (error if none); `all_trays()` skips windows of the current process; `watch_taskbar_show` takes the pid from `explorer_tray()`.

- [ ] **Step 1: Write the failing test** `explorer_tray_skips_windows_of_this_process`: create a hidden, non-topmost window of class `Shell_TrayWnd` in the test process (via `create_window` with class `"Shell_TrayWnd"`), then assert `explorer_tray()` is `Some(h)` with `h != ours` and `GetWindowThreadProcessId(h)` ≠ `GetCurrentProcessId()`. Destroy the window at the end. (Non-topmost keeps it below explorer's, so other apps are not disturbed while the test runs.)
- [ ] **Step 2: Run** `cargo test -p newwin-platform --test taskbar explorer_tray` → FAIL (`explorer_tray` not found).
- [ ] **Step 3: Implement** `explorer_tray` with `EnumWindows` + class name + pid filter; switch the three call sites to it.
- [ ] **Step 4: Run** `cargo test -p newwin-platform --test taskbar` → PASS (existing taskbar tests too).
- [ ] **Step 5: Commit** `fix(platform): the native taskbar is looked up by process, ignoring our own tray window`.

---

### Task 4: Tray host window (`newwin-platform`)

**Files:**
- Create: `crates/platform/src/tray_host.rs`
- Modify: `crates/platform/src/lib.rs` (`pub mod tray_host;`), `crates/platform/Cargo.toml` (feature `Win32_System_DataExchange` for `COPYDATASTRUCT`)
- Test: `crates/platform/tests/tray_host.rs`

**Interfaces:**
- Consumes: `newwin_core::tray::*` (Task 1), `taskbar::explorer_tray` (Task 3), `taskbar::taskbar_created_message`.
- Produces (`newwin_platform::tray_host`):
  - `pub enum Forward { Explorer, Window(HWND), Nowhere }`
  - `pub struct HostOptions { pub class: &'static str, pub topmost: bool, pub forward: Forward }` — production uses `{ class: "Shell_TrayWnd", topmost: true, forward: Forward::Explorer }`; tests use their own class and `Forward::Window`.
  - `pub struct TrayHost` with:
    - `pub fn create(opts: HostOptions, notify: HWND, msg: u32) -> Result<TrayHost>` — after every model change other than `Ignored` it posts `msg` to `notify` with `wParam` = the `Change` as `usize` (`Added=0, Updated=1, Removed=2, Focus=4`).
    - `pub fn hwnd(&self) -> HWND`
    - `pub fn icons(&self) -> Vec<TrayIcon>` (snapshot)
    - `pub fn prune(&self) -> bool` — `IsWindow` on every icon's `hwnd`
    - `pub fn set_anchor(&self, r: RECT)` — the rectangle answered to `Shell_NotifyIconGetRect`
    - `pub fn ensure_on_top(&self) -> bool` — when `topmost`: if `FindWindowW(class)` is not ours, `SetWindowPos(HWND_TOPMOST, SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE)` and re-check; returns whether we are first
    - `pub fn announce(&self)` — `PostMessageW(HWND_BROADCAST, TaskbarCreated)`
    - `pub fn explorer_restarted(&self)` — re-resolve the forward target, `clear()` the model, `ensure_on_top`, `announce`
    - `Drop`: destroy the window, then `announce` so apps go back to explorer
  - `pub enum TrayClick { Left, Right, Key }`
  - `pub fn send_click(icon: &TrayIcon, click: TrayClick, at: POINT)` — v4: `PostMessage(hwnd, cb, MAKEWPARAM(x, y), MAKELPARAM(event, id))` with event `NIN_SELECT = 0x400` / `WM_CONTEXTMENU` / `NIN_KEYSELECT = 0x401`; older: `wParam = id`, `lParam` = `WM_LBUTTONDOWN` then `WM_LBUTTONUP` (or the R pair). Calls `AllowSetForegroundWindow(icon.pid)` first. For `IconKey::Guid` icons `id` is 0.

**Design decisions:**
- **Own wndproc, not `window::Handler`.** State lives in a `thread_local! { static HOSTS: RefCell<Vec<HostState>> }` keyed by `HWND`; the wndproc borrows only around parsing/model updates and never while calling out. A synchronous forward (`SendMessageTimeoutW`) is made with no borrow held, so a re-entrant `WM_COPYDATA` during it is handled normally.
- `WM_COPYDATA`: read `COPYDATASTRUCT` from `lParam`; if `lpData` is null or `cbData > 65_536` → return `FALSE`. `dwData == 1`: parse; on `Err` return `FALSE`; apply with `pid` from `GetWindowThreadProcessId(HWND(hwnd))`; queue a copy of the bytes for the forwarder; return `TRUE` unless `Ignored`. `dwData == 3`: answer `MAKELONG(left, top)` for message 1, `MAKELONG(right, bottom)` for 2 (layout as recorded in the Task 2 notes; if the notes differ, follow the notes). Anything else: forward synchronously to the target with `SendMessageTimeoutW(SMTO_ABORTIFHUNG, 1000)` and return its result (0 on timeout or no target). Never forward to our own window.
- **Forwarder thread** `newwin-tray-fwd`: `mpsc::Receiver<Vec<u8>>`, target in an `AtomicIsize`; for each buffer builds a `COPYDATASTRUCT { dwData: 1, … }` and calls `SendMessageTimeoutW(target, WM_COPYDATA, wParam = host hwnd, SMTO_ABORTIFHUNG | SMTO_BLOCK, 500)`. Sleeps on the channel otherwise.
- `create` also makes a child window of class `TrayNotifyWnd` (same wndproc, no behaviour) for old apps that look it up under `Shell_TrayWnd`.
- `create` for `topmost` calls `ensure_on_top` and logs `tray host first in z-order: {bool}`.
- `pub fn watch_reorder(notify: HWND, msg: u32) -> Result<WinEventHookGuard>` in the same module: `SetWinEventHook(EVENT_OBJECT_REORDER, …, explorer pid, WINEVENT_OUTOFCONTEXT)`; the callback only posts `msg`.

- [ ] **Step 1: Write the failing tests** in `crates/platform/tests/tray_host.rs`. Helpers: `fn copydata(to: HWND, data: usize, bytes: &[u8]) -> isize` (sends `WM_COPYDATA` with `SendMessageW`), a recording "fake explorer" window (class `NewWin.TestExplorer`) that stores every `WM_COPYDATA` it gets and answers `0x55` to `dwData == 0`, and the `nid` builder from Task 1 copied into the test. Run the message loop with `PeekMessageW` between steps. Tests:
  - `add_then_delete_updates_model_and_notifies` — host `{ class: "NewWin.TestTray", topmost: false, forward: Window(fake) }`; send an Add → returns 1, `icons().len() == 1`, a posted `msg` with `wParam == 0` arrives at the notify window; Delete → `icons()` empty.
  - `notify_messages_are_forwarded_in_order` — three Adds with tips "1","2","3"; after pumping, the fake received three `dwData == 1` buffers whose tips parse as "1","2","3".
  - `appbar_message_forwarded_while_caller_is_busy` — from inside a `Handler` of another window (busy `RefCell`), send `dwData == 0` to the host; the call returns `0x55` from the fake.
  - `garbage_copydata_returns_false` — `dwData == 1` with 3 random bytes, and with a null `lpData` → both return 0, model empty, no notification.
  - `forward_to_dead_window_does_not_block` — fake destroyed before an Add; the Add returns within 50 ms and the model has the icon.
  - `prune_removes_icons_of_destroyed_windows` — add an icon whose `hwnd` is a real test window, destroy that window, `prune()` → true, model empty.
- [ ] **Step 2: Run** `cargo test -p newwin-platform --test tray_host` → FAIL (module missing).
- [ ] **Step 3: Implement `crates/platform/src/tray_host.rs`** per the interfaces and design decisions.
- [ ] **Step 4: Run** `cargo test -p newwin-platform --test tray_host` → PASS; full `cargo test --workspace` green.
- [ ] **Step 5: Commit** `feat(platform): tray host window with forwarding to explorer`.

---

### Task 5: Tray host in the shell

**Files:**
- Modify: `apps/shell/src/app.rs` (field `tray_host: Option<TrayHost>`, `reorder_watch: Option<WinEventHookGuard>`, message constants `WM_APP_TRAY = WM_APP + 7`, `WM_APP_REORDER = WM_APP + 12`, start/shutdown order, `on_ping_message` cases)

**Interfaces:**
- Consumes: `TrayHost`, `HostOptions`, `Forward`, `watch_reorder` (Task 4).
- Produces: `App::tray_host` alive for the shell's lifetime; `App::on_tray_changed(&mut self, change: usize)` (empty body until Task 9 fills it).

Order rules (spec §5.1, §5.4):
- **Start:** after `NativeTaskbar::hide` and **before** `AppBar::register` (so our own registration already goes through the host and is proven to work): create the host with notify = `ping`, then `watch_reorder(ping, WM_APP_REORDER)`.
- `WM_APP_REORDER` → `host.ensure_on_top()`.
- **Explorer restart** (`taskbar_created` in `on_ping_message`): after `reapply_taskbar(true)`: `host.explorer_restarted()`, recreate `reorder_watch`, then `on_explorer_restarted()` (AppBar). The host broadcasts `TaskbarCreated` too; our own ping window receives it — ignore a `TaskbarCreated` that arrives within 2 s after our own `announce` (store `last_announce: Instant`) so it does not loop.
- **Shutdown** (`run_shell` after the loop, and `QUIT`): drop the host **before** `restore_taskbar()` so restoring talks to explorer directly.

- [ ] **Step 1: Write the failing test** `own_announce_is_not_an_explorer_restart` in `apps/shell/src/app.rs` tests: a pure helper `fn is_own_announce(last: Option<Instant>, now: Instant) -> bool` (true within 2 s).
- [ ] **Step 2: Run** `cargo test -p newwin-shell own_announce` → FAIL.
- [ ] **Step 3: Implement** the helper, the fields, the start/restart/shutdown order above, and log `tray host up, first in z-order: {bool}`.
- [ ] **Step 4: Verify live.** `cargo build --release`; start with `NEWWIN_DEBUG=1`; in `%LOCALAPPDATA%\NewWin\logs\newwin.log` expect `tray host up, first in z-order: true` and `WM_COPYDATA` registrations within 2 s. Switch `config.toml` to `mode = "pinned"`, restart NewWin: a maximized window must start below the bar (the AppBar goes through the host). Back to `auto-hide`. Kill explorer (`Stop-Process -Name explorer -Force`; it restarts itself): the log shows the restart path exactly once and icons register again. Kill the shell (`Stop-Process -Name NewWin -Force`; the watchdog restarts it): the new log shows registrations again within 2 s.
- [ ] **Step 5: Commit** `feat(shell): run the tray host, keep it first in z-order, release it before restoring the taskbar`.

---

### Task 6: Tweens, vector drawing, icons, emphasised text

**Files:**
- Create: `crates/core/src/anim.rs`, `crates/ui/src/icons.rs`
- Modify: `crates/core/src/lib.rs`, `crates/ui/src/paint.rs`, `crates/ui/src/text.rs`, `crates/ui/src/theme.rs`, `crates/ui/src/lib.rs`, `crates/platform/src/clock.rs`
- Test: `crates/core/tests/anim.rs`, `crates/ui/src/icons.rs` (unit tests for geometry)

**Interfaces:**
- Produces:
  - `newwin_core::anim::Tween` — `#[derive(Clone, Copy, Debug, PartialEq)]`, `pub fn at_rest(v: f32) -> Tween`, `pub fn to(&mut self, target: f32, now_ms: u64, dur_ms: u32)` (starts from the current value at `now_ms`; same target → no-op), `pub fn value(&self, now_ms: u64) -> f32` (ease-out cubic `1 − (1 − t)³`, identical to `newwin_ui::ease_out_progress`), `pub fn running(&self, now_ms: u64) -> bool`, `pub fn target(&self) -> f32`.
  - `newwin_platform::clock::now_ms() -> u64` (`GetTickCount64`).
  - `Painter::stroke_polyline(&mut self, pts: &[PointF], c: Color, width: f32)` and `Painter::stroke_arc(&mut self, center: PointF, radius: f32, start_deg: f32, sweep_deg: f32, c: Color, width: f32)` — round caps and joins (`ID2D1StrokeStyle` with `D2D1_CAP_STYLE_ROUND`, `D2D1_LINE_JOIN_ROUND`, created once per `Painter` call), `Painter::fill_polygon(&mut self, pts: &[PointF], c: Color)`; `Painter::push_opacity(&mut self, o: f32)` / `pop_opacity` via a layer for cross-fades.
  - `TextCache::layout_emphasis(&mut self, s: &str, style: &TextStyle, max_width: f32, bold: Option<(usize, usize)>) -> Result<TextLayout>` — `SetFontWeight(WEIGHT_SEMIBOLD, DWRITE_TEXT_RANGE)` over the UTF-16 range.
  - `newwin_ui::icons` (all sizes in physical px, drawn centred on `c` inside a box of side `s`, stroke `theme::ICON_STROKE` scaled by the caller):
    - `pub fn chevron(p: &mut Painter, c: PointF, s: f32, turn: f32, color: Color, w: f32)` — `^` turned by `turn` × 180°
    - `pub fn ethernet(p, c, s, color, w)` — port outline with three pins
    - `pub fn wifi(p, c, s, bars: f32, color, w)` — dot + 3 arcs; arc `k` (1..=3) drawn with opacity `clamp(bars − (k − 1), 0, 1)` (fractional for animation)
    - `pub fn slash(p, c, s, progress: f32, color, w)` — diagonal from top-left to `progress` of the way to bottom-right
    - `pub fn speaker(p, c, s, waves: f32, mute: f32, color, w)` — body polygon + up to 3 wave arcs with opacity like `wifi`, then `slash(mute)`
    - `pub fn magnifier` is **not** added (search is M3).
    - `pub fn chevron_points(c: PointF, s: f32, turn: f32) -> [PointF; 3]` — exposed for tests
- Theme constants from Global Constraints.

- [ ] **Step 1: Write the failing tests** `crates/core/tests/anim.rs`:

```rust
use newwin_core::anim::Tween;

#[test]
fn tween_eases_out_and_rests() {
    let mut t = Tween::at_rest(0.0);
    t.to(100.0, 1000, 100);
    assert_eq!(t.value(1000), 0.0);
    assert_eq!(t.value(1050), 87.5);
    assert_eq!(t.value(1100), 100.0);
    assert!(t.running(1099) && !t.running(1100));
}

#[test]
fn retarget_starts_from_current_value() {
    let mut t = Tween::at_rest(0.0);
    t.to(100.0, 0, 100);
    t.to(0.0, 50, 100);
    assert_eq!(t.value(50), 87.5);
    assert_eq!(t.value(150), 0.0);
    let before = t;
    t.to(0.0, 60, 100);
    assert_eq!(t, before, "same target is a no-op");
}
```

  and in `icons.rs`: `chevron_turns_upside_down` — `chevron_points(PointF{x:10.,y:10.}, 8.0, 0.0)` has its middle point above the ends (`y` smaller); with `turn = 1.0` below; ends keep their `x` order mirrored.
- [ ] **Step 2: Run** `cargo test -p newwin-core --test anim` and `cargo test -p newwin-ui chevron` → FAIL.
- [ ] **Step 3: Implement** everything in Interfaces.
- [ ] **Step 4: Run** both → PASS; `cargo test --workspace` green.
- [ ] **Step 5: Commit** `feat(ui): tweens, round-capped vector strokes, monochrome icons, emphasised text`.

---

### Task 7: Glass panels and tooltips

**Files:**
- Create: `crates/ui/src/panel.rs`, `crates/ui/src/tooltip.rs`
- Modify: `crates/ui/src/lib.rs`, `crates/ui/src/popup.rs` (implement `GlassWindow`), `apps/shell/src/app/glass_regions.rs`, `apps/shell/src/app.rs` (field `panel: Option<ShellPanel>`, `tooltip: Option<Tooltip>`), `apps/shell/src/app/menus.rs` (`close_popup` also closes the panel; opening a menu closes the panel and vice versa)
- Test: `crates/ui/src/panel.rs` unit tests, `crates/ui/tests/panel.rs` (window test), `apps/shell/src/app/glass_regions.rs` tests

**Interfaces:**
- Produces (`newwin_ui`):
  - `pub trait GlassWindow { fn windows(&self) -> Vec<(HWND, RECT)>; fn radius(&self) -> f32; fn set_backdrop(&self, hwnd: HWND, frame: &Frame); fn set_capturable(&self, on: bool); fn set_glass_unavailable(&self); }` — implemented by `Popup` (radius `POPUP_RADIUS`·dpi), `Panel<C>`, `Tooltip`.
  - `pub enum PanelInput { Move(PointF), Leave, Press(PointF), Release(PointF), RightRelease(PointF), Wheel(i16), Key(u16), Char(char) }`
  - `#[derive(Default)] pub struct PanelReply { pub repaint: bool, pub resize: bool, pub action: Option<u32>, pub close: bool }`
  - `pub trait PanelContent { fn size(&self) -> (f32, f32); fn paint(&mut self, p: &mut Painter, now_ms: u64); fn input(&mut self, ev: PanelInput, now_ms: u64) -> PanelReply; fn animating(&self, now_ms: u64) -> bool; }` (physical px)
  - `pub struct Panel<C: PanelContent + 'static>`:
    - `pub fn show(gfx: &Rc<Gfx>, content: C, anchor: PointF, dpi: u32, notify: HWND, msg: u32) -> Result<Panel<C>>` — top-centre of the panel at `anchor` (screen px), kept inside the monitor; glass layer radius `PANEL_RADIUS`·dpi, or `h/2` when the height ≤ 44·dpi/96 (capsule); takes foreground like a root `Popup`; plays `slide_y_both` from `−PANEL_DROP_PX`·dpi to 0 and `fade` 0→1 over `PANEL_SHOW_MS`.
    - `pub fn update(&self, f: impl FnOnce(&mut C))` — then resize if `size()` changed (height animated with a `Tween` over 120 ms) and repaint.
    - `pub fn hwnd(&self) -> HWND`
    - posts `msg` to `notify` with `wParam` = action id, `lParam` = `PICKED` for `reply.action`; `lParam = CLOSED`, `wParam` = panel hwnd on close (Esc, deactivation, `reply.close`). Close plays the reverse animation over `PANEL_HIDE_MS` and hides the window when it ends; the owner drops the `Panel` on `CLOSED`, exactly like `Popup`.
    - Own 16 ms frame timer while `content.animating(now)`; dirty = whole panel.
  - `pub struct Tooltip` — `pub fn show(gfx: &Rc<Gfx>, text: &str, anchor: PointF, style: &PopupStyle) -> Result<Tooltip>` (capsule, text `TOOLTIP_TEXT` px, never activates, `WS_EX_TRANSPARENT`), `pub fn set_text(&self, text: &str)`, fade-in 120 ms.
  - `pub fn panel_rect(anchor: PointF, size: (f32, f32), monitor: RECT) -> RECT` — pure, exposed for tests.
- Shell: `pub(super) enum ShellPanel {}` — starts empty; Task 9 adds `Tray(Panel<TrayList>)`, Task 11 `Volume(Panel<VolumePanel>)`, Task 14 `Calendar(Panel<CalendarPanel>)`. `impl ShellPanel { fn glass(&self) -> &dyn GlassWindow; fn hwnd(&self) -> HWND }` (`match *self {}` while empty). `fn glass_windows(&self) -> Vec<&dyn GlassWindow>` (popup, panel, tooltip) replaces the popup-only code in `sync_glass`, `on_snip`, `on_glass` (frame dispatch by hwnd) and the `UNAVAILABLE` branch. A panel feeds `VisInput::OverlayOpened`/`OverlayClosed` like a menu.

- [ ] **Step 1: Write the failing tests.**
  - `panel_rect_centres_and_clamps`: anchor (500, 50), size (260, 120), monitor 0..1920×1080 → `RECT { left: 370, top: 50, right: 630, bottom: 170 }`; anchor (1900, 50) → right edge 1920, left 1660.
  - `crates/ui/tests/panel.rs` `panel_posts_action_then_closed`: a test content whose `input(Release(_))` returns `{ action: Some(7), close: true }`; show the panel, send `WM_LBUTTONUP` to it, pump → notify receives `(7, PICKED)` then `(hwnd, CLOSED)`.
  - In `glass_regions.rs` extend `popups_are_always_included` to take radii from the windows (panel radius 16 at 96 dpi).
- [ ] **Step 2: Run** `cargo test -p newwin-ui panel` and `cargo test -p newwin-shell glass` → FAIL.
- [ ] **Step 3: Implement** `panel.rs` (reuse `popup.rs`'s window/handler pattern: `Rc<RefCell<State<C>>>` behind `window::Handler`), `tooltip.rs`, the trait impls, and the shell collector.
- [ ] **Step 4: Run** the tests → PASS; `cargo test --workspace` green.
- [ ] **Step 5: Commit** `feat(ui): glass panels with drop-in animation, glass tooltips`.

---

### Task 8: Right side of the bar

**Files:**
- Create: `crates/core/src/items.rs`, `apps/shell/src/app/right_view.rs`
- Modify: `crates/core/src/lib.rs`, `apps/shell/src/app/apps_view.rs` (`Target` gains `Tray` and `Right(RightItem)`; left edge starts after the chevron + separator; date/time move out of `relayout`/`paint` into `right_view`), `apps/shell/src/app/clock_view.rs`, `apps/shell/src/app.rs` (fields `right: Vec<RightSlot>`, `hover_pill: (Tween, Tween, Tween)` for x, width, opacity, `ANIM_TIMER`, `TOOLTIP_TIMER`, `WM_MOUSEWHEEL` on the bar), `apps/shell/src/app/visibility.rs` (`on_timer` cases), `crates/platform/src/launch.rs` (`open_notification_center`), `crates/core/src/strings.rs`
- Test: `crates/core/tests/items.rs`

**Interfaces:**
- Consumes: `Tween` (Task 6), icons (Task 6), `Tooltip` (Task 7).
- Produces:
  - `newwin_core::items::RightItem` — `#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)] enum { Layout, Network, Volume, Date, Time }`
  - `pub fn parse_right(names: &[String]) -> Vec<RightItem>` — `"layout" | "network" | "volume" | "date" | "time"`, case-insensitive, first occurrence wins; `"search"` and unknown names skipped.
  - `pub fn place_right(items: &[(RightItem, f32)], right_edge: f32, gap: f32, separator: f32) -> (Vec<(RightItem, f32)>, Option<f32>)` — x of each item laid out right-to-left ending at `right_edge`, `gap` between items; when a non-clock item precedes the first clock item (`Date`/`Time`) a separator slot of width `separator` sits between them and its centre x is returned.
  - `newwin_platform::launch::open_notification_center() -> Result<()>` — `SendInput` LWIN↓ N↓ N↑ LWIN↑.
  - Shell `RightSlot { item: RightItem, rect: RectF }`; `App::right_tooltip(&self, item) -> Option<String>`; `App::on_right_click(&mut self, item, button)`; `App::on_wheel(&mut self, delta: i16, p: PointF)`.
  - Strings: none new here.

Behaviour:
- Left: `⌃` chevron slot 26×26 (pill size) at the bar's left edge + 6 px, then a separator (1 px, `theme::SEPARATOR`, 14 px tall), then app pills. Chevron `turn` is a `Tween` (0 closed, 1 open, `HOVER_GLIDE_MS`).
- Right: widths — icon items 26 px, text items text width + 2·`PILL_PAD_X`; gap `ITEM_GAP`; separator slot 13 px; right edge = bar width − 6 px. Until Tasks 11–13 land, Volume/Network/Layout are skipped by the placement (their data is `None`).
- **Hover pill** for chevron + right items: one pill whose x/width tween to the hovered slot over `HOVER_GLIDE_MS`, opacity tween 0↔1 over 120 ms; drawn with `theme::HOVER_PILL`. Hover text colour `TEXT`, rest `TEXT_INACTIVE`, time stays `TEXT_STRONG`. Left app pills keep M1 behaviour.
- **Frames:** while any tween `running(now)`, `ANIM_TIMER` (16 ms) repaints the bar; it is not re-armed when all are at rest.
- **Tooltips:** hovering a right item for `TOOLTIP_DELAY_MS` shows its tooltip under it; moving to another item while one is shown swaps the text at once; leaving the bar, a click or the bar hiding removes it. Date tooltip = `GetDateFormatEx(DATE_LONGDATE)`; time has none.
- Date click → `open_notification_center()`; the outcome of probe 2b decides Task 14.

- [ ] **Step 1: Write the failing tests** `crates/core/tests/items.rs`:

```rust
use newwin_core::items::*;
use RightItem::*;

fn names(v: &[&str]) -> Vec<String> { v.iter().map(|s| s.to_string()).collect() }

#[test]
fn parse_skips_search_unknown_and_duplicates() {
    assert_eq!(parse_right(&names(&["layout", "network", "volume", "search", "date", "time"])), vec![Layout, Network, Volume, Date, Time]);
    assert_eq!(parse_right(&names(&["Time", "bogus", "time", "DATE"])), vec![Time, Date]);
}

#[test]
fn placement_right_to_left_with_separator() {
    let (xs, sep) = place_right(&[(Network, 26.0), (Volume, 26.0), (Date, 80.0), (Time, 40.0)], 1000.0, 2.0, 13.0);
    // Time 960..1000, gap, Date 878..958, gap + separator 863..876 + gap, Volume 835..861, gap, Network 807..833.
    assert_eq!(xs, vec![(Network, 807.0), (Volume, 835.0), (Date, 878.0), (Time, 960.0)]);
    assert_eq!(sep, Some(869.5));
}

#[test]
fn no_separator_without_status_items() {
    let (xs, sep) = place_right(&[(Date, 80.0), (Time, 40.0)], 1000.0, 2.0, 13.0);
    assert_eq!(xs, vec![(Date, 878.0), (Time, 960.0)]);
    assert_eq!(sep, None);
}
```

- [ ] **Step 2: Run** `cargo test -p newwin-core --test items` → FAIL.
- [ ] **Step 3: Implement** `items.rs`, then `right_view.rs` (relayout + paint of the right side and chevron, hover pill tweens, tooltip timing, clicks), move date/time painting there, add `ANIM_TIMER`/`TOOLTIP_TIMER` handling and `open_notification_center`.
- [ ] **Step 4: Run** `cargo test --workspace` → PASS. Live: release build, hover chevron → date → time: one pill glides, date tooltip appears after ½ s; clicking the date opens the notification center (or record that it does not).
- [ ] **Step 5: Commit** `feat(shell): right side of the bar with gliding hover pill, tooltips and the tray chevron`.

---

### Task 9: Tray panel

**Files:**
- Create: `crates/ui/src/tray_list.rs`, `apps/shell/src/app/tray_view.rs`
- Modify: `crates/ui/src/lib.rs`, `apps/shell/src/app.rs` (`on_tray_changed`, `WM_APP_PANEL = WM_APP + 11` routing), `apps/shell/src/app/menus.rs` (NewWin row menu reuses `RESTART_NEWWIN`/`QUIT` and a new `NEWWIN_SETTINGS` handler), `crates/core/src/strings.rs`, `crates/config/src/lib.rs` (`write_default_if_missing`), `crates/platform/src/appinfo.rs` (`process_display_name`)
- Test: `crates/ui/src/tray_list.rs` unit tests, `crates/config/tests/…` (existing config test file) for `write_default_if_missing`

**Interfaces:**
- Consumes: `TrayHost::{icons, prune, set_anchor}`, `send_click`, `TrayClick` (Task 4); `row_label`, `match_range` (Task 1); `Panel`, `PanelContent` (Task 7); `layout_emphasis` (Task 6).
- Produces:
  - `newwin_ui::tray_list::{TrayRow, TrayList, TrayOp, action_id, decode_action}`:
    - `#[derive(Clone, Debug, PartialEq)] pub struct TrayRow { pub label: String, pub header: bool }` — row 0 is the NewWin header.
    - `pub enum TrayOp { Left, Right, Key }`; `pub fn action_id(row: usize, op: TrayOp) -> u32` = `(row << 2) | op`; `pub fn decode_action(id: u32) -> (usize, TrayOp)`.
    - `pub struct TrayList` — `pub fn new(rows: Vec<TrayRow>, empty_text: String, style: PopupStyle, gfx: &Rc<Gfx>) -> TrayList`, `pub fn set_rows(&mut self, rows: Vec<TrayRow>)`, `pub fn query(&self) -> &str`, `pub fn visible(&self) -> Vec<(usize, Option<(usize, usize)>)>` (index into `rows` + emphasis; header always visible, apps filtered by `match_range`), implements `PanelContent`.
    - Width = clamp(widest label + 2·pad, 220, 360)·dpi/96; row height `POPUP_ITEM_HEIGHT`; at most 14 visible rows, then wheel-scroll (offset `Tween`), bottom 24 px fade mask when more rows below; separator after the header; empty filter result or no apps → one `TEXT_INACTIVE` row with `empty_text`.
    - Input: `Move` → hover row (pill glides with `HOVER_GLIDE_MS`); `Release` → `action_id(row, Left)` + close; `RightRelease` → `action_id(row, Right)` (no close — the app's own menu takes focus, the panel closes on deactivation); `Key(VK_UP/DOWN)` moves hover; `Key(VK_RETURN)` → first visible app row or hovered row with `Key`; `Key(VK_ESCAPE)` → close; `Char(c)` (printable) appends to the query; `Key(VK_BACK)` pops.
  - `newwin_config::write_default_if_missing(path: &Path) -> std::io::Result<()>` — writes `toml::to_string_pretty(&Config::default())` with a leading comment line `# NewWin settings — see the README`, creating the directory.
  - `newwin_platform::appinfo::process_display_name(pid: u32) -> Option<String>` — `file_description(process_image_path(pid))`, else the exe file stem.
  - Strings: `no_background_apps` ("No background apps" / "Немає фонових програм"), `settings_ellipsis` ("Settings…" / "Налаштування…").

Shell behaviour (`tray_view.rs`):
- `Target::Tray` left click → if open, close; else `host.prune()`, build rows: header `TrayRow { label: "NewWin", header: true }`, then each non-hidden icon in model order with `row_label(&icon.tip, &name(pid))` where `name` is cached in `HashMap<u32, String>` via `process_display_name` (fallback `"pid {pid}"` never shown: use exe name, else skip the row). Store the icon keys parallel to rows. Open `Panel<TrayList>` anchored under the chevron; `host.set_anchor(chevron screen rect)`; chevron tween → 1.
- `on_tray_changed` → if the tray panel is open, rebuild rows and `panel.update(|l| l.set_rows(rows))`.
- Panel action → `decode_action`; row 0 Left → `write_default_if_missing` + `shell_execute("open", config_path, "")`; row 0 Right → close panel, open a `Popup` at the row with `settings_ellipsis` · `restart_newwin` · `quit`; app rows → `send_click(icon, op, centre of the row in screen px)`. Closing → chevron tween → 0.

- [ ] **Step 1: Write the failing tests** in `tray_list.rs` (pure parts only — construct with `rows` and call `visible()` / `input()` via a test-only constructor `TrayList::for_test(rows)` that skips text layout):
  - `action_ids_roundtrip` — `decode_action(action_id(5, TrayOp::Right)) == (5, TrayOp::Right)`.
  - `typing_filters_apps_but_keeps_header` — rows NewWin/Telegram/Steam; with an empty query `visible()` = `[(0, None), (1, None), (2, None)]` (no bold without a query; the header never gets emphasis); `input(Char('t'))`, `input(Char('e'))`, `input(Char('l'))` → `[(0, None), (1, Some((0, 3)))]`; `input(Key(VK_BACK))` three times → all three visible again.
  - `enter_clicks_first_visible_app` — after typing `"st"`, `input(Key(VK_RETURN))` → `action == Some(action_id(2, TrayOp::Key))`.
  - `right_release_does_not_close` — reply `close == false`.
  - In config tests: `write_default_if_missing_creates_once` — temp dir: file created and loads as `Config::default()`; a second call leaves a modified file untouched.
- [ ] **Step 2: Run** `cargo test -p newwin-ui tray_list` and `cargo test -p newwin-config` → FAIL.
- [ ] **Step 3: Implement** `tray_list.rs`, `tray_view.rs`, the config helper, `process_display_name`, strings (both languages).
- [ ] **Step 4: Run** `cargo test --workspace` → PASS. Live with the user: open `⌃` — NewWin + their tray apps; type `tel` → Telegram bold; left click opens Telegram; right click shows NVIDIA's own menu; kill Discord with `Stop-Process` and reopen the tray → no Discord row.
- [ ] **Step 5: Commit** `feat: tray panel with type-to-filter, app clicks and the NewWin row`.

---

### Task 10: Audio service

**Files:**
- Create: `crates/core/src/audio.rs`, `crates/platform/src/audio.rs`
- Modify: `crates/core/src/lib.rs`, `crates/platform/src/lib.rs`, `crates/platform/Cargo.toml` (features `Win32_Media_Audio`, `Win32_Media_Audio_Endpoints`, `Win32_Devices_FunctionDiscovery`, `Win32_UI_Shell_PropertiesSystem` already there)
- Test: `crates/core/tests/audio.rs`, `crates/platform/tests/audio.rs`

**Interfaces:**
- Produces:
  - `newwin_core::audio`:
    - `pub fn waves_for(level: u8) -> u8` — 0 → 0, 1..=33 → 1, 34..=66 → 2, 67..=100 → 3.
    - `pub fn percent(scalar: f32) -> u8` — `round(scalar·100)` clamped 0..=100, NaN → 0.
    - `#[derive(Default)] pub struct Wheel { rest: i32 }` with `pub fn notches(&mut self, delta: i16) -> i32` — accumulates `WHEEL_DELTA = 120` units, keeps the remainder (touchpads send small deltas).
    - `pub fn step(level: u8, notches: i32, per_notch: u8) -> u8` — clamped 0..=100.
    - `#[derive(Clone, Debug, PartialEq)] pub enum SpeakerIcon { Unavailable, Level { waves: u8, muted: bool } }`, `pub fn speaker_icon(state: Option<(u8, bool)>) -> SpeakerIcon`.
  - `newwin_platform::audio`:
    - `#[derive(Clone, Debug, PartialEq)] pub struct AudioDevice { pub id: String, pub name: String }`
    - `#[derive(Clone, Debug, PartialEq)] pub struct AudioSnapshot { pub level: u8, pub muted: bool, pub devices: Vec<AudioDevice>, pub default_id: Option<String> }`
    - `pub const AUDIO_VOLUME: usize = 1; pub const AUDIO_DEVICES: usize = 2;` (posted `wParam`; for `AUDIO_VOLUME`, `lParam` = `level | (muted as isize) << 8`)
    - `pub struct AudioService` — `pub fn start(notify: HWND, msg: u32) -> AudioService` (never fails: without devices it is in the unavailable state and keeps the device-notification client registered), `pub fn snapshot(&self) -> Option<AudioSnapshot>`, `pub fn set_level(&self, level: u8)`, `pub fn set_muted(&self, muted: bool)`, `pub fn set_default(&self, id: &str) -> Result<()>` (`IPolicyConfig`, all three roles), `pub fn rebind(&mut self)` (call on `AUDIO_DEVICES`: re-get the default endpoint and move the volume callback to it).
    - Our writes pass `EVENT_CONTEXT: GUID = GUID::from_u128(0x5f3c_8a1e_2b7d_4c90_9e61_0a4d_b2c3_e7f8)`; the volume callback ignores notifications with that context.
    - COM callbacks via `#[windows::core::implement(IAudioEndpointVolumeCallback)]` and `#[windows::core::implement(IMMNotificationClient)]` (if the attribute is not found, add the `windows-core` 0.62 dependency to `newwin-platform` and use `windows_core::implement`). They only `PostMessageW`.

- [ ] **Step 1: Write the failing tests.** `crates/core/tests/audio.rs`:

```rust
use newwin_core::audio::*;

#[test]
fn waves_thresholds() {
    assert_eq!([0, 1, 33, 34, 66, 67, 100].map(waves_for), [0, 1, 1, 2, 2, 3, 3]);
}

#[test]
fn percent_rounds_and_clamps() {
    assert_eq!([0.0, 0.424, 0.425, 1.0, 1.7, -0.2, f32::NAN].map(percent), [0, 42, 43, 100, 100, 0, 0]);
}

#[test]
fn wheel_accumulates_touchpad_deltas() {
    let mut w = Wheel::default();
    assert_eq!(w.notches(40), 0);
    assert_eq!(w.notches(40), 0);
    assert_eq!(w.notches(40), 1);
    assert_eq!(w.notches(-240), -2);
}

#[test]
fn step_clamps() {
    assert_eq!(step(99, 1, 2), 100);
    assert_eq!(step(1, -1, 2), 0);
    assert_eq!(step(50, 3, 2), 56);
}

#[test]
fn no_device_maps_to_unavailable() {
    assert_eq!(speaker_icon(None), SpeakerIcon::Unavailable);
    assert_eq!(speaker_icon(Some((0, false))), SpeakerIcon::Level { waves: 0, muted: false });
    assert_eq!(speaker_icon(Some((80, true))), SpeakerIcon::Level { waves: 3, muted: true });
}
```

  `crates/platform/tests/audio.rs` `snapshot_is_consistent` — `AudioService::start(test window, msg)`; if `snapshot()` is `Some(s)`: `s.level <= 100`, `s.default_id` is one of `s.devices` ids or `None`, every device has a non-empty name. Passes on machines without audio (`None`).
- [ ] **Step 2: Run** `cargo test -p newwin-core --test audio`, `cargo test -p newwin-platform --test audio` → FAIL.
- [ ] **Step 3: Implement** both modules; move the `IPolicyConfig` declaration from the Task 2 probe into `platform/src/audio.rs`.
- [ ] **Step 4: Run** → PASS; `cargo test --workspace` green.
- [ ] **Step 5: Commit** `feat: audio service with live volume, device list and default-device switching`.

---

### Task 11: Volume on the bar and its panel

**Files:**
- Create: `crates/ui/src/slider.rs`, `crates/ui/src/volume_panel.rs`, `apps/shell/src/app/volume_view.rs`
- Modify: `crates/ui/src/lib.rs`, `apps/shell/src/app.rs` (`audio: Option<AudioService>`, `WM_APP_AUDIO = WM_APP + 8`, `ShellPanel::Volume`), `apps/shell/src/app/right_view.rs` (Volume slot, wheel, middle click), `crates/core/src/strings.rs`
- Test: `crates/ui/src/slider.rs`, `crates/ui/src/volume_panel.rs` unit tests

**Interfaces:**
- Consumes: `AudioService`, `AudioSnapshot`, `AUDIO_*` (Task 10); `waves_for`, `Wheel`, `step`, `speaker_icon` (Task 10); `icons::speaker` (Task 6); `Panel` (Task 7).
- Produces:
  - `newwin_ui::slider`: `pub fn level_at(x: f32, track: RectF) -> u8` (clamped, rounded), `pub fn thumb_x(level: u8, track: RectF) -> f32`.
  - `newwin_ui::volume_panel::{VolumePanel, VolumeAction, encode, decode}`: actions `SetLevel(u8)` = `1000 + level`, `ToggleMute` = 1, `PickDevice(i)` = `2000 + i`, `SoundSettings` = 3. `VolumePanel::new(snapshot: Option<AudioSnapshot>, strings: (&str /*no devices*/, &str /*sound settings*/), style: PopupStyle, gfx)`, `pub fn set_snapshot(&mut self, s: Option<AudioSnapshot>, now_ms: u64)` (level tweens over 120 ms; added/removed device rows fade over 120 ms), `pub fn show_settings_row(&mut self)`.
  - Layout (logical px, ×dpi/96): width 260; row 1 height 44 = mute button 26 + 10 gap + track (4 px tall, white 0.14, filled part white 0.85) + 10 gap + value text right-aligned with `DWRITE_FONT_FEATURE_TAG_TABULAR_FIGURES`; thumb = glass capsule 14 px (`paint_glass` in a 14×14 rect). Then separator and device rows (`POPUP_ITEM_HEIGHT`), current device bold in `ACTIVE_PILL`; optional `Sound settings…` row; no devices → one `TEXT_INACTIVE` row.
  - Input: `Press` on the track starts a drag and emits `SetLevel(level_at)`; `Move` while dragging emits `SetLevel` (no tween — follows the pointer); `Release` ends the drag; a click on the track without drag tweens the thumb (120 ms); `Wheel` ±2 per notch; `Key` ←/→ ±1, `Home`/`End`; mute button → `ToggleMute`; device row → `PickDevice(i)`.
  - Strings: `no_output_devices` ("No output devices" / "Немає пристроїв виводу"), `sound_settings` ("Sound settings…" / "Налаштування звуку…").

Shell behaviour:
- Volume slot draws `icons::speaker` with `waves` and `mute` tweens (100 ms per wave, 120 ms for the slash); `Unavailable` = speaker + full slash in `TEXT_INACTIVE`.
- Wheel over the Volume slot → `audio.set_level(step(level, wheel.notches(delta), 2))`, tooltip `"{device name} · {level}%"` shown immediately and updated; middle click → toggle mute; left click → toggle the volume panel.
- `WM_APP_AUDIO`: `AUDIO_VOLUME` → update icon state (and the open panel); `AUDIO_DEVICES` → `audio.rebind()`, `snapshot()`, update icon and panel.
- Panel actions → service calls; `set_default` error → `log::warn!` once per run and `panel.update(|v| v.show_settings_row())`; `SoundSettings` → `shell_execute("open", "ms-settings:sound", "")`.

- [ ] **Step 1: Write the failing tests:** `slider_maps_both_ways` — track x 100..300: `level_at(100.0) == 0`, `level_at(200.0) == 50`, `level_at(999.0) == 100`, `thumb_x(50, track) == 200.0`; `volume_actions_roundtrip` — `decode(encode(PickDevice(3))) == PickDevice(3)`, `decode(1042) == SetLevel(42)`; `drag_emits_levels_without_tween` (`VolumePanel::for_test(snapshot)` skips text layout and exposes `track() -> RectF`: `Press` at 30 % of the track → action `SetLevel(30)`; `Move` to 60 % → `SetLevel(60)`; `Release` → no action).
- [ ] **Step 2: Run** `cargo test -p newwin-ui slider volume` → FAIL.
- [ ] **Step 3: Implement** slider, panel content, shell wiring, strings.
- [ ] **Step 4: Run** `cargo test --workspace` → PASS. Live with the user: wheel over the speaker changes volume and the tooltip follows; middle click mutes (slash draws in); panel: drag, keyboard volume keys move the slider smoothly, switching headphones ↔ speakers moves the sound; unplugging a device removes its row with a fade.
- [ ] **Step 5: Commit** `feat: volume on the bar with live glass panel and device switching`.

---

### Task 12: Network

**Files:**
- Create: `crates/core/src/network.rs`, `crates/platform/src/network.rs`
- Modify: `crates/core/src/lib.rs`, `crates/platform/src/lib.rs`, `crates/platform/Cargo.toml` (features `Win32_NetworkManagement_IpHelper`, `Win32_NetworkManagement_Ndis`, `Win32_Networking_WinSock`, `Win32_NetworkManagement_WiFi`), `apps/shell/src/app.rs` (`network: Option<NetworkWatch>`, `WM_APP_NET = WM_APP + 9`, `NET_DEBOUNCE_TIMER`), `apps/shell/src/app/right_view.rs` (Network slot), `crates/core/src/strings.rs`
- Test: `crates/core/tests/network.rs`

**Interfaces:**
- Produces:
  - `newwin_core::network`:
    - `#[derive(Clone, Debug, PartialEq)] pub enum NetKind { Ethernet, WiFi { signal: u8 }, None }`
    - `#[derive(Clone, Debug, PartialEq)] pub struct NetInfo { pub kind: NetKind, pub internet: bool, pub name: String, pub ipv4: Option<String> }`
    - `#[derive(Clone, Copy, Debug, PartialEq)] pub enum NetIcon { Ethernet { internet: bool }, WiFi { bars: u8, internet: bool }, Disconnected }`
    - `pub fn icon_for(info: &NetInfo) -> NetIcon` — `bars` = `waves_for`-style thresholds on `signal` (0 → 0, 1..=33 → 1, 34..=66 → 2, 67..=100 → 3).
    - `pub fn tooltip(info: &NetInfo, not_connected: &str, no_internet: &str) -> String` — `"{name} · {ipv4}"`; without ipv4 just `name`; `None` → `not_connected`; connected without internet → `"{name} · {no_internet}"`.
  - `newwin_platform::network`:
    - `pub struct NetworkWatch` — `pub fn start(notify: HWND, msg: u32) -> Result<NetworkWatch>` (`NotifyIpInterfaceChange(AF_UNSPEC)` + `NotifyNetworkConnectivityHintChange`; callbacks only post `msg`); `Drop` cancels both (`CancelMibChangeNotify2`).
    - `pub fn current() -> NetInfo` — `GetBestInterfaceEx(0.0.0.0)` → adapter from `GetAdaptersAddresses(AF_INET)` → `IfType` (6 Ethernet, 71 Wi-Fi), `FriendlyName`, first unicast IPv4; Wi-Fi: SSID as `name` and `wlanSignalQuality` from `WlanQueryInterface(wlan_intf_opcode_current_connection)`; `internet` from `GetNetworkConnectivityHint` (`NetworkConnectivityLevelHintInternetAccess` only). No best interface → `NetKind::None`.
    - When the current kind is Wi-Fi, `NetworkWatch::follow_wifi(&mut self, on: bool)` registers/unregisters `WlanRegisterNotification(WLAN_NOTIFICATION_SOURCE_MSM)` for signal changes (posting the same `msg`).
  - Strings: `not_connected` ("Not connected" / "Немає підключення"), `no_internet` ("No internet" / "Без інтернету").

Shell: `WM_APP_NET` → (re)arm `NET_DEBOUNCE_TIMER` 300 ms; on the timer → `current()`, `follow_wifi(kind is WiFi)`, and if `icon_for` changed → cross-fade old/new icon over 160 ms (two-icon draw with `push_opacity`). Disconnected = Ethernet outline at `TEXT_INACTIVE` + full slash; no internet = type icon + slash. Left click → `shell_execute("open", "ms-settings:network", "")`. Tooltip from `tooltip()`.

- [ ] **Step 1: Write the failing tests** `crates/core/tests/network.rs`:

```rust
use newwin_core::network::*;

fn info(kind: NetKind, internet: bool, ip: Option<&str>) -> NetInfo {
    NetInfo { kind, internet, name: "Ethernet".into(), ipv4: ip.map(String::from) }
}

#[test]
fn icons() {
    assert_eq!(icon_for(&info(NetKind::Ethernet, true, None)), NetIcon::Ethernet { internet: true });
    assert_eq!(icon_for(&info(NetKind::WiFi { signal: 50 }, false, None)), NetIcon::WiFi { bars: 2, internet: false });
    assert_eq!(icon_for(&info(NetKind::WiFi { signal: 0 }, true, None)), NetIcon::WiFi { bars: 0, internet: true });
    assert_eq!(icon_for(&info(NetKind::None, false, None)), NetIcon::Disconnected);
}

#[test]
fn tooltips() {
    assert_eq!(tooltip(&info(NetKind::Ethernet, true, Some("192.168.1.10")), "Not connected", "No internet"), "Ethernet · 192.168.1.10");
    assert_eq!(tooltip(&info(NetKind::Ethernet, false, Some("10.0.0.2")), "Not connected", "No internet"), "Ethernet · No internet");
    assert_eq!(tooltip(&info(NetKind::None, false, None), "Not connected", "No internet"), "Not connected");
}
```

- [ ] **Step 2: Run** `cargo test -p newwin-core --test network` → FAIL.
- [ ] **Step 3: Implement** core, platform and shell parts.
- [ ] **Step 4: Run** `cargo test --workspace` → PASS. Live: unplug Ethernet → icon fades to "no network" within ~½ s; plug back → Ethernet; tooltip shows the adapter and IP.
- [ ] **Step 5: Commit** `feat: network status on the bar from IP Helper events`.

---

### Task 13: Keyboard layout

**Files:**
- Create: `crates/core/src/input_lang.rs`, `crates/platform/src/keyboard_layout.rs`, `crates/platform/src/input_hook.rs`
- Modify: `crates/core/src/lib.rs`, `crates/platform/src/lib.rs`, `apps/shell/src/app.rs` (`input_hook: Option<InputHook>`, `WM_APP_LAYOUT = WM_APP + 10`, `LAYOUT_RECHECK_TIMER`), `apps/shell/src/app/apps_view.rs` (re-read the layout on `WindowEvent::Activated`), `apps/shell/src/app/right_view.rs` (Layout slot)
- Test: `crates/core/tests/input_lang.rs`

**Interfaces:**
- Produces:
  - `newwin_core::input_lang`:
    - `pub fn display_code(iso639_2: &str) -> String` — trimmed, uppercased; empty → `"—"`.
    - `#[derive(Default)] pub struct LayoutHotkeys` with `pub fn key(&mut self, vk: u16, down: bool) -> bool` — true when a layout switch may have happened: on the release that ends an Alt+Shift or Ctrl+Shift chord with no other key pressed during the chord (either key may come first; left/right variants `VK_LMENU/RMENU/MENU`, `VK_LSHIFT/RSHIFT/SHIFT`, `VK_LCONTROL/RCONTROL/CONTROL`); and on every Space press while a Win key (`VK_LWIN`/`VK_RWIN`) is held.
  - `newwin_platform::keyboard_layout`: `pub struct LayoutInfo { pub code: String /*ISO 639-2 like "eng"*/, pub name: String /*localized display name*/ }`; `pub fn current_layout() -> Option<LayoutInfo>` (foreground window's thread → `GetKeyboardLayout` → `LCIDToLocaleName` → `GetLocaleInfoEx(LOCALE_SISO639LANGNAME2 | LOCALE_SLOCALIZEDDISPLAYNAME)`); `pub fn switch_next()` (`PostMessageW(GetForegroundWindow(), WM_INPUTLANGCHANGEREQUEST, INPUTLANGCHANGE_FORWARD, 0)`).
  - `newwin_platform::input_hook::InputHook` — `pub fn start(notify: HWND, msg: u32) -> Result<InputHook>`: thread `newwin-input` installs `WH_KEYBOARD_LL`, runs `GetMessageW`; the hook proc feeds `LayoutHotkeys` (thread-local) and posts `msg` when it returns true, then always `CallNextHookEx`; never blocks. `Drop`: `PostThreadMessageW(WM_QUIT)` + join. M3 will add the Win key logic to this same hook.

Shell: `WM_APP_LAYOUT` and a click on the slot (after `switch_next()`) → arm `LAYOUT_RECHECK_TIMER` 50 ms; on the timer, `WindowEvent::Activated` and start → `current_layout()`; if the code changed, cross-fade the text over 120 ms. Tooltip = `name`.

- [ ] **Step 1: Write the failing tests** `crates/core/tests/input_lang.rs`:

```rust
use newwin_core::input_lang::*;

const SHIFT: u16 = 0xA0; // VK_LSHIFT
const ALT: u16 = 0xA4; // VK_LMENU
const CTRL: u16 = 0xA2; // VK_LCONTROL
const WIN: u16 = 0x5B; // VK_LWIN
const SPACE: u16 = 0x20;
const A: u16 = 0x41;

#[test]
fn codes() {
    assert_eq!(display_code("eng"), "ENG");
    assert_eq!(display_code(" ukr "), "UKR");
    assert_eq!(display_code(""), "—");
}

#[test]
fn alt_shift_in_either_order() {
    let mut h = LayoutHotkeys::default();
    assert!(!h.key(ALT, true));
    assert!(!h.key(SHIFT, true));
    assert!(h.key(SHIFT, false));
    assert!(!h.key(ALT, false), "one report per chord");
    assert!(!h.key(SHIFT, true));
    assert!(!h.key(ALT, true));
    assert!(h.key(ALT, false));
    h.key(SHIFT, false);
}

#[test]
fn ctrl_shift_counts_but_not_with_another_key() {
    let mut h = LayoutHotkeys::default();
    h.key(CTRL, true);
    h.key(SHIFT, true);
    assert!(h.key(SHIFT, false));
    h.key(CTRL, false);
    h.key(CTRL, true);
    h.key(SHIFT, true);
    h.key(A, true);
    h.key(A, false);
    assert!(!h.key(SHIFT, false), "Ctrl+Shift+A is a shortcut, not a switch");
    h.key(CTRL, false);
}

#[test]
fn win_space_reports_each_press() {
    let mut h = LayoutHotkeys::default();
    h.key(WIN, true);
    assert!(h.key(SPACE, true));
    h.key(SPACE, false);
    assert!(h.key(SPACE, true));
    h.key(SPACE, false);
    h.key(WIN, false);
    assert!(!h.key(SPACE, true));
}
```

- [ ] **Step 2: Run** `cargo test -p newwin-core --test input_lang` → FAIL.
- [ ] **Step 3: Implement** core, platform and shell parts.
- [ ] **Step 4: Run** `cargo test --workspace` → PASS. Live: `ENG` ↔ `UKR` with Alt+Shift and Win+Space in Notepad, switching windows with different layouts, clicking the slot switches the foreground window's layout; typing elsewhere is never delayed (the hook does nothing slow).
- [ ] **Step 5: Commit** `feat: keyboard layout on the bar with a low-level hook for layout hotkeys`.

---

### Task 14: Calendar fallback — only if probe 2b failed

Skip this task (mark it skipped in the ledger) when `docs/superpowers/notes/2026-10-22-m2-probes.md` says Win+N opens the notification center with the hidden taskbar.

**Files:**
- Create: `crates/core/src/calendar.rs`, `crates/ui/src/calendar_panel.rs`
- Modify: `apps/shell/src/app/right_view.rs` (Date click opens `ShellPanel::Calendar`), `apps/shell/src/app.rs` (`ShellPanel::Calendar`)
- Test: `crates/core/tests/calendar.rs`

**Interfaces:**
- Produces: `newwin_core::calendar::month_grid(year: i32, month: u8, first_day: Weekday) -> [[Option<u8>; 7]; 6]` with `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Weekday { Mon, Tue, Wed, Thu, Fri, Sat, Sun }` (Gregorian, leap years); `pub fn add_months(year: i32, month: u8, delta: i32) -> (i32, u8)`. `CalendarPanel` (PanelContent): header `‹ October 2026 ›` (month name via `GetDateFormatEx` with format `"MMMM yyyy"`), weekday initials from the locale, 6×7 day grid in `TEXT`, other-month cells empty, today in `ACTIVE_PILL`; `‹ ›` and the wheel move months with a 120 ms cross-fade. First day of week from `LOCALE_IFIRSTDAYOFWEEK`.

- [ ] **Step 1: Write the failing tests:** `october_2026_starts_on_thursday` — `month_grid(2026, 10, Weekday::Mon)[0] == [None, None, None, Some(1), Some(2), Some(3), Some(4)]`, last day 31 in row 4 col 5 (`[4][5] == Some(31)`); `february_leap_year` — 2028-02 has 29 days, 2026-02 has 28; `add_months(2026, 12, 1) == (2027, 1)`, `add_months(2026, 1, -1) == (2025, 12)`.
- [ ] **Step 2: Run** `cargo test -p newwin-core --test calendar` → FAIL.
- [ ] **Step 3: Implement** grid, panel, wiring.
- [ ] **Step 4: Run** `cargo test --workspace` → PASS; live: date click opens the glass calendar, today highlighted, months scroll.
- [ ] **Step 5: Commit** `feat: glass month calendar under the date`.

---

### Task 15: Measurements and acceptance

**Files:**
- Create: `docs/superpowers/notes/2026-10-22-m2-acceptance.md`
- Modify: `scripts/measure.ps1` (also report thread count and wakeups of `newwin-tray-fwd` and `newwin-input` via per-thread `TotalProcessorTime` deltas), `docs/superpowers/specs/2026-10-22-newwin-m2-tray-design.md` (status line → `реалізовано (M2)`)

- [ ] **Step 1: Run** `scripts/measure.ps1` for the idle case (bar hidden, 10 min) and with the tray panel and volume panel each open for 60 s over a still window. Expected: idle 0 % CPU and zero CPU time on both new threads; private working set < 20 MB; panel first frame < 16 ms (log `panel first-frame-ms=` from `Panel::show`, add the log line if missing).
- [ ] **Step 2: Write the acceptance notes:** numbers, and the live checklist from the spec §7 with each item marked *checked by user* / *not yet checked*: NVIDIA, Telegram, Discord, Steam left/right click; pinned bar reserves space; explorer restart → icons back; kill shell → explorer keeps icons; headphones ↔ speakers; keyboard volume moves the slider; Ethernet unplug; `ENG` ↔ `UKR` by keyboard and click; Win+N or calendar.
- [ ] **Step 3: Run** `cargo test --workspace` and `cargo clippy --workspace --all-targets` → green.
- [ ] **Step 4: Commit** `docs: M2 measurements and acceptance notes`.
