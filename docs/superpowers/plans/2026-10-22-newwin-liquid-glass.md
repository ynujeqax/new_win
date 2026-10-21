# Liquid Glass Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The bar and its popups get "liquid glass": the screen under them blurred by NewWin itself, plus tint, sheen and a bright rim — without Windows transparency effects.

**Architecture:** Pure image math (downsample, blur, change filter, rect math) lives in `newwin-core`. A `newwin-glass` thread in `newwin-ui` owns a hardware D3D11 device and DXGI Desktop Duplication only while some glass window wants a backdrop, and posts small blurred BGRA images to the UI thread. `Layer` gains a glass visual tree (rounded clip → tiny backdrop surface scaled 4× by DWM) under the existing content surface; the content paints tint/sheen/rim once.

**Tech Stack:** Rust 2024, `windows` 0.62 (D3D11, DXGI duplication, DirectComposition, Direct2D), WARP for our own rendering.

**Spec:** `docs/superpowers/specs/2026-10-22-newwin-liquid-glass-design.md` (extends `docs/superpowers/specs/2026-10-21-newwin-design.md`). Spike with working reference code: `crates/ui/examples/own_blur_probe.rs`, results in `docs/superpowers/notes/2026-10-21-glass-probe.md`.

## Global Constraints

- Our rendering stays on WARP; the hardware D3D11 device exists only for capture and only while a region is registered.
- Glass windows call `SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)`.
- Capture area = window rect + 24 px each side, clipped to the output. Downsample factor 4 (4×4 average), box blur radius 2, 3 passes. At most one recompute per 33 ms. An unchanged blurred image is never posted.
- Layers, bottom to top: blurred backdrop; tint `rgba(8,8,10,0.55)`; sheen = vertical white gradient 0.16 @0 → 0.02 @0.55 → 0.05 @1; rim 1 px = vertical white gradient 0.50 @0 → 0.10 @0.35 → 0.14 @1; content. Without a backdrop the tint is 0.88. No shadow.
- Bar corner radius stays `appearance.corner_radius` (11); popups keep `theme::POPUP_RADIUS`.
- Config: `[appearance] glass = true` (default); `background_opacity` is dropped (old files still load).
- Threads talk only through `PostMessage(WM_APP+n)` with an owned `Box` in LPARAM (spec §4).
- Budget: hidden bar = no capture device, no extra wakeups; visible over static content ≤ 0.05 % machine CPU; over video ≤ 0.3 %; shell private WS < 20 MB while capturing.
- A glass failure never panics or blocks the UI thread; it only removes the backdrop layer.
- Out of scope here: Spotlight (M3, tint 0.72) and the settings window (M4) reuse `Backdrop::Glass` + `paint_glass` in their own plans.

## Review Focus

1. **Bar hides while a frame is in flight** → a frame for a region that is no longer registered (or for a destroyed popup HWND) is dropped, never drawn into a dead layer. Test in Task 5.
2. **Device lost / `DXGI_ERROR_ACCESS_LOST` (UAC prompt, resolution change)** → the thread recreates duplication and keeps going; the bar keeps the last image. Test in Task 3.
3. **Region partly off-screen (bar at the top edge, menu near the right edge)** → the capture area is clipped to the output, never an out-of-range copy. Test in Task 1.
4. **Reveal starts mid-hide** → the backdrop's counter-offset continues from the same point as the bar, so the blur never jumps. Test in Task 5.
5. **`glass = false` or capture unavailable** → fallback look (tint 0.88, sheen and rim kept), no capture thread work. Test in Tasks 4 and 5.

---

### Task 1: Glass image math (`newwin-core`)

**Files:**
- Create: `crates/core/src/glass.rs`
- Modify: `crates/core/src/lib.rs` (add `pub mod glass;`)
- Test: `crates/core/tests/glass.rs`

