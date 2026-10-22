//! The `newwin-glass` thread: reads the screen under glass windows with DXGI Desktop Duplication
//! and posts small blurred images to the UI thread (spec: liquid glass §3).
//!
//! The hardware D3D11 device exists only while at least one region is registered; with no
//! regions the thread blocks on its channel and never wakes.

use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

use newwin_core::glass::{ChangeFilter, Image, MIN_INTERVAL_MS, PxRect, any_hit, blur_strip, capture_area};
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
use windows::core::{Interface, Result};

/// A glass window that wants a backdrop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    /// The window's HWND as an integer; frames carry it back.
    pub id: u64,
    /// The window's rectangle in physical desktop pixels.
    pub window: PxRect,
}

/// A blurred image of `area` (desktop pixels), downsampled by `newwin_core::glass::SCALE`.
pub struct Frame {
    pub id: u64,
    pub area: PxRect,
    pub image: Image,
}

/// WPARAM of the posted message: LPARAM is a `Box<Frame>`, take it with [`take_frame`].
pub const FRAME: usize = 1;
/// WPARAM of the posted message: the screen cannot be captured; LPARAM is 0.
pub const UNAVAILABLE: usize = 2;

const RETRY_AFTER_LOST: Duration = Duration::from_millis(250);

enum Cmd {
    Regions(Vec<Region>),
    SimulateLost,
}

pub struct Backdrops {
    tx: Sender<Cmd>,
}

impl Backdrops {
    pub fn spawn(notify: HWND, msg: u32) -> Backdrops {
        let (tx, rx) = mpsc::channel();
        let notify = notify.0 as isize;
        std::thread::Builder::new()
            .name("newwin-glass".into())
            .spawn(move || Thread { rx, notify, msg, slots: Vec::new(), capture: None, unavailable: false }.run())
            .expect("cannot spawn the glass thread");
        Backdrops { tx }
    }

    /// Replaces the set of regions; an empty set releases the capture device.
    pub fn set_regions(&self, regions: Vec<Region>) {
        let _ = self.tx.send(Cmd::Regions(regions));
    }

    /// Test hook: drops the duplication exactly as `DXGI_ERROR_ACCESS_LOST` does.
    #[doc(hidden)]
    pub fn simulate_access_lost(&self) {
        let _ = self.tx.send(Cmd::SimulateLost);
    }
}

/// # Safety
/// `lp` must be the LPARAM of a [`FRAME`] message posted by [`Backdrops`], taken exactly once.
pub unsafe fn take_frame(lp: LPARAM) -> Box<Frame> {
    unsafe { Box::from_raw(lp.0 as *mut Frame) }
}

struct Slot {
    region: Region,
    /// Capture area in desktop pixels.
    area: PxRect,
    staging: Option<ID3D11Texture2D>,
    /// Needs a copy from the next frame regardless of dirty rects (new region or new duplication).
    fresh: bool,
    pending: bool,
    last: Option<Instant>,
    filter: ChangeFilter,
}

struct Capture {
    device: ID3D11Device,
    ctx: ID3D11DeviceContext,
    output: IDXGIOutput1,
    /// The output's desktop rectangle.
    bounds: PxRect,
    dup: Option<IDXGIOutputDuplication>,
    lost_at: Option<Instant>,
}

struct Thread {
    rx: Receiver<Cmd>,
    notify: isize,
    msg: u32,
    slots: Vec<Slot>,
    capture: Option<Capture>,
    /// Creating the device failed for the current region set; wait for the next one.
    unavailable: bool,
}

impl Thread {
    fn run(mut self) {
        loop {
            let idle = self.slots.is_empty() || self.unavailable;
            let first = if idle {
                match self.rx.recv() {
                    Ok(c) => Some(c),
                    Err(_) => return,
                }
            } else {
                None
            };
            if let Some(c) = first {
                self.handle(c);
            }
            loop {
                match self.rx.try_recv() {
                    Ok(c) => self.handle(c),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return,
                }
            }
            if self.slots.is_empty() || self.unavailable {
                continue;
            }
            if let Err(e) = self.step() {
                log::warn!("glass capture: {e}");
            }
        }
    }

