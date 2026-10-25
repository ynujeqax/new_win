//! Icon pixels (1.0.1 spec §3): as Explorer shows an app (worker thread, needs COM), and from the
//! `hIcon` an app gave the tray (UI thread).

use newwin_core::app_icon::{IconImage, opaque_if_no_alpha, premultiply};
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDC, GetDIBits, GetObjectW, HBITMAP, HDC, ReleaseDC, SelectObject,
};
use windows::Win32::UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_ICONONLY};
use windows::Win32::UI::WindowsAndMessaging::{DI_MASK, DI_NORMAL, DrawIconEx, HICON};
use windows::core::{HRESULT, HSTRING};

/// How often and how long `shell_icon` waits for an icon the shell is still extracting.
const PENDING_TRIES: u32 = 10;
const PENDING_WAIT_MS: u64 = 30;
/// `E_PENDING`, not exported by the `windows` crate.
const E_PENDING: HRESULT = HRESULT(0x8000_000Au32 as i32);

fn info(w: u32, h: u32) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            // Negative: rows top to bottom.
            biHeight: -(h as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// The icon Explorer shows for `source` (a file path or `shell:AppsFolder\…`), `px` square.
pub fn shell_icon(source: &str, px: u32) -> Option<IconImage> {
    let factory: IShellItemImageFactory = unsafe { SHCreateItemFromParsingName(&HSTRING::from(source), None) }
        .map_err(|e| log::debug!("no shell item for {source}: {e}"))
        .ok()?;
    let size = SIZE { cx: px as i32, cy: px as i32 };
    // E_PENDING: the shell is still extracting this icon for someone else (Explorer drawing the
    // same app, a second request). This runs on the worker thread, so a short wait is fine.
    let mut tries = 0;
    let hbm = loop {
        match unsafe { factory.GetImage(size, SIIGBF_ICONONLY) } {
            Ok(h) => break h,
            Err(e) if e.code() == E_PENDING && tries < PENDING_TRIES => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(PENDING_WAIT_MS));
            }
            Err(e) => {
                log::debug!("no icon for {source}: {e}");
                return None;
            }
        }
    };
    let image = bitmap_pixels(hbm);
    unsafe {
        let _ = DeleteObject(hbm.into());
    }
    let mut image = image?;
    opaque_if_no_alpha(&mut image.bgra);
    // The factory hands out straight alpha on some builds and premultiplied on others: a colour
    // above its alpha can only be straight.
    if image.bgra.as_chunks::<4>().0.iter().any(|p| p[0] > p[3] || p[1] > p[3] || p[2] > p[3]) {
        premultiply(&mut image.bgra);
    }
    Some(image)
}

fn bitmap_pixels(hbm: HBITMAP) -> Option<IconImage> {
    let mut bm = BITMAP::default();
    if unsafe { GetObjectW(hbm.into(), size_of::<BITMAP>() as i32, Some((&raw mut bm).cast())) } == 0 {
        return None;
    }
    let (w, h) = (bm.bmWidth.unsigned_abs(), bm.bmHeight.unsigned_abs());
    if w == 0 || h == 0 {
        return None;
    }
    let mut bgra = vec![0u8; (w * h * 4) as usize];
    let mut bi = info(w, h);
    let screen = unsafe { GetDC(None) };
    let rows = unsafe { GetDIBits(screen, hbm, 0, h, Some(bgra.as_mut_ptr().cast()), &mut bi, DIB_RGB_COLORS) };
    unsafe { ReleaseDC(None, screen) };
    (rows == h as i32).then_some(IconImage { width: w, height: h, bgra })
}

/// A 32-bit DIB section `px` square, cleared, selected into its own memory DC.
struct Canvas {
    dc: HDC,
    bitmap: HBITMAP,
    bits: *mut u8,
    px: u32,
}

impl Canvas {
    fn new(px: u32) -> Option<Canvas> {
        unsafe {
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                return None;
            }
            let mut bits = std::ptr::null_mut();
            let Ok(bitmap) = CreateDIBSection(Some(dc), &info(px, px), DIB_RGB_COLORS, &mut bits, None, 0) else {
                let _ = DeleteDC(dc);
                return None;
            };
            SelectObject(dc, bitmap.into());
            std::ptr::write_bytes(bits.cast::<u8>(), 0, (px * px * 4) as usize);
            Some(Canvas { dc, bitmap, bits: bits.cast(), px })
        }
    }

    fn draw(&self, icon: HICON, flags: windows::Win32::UI::WindowsAndMessaging::DI_FLAGS) -> bool {
        let p = self.px as i32;
        unsafe { DrawIconEx(self.dc, 0, 0, icon, p, p, 0, None, flags) }.is_ok()
    }

    fn pixels(&self) -> Vec<u8> {
        unsafe { std::slice::from_raw_parts(self.bits, (self.px * self.px * 4) as usize) }.to_vec()
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteDC(self.dc);
            let _ = DeleteObject(self.bitmap.into());
        }
    }
}

/// The tray icon `hicon` (a 32-bit handle value), `px` square; `None` for a handle that is gone.
pub fn hicon_image(hicon: u32, px: u32) -> Option<IconImage> {
    if hicon == 0 || px == 0 {
        return None;
    }
    // Handles are 32 bits wide, sign-extended to pointer size.
    let icon = HICON(hicon as i32 as isize as *mut _);
    let canvas = Canvas::new(px)?;
    if !canvas.draw(icon, DI_NORMAL) {
        log::debug!("tray icon {hicon:#x} could not be drawn");
        return None;
    }
    let mut bgra = canvas.pixels();
    if bgra.as_chunks::<4>().0.iter().all(|p| p[3] == 0) {
        // An old icon with a mask and no alpha: opaque where the mask is black.
        let mask = Canvas::new(px)?;
        if !mask.draw(icon, DI_MASK) {
            return None;
        }
        for (p, m) in bgra.as_chunks_mut::<4>().0.iter_mut().zip(mask.pixels().as_chunks::<4>().0.iter()) {
            let drawn = m[0] == 0 && m[1] == 0 && m[2] == 0;
            if drawn {
                p[3] = 255;
            } else {
                p.copy_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    Some(IconImage { width: px, height: px, bgra })
}