**Interfaces:**
- Produces (all in `newwin_core::glass`):
  - consts `PAD: i32 = 24`, `SCALE: usize = 4`, `BLUR_RADIUS: usize = 2`, `BLUR_PASSES: usize = 3`, `MIN_INTERVAL_MS: u64 = 33`
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub struct PxRect { pub left: i32, pub top: i32, pub right: i32, pub bottom: i32 }` with `fn width(&self) -> i32`, `fn height(&self) -> i32`, `fn intersects(&self, o: &PxRect) -> bool`
  - `pub fn capture_area(window: PxRect, output: PxRect) -> Option<PxRect>` — window grown by `PAD`, clipped to `output`; `None` if empty after clipping
  - `pub struct Image { pub width: usize, pub height: usize, pub bgra: Vec<u8> }` (alpha always 255)
  - `pub fn blur_strip(src: &[u8], pitch: usize, width: usize, height: usize) -> Image` — BGRA source → downsample `SCALE` (drops the remainder rows/cols) → `BLUR_PASSES` box blurs of `BLUR_RADIUS` → BGRA
  - `pub fn any_hit(changed: &[PxRect], area: &PxRect) -> bool`
  - `#[derive(Default)] pub struct ChangeFilter` with `pub fn changed(&mut self, img: &Image) -> bool` (true on first call and whenever bytes or size differ)

- [ ] **Step 1: Write the failing tests** in `crates/core/tests/glass.rs`:

```rust
use newwin_core::glass::*;

fn solid(w: usize, h: usize, bgr: [u8; 3]) -> Vec<u8> {
    (0..w * h).flat_map(|_| [bgr[0], bgr[1], bgr[2], 255]).collect()
}

#[test]
fn capture_area_grows_and_clips() {
    let out = PxRect { left: 0, top: 0, right: 2560, bottom: 1440 };
    let bar = PxRect { left: 6, top: 6, right: 2554, bottom: 36 };
    assert_eq!(capture_area(bar, out), Some(PxRect { left: 0, top: 0, right: 2560, bottom: 60 }));
    let mid = PxRect { left: 200, top: 200, right: 1400, bottom: 234 };
    assert_eq!(capture_area(mid, out), Some(PxRect { left: 176, top: 176, right: 1424, bottom: 258 }));
    let off = PxRect { left: 3000, top: 10, right: 3100, bottom: 40 };
    assert_eq!(capture_area(off, out), None);
}

#[test]
fn solid_color_stays_solid() {
    let img = blur_strip(&solid(64, 16, [10, 120, 250]), 64 * 4, 64, 16);
    assert_eq!((img.width, img.height), (16, 4));
    assert!(img.bgra.chunks(4).all(|p| p == [10, 120, 250, 255]));
}

#[test]
fn pitch_and_remainder_are_respected() {
    // 66x18 with a 300-byte pitch: 2 extra columns/rows are dropped, padding bytes ignored.
    let mut src = vec![0u8; 300 * 18];
    for y in 0..18 { for x in 0..66 { src[y * 300 + x * 4..][..4].copy_from_slice(&[50, 50, 50, 255]); } }
    let img = blur_strip(&src, 300, 66, 18);
    assert_eq!((img.width, img.height), (16, 4));
    assert!(img.bgra.chunks(4).all(|p| p == [50, 50, 50, 255]));
}

#[test]
fn a_bright_spot_spreads_symmetrically_and_edges_do_not_darken() {
    let (w, h) = (64, 64);
    let mut src = solid(w, h, [0, 0, 0]);
    for y in 28..36 { for x in 28..36 { src[(y * w + x) * 4..][..3].copy_from_slice(&[255, 255, 255]); } }
    let img = blur_strip(&src, w * 4, w, h);
    let at = |x: usize, y: usize| img.bgra[(y * img.width + x) * 4];
    assert!(at(7, 7) > at(5, 7) && at(5, 7) > 0);
    assert_eq!(at(5, 7), at(9, 7));
    assert_eq!(at(7, 5), at(7, 9));
    let edge = blur_strip(&solid(w, h, [200, 200, 200]), w * 4, w, h);
    assert_eq!(edge.bgra[0], 200);
}

#[test]
fn hits_only_when_a_changed_rect_overlaps() {
    let area = PxRect { left: 100, top: 0, right: 200, bottom: 50 };
    assert!(!any_hit(&[], &area));
    assert!(!any_hit(&[PxRect { left: 0, top: 0, right: 100, bottom: 50 }], &area));
    assert!(any_hit(&[PxRect { left: 199, top: 49, right: 300, bottom: 60 }], &area));
}

#[test]
fn change_filter_passes_first_and_changed_images_only() {
    let mut f = ChangeFilter::default();
    let a = Image { width: 1, height: 1, bgra: vec![1, 2, 3, 255] };
    assert!(f.changed(&a));
    assert!(!f.changed(&a));
    assert!(f.changed(&Image { width: 1, height: 1, bgra: vec![1, 2, 4, 255] }));
    assert!(f.changed(&Image { width: 2, height: 1, bgra: vec![1, 2, 4, 255, 1, 2, 4, 255] }));
}
```