    fn handle(&mut self, c: Cmd) {
        match c {
            Cmd::Regions(regions) => {
                self.unavailable = false;
                let mut old = std::mem::take(&mut self.slots);
                for region in regions {
                    // Keep a region's filter so re-sending the same set posts nothing new.
                    let slot = match old.iter().position(|s| s.region == region) {
                        Some(i) => old.swap_remove(i),
                        None => Slot {
                            region,
                            area: PxRect { left: 0, top: 0, right: 0, bottom: 0 },
                            staging: None,
                            fresh: true,
                            pending: false,
                            last: None,
                            filter: ChangeFilter::default(),
                        },
                    };
                    self.slots.push(slot);
                }
                if self.slots.is_empty() {
                    self.capture = None;
                } else if let Some(c) = &self.capture {
                    let bounds = c.bounds;
                    self.place(bounds);
                }
            }
            Cmd::SimulateLost => {
                if let Some(c) = self.capture.as_mut() {
                    c.dup = None;
                    c.lost_at = Some(Instant::now());
                }
            }
        }
    }

    /// Computes each slot's capture area on `bounds`; slots off that output get an empty area.
    fn place(&mut self, bounds: PxRect) {
        for s in &mut self.slots {
            let area = capture_area(s.region.window, bounds).unwrap_or(PxRect { left: 0, top: 0, right: 0, bottom: 0 });
            if area != s.area {
                s.area = area;
                s.staging = None;
                s.fresh = true;
            }
        }
    }

    fn post(&self, wp: usize, lp: isize) -> bool {
        unsafe { PostMessageW(Some(HWND(self.notify as *mut _)), self.msg, WPARAM(wp), LPARAM(lp)).is_ok() }
    }

    fn step(&mut self) -> Result<()> {
        if self.capture.is_none() {
            let center = self.slots[0].region.window;
            match Capture::new((center.left + center.right) / 2, (center.top + center.bottom) / 2) {
                Ok(c) => {
                    let bounds = c.bounds;
                    self.capture = Some(c);
                    self.place(bounds);
                }
                Err(e) => {
                    log::warn!("glass capture unavailable: {e}");
                    self.unavailable = true;
                    self.post(UNAVAILABLE, 0);
                    return Ok(());
                }
            }
        }
        let cap = self.capture.as_mut().expect("capture just created");
        if cap.dup.is_none() {
            if cap.lost_at.is_some_and(|t| t.elapsed() < RETRY_AFTER_LOST) {
                std::thread::sleep(RETRY_AFTER_LOST.saturating_sub(cap.lost_at.unwrap().elapsed()));
                return Ok(());
            }
            match unsafe { cap.output.DuplicateOutput(&cap.device) } {
                Ok(d) => {
                    cap.dup = Some(d);
                    cap.lost_at = None;
                    for s in &mut self.slots {
                        s.fresh = true;
                    }
                }
                Err(e) => {
                    cap.lost_at = Some(Instant::now());
                    return Err(e);
                }
            }
        }

        let timeout = self.next_wake_ms();
        let cap = self.capture.as_mut().expect("capture exists");
        let dup = cap.dup.clone().expect("duplication exists");
        let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut res = None;
        match unsafe { dup.AcquireNextFrame(timeout, &mut info, &mut res) } {
            Ok(()) => {
                let r = if info.LastPresentTime != 0 { copy_changed(cap, &dup, res.as_ref(), &mut self.slots) } else { Ok(()) };
                unsafe {
                    let _ = dup.ReleaseFrame();
                }
                if let Err(e) = r {
                    if e.code() == DXGI_ERROR_ACCESS_LOST {
                        cap.dup = None;
                        cap.lost_at = Some(Instant::now());
                    }
                    return Err(e);
                }
            }
            Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => {}
            Err(e) => {
                cap.dup = None;
                cap.lost_at = Some(Instant::now());
                if e.code() != DXGI_ERROR_ACCESS_LOST {
                    return Err(e);
                }
            }
        }
        self.recompute_due()
    }

