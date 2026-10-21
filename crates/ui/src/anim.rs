//! Slide/fade animations executed by DWM (no frames rendered by us).

use windows::Win32::Graphics::DirectComposition::{IDCompositionAnimation, IDCompositionVisual2, IDCompositionVisual3};
use windows::core::{Interface, Result};

use crate::Gfx;

/// Ease-out cubic from `from` to `to` over `ms`: v(t) = from + Δ·(1 − (1 − t/T)³).
fn ease_out(gfx: &Gfx, from: f32, to: f32, ms: u32) -> Result<IDCompositionAnimation> {
    let anim = unsafe { gfx.devices()?.dcomp.CreateAnimation()? };
    let t = (ms.max(1) as f64) / 1000.0;
    let d = (to - from) as f64;
    unsafe {
        anim.AddCubic(0.0, from, (3.0 * d / t) as f32, (-3.0 * d / (t * t)) as f32, (d / (t * t * t)) as f32)?;
        anim.End(t, to)?;
    }
    Ok(anim)
}

pub fn slide_y(gfx: &Gfx, v: &IDCompositionVisual2, from: f32, to: f32, ms: u32) -> Result<()> {
    let anim = ease_out(gfx, from, to, ms)?;
    unsafe { v.SetOffsetY(&anim)? };
    gfx.commit()
}

pub fn fade(gfx: &Gfx, v: &IDCompositionVisual2, from: f32, to: f32, ms: u32) -> Result<()> {
    let anim = ease_out(gfx, from, to, ms)?;
    let v3: IDCompositionVisual3 = v.cast()?;
    unsafe { v3.SetOpacity(&anim)? };
    gfx.commit()
}

/// Jumps to a resting position/opacity without animating.
pub fn set_offset_opacity(gfx: &Gfx, v: &IDCompositionVisual2, y: f32, opacity: f32) -> Result<()> {
    let v3: IDCompositionVisual3 = v.cast()?;
    unsafe {
        v.SetOffsetY2(y)?;
        v3.SetOpacity2(opacity)?;
    }
    gfx.commit()
}