- [ ] **Step 2: Run them to verify they fail**
Run: `cargo test -p newwin-core --test glass`
Expected: compile error, `glass` not found in `newwin_core`.

- [ ] **Step 3: Implement `crates/core/src/glass.rs`** with the interfaces above. The box blur is separable (horizontal then vertical pass per iteration) and averages over the window clamped to the image (`lo = i.saturating_sub(r)`, `hi = min(i + r, n - 1)`, divide by the clamped count) — that is what keeps edges from darkening. Accumulate in `u32`/`f32`, round to nearest on output. The probe's `box_blur` is a working reference.

- [ ] **Step 4: Run the tests**
Run: `cargo test -p newwin-core --test glass`
Expected: `test result: ok. 6 passed`.

- [ ] **Step 5: Commit** `feat(core): glass image math (capture area, downsample+blur, change filter)`.

---

### Task 2: `glass` config flag (`newwin-config`)

**Files:**
- Modify: `crates/config/src/lib.rs` (`Appearance`: remove `background_opacity`, add `pub glass: bool`, default `true`)
- Test: `crates/config/tests/config.rs`

**Interfaces:**
- Produces: `Appearance.glass: bool`.

- [ ] **Step 1: Write the failing test** `glass_flag_defaults_on_and_old_opacity_is_ignored`: `Appearance::default().glass == true`; parsing `"[appearance]\nglass = false\n"` gives `glass == false`; parsing `"[appearance]\nbackground_opacity = 0.2\nheight = 32\n"` succeeds with `glass == true` and `height == 32`. Use the parse entry point the existing tests in this file use.
- [ ] **Step 2: Run** `cargo test -p newwin-config` → Expected: FAIL (no field `glass`).
- [ ] **Step 3: Implement.** Remove every use of `background_opacity` in the workspace except `apps/shell/src/bar.rs::paint_background`, which Task 4 rewrites; there, temporarily keep the white overlay out (just tint + border) so the workspace builds.
- [ ] **Step 4: Run** `cargo test --workspace` → Expected: all pass.
- [ ] **Step 5: Commit** `feat(config): appearance.glass replaces background_opacity`.

---

### Task 3: Capture thread (`newwin-ui`)

**Files:**
- Create: `crates/ui/src/backdrop.rs`
- Modify: `crates/ui/src/lib.rs` (`pub mod backdrop;`), `crates/ui/Cargo.toml` (add `newwin-core` if missing)
- Test: `crates/ui/tests/backdrop.rs`

**Interfaces:**
- Consumes: Task 1 (`PxRect`, `capture_area`, `blur_strip`, `any_hit`, `ChangeFilter`, `Image`, `MIN_INTERVAL_MS`).
- Produces (in `newwin_ui::backdrop`):
  - `pub struct Region { pub id: u64, pub window: PxRect }` — `id` is the glass window's HWND as `u64`; `window` in physical desktop px.
  - `pub struct Frame { pub id: u64, pub area: PxRect, pub image: Image }` — `area` is the capture area the image covers (so the receiver can place it).
  - `pub const FRAME: usize = 1; pub const UNAVAILABLE: usize = 2;` — WPARAM of the posted message; `FRAME` carries `Box<Frame>` in LPARAM, `UNAVAILABLE` carries 0.
  - `pub struct Backdrops` with `pub fn spawn(notify: HWND, msg: u32) -> Backdrops` and `pub fn set_regions(&self, regions: Vec<Region>)` (replaces the whole set; empty = release the device).
  - `pub unsafe fn take_frame(lp: LPARAM) -> Box<Frame>`.
  - `#[doc(hidden)] pub fn simulate_access_lost(&self)` — test hook: the thread drops its duplication exactly as on `DXGI_ERROR_ACCESS_LOST`.