    /// How long to wait for the next screen change: until the earliest rate-limited slot is due.
    fn next_wake_ms(&self) -> u32 {
        let min = Duration::from_millis(MIN_INTERVAL_MS);
        self.slots
            .iter()
            .filter(|s| s.pending)
            .map(|s| s.last.map_or(0, |t| min.saturating_sub(t.elapsed()).as_millis() as u32))
            .min()
            .unwrap_or(100)
    }

    /// Blurs the slots whose copy is waiting and whose interval has passed; posts changed images.
    fn recompute_due(&mut self) -> Result<()> {
        let min = Duration::from_millis(MIN_INTERVAL_MS);
        let Some(cap) = self.capture.as_ref() else { return Ok(()) };
        for i in 0..self.slots.len() {
            let s = &mut self.slots[i];
            if !s.pending || s.last.is_some_and(|t| t.elapsed() < min) {
                continue;
            }
            let Some(staging) = s.staging.clone() else { continue };
            s.pending = false;
            s.last = Some(Instant::now());
            let image = unsafe {
                let mut m = D3D11_MAPPED_SUBRESOURCE::default();
                cap.ctx.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut m))?;
                let (w, h) = (s.area.width() as usize, s.area.height() as usize);
                let bytes = std::slice::from_raw_parts(m.pData as *const u8, m.RowPitch as usize * (h - 1) + w * 4);
                let img = blur_strip(bytes, m.RowPitch as usize, w, h);
                cap.ctx.Unmap(&staging, 0);
                img
            };
            if !s.filter.changed(&image) {
                continue;
            }
            let frame = Box::into_raw(Box::new(Frame { id: s.region.id, area: s.area, image }));
            let (notify, msg) = (self.notify, self.msg);
            let posted = unsafe { PostMessageW(Some(HWND(notify as *mut _)), msg, WPARAM(FRAME), LPARAM(frame as isize)).is_ok() };
            if !posted {
                drop(unsafe { Box::from_raw(frame) });
            }
        }
        Ok(())
    }
}

/// Copies the area of every slot that is fresh or touched by this frame's dirty/move rects.
fn copy_changed(cap: &Capture, dup: &IDXGIOutputDuplication, res: Option<&IDXGIResource>, slots: &mut [Slot]) -> Result<()> {
    let ox = cap.bounds.left;
    let oy = cap.bounds.top;
    let changed = changed_rects(dup)?;
    let tex: ID3D11Texture2D = res.expect("frame without a resource").cast()?;
    for s in slots.iter_mut() {
        if s.area.width() <= 0 || s.area.height() <= 0 {
            continue;
        }
        let local = PxRect { left: s.area.left - ox, top: s.area.top - oy, right: s.area.right - ox, bottom: s.area.bottom - oy };
        if !s.fresh && !any_hit(&changed, &local) {
            continue;
        }
        if s.staging.is_none() {
            s.staging = Some(staging_texture(&cap.device, local.width() as u32, local.height() as u32)?);
        }
        let b = D3D11_BOX { left: local.left as u32, top: local.top as u32, front: 0, right: local.right as u32, bottom: local.bottom as u32, back: 1 };
        unsafe { cap.ctx.CopySubresourceRegion(s.staging.as_ref().unwrap(), 0, 0, 0, 0, &tex, 0, Some(&b)) };
        s.fresh = false;
        s.pending = true;
    }
    Ok(())
}

