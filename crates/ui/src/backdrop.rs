//! The `newwin-glass` thread: reads the screen under glass windows with DXGI Desktop Duplication
//! and posts small blurred images to the UI thread (spec: liquid glass §3).
//!
//! The hardware D3D11 device exists only while at least one region is registered; with no
//! regions the thread blocks on its channel and never wakes.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::time::{Duration, Instant};

use newwin_core::glass::{ChangeFilter, Image, MIN_INTERVAL_MS, PxRect, any_hit, capture_area, fits, glass_image, retry_delay_ms};
use windows::Win32::Foundation::{E_BOUNDS, HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
use windows::core::{Error, Interface, Result};

/// A glass window that wants a backdrop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Region {
    /// The window's HWND as an integer; frames carry it back.
    pub id: u64,
    /// The window's rectangle in physical desktop pixels.
    pub window: PxRect,
    /// Corner radius of the glass shape, physical px (for the lens along its edge).
    pub radius: f32,
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

enum Cmd {
    Regions(Vec<Region>),
    Refresh,
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
            .spawn(move || {
                Thread { rx, notify, msg, slots: Vec::new(), capture: None, failures: 0, retry_at: None, unavailable_posted: false }.run()
            })
            .expect("cannot spawn the glass thread");
        Backdrops { tx }
    }

    /// Replaces the set of regions; an empty set releases the capture device.
    pub fn set_regions(&self, regions: Vec<Region>) {
        let _ = self.tx.send(Cmd::Regions(regions));
    }

    /// The display changed (resolution, monitors, DPI): rebuild the capture with fresh bounds.
    pub fn refresh(&self) {
        let _ = self.tx.send(Cmd::Refresh);
    }

    /// Test hook: behaves exactly as a `DXGI_ERROR_ACCESS_LOST` from the duplication.
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
    /// The output's desktop rectangle, read when the capture was built.
    bounds: PxRect,
    dup: IDXGIOutputDuplication,
}

struct Thread {
    rx: Receiver<Cmd>,
    notify: isize,
    msg: u32,
    slots: Vec<Slot>,
    capture: Option<Capture>,
    /// Failed (re)builds or lost captures in a row; reset by a healthy step.
    failures: u32,
    /// When the next rebuild may be tried.
    retry_at: Option<Instant>,
    /// UNAVAILABLE was posted for the current failure streak.
    unavailable_posted: bool,
}

impl Thread {
    fn run(mut self) {
        loop {
            let first = if self.slots.is_empty() {
                match self.rx.recv() {
                    Ok(c) => Some(c),
                    Err(_) => return,
                }
            } else if let Some(wait) = self.retry_wait() {
                match self.rx.recv_timeout(wait) {
                    Ok(c) => Some(c),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => return,
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
            if self.slots.is_empty() || self.retry_wait().is_some() {
                continue;
            }
            if let Err(e) = self.step() {
                self.lose(e);
            }
        }
    }

    /// Time left before a rebuild may be tried, if a rebuild is pending.
    fn retry_wait(&self) -> Option<Duration> {
        let t = self.retry_at?;
        let left = t.saturating_duration_since(Instant::now());
        (self.capture.is_none() && !left.is_zero()).then_some(left)
    }

    /// Any failure drops the whole capture (device, duplication and the output bounds read with
    /// them) and rebuilds it after a growing pause: the display may have changed, the driver may
    /// have been reset, or a secure desktop may be up. Only the first failure of a streak is logged.
    fn lose(&mut self, e: Error) {
        self.capture = None;
        self.failures = self.failures.saturating_add(1);
        self.retry_at = Some(Instant::now() + Duration::from_millis(retry_delay_ms(self.failures)));
        if self.failures == 1 {
            log::warn!("glass capture lost, rebuilding: {e}");
        }
    }

    fn handle(&mut self, c: Cmd) {
        match c {
            Cmd::Regions(regions) => {
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
                    self.retry_at = None;
                } else if let Some(c) = &self.capture {
                    let bounds = c.bounds;
                    self.place(bounds);
                } else {
                    // A new set (the bar is about to show) retries a failed capture right away.
                    self.retry_at = None;
                }
            }
            Cmd::Refresh => {
                self.capture = None;
                self.retry_at = None;
                self.failures = 0;
            }
            Cmd::SimulateLost => {
                if self.capture.is_some() {
                    self.lose(Error::from(DXGI_ERROR_ACCESS_LOST));
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
            let c = match Capture::new((center.left + center.right) / 2, (center.top + center.bottom) / 2) {
                Ok(c) => c,
                Err(e) => {
                    if !self.unavailable_posted {
                        self.unavailable_posted = true;
                        self.post(UNAVAILABLE, 0);
                    }
                    return Err(e);
                }
            };
            let bounds = c.bounds;
            self.capture = Some(c);
            // Staging textures belong to the old device.
            for s in &mut self.slots {
                s.fresh = true;
                s.staging = None;
            }
            self.place(bounds);
        }

        let timeout = self.next_wake_ms();
        let cap = self.capture.as_ref().expect("capture exists");
        let dup = cap.dup.clone();
        let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut res = None;
        match unsafe { dup.AcquireNextFrame(timeout, &mut info, &mut res) } {
            Ok(()) => {
                let r = if info.LastPresentTime != 0 { copy_changed(cap, &dup, res.as_ref(), &mut self.slots) } else { Ok(()) };
                unsafe {
                    let _ = dup.ReleaseFrame();
                }
                r?;
            }
            Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => {}
            Err(e) => return Err(e),
        }
        self.recompute_due()?;
        self.failures = 0;
        self.unavailable_posted = false;
        Ok(())
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
                let img = glass_image(bytes, m.RowPitch as usize, s.area, s.region.window, s.region.radius);
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
    let tex: ID3D11Texture2D = res.ok_or_else(|| Error::from(DXGI_ERROR_INVALID_CALL))?.cast()?;
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    unsafe { tex.GetDesc(&mut desc) };
    for s in slots.iter_mut() {
        if s.area.width() <= 0 || s.area.height() <= 0 {
            continue;
        }
        let local = PxRect { left: s.area.left - ox, top: s.area.top - oy, right: s.area.right - ox, bottom: s.area.bottom - oy };
        if !s.fresh && !any_hit(&changed, &local) {
            continue;
        }
        // Bounds read before a resolution change: rebuild instead of copying out of range.
        if !fits(local, desc.Width, desc.Height) {
            return Err(Error::new(E_BOUNDS, "the screen is smaller than the capture area"));
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
            let ctx = ctx.ok_or_else(|| Error::from(DXGI_ERROR_INVALID_CALL))?;
            Ok(Capture { device, ctx, bounds, dup })
        }
    }
}