**Thread behavior** (`newwin-glass`): with no regions it blocks on the channel. With regions it creates (once) the factory/adapter/output/hardware device/duplication — the output is the one whose `DesktopCoordinates` contains the first region; regions on other outputs are skipped — then loops: drain commands; `AcquireNextFrame(100)`; for each region whose `any_hit` (dirty + move rects, or "first frame after (re)creation") copy its area to that region's staging texture and mark it pending; release the frame; if pending and ≥ `MIN_INTERVAL_MS` since this region's last recompute, map, `blur_strip`, pass through the region's `ChangeFilter`, post `FRAME` if changed. On `DXGI_ERROR_ACCESS_LOST` drop duplication and retry on the next loop (after 250 ms); if creation fails, post `UNAVAILABLE` once per `set_regions` call and block until the next command. A posted `Box` whose `PostMessageW` fails is dropped on the thread. Staging textures are recreated when a region's area size changes.

- [ ] **Step 1: Write the failing tests** in `crates/ui/tests/backdrop.rs`. Helpers: a message-only notify window (`newwin_platform::window::create_test_window` with `HWND_MESSAGE` parent, or a hidden popup) and a pump that waits up to N ms for one message of `msg`. Create a visible test window filled with solid red via `WM_ERASEBKGND`/GDI at a fixed on-screen rect (e.g. 300,300 → 700,400).
  - `frames_show_what_is_under_the_region`: `set_regions(vec![Region { id: 7, window: <rect inside the red window> }])` → within 2 s a `FRAME` arrives with `id == 7` and the center pixel of `image` within ±8 of red `(b=0,g=0,r=255)`.
  - `an_unchanged_screen_posts_nothing_more`: after the first frame, no further `FRAME` for 1 s.
  - `empty_regions_stop_frames`: `set_regions(vec![])`, repaint the red window green, no `FRAME` for 1 s.
  - `capture_survives_access_lost` (Review Focus 2): `simulate_access_lost()`, then repaint the window green → a `FRAME` with green arrives within 2 s.
- [ ] **Step 2: Run** `cargo test -p newwin-ui --test backdrop` → Expected: compile error (`backdrop` missing).
- [ ] **Step 3: Implement `backdrop.rs`** per the behavior above; device and duplication creation can follow `own_blur_probe.rs`.
- [ ] **Step 4: Run** `cargo test -p newwin-ui --test backdrop -- --test-threads=1` → Expected: 4 passed.
- [ ] **Step 5: Commit** `feat(ui): newwin-glass capture thread with desktop duplication`.

---

### Task 4: Glass layers and painting (`newwin-ui`)

**Files:**
- Modify: `crates/ui/src/layer.rs`, `crates/ui/src/paint.rs`, `crates/ui/src/theme.rs`, `crates/ui/src/anim.rs`, `crates/ui/src/lib.rs`
- Create: `crates/ui/src/glass.rs`
- Test: `crates/ui/tests/glass_layer.rs`

**Interfaces:**
- Consumes: Task 1 `Image`, `PxRect`; Task 3 `Frame`.
- Produces:
  - `Backdrop::Glass` replaces `Backdrop::SystemAcrylic` (remove the Acrylic path). `Layer::new(gfx, hwnd, Backdrop::Glass, w, h)` also sets `WDA_EXCLUDEFROMCAPTURE`.
  - Visual tree for `Glass`: `root ─ glass(rounded clip) ─ backdrop` and `root ─ content` above it. `Layer::visual()` keeps returning `root`.
  - `pub fn set_corner_radius(&mut self, r: f32)` (clip radius, physical px).
  - `pub fn set_backdrop(&mut self, frame: &Frame, window_origin: (i32, i32)) -> Result<()>` — uploads `frame.image` into a backdrop surface of that size (recreated on size change), transform = scale `SCALE` then translate `(area.left - origin.x, area.top - origin.y)`, linear interpolation, commit. Keeps the last frame so a device loss re-uploads it.
  - `pub fn clear_backdrop(&mut self) -> Result<()>`, `pub fn has_backdrop(&self) -> bool`.
  - `pub fn backdrop_visual(&mut self) -> Result<Option<IDCompositionVisual2>>` (for the counter-animation).
  - `paint.rs`: `pub fn fill_round_rect_v(&mut self, r: RectF, radius: f32, stops: &[(f32, Color)])` and `pub fn stroke_round_rect_v(&mut self, r: RectF, radius: f32, stops: &[(f32, Color)], width: f32)` — vertical linear gradients from `r.y` to `r.bottom()`.
  - `theme.rs`: `GLASS_TINT = Color::rgba(8,8,10,0.55)`, `GLASS_TINT_SOLID = Color::rgba(8,8,10,0.88)`, `GLASS_SHEEN: [(f32, Color); 3]`, `GLASS_RIM: [(f32, Color); 3]` with the spec values; remove `TINT`, `BORDER`, `POPUP_BG` once nothing uses them.
  - `glass.rs`: `pub fn glass_tint(with_backdrop: bool) -> Color` (`GLASS_TINT` or `GLASS_TINT_SOLID`) and `pub fn paint_glass(p: &mut Painter, r: RectF, radius: f32, with_backdrop: bool)` — that tint, sheen, 1 px rim. (`Color` needs a readable alpha: add `pub fn a(&self) -> f32` if the field is private.)
  - `anim.rs`: `pub fn slide_y_both(gfx, outer: &IDCompositionVisual2, inner: Option<&IDCompositionVisual2>, from: f32, to: f32, ms: u32) -> Result<()>` — `outer` animates `from → to`, `inner` animates `-from → -to` with the same curve, one commit; `set_offset_opacity` gets the same optional `inner` (set to `-y`).

