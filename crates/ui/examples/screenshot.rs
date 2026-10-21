//! Dev tool: captures the primary output through DXGI Desktop Duplication (correct with HDR/DWM,
//! unlike GDI BitBlt) and writes a 24-bit BMP.
//! Usage: `screenshot <out.bmp> [x y w h]`

use std::io::Write;

use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::core::{Interface, Result};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let out = args.get(1).expect("usage: screenshot <out.bmp> [x y w h]");
    let nums: Vec<u32> = args.iter().skip(2).filter_map(|s| s.parse().ok()).collect();

    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let adapter = factory.EnumAdapters1(0)?;
        let output: IDXGIOutput1 = adapter.EnumOutputs(0)?.cast()?;
        let mut device = None;
        let mut context = None;
        D3D11CreateDevice(&adapter, D3D_DRIVER_TYPE_UNKNOWN, Default::default(), D3D11_CREATE_DEVICE_FLAG(0), None, D3D11_SDK_VERSION, Some(&mut device), None, Some(&mut context))?;
        let (device, context) = (device.unwrap(), context.unwrap());
        let dup = output.DuplicateOutput(&device)?;

        // The first frames can be empty until the compositor presents; retry briefly.
        let mut texture: Option<ID3D11Texture2D> = None;
        for _ in 0..20 {
            let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut resource = None;
            if dup.AcquireNextFrame(200, &mut info, &mut resource).is_ok() {
                if info.LastPresentTime != 0 {
                    texture = Some(resource.unwrap().cast()?);
                    break;
                }
                dup.ReleaseFrame()?;
            }
        }
        let texture = texture.expect("no frame captured");
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        texture.GetDesc(&mut desc);
        let staging_desc = D3D11_TEXTURE2D_DESC {
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
            ..desc
        };
        let mut staging = None;
        device.CreateTexture2D(&staging_desc, None, Some(&mut staging))?;
        let staging = staging.unwrap();
        context.CopyResource(&staging, &texture);
        dup.ReleaseFrame()?;

        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
        let (sx, sy, sw, sh) = match nums.as_slice() {
            [x, y, w, h] => (*x, *y, (*w).min(desc.Width - x), (*h).min(desc.Height - y)),
            _ => (0, 0, desc.Width, desc.Height),
        };
        let row_bytes = (sw * 3).div_ceil(4) * 4;
        let mut bmp = Vec::with_capacity(54 + (row_bytes * sh) as usize);
        let file_size = 54 + row_bytes * sh;
        bmp.extend_from_slice(b"BM");
        bmp.extend_from_slice(&file_size.to_le_bytes());
        bmp.extend_from_slice(&[0; 4]);
        bmp.extend_from_slice(&54u32.to_le_bytes());
        bmp.extend_from_slice(&40u32.to_le_bytes());
        bmp.extend_from_slice(&(sw as i32).to_le_bytes());
        bmp.extend_from_slice(&(sh as i32).to_le_bytes());
        bmp.extend_from_slice(&1u16.to_le_bytes());
        bmp.extend_from_slice(&24u16.to_le_bytes());
        bmp.extend_from_slice(&[0; 24]);
        let base = mapped.pData as *const u8;
        // BMP rows are bottom-up; the desktop texture is B8G8R8A8.
        for row in (0..sh).rev() {
            let src = base.add(((sy + row) * mapped.RowPitch) as usize + (sx * 4) as usize);
            let px = std::slice::from_raw_parts(src, (sw * 4) as usize);
            for p in px.chunks_exact(4) {
                bmp.extend_from_slice(&p[..3]);
            }
            bmp.extend(std::iter::repeat_n(0, (row_bytes - sw * 3) as usize));
        }
        context.Unmap(&staging, 0);
        std::fs::File::create(out).and_then(|mut f| f.write_all(&bmp)).expect("write failed");
        println!("format={:?} {}x{} captured {}x{}", desc.Format, desc.Width, desc.Height, sw, sh);
    }
    Ok(())
}
