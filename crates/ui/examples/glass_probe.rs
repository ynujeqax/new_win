//! Spike: can a frameless tool window get the system Acrylic backdrop with rounded corners (variant `a`),
//! or do we draw our own tinted rounded rect on a transparent DirectComposition window (variant `b`)?
//!
//! Shows a half-white / half-black backdrop window with the probe bar on top for N seconds.
//! Usage: `cargo run -p newwin-ui --example glass_probe -- a|b [seconds]`

use windows_numerics::Matrix3x2;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{D2D_RECT_F, D2D1_COLOR_F};
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device};
use windows::Win32::Graphics::DirectComposition::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::Graphics::Gdi::{BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, PAINTSTRUCT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{Interface, PCWSTR, Result, w};

const BAR_W: i32 = 600;
const BAR_H: i32 = 30;
const ORIGIN: (i32, i32) = (200, 200);

unsafe extern "system" fn backdrop_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_PAINT {
        unsafe {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let half = RECT { right: rc.right / 2, ..rc };
            let white = CreateSolidBrush(COLORREF(0x00FFFFFF));
            FillRect(hdc, &half, white);
            let _ = DeleteObject(white.into());
            let rest = RECT { left: rc.right / 2, ..rc };
            let black = CreateSolidBrush(COLORREF(0));
            FillRect(hdc, &rest, black);
            let _ = DeleteObject(black.into());
            // Stripes on the white half so blur is visible.
            let stripe = CreateSolidBrush(COLORREF(0x000000FF));
            for i in 0..12 {
                let s = RECT { left: i * 30, top: 0, right: i * 30 + 8, bottom: rc.bottom };
                if s.right < rc.right / 2 {
                    FillRect(hdc, &s, stripe);
                }
            }
            let _ = DeleteObject(stripe.into());
            let _ = EndPaint(hwnd, &ps);
        }
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

unsafe extern "system" fn bar_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

fn color(r: u8, g: u8, b: u8, a: f32) -> D2D1_COLOR_F {
    // Premultiplied surface: D2D premultiplies brush colors itself.
    D2D1_COLOR_F { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let variant = args.get(1).map(String::as_str).unwrap_or("a");
    let secs: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(6);

    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let instance = GetModuleHandleW(None)?;
        let cursor = LoadCursorW(None, IDC_ARROW)?;
        for (name, proc_) in [(w!("ProbeBackdrop"), Some(backdrop_proc as _)), (w!("ProbeBar"), Some(bar_proc as _))] {
            RegisterClassW(&WNDCLASSW { lpfnWndProc: proc_, hInstance: instance.into(), lpszClassName: name, hCursor: cursor, ..Default::default() });
        }

        let _backdrop = CreateWindowExW(
            WS_EX_TOOLWINDOW, w!("ProbeBackdrop"), w!("backdrop"), WS_POPUP | WS_VISIBLE,
            ORIGIN.0 - 100, ORIGIN.1 - 60, BAR_W + 200, BAR_H + 120, None, None, Some(instance.into()), None,
        )?;

        let bar = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_NOREDIRECTIONBITMAP,
            w!("ProbeBar"), w!("bar"), WS_POPUP,
            ORIGIN.0, ORIGIN.1, BAR_W, BAR_H, None, None, Some(instance.into()), None,
        )?;

        if variant == "a" {
            let dark: i32 = 1;
            let _ = DwmSetWindowAttribute(bar, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const _ as _, 4);
            let backdrop = DWMSBT_TRANSIENTWINDOW;
            let r1 = DwmSetWindowAttribute(bar, DWMWA_SYSTEMBACKDROP_TYPE, &backdrop as *const _ as _, 4);
            let corner = DWMWCP_ROUND;
            let r2 = DwmSetWindowAttribute(bar, DWMWA_WINDOW_CORNER_PREFERENCE, &corner as *const _ as _, 4);
            let r3 = DwmExtendFrameIntoClientArea(bar, &MARGINS { cxLeftWidth: -1, cxRightWidth: -1, cyTopHeight: -1, cyBottomHeight: -1 });
            println!("backdrop={r1:?} corner={r2:?} extend={r3:?}");
        }

        // Device chain: D3D11 → DXGI → D2D device context + DirectComposition.
        let mut d3d: Option<ID3D11Device> = None;
        D3D11CreateDevice(None, D3D_DRIVER_TYPE_HARDWARE, Default::default(), D3D11_CREATE_DEVICE_BGRA_SUPPORT, None, D3D11_SDK_VERSION, Some(&mut d3d), None, None)?;
        println!("d3d ok");
        let dxgi: IDXGIDevice = d3d.unwrap().cast()?;
        let factory: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let d2d_device = factory.CreateDevice(&dxgi)?;
        let dc_owner = d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
        let dcomp: IDCompositionDesktopDevice = DCompositionCreateDevice2(&d2d_device)?;
        println!("dcomp ok");
        let target = dcomp.CreateTargetForHwnd(bar, true)?;
        let visual = dcomp.CreateVisual()?;
        let surface = dcomp.CreateSurface(BAR_W as u32, BAR_H as u32, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_ALPHA_MODE_PREMULTIPLIED)?;
        let _ = dc_owner;

        println!("surface ok");
        let mut offset = POINT::default();
        let dc: ID2D1DeviceContext = surface.BeginDraw(None, &mut offset)?;
        dc.SetTransform(&Matrix3x2::translation(offset.x as f32, offset.y as f32));
        dc.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
        dc.Clear(Some(&color(0, 0, 0, 0.0)));
        let full = D2D_RECT_F { left: 0.5, top: 0.5, right: BAR_W as f32 - 0.5, bottom: BAR_H as f32 - 0.5 };
        let rr = D2D1_ROUNDED_RECT { rect: full, radiusX: 11.0, radiusY: 11.0 };
        if variant == "b" {
            dc.FillRoundedRectangle(&rr, &dc.CreateSolidColorBrush(&color(18, 18, 20, 0.88), None)?);
        }
        dc.FillRoundedRectangle(&rr, &dc.CreateSolidColorBrush(&color(255, 255, 255, 0.065), None)?);
        dc.DrawRoundedRectangle(&rr, &dc.CreateSolidColorBrush(&color(255, 255, 255, 0.08), None)?, 1.0, None);

        println!("shapes ok");
        let dw: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let fmt = dw.CreateTextFormat(w!("Segoe UI Variable Text"), None, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, 13.0, w!("en-us"))?;
        fmt.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        let text: Vec<u16> = "Google Chrome     Visual Studio Code     File Explorer                    Wed Oct 21   2:32 PM".encode_utf16().collect();
        let layout_rect = D2D_RECT_F { left: 16.0, top: 0.0, right: BAR_W as f32 - 16.0, bottom: BAR_H as f32 };
        dc.DrawText(&text, &fmt, &layout_rect, &dc.CreateSolidColorBrush(&color(236, 236, 236, 1.0), None)?, D2D1_DRAW_TEXT_OPTIONS_NONE, DWRITE_MEASURING_MODE_NATURAL);
        surface.EndDraw()?;

        println!("text ok");
        visual.SetContent(&surface)?;
        target.SetRoot(&visual)?;
        dcomp.Commit()?;
        let _ = ShowWindow(bar, SW_SHOWNOACTIVATE);
        for (name, h) in [("backdrop", _backdrop), ("bar", bar)] {
            let mut cloaked = 0u32;
            let _ = DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut cloaked as *mut _ as _, 4);
            let mut rc = RECT::default();
            let _ = GetWindowRect(h, &mut rc);
            println!("{name}: visible={} cloaked={cloaked} rect={:?}", IsWindowVisible(h).as_bool(), (rc.left, rc.top, rc.right, rc.bottom));
        }
        let mut buf = [0u16; 128];
        let mut needed = 0u32;
        let desk = windows::Win32::System::StationsAndDesktops::GetThreadDesktop(windows::Win32::System::Threading::GetCurrentThreadId())?;
        let _ = windows::Win32::System::StationsAndDesktops::GetUserObjectInformationW(
            windows::Win32::Foundation::HANDLE(desk.0),
            windows::Win32::System::StationsAndDesktops::UOI_NAME,
            Some(buf.as_mut_ptr() as _),
            256,
            Some(&mut needed),
        );
        println!("desktop={}", String::from_utf16_lossy(&buf[..(needed as usize / 2).saturating_sub(1)]));

        // A thread timer (no hwnd); other WM_TIMERs belong to system windows and must not end the probe.
        let quit_timer = SetTimer(None, 0, secs * 1000, None);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_TIMER && msg.hwnd.is_invalid() && msg.wParam.0 == quit_timer {
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = PCWSTR::null();
    }
    Ok(())
}