- [ ] **Step 1: Write the failing tests** in `crates/ui/tests/glass_layer.rs` (test window via `newwin_platform::window::create_test_window`, `Gfx` as other tests/examples create it):
  - `glass_layer_is_excluded_from_capture`: after `Layer::new(.., Backdrop::Glass, ..)`, `GetWindowDisplayAffinity` returns `WDA_EXCLUDEFROMCAPTURE`.
  - `backdrop_round_trip`: `has_backdrop()` false → `set_backdrop(&frame_4x2)` → true → `clear_backdrop()` → false; a second `set_backdrop` with a different image size succeeds.
  - `glass_tint_depends_on_backdrop` (Review Focus 5): `glass_tint(true).a == 0.55`, `glass_tint(false).a == 0.88`.
- [ ] **Step 2: Run** `cargo test -p newwin-ui --test glass_layer` → Expected: compile errors for the missing API.
- [ ] **Step 3: Implement** the interfaces above. DComp names in `windows` 0.62: `CreateRectangleClip` + `SetLeft2/…/SetBottomRightRadiusY2`, `SetClip`, `SetBitmapInterpolationMode(DCOMPOSITION_BITMAP_INTERPOLATION_MODE_LINEAR)`, `SetTransform2`, `AddVisual(child, insertabove, reference)` (see `own_blur_probe.rs`). Gradient collections come from `dc.cast::<ID2D1RenderTarget>()?.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)`.
- [ ] **Step 4: Run** `cargo test --workspace` and `cargo clippy --workspace --all-targets` → Expected: all pass, no warnings.
- [ ] **Step 5: Commit** `feat(ui): glass layer tree, gradient painting, counter-animated backdrop`.

---

### Task 5: Wire glass into the bar and popups (`apps/shell`, `newwin-ui` popup)

**Files:**
- Modify: `apps/shell/src/app.rs` (spawn `Backdrops` when `config.appearance.glass`, route `WM_APP_GLASS = WM_APP + 6`), `apps/shell/src/app/visibility.rs` (region sync, counter-animation), `apps/shell/src/bar.rs` (`paint_background` → `paint_glass`), `apps/shell/src/app/menus.rs` (popup regions and frames), `crates/ui/src/popup.rs` (glass layer, `paint_glass`, regions)
- Test: `apps/shell/src/app/glass_regions.rs` (new, pure) with unit tests inside

**Interfaces:**
- Consumes: Task 3 `Backdrops`, `Region`, `Frame`, `FRAME`, `UNAVAILABLE`, `take_frame`; Task 4 `Backdrop::Glass`, `set_backdrop`, `clear_backdrop`, `backdrop_visual`, `slide_y_both`, `paint_glass`.
- Produces:
  - `glass_regions.rs`: `pub(super) fn wanted_regions(bar: Option<(u64, PxRect)>, bar_visible: bool, in_zone: bool, popups: &[(u64, PxRect)]) -> Vec<Region>` — the bar region is included iff `bar_visible || in_zone`; every open popup window is included.
  - `pub(super) fn frame_target(frame_id: u64, regions: &[Region]) -> bool` — a frame is applied only if its id is still registered (Review Focus 1).
  - `Popup`: `pub fn windows(&self) -> Vec<(HWND, RECT)>` (root and the open submenu), `pub fn set_backdrop(&mut self, hwnd: HWND, frame: &Frame)`, `pub fn set_glass_unavailable(&mut self)`; and a new notification code `REGIONS` (alongside `PICKED`/`CLOSED`) posted when a submenu opens or closes.

