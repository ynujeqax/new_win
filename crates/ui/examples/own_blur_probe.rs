//! Spike (throwaway): our own backdrop blur without Windows transparency effects.
//! The bar reads the screen strip under itself with DXGI Desktop Duplication (only when that strip
//! changed), downsamples it 4x, box-blurs it on the CPU and draws it scaled up under a liquid-glass
//! tint. The bar is excluded from capture so it never blurs itself.
//!
//! `own_blur_probe bar [secs] [hw|keep]` — the bar at (200, 200), 1200x34; prints its stats.
//! `own_blur_probe anim [secs]` — a moving colorful backdrop under the bar (run as another process).

use std::time::{Duration, Instant};

use windows::Win32::Foundation::{COLORREF, FILETIME, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_DRIVER_TYPE_WARP};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::DirectComposition::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{Interface, Result, w};
use windows_numerics::Matrix3x2;

const BAR: RECT = RECT { left: 200, top: 200, right: 1400, bottom: 234 };
const PAD: i32 = 24;
const SCALE: i32 = 4;

static mut PHASE: i32 = 0;

unsafe extern "system" fn anim_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_TIMER => {
                PHASE += 6;
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                let colors = [0x002E57E4u32, 0x0012A7F3, 0x005C3329, 0x00EBC05B, 0x00FFFFFF, 0x00101010];
                let mut x = -(PHASE % 360);
                let mut i = 0;
                while x < rc.right {
                    let b = CreateSolidBrush(COLORREF(colors[i % colors.len()]));
                    FillRect(hdc, &RECT { left: x, top: 0, right: x + 60, bottom: rc.bottom }, b);
                    let _ = DeleteObject(b.into());
                    x += 60;
                    i += 1;
                }
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

unsafe extern "system" fn bar_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

fn pump_until(deadline: Instant) -> bool {
    let mut msg = MSG::default();
    unsafe {
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Instant::now() < deadline
}

fn cpu_ms() -> u64 {
    let (mut c, mut e, mut k, mut u) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
    unsafe { GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u).unwrap() };
    let t = |f: FILETIME| ((f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64) / 10_000;
    t(k) + t(u)
}

fn color(r: u8, g: u8, b: u8, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a }
}

/// Box blur, radius `r`, horizontal then vertical, on RGB f32 pixels.
fn box_blur(px: &mut [[f32; 3]], w: usize, h: usize, r: usize) {
    let mut tmp = px.to_vec();
    for y in 0..h {
        for x in 0..w {
            let (lo, hi) = (x.saturating_sub(r), (x + r).min(w - 1));
            let mut s = [0.0; 3];
            for i in lo..=hi {
                for c in 0..3 {
                    s[c] += px[y * w + i][c];
                }
            }
            let n = (hi - lo + 1) as f32;
            tmp[y * w + x] = [s[0] / n, s[1] / n, s[2] / n];
        }
    }
    for y in 0..h {
        for x in 0..w {
            let (lo, hi) = (y.saturating_sub(r), (y + r).min(h - 1));
            let mut s = [0.0; 3];
            for i in lo..=hi {
                for c in 0..3 {
                    s[c] += tmp[i * w + x][c];
                }
            }
            let n = (hi - lo + 1) as f32;
            px[y * w + x] = [s[0] / n, s[1] / n, s[2] / n];
        }
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("bar");
    let secs: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10);
    let deadline = Instant::now() + Duration::from_secs(secs);

    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let instance = GetModuleHandleW(None)?;
        if mode == "anim" {
            RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(anim_proc), hInstance: instance.into(), lpszClassName: w!("ProbeAnim"), ..Default::default() });
            let h = CreateWindowExW(WS_EX_TOOLWINDOW, w!("ProbeAnim"), w!("anim"), WS_POPUP | WS_VISIBLE, 100, 150, 1400, 140, None, None, Some(instance.into()), None)?;
            SetTimer(Some(h), 1, 16, None);
            while pump_until(deadline) {
                MsgWaitForMultipleObjects(None, false, 50, QS_ALLINPUT);
            }
            return Ok(());
        }

        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(bar_proc), hInstance: instance.into(), lpszClassName: w!("ProbeOwnBlur"), ..Default::default() });
        let (bw, bh) = (BAR.right - BAR.left, BAR.bottom - BAR.top);
        let bar = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_NOREDIRECTIONBITMAP,
            w!("ProbeOwnBlur"), w!("bar"), WS_POPUP, BAR.left, BAR.top, bw, bh, None, None, Some(instance.into()), None,
        )?;
        SetWindowDisplayAffinity(bar, WDA_EXCLUDEFROMCAPTURE)?;

        // Rendering exactly like the shell: WARP D2D + DirectComposition.
        let mut warp = None;
        D3D11CreateDevice(None, D3D_DRIVER_TYPE_WARP, Default::default(), D3D11_CREATE_DEVICE_BGRA_SUPPORT, None, D3D11_SDK_VERSION, Some(&mut warp), None, None)?;
        let dxgi: IDXGIDevice = warp.unwrap().cast()?;
        let factory: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let d2d = factory.CreateDevice(&dxgi)?;
        let dcomp: IDCompositionDesktopDevice = DCompositionCreateDevice2(&d2d)?;
        let target = dcomp.CreateTargetForHwnd(bar, true)?;
        // root ─ glass (rounded clip) ─ backdrop (tiny blurred bitmap, scaled 4x by DWM)
        //      └ overlay (tint, sheen, rims, text: drawn once)
        let root = dcomp.CreateVisual()?;
        let glass = dcomp.CreateVisual()?;
        let backdrop = dcomp.CreateVisual()?;
        let visual = dcomp.CreateVisual()?;
        let surface = dcomp.CreateSurface(bw as u32, bh as u32, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_ALPHA_MODE_PREMULTIPLIED)?;
        visual.SetContent(&surface)?;
        let clip = dcomp.CreateRectangleClip()?;
        clip.SetLeft2(0.0)?;
        clip.SetTop2(0.0)?;
        clip.SetRight2(bw as f32)?;
        clip.SetBottom2(bh as f32)?;
        for r in [11.0f32] {
            clip.SetTopLeftRadiusX2(r)?;
            clip.SetTopLeftRadiusY2(r)?;
            clip.SetTopRightRadiusX2(r)?;
            clip.SetTopRightRadiusY2(r)?;
            clip.SetBottomLeftRadiusX2(r)?;
            clip.SetBottomLeftRadiusY2(r)?;
            clip.SetBottomRightRadiusX2(r)?;
            clip.SetBottomRightRadiusY2(r)?;
        }
        glass.SetClip(&clip)?;
        backdrop.SetBitmapInterpolationMode(DCOMPOSITION_BITMAP_INTERPOLATION_MODE_LINEAR)?;
        glass.AddVisual(&backdrop, false, None)?;
        root.AddVisual(&glass, false, None)?;
        root.AddVisual(&visual, true, &glass)?;
        target.SetRoot(&root)?;
        let dw: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let fmt = dw.CreateTextFormat(w!("Segoe UI Variable Text"), None, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, 13.0, w!("en-us"))?;
        fmt.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        let label: Vec<u16> = "Steam     Claude     File Explorer                                                                       Thu Oct 22   2:37 AM".encode_utf16().collect();

        // Capture: Desktop Duplication needs a device on the output's adapter.
        let t0 = Instant::now();
        let dxgi_factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let adapter = dxgi_factory.EnumAdapters1(0)?;
        let output: IDXGIOutput1 = adapter.EnumOutputs(0)?.cast()?;
        let (mut hw, mut ctx) = (None, None);
        D3D11CreateDevice(&adapter, D3D_DRIVER_TYPE_UNKNOWN, Default::default(), D3D11_CREATE_DEVICE_FLAG(0), None, D3D11_SDK_VERSION, Some(&mut hw), None, Some(&mut ctx))?;
        let (hw, ctx): (ID3D11Device, ID3D11DeviceContext) = (hw.unwrap(), ctx.unwrap());
        let dup = output.DuplicateOutput(&hw)?;
        let setup_ms = t0.elapsed().as_millis();
        let odesc = dup.GetDesc();
        let (sw, sh) = (odesc.ModeDesc.Width as i32, odesc.ModeDesc.Height as i32);

        let cap = RECT { left: (BAR.left - PAD).max(0), top: (BAR.top - PAD).max(0), right: (BAR.right + PAD).min(sw), bottom: (BAR.bottom + PAD).min(sh) };
        let (cw, ch) = (cap.right - cap.left, cap.bottom - cap.top);
        let (dw_, dh_) = ((cw / SCALE) as usize, (ch / SCALE) as usize);
        let mut staging = None;
        hw.CreateTexture2D(
            &D3D11_TEXTURE2D_DESC {
                Width: cw as u32, Height: ch as u32, MipLevels: 1, ArraySize: 1, Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 }, Usage: D3D11_USAGE_STAGING, BindFlags: 0,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32, MiscFlags: 0,
            },
            None,
            Some(&mut staging),
        )?;
        let staging = staging.unwrap();

        let dc_ctx = d2d.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
        let bmp_props = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
            dpiX: 96.0, dpiY: 96.0, ..Default::default()
        };
        let small = dc_ctx.CreateBitmap(D2D_SIZE_U { width: dw_ as u32, height: dh_ as u32 }, None, 0, &bmp_props)?;
        let small_surface = dcomp.CreateSurface(dw_ as u32, dh_ as u32, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_ALPHA_MODE_PREMULTIPLIED)?;
        backdrop.SetContent(&small_surface)?;
        backdrop.SetTransform2(
            &(Matrix3x2 { M11: SCALE as f32, M12: 0.0, M21: 0.0, M22: SCALE as f32, M31: 0.0, M32: 0.0 }
                * Matrix3x2::translation((cap.left - BAR.left) as f32, (cap.top - BAR.top) as f32)),
        )?;
        draw_overlay(&surface, bw, bh, &fmt, &label)?;

        let _ = ShowWindow(bar, SW_SHOWNOACTIVATE);
        let (mut frames, mut updates, mut skipped, mut first_update_ms) = (0u32, 0u32, 0u32, None);
        let mut last_update = Instant::now() - Duration::from_secs(1);
        let mut px = vec![[0f32; 3]; dw_ * dh_];
        let mut out = vec![0u8; dw_ * dh_ * 4];
        let mut prev = vec![1u8; dw_ * dh_ * 4];
        let mut unchanged = 0u32;
        let mut rects = vec![RECT::default(); 64];
        let cpu0 = cpu_ms();
        let mut need = false;

        while pump_until(deadline) {
            let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut res = None;
            let got = dup.AcquireNextFrame(100, &mut info, &mut res).is_ok();
            if got {
                frames += 1;
            }
            if got && info.LastPresentTime != 0 {
                let mut bytes = 0u32;
                let ok = dup.GetFrameDirtyRects((rects.len() * size_of::<RECT>()) as u32, rects.as_mut_ptr(), &mut bytes).is_ok();
                let n = bytes as usize / size_of::<RECT>();
                let hit = !ok || rects[..n].iter().any(|r| r.left < cap.right && r.right > cap.left && r.top < cap.bottom && r.bottom > cap.top);
                // Moves count as changes too.
                let mut mbytes = 0u32;
                let mut moves = [DXGI_OUTDUPL_MOVE_RECT::default(); 16];
                let _ = dup.GetFrameMoveRects((moves.len() * size_of::<DXGI_OUTDUPL_MOVE_RECT>()) as u32, moves.as_mut_ptr(), &mut mbytes);
                if hit || mbytes > 0 || updates == 0 {
                    // The GPU copy of the strip is cheap; the CPU work waits for the rate limit.
                    let tex: ID3D11Texture2D = res.take().unwrap().cast()?;
                    let bx = D3D11_BOX { left: cap.left as u32, top: cap.top as u32, front: 0, right: cap.right as u32, bottom: cap.bottom as u32, back: 1 };
                    ctx.CopySubresourceRegion(&staging, 0, 0, 0, 0, &tex, 0, Some(&bx));
                    need = true;
                }
            }
            if got {
                let _ = dup.ReleaseFrame();
            }
            if need && last_update.elapsed() >= Duration::from_millis(33) {
                let mut m = D3D11_MAPPED_SUBRESOURCE::default();
                ctx.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut m))?;
                let base = m.pData as *const u8;
                for y in 0..dh_ {
                    for x in 0..dw_ {
                        let mut s = [0f32; 3];
                        for yy in 0..SCALE as usize {
                            let row = base.add((y * SCALE as usize + yy) * m.RowPitch as usize);
                            for xx in 0..SCALE as usize {
                                let p = row.add((x * SCALE as usize + xx) * 4);
                                s[0] += *p as f32;
                                s[1] += *p.add(1) as f32;
                                s[2] += *p.add(2) as f32;
                            }
                        }
                        let k = (SCALE * SCALE) as f32;
                        px[y * dw_ + x] = [s[0] / k, s[1] / k, s[2] / k];
                    }
                }
                ctx.Unmap(&staging, 0);
                for _ in 0..3 {
                    box_blur(&mut px, dw_, dh_, 2);
                }
                for (i, p) in px.iter().enumerate() {
                    out[i * 4] = p[0] as u8;
                    out[i * 4 + 1] = p[1] as u8;
                    out[i * 4 + 2] = p[2] as u8;
                    out[i * 4 + 3] = 255;
                }
                // Our own commit dirties the strip too; an unchanged picture must not redraw,
                // or the bar keeps waking itself up.
                need = false;
                if out == prev {
                    unchanged += 1;
                    continue;
                }
                prev.copy_from_slice(&out);
                small.CopyFromMemory(None, out.as_ptr() as _, (dw_ * 4) as u32)?;

                let mut off = POINT::default();
                let dc: ID2D1DeviceContext = small_surface.BeginDraw(None, &mut off)?;
                let dst = D2D_RECT_F { left: off.x as f32, top: off.y as f32, right: (off.x as usize + dw_) as f32, bottom: (off.y as usize + dh_) as f32 };
                dc.DrawBitmap(&small, Some(&dst), 1.0, D2D1_INTERPOLATION_MODE_NEAREST_NEIGHBOR, None, None);
                small_surface.EndDraw()?;
                dcomp.Commit()?;
                updates += 1;
                need = false;
                last_update = Instant::now();
                first_update_ms.get_or_insert(t0.elapsed().as_millis());
                // At most 30 updates a second; changes meanwhile accumulate in the duplication.
                MsgWaitForMultipleObjects(None, false, 33, QS_ALLINPUT);
            } else if need {
                skipped += 1;
            }
        }
        let cpu = cpu_ms() - cpu0;
        println!(
            "setup={setup_ms}ms first_update={first_update_ms:?}ms frames={frames} updates={updates} unchanged={unchanged} rate_limited={skipped} cpu={cpu}ms over {secs}s ({:.3}% of one core) capture={cw}x{ch} small={dw_}x{dh_}",
            cpu as f64 / (secs as f64 * 10.0)
        );
        // Hidden bar: drop the capture device; does the GPU driver's thread pool go away?
        drop((dup, staging, ctx, hw, output, adapter, dxgi_factory));
        println!("released");
        let rest = Instant::now() + Duration::from_secs(4);
        while pump_until(rest) {
            MsgWaitForMultipleObjects(None, false, 100, QS_ALLINPUT);
        }
        let t1 = Instant::now();
        let f2: IDXGIFactory1 = CreateDXGIFactory1()?;
        let a2 = f2.EnumAdapters1(0)?;
        let o2: IDXGIOutput1 = a2.EnumOutputs(0)?.cast()?;
        let mut hw2: Option<ID3D11Device> = None;
        D3D11CreateDevice(&a2, D3D_DRIVER_TYPE_UNKNOWN, Default::default(), D3D11_CREATE_DEVICE_FLAG(0), None, D3D11_SDK_VERSION, Some(&mut hw2), None, None)?;
        let _dup2 = o2.DuplicateOutput(&hw2.unwrap())?;
        println!("second setup={}ms", t1.elapsed().as_millis());
    }
    Ok(())
}