/// Dirty and move-destination rectangles of the acquired frame, in output coordinates.
fn changed_rects(dup: &IDXGIOutputDuplication) -> Result<Vec<PxRect>> {
    let to_px = |r: &RECT| PxRect { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
    let mut out = Vec::new();
    unsafe {
        let mut need = 0u32;
        let mut dirty = vec![RECT::default(); 32];
        loop {
            match dup.GetFrameDirtyRects((dirty.len() * size_of::<RECT>()) as u32, dirty.as_mut_ptr(), &mut need) {
                Ok(()) => break,
                Err(e) if e.code() == DXGI_ERROR_MORE_DATA => dirty.resize(need as usize / size_of::<RECT>() + 1, RECT::default()),
                Err(e) => return Err(e),
            }
        }
        out.extend(dirty[..need as usize / size_of::<RECT>()].iter().map(to_px));
        let mut moves = vec![DXGI_OUTDUPL_MOVE_RECT::default(); 16];
        loop {
            match dup.GetFrameMoveRects((moves.len() * size_of::<DXGI_OUTDUPL_MOVE_RECT>()) as u32, moves.as_mut_ptr(), &mut need) {
                Ok(()) => break,
                Err(e) if e.code() == DXGI_ERROR_MORE_DATA => {
                    moves.resize(need as usize / size_of::<DXGI_OUTDUPL_MOVE_RECT>() + 1, DXGI_OUTDUPL_MOVE_RECT::default())
                }
                Err(e) => return Err(e),
            }
        }
        out.extend(moves[..need as usize / size_of::<DXGI_OUTDUPL_MOVE_RECT>()].iter().map(|m| to_px(&m.DestinationRect)));
    }
    Ok(out)
}

fn staging_texture(device: &ID3D11Device, width: u32, height: u32) -> Result<ID3D11Texture2D> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Usage: D3D11_USAGE_STAGING,
        BindFlags: 0,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: 0,
    };
    let mut t = None;
    unsafe { device.CreateTexture2D(&desc, None, Some(&mut t))? };
    Ok(t.expect("CreateTexture2D returned no texture"))
}

impl Capture {
    /// Device and output for the monitor containing desktop point (`x`, `y`) (else the first one).
    fn new(x: i32, y: i32) -> Result<Capture> {
        unsafe {
            let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
            let mut found: Option<(IDXGIAdapter1, IDXGIOutput, PxRect)> = None;
            let mut first: Option<(IDXGIAdapter1, IDXGIOutput, PxRect)> = None;
            let mut a = 0;
            while let Ok(adapter) = factory.EnumAdapters1(a) {
                let mut o = 0;
                while let Ok(output) = adapter.EnumOutputs(o) {
                    let d = output.GetDesc()?;
                    let c = d.DesktopCoordinates;
                    let bounds = PxRect { left: c.left, top: c.top, right: c.right, bottom: c.bottom };
                    let inside = x >= c.left && x < c.right && y >= c.top && y < c.bottom;
                    if inside && found.is_none() {
                        found = Some((adapter.clone(), output.clone(), bounds));
                    }
                    if first.is_none() {
                        first = Some((adapter.clone(), output, bounds));
                    }
                    o += 1;
                }
                a += 1;
            }
            let (adapter, output, bounds) = found.or(first).ok_or_else(|| windows::core::Error::from(DXGI_ERROR_NOT_FOUND))?;
            let (mut device, mut ctx) = (None, None);
            D3D11CreateDevice(&adapter, D3D_DRIVER_TYPE_UNKNOWN, Default::default(), D3D11_CREATE_DEVICE_FLAG(0), None, D3D11_SDK_VERSION, Some(&mut device), None, Some(&mut ctx))?;
            let device = device.expect("D3D11CreateDevice returned no device");
            let output: IDXGIOutput1 = output.cast()?;
            let dup = output.DuplicateOutput(&device)?;
            Ok(Capture { device, ctx: ctx.expect("no immediate context"), output, bounds, dup: Some(dup), lost_at: None })
        }
    }
}

