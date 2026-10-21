# Glass probe (Task 8)

Probe: `cargo run -p newwin-ui --example glass_probe -- a|b [seconds]` — a 600×30 bar over a half white
(red stripes) / half black backdrop. Captured with `cargo run -p newwin-ui --example screenshot` (DXGI
Desktop Duplication; GDI `CopyFromScreen` returned stale frames on this HDR WOLED setup).

## Variant A — system Acrylic (`DWMSBT_TRANSIENTWINDOW`, dark mode, `DWMWCP_ROUND`, frame extended)
- All DWM calls return S_OK on 26100; the window gets a backdrop, DWM round corners (~8 px) and a DWM shadow.
- The backdrop renders as an almost opaque mid-gray (~#606060) both over white/stripes and over black:
  the stripes are not visible through it, so the blur adds nothing, and on a black wallpaper it is a light
  gray slab — far brighter than the agreed design.
- Corner radius is fixed by DWM (8 px), not the spec's 11 px.

## Variant B — tint, drawn by Direct2D on a `WS_EX_NOREDIRECTIONBITMAP` + DirectComposition window
- Rounded rect radius 11 with tint `rgba(18,18,20,0.88)` + overlay `rgba(255,255,255,0.065)` + 1 px border.
- Corners transparent and antialiased, text crisp (grayscale AA), readable over white and over black.
- Over black it matches the approved mockup.

## Other findings
- `IDCompositionSurface::BeginDraw` can return `ID2D1DeviceContext` only when the composition device is
  created from the D2D device (`DCompositionCreateDevice2(&d2d_device)` → `IDCompositionDesktopDevice`);
  created from DXGI it fails with E_INVALIDARG.
- A message loop must only react to its own timer id: system components post `WM_TIMER` to the thread
  within ~150 ms.

DECISION: Tint

## Follow-up 2026-10-22 — why Acrylic was a gray slab
- The user's Windows has Transparency effects off (`HKCU\...\Themes\Personalize\EnableTransparency = 0`).
  With it off, DWM replaces every backdrop with a solid fill — that is the gray slab from variant A.
- Also tried the undocumented `SetWindowCompositionAttribute` accent policy (acrylic = 4, blur = 3) under a
  light tint: same result, a solid dark fill; the stripes behind are not visible. No blur path exists while
  the setting is off (capturing and blurring the screen ourselves is ruled out: constant GPU/CPU work).
- The user picked "liquid glass" (variant C in the chat mockup: blur + bright top rim + light rim + soft shadow).

## Own blur without Windows transparency (2026-10-22) — `cargo run --release -p newwin-ui --example own_blur_probe -- bar 20`
Design probed: the bar is excluded from capture (`WDA_EXCLUDEFROMCAPTURE`); DXGI Desktop Duplication on a
hardware D3D11 device; when a dirty/move rect hits the strip under the bar (+24 px), the GPU copies the strip
(1248x82) to a staging texture; at most 30 times a second the CPU downsamples it 4x (312x20), box-blurs it
(radius 2, 3 passes ≈ gaussian) and uploads it to a tiny DComp surface. DWM scales it 4x (linear) under a
rounded-rect clip; tint, sheen, rims and text live in a separate visual drawn once.

| Scenario (20 s, bar 1200x34) | Updates | Probe CPU | Threads / private WS |
|---|---|---|---|
| 60 fps moving stripes under the bar | 420 (21/s) | 2.0 % of one core (0.13 % of the machine) | 32 / 13 MB |
| Static content under the bar | 1 | 47 ms total (0.015 % of the machine) | 32 / 13 MB |
| Capture device released (bar hidden) | — | — | 11 / 7.5 MB |

- Capture setup (device + `DuplicateOutput`) 96–126 ms, first blurred frame 4 ms after that: fits in the
  150 ms hover delay before the reveal, so the device can be created on zone enter and released on hide.
- The GPU driver's threads (+21) exist only while the device exists; releasing it returns to 11 threads.
- First version redrew the whole bar on WARP per update: 5.3 % of one core. Two fixes mattered:
  (1) only the tiny backdrop surface changes, DWM scales it; (2) our own commit dirties the strip, so an
  unchanged picture must be skipped — without that the bar woke itself 21 times a second over static content.
- Cost of the exclusion: the bar is not in screenshots or screen recordings (user accepted, 2026-10-22).

DECISION (spike): own blur is viable. Next: spec addition, then implementation in the shell.
