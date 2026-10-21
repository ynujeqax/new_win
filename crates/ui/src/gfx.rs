//! One set of GPU devices per process, recreated transparently after device loss.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_WARP;
use windows::Win32::Graphics::Direct3D11::{D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice};
use windows::Win32::Graphics::DirectComposition::{DCompositionCreateDevice2, IDCompositionDesktopDevice};
use windows::Win32::Graphics::DirectWrite::{DWRITE_FACTORY_TYPE_SHARED, DWriteCreateFactory, IDWriteFactory};
use windows::Win32::Graphics::Dxgi::{DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET, IDXGIDevice};
use windows::core::{HRESULT, Interface, Result};

#[derive(Clone)]
pub(crate) struct Devices {
    pub(crate) dcomp: IDCompositionDesktopDevice,
}

pub struct Gfx {
    d2d_factory: ID2D1Factory1,
    dwrite: IDWriteFactory,
    devices: RefCell<Option<Devices>>,
    generation: Cell<u32>,
}

/// Not exported by the windows crate.
const D2DERR_RECREATE_TARGET: HRESULT = HRESULT(0x8899_000C_u32 as i32);

pub(crate) fn is_device_lost(code: HRESULT) -> bool {
    code == D2DERR_RECREATE_TARGET || code == DXGI_ERROR_DEVICE_REMOVED || code == DXGI_ERROR_DEVICE_RESET
}

impl Gfx {
    pub fn new() -> Result<Rc<Gfx>> {
        let d2d_factory: ID2D1Factory1 = unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)? };
        let dwrite: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)? };
        let gfx = Gfx { d2d_factory, dwrite, devices: RefCell::new(None), generation: Cell::new(0) };
        gfx.devices()?;
        Ok(Rc::new(gfx))
    }

    fn create_devices(&self) -> Result<Devices> {
        unsafe {
            // WARP (software) instead of the GPU driver: our surfaces are tiny and redrawn only on
            // change, while the vendor driver would add ~50 threads, a 60 Hz wakeup and tens of MB.
            // Composition itself still runs on the GPU inside DWM.
            let mut d3d = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_WARP,
                Default::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut d3d),
                None,
                None,
            )?;
            let dxgi: IDXGIDevice = d3d.expect("D3D11CreateDevice returned no device").cast()?;
            let d2d = self.d2d_factory.CreateDevice(&dxgi)?;
            // Created from the D2D device so surfaces hand out ID2D1DeviceContext in BeginDraw.
            let dcomp: IDCompositionDesktopDevice = DCompositionCreateDevice2(&d2d)?;
            Ok(Devices { dcomp })
        }
    }

    /// Current devices, creating them after a loss.
    pub(crate) fn devices(&self) -> Result<Devices> {
        if let Some(d) = self.devices.borrow().as_ref() {
            return Ok(d.clone());
        }
        let d = self.create_devices()?;
        *self.devices.borrow_mut() = Some(d.clone());
        Ok(d)
    }

    pub(crate) fn device_lost(&self) {
        log::warn!("graphics device lost, recreating");
        *self.devices.borrow_mut() = None;
        self.generation.set(self.generation.get() + 1);
    }

    pub(crate) fn generation(&self) -> u32 {
        self.generation.get()
    }

    pub fn dwrite(&self) -> &IDWriteFactory {
        &self.dwrite
    }

    /// Hands every pending visual/animation change to DWM in one batch.
    pub fn commit(&self) -> Result<()> {
        let d = self.devices()?;
        let r = unsafe { d.dcomp.Commit() };
        if let Err(e) = &r
            && is_device_lost(e.code())
        {
            self.device_lost();
        }
        r
    }
}
