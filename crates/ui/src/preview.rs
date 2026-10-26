//! Off-screen rendering into a .bmp file, for looking at icons and controls pixel by pixel. The
//! glass windows hide themselves from screen capture, so a screenshot cannot show them.

use std::io::Write;

use windows::Win32::Graphics::Direct2D::Common::{D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT, D2D_SIZE_U};
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_WARP;
use windows::Win32::Graphics::Direct3D11::{D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::core::{Error, Interface, Result};

use crate::{Color, Painter, RectF};

/// Draws a `width` × `height` picture over `background` and saves it as a 24-bit .bmp.
pub fn render_bmp(path: &str, width: u32, height: u32, background: Color, draw: impl FnOnce(&mut Painter)) -> Result<()> {
    let px = render_pixels(width, height, background, draw)?;
    let row = (width * 3).div_ceil(4) * 4;
    let mut out = Vec::with_capacity(54 + (row * height) as usize);
    out.extend_from_slice(b"BM");
    for v in [54 + row * height, 0, 54, 40, width, height] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&[0; 24]);
    for line in px.chunks(width as usize).rev() {
        for [r, g, b] in line {
            out.extend_from_slice(&[*b, *g, *r]);
        }
        out.resize(out.len() + (row - width * 3) as usize, 0);
    }
    std::fs::File::create(path).and_then(|mut f| f.write_all(&out)).map_err(|e| Error::new(windows::core::HRESULT(-1), e.to_string()))
}

/// Draws a `width` × `height` picture over `background`: its pixels, RGB, rows top down.
pub fn render_pixels(width: u32, height: u32, background: Color, draw: impl FnOnce(&mut Painter)) -> Result<Vec<[u8; 3]>> {
    unsafe {
        let factory: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let mut d3d = None;
        D3D11CreateDevice(None, D3D_DRIVER_TYPE_WARP, Default::default(), D3D11_CREATE_DEVICE_BGRA_SUPPORT, None, D3D11_SDK_VERSION, Some(&mut d3d), None, None)?;
        let dxgi: IDXGIDevice = d3d.ok_or_else(Error::empty)?.cast()?;
        let dc = factory.CreateDevice(&dxgi)?.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
        let format = D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED };
        let props = |options| D2D1_BITMAP_PROPERTIES1 { pixelFormat: format, dpiX: 96.0, dpiY: 96.0, bitmapOptions: options, ..Default::default() };
        let size = D2D_SIZE_U { width, height };
        let target = dc.CreateBitmap(size, None, 0, &props(D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW))?;
        dc.SetTarget(&target);
        dc.BeginDraw();
        dc.Clear(Some(&background.d2d()));
        let mut p = Painter::new(&dc, RectF::new(0.0, 0.0, width as f32, height as f32));
        draw(&mut p);
        drop(p);
        dc.EndDraw(None, None)?;
        let cpu = dc.CreateBitmap(size, None, 0, &props(D2D1_BITMAP_OPTIONS_CPU_READ | D2D1_BITMAP_OPTIONS_CANNOT_DRAW))?;
        cpu.CopyFromBitmap(None, &target, None)?;
        let map = cpu.Map(D2D1_MAP_OPTIONS_READ)?;
        let mut out = Vec::with_capacity((width * height) as usize);
        for y in 0..height {
            let src = std::slice::from_raw_parts(map.bits.add((y * map.pitch) as usize), (width * 4) as usize);
            out.extend(src.chunks(4).map(|px| [px[2], px[1], px[0]]));
        }
        cpu.Unmap()?;
        Ok(out)
    }
}
