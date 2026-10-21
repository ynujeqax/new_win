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