/// Tint, sheen, rims and text: drawn once, composed above the backdrop.
unsafe fn draw_overlay(surface: &IDCompositionSurface, bw: i32, bh: i32, fmt: &IDWriteTextFormat, label: &[u16]) -> Result<()> {
    unsafe {
        let mut off = POINT::default();
            let dc: ID2D1DeviceContext = surface.BeginDraw(None, &mut off)?;
            dc.SetTransform(&Matrix3x2::translation(off.x as f32, off.y as f32));
            dc.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            dc.Clear(Some(&color(0, 0, 0, 0.0)));
            let full = D2D_RECT_F { left: 0.5, top: 0.5, right: bw as f32 - 0.5, bottom: bh as f32 - 0.5 };
            let rr = D2D1_ROUNDED_RECT { rect: full, radiusX: 11.0, radiusY: 11.0 };
            // Liquid glass: darkening tint, soft top sheen, bright top rim, faint full rim.
            dc.FillRoundedRectangle(&rr, &dc.CreateSolidColorBrush(&color(8, 8, 10, 0.55), None)?);
            let stops = [
                D2D1_GRADIENT_STOP { position: 0.0, color: color(255, 255, 255, 0.16) },
                D2D1_GRADIENT_STOP { position: 0.55, color: color(255, 255, 255, 0.02) },
                D2D1_GRADIENT_STOP { position: 1.0, color: color(255, 255, 255, 0.05) },
            ];
            let coll = dc.cast::<ID2D1RenderTarget>()?.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)?;
            let sheen = dc.CreateLinearGradientBrush(
                &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: windows_numerics::Vector2 { X: 0.0, Y: 0.0 }, endPoint: windows_numerics::Vector2 { X: 0.0, Y: bh as f32 } },
                None,
                &coll,
            )?;
            dc.FillRoundedRectangle(&rr, &sheen);
            let rim_stops = [
                D2D1_GRADIENT_STOP { position: 0.0, color: color(255, 255, 255, 0.50) },
                D2D1_GRADIENT_STOP { position: 0.35, color: color(255, 255, 255, 0.10) },
                D2D1_GRADIENT_STOP { position: 1.0, color: color(255, 255, 255, 0.14) },
            ];
            let rim_coll = dc.cast::<ID2D1RenderTarget>()?.CreateGradientStopCollection(&rim_stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)?;
            let rim = dc.CreateLinearGradientBrush(
                &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: windows_numerics::Vector2 { X: 0.0, Y: 0.0 }, endPoint: windows_numerics::Vector2 { X: 0.0, Y: bh as f32 } },
                None,
                &rim_coll,
            )?;
            dc.DrawRoundedRectangle(&rr, &rim, 1.0, None);
            let lr = D2D_RECT_F { left: 16.0, top: 0.0, right: bw as f32 - 16.0, bottom: bh as f32 };
            dc.DrawText(label, fmt, &lr, &dc.CreateSolidColorBrush(&color(236, 236, 240, 1.0), None)?, D2D1_DRAW_TEXT_OPTIONS_NONE, DWRITE_MEASURING_MODE_NATURAL);
            surface.EndDraw()
    }
}
