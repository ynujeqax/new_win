//! App icons on screen (1.0.1 spec §4): D2D bitmaps made from the shared `IconImage`s, kept
//! while the image lives and remade on another D2D device (after a device loss or release).

use std::rc::{Rc, Weak};

use newwin_core::app_icon::{ICON_LOGICAL_PX, IconImage};
use newwin_platform::dpi::scale;
use windows::Win32::Graphics::Direct2D::Common::{D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT};
use windows::Win32::Graphics::Direct2D::{D2D1_BITMAP_OPTIONS_NONE, D2D1_BITMAP_PROPERTIES1, ID2D1Bitmap1, ID2D1DeviceContext};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::core::Interface;

use crate::RectF;

struct Entry {
    image: Weak<IconImage>,
    /// The D2D device the bitmap belongs to.
    device: usize,
    bitmap: ID2D1Bitmap1,
}

#[derive(Default)]
pub struct IconBitmaps {
    entries: Vec<Entry>,
}

impl IconBitmaps {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The bitmap of `img` on `dc`'s device, made on first use.
    pub(crate) fn bitmap(&mut self, dc: &ID2D1DeviceContext, img: &Rc<IconImage>) -> Option<ID2D1Bitmap1> {
        let device = unsafe { dc.GetDevice() }.ok()?.as_raw() as usize;
        let ptr = Rc::as_ptr(img);
        if let Some(e) = self.entries.iter().find(|e| e.device == device && e.image.as_ptr() == ptr) {
            return Some(e.bitmap.clone());
        }
        self.entries.retain(|e| e.image.strong_count() > 0 && e.device == device);
        let props = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
            dpiX: 96.0,
            dpiY: 96.0,
            bitmapOptions: D2D1_BITMAP_OPTIONS_NONE,
            ..Default::default()
        };
        let size = D2D_SIZE_U { width: img.width, height: img.height };
        let bitmap = unsafe { dc.CreateBitmap(size, Some(img.bgra.as_ptr().cast()), img.width * 4, &props) }.ok()?;
        self.entries.push(Entry { image: Rc::downgrade(img), device, bitmap: bitmap.clone() });
        Some(bitmap)
    }
}

/// The icon's square: [`ICON_LOGICAL_PX`] at `dpi`, from `left`, centred on `mid_y`, whole pixels.
pub fn icon_rect(left: f32, mid_y: f32, dpi: u32) -> RectF {
    let side = scale(ICON_LOGICAL_PX, dpi).round();
    RectF::new(left.round(), (mid_y - side / 2.0).round(), side, side)
}