- [ ] **Step 1: Write the failing unit tests** in `glass_regions.rs`:
  - `bar_region_follows_visibility_or_zone`: hidden & not in zone → no bar region; in zone → bar region; visible → bar region.
  - `popups_are_always_included`: two popups → two regions plus the bar when visible.
  - `stale_frames_are_dropped`: `frame_target(9, &[])` false; `frame_target(9, &[Region { id: 9, .. }])` true.
- [ ] **Step 2: Run** `cargo test -p newwin-shell glass_regions` → Expected: compile error (module missing).
- [ ] **Step 3: Implement.**
  - `App::sync_glass(&mut self)` computes `wanted_regions` and calls `set_regions` only when the set differs from the last one sent (keep it in `App`). Call it after `feed`, on pointer enter/leave, in the `HIDE_WINDOW_TIMER` handler, on popup open/close/`REGIONS`, on `reposition`, and on `WM_DISPLAYCHANGE`/`WM_DPICHANGED`.
  - `WM_APP_GLASS` + `FRAME`: drop unless `frame_target`; bar id → `bar_layer.set_backdrop(&frame, (bar_rect.left, bar_rect.top))`, else the popup that owns that HWND. If the bar had no backdrop before, repaint it with `with_backdrop = true`. `UNAVAILABLE` → `clear_backdrop` on all glass layers and repaint with `with_backdrop = false`.
  - `animate()`: use `slide_y_both(.., outer = root visual, inner = layer.backdrop_visual())`, including the mid-hide resume path, so the blur stays fixed on screen (Review Focus 4). Fade stays on the root only.
  - `glass = false`: no `Backdrops` spawned, layers still use `Backdrop::Glass` tree but never get a frame → fallback paint.
- [ ] **Step 4: Run** `cargo test --workspace`, `cargo clippy --workspace --all-targets`, `cargo build --release` → Expected: all pass, no warnings.
- [ ] **Step 5: Live check** (stop NewWin first, start with `NEWWIN_DEBUG=1`): reveal the bar over a browser — glass with visible blur; reveal mid-hide — no jump of the blurred image; right-click menu and a submenu — both glass; `glass = false` in config + restart — fallback look; hide the bar and wait 5 s — `Get-Process NewWin` threads back to the pre-glass count (~11 for the shell).
- [ ] **Step 6: Commit** `feat(shell): liquid glass on the bar and popups`.

---

### Task 6: Measurement and acceptance for glass

**Files:**
- Modify: `scripts/measure.ps1` (add `-Label` printed in the header; no other change), `docs/superpowers/notes/2026-10-21-m1-acceptance.md` (new "Glass" section)

**Interfaces:**
- Consumes: the release build from Task 5.

- [ ] **Step 1: Hidden bar idle** — `powershell -File scripts/measure.ps1 -Minutes 10 -Label hidden` with the bar hidden. Expected: all PASS as in the M1 run.
- [ ] **Step 2: Pinned over static content** — set `mode = "pinned"`, restart, leave the screen still: `measure.ps1 -Minutes 2 -Label pinned-static`. Expected: shell CPU ≤ 0.05 %, private WS < 20 MB.
- [ ] **Step 3: Pinned over video** — a video playing in a window under the bar: `measure.ps1 -Minutes 2 -Label pinned-video`. Expected: shell CPU ≤ 0.3 % (the script's 0.05 % CPU line will say FAIL here; record the number against the 0.3 % budget instead).
- [ ] **Step 4: Pinned, video elsewhere** (not under the bar): Expected: shell CPU ≤ 0.05 %.
- [ ] **Step 5: Record** all four results and the Task 5 live checks in the acceptance notes; set `mode` back to `auto-hide`.
- [ ] **Step 6: Commit** `test: glass measurements and acceptance notes`.
