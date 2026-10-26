//! Slide/fade animations executed by DWM (no frames rendered by us).

use windows::Win32::Graphics::DirectComposition::{IDCompositionAnimation, IDCompositionVisual2, IDCompositionVisual3};
use windows::core::{Interface, Result};

use crate::Gfx;

/// Fraction of the way an ease-out animation has travelled at time fraction `t` (0..=1).
/// Same curve as the DWM animations below, so callers can tell where a running animation is.
pub fn ease_out_progress(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

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

/// Ease-out "back": overshoots `to` slightly before settling, like a spring.
/// v(u) = from + Δ·((c+3)u − (2c+3)u² + (c+1)u³), u = t/T; peaks about 2.5 % past `to` for c = 0.8.
fn ease_out_back(gfx: &Gfx, from: f32, to: f32, ms: u32) -> Result<IDCompositionAnimation> {
    const C: f64 = 0.8;
    let anim = unsafe { gfx.devices()?.dcomp.CreateAnimation()? };
    let t = (ms.max(1) as f64) / 1000.0;
    let d = (to - from) as f64;
    unsafe {
        anim.AddCubic(0.0, from, (d * (C + 3.0) / t) as f32, (-d * (2.0 * C + 3.0) / (t * t)) as f32, (d * (C + 1.0) / (t * t * t)) as f32)?;
        anim.End(t, to)?;
    }
    Ok(anim)
}

/// Scales `v` about `center` from `from` to `to`; `spring` overshoots a little on the way
/// (opening), otherwise it eases out (closing).
pub fn scale(gfx: &Gfx, v: &IDCompositionVisual2, from: f32, to: f32, ms: u32, center: (f32, f32), spring: bool) -> Result<()> {
    let t = unsafe { gfx.devices()?.dcomp.CreateScaleTransform()? };
    let curve = |a: f32, b: f32| if spring { ease_out_back(gfx, a, b, ms) } else { ease_out(gfx, a, b, ms) };
    unsafe {
        t.SetCenterX2(center.0)?;
        t.SetCenterY2(center.1)?;
        t.SetScaleX(&curve(from, to)?)?;
        t.SetScaleY(&curve(from, to)?)?;
        v.SetTransform(&t)?;
    }
    gfx.commit()
}

pub fn slide_y(gfx: &Gfx, v: &IDCompositionVisual2, from: f32, to: f32, ms: u32) -> Result<()> {
    let anim = ease_out(gfx, from, to, ms)?;
    unsafe { v.SetOffsetY(&anim)? };
    gfx.commit()
}

/// Slides `outer` from `from` to `to` and `inner` the opposite way on the same curve, so
/// `inner`'s content stays put on screen while `outer` moves (the glass backdrop). One commit.
pub fn slide_y_both(gfx: &Gfx, outer: &IDCompositionVisual2, inner: Option<&IDCompositionVisual2>, from: f32, to: f32, ms: u32) -> Result<()> {
    unsafe { outer.SetOffsetY(&ease_out(gfx, from, to, ms)?)? };
    if let Some(inner) = inner {
        unsafe { inner.SetOffsetY(&ease_out(gfx, -from, -to, ms)?)? };
    }
    gfx.commit()
}

pub fn fade(gfx: &Gfx, v: &IDCompositionVisual2, from: f32, to: f32, ms: u32) -> Result<()> {
    let anim = ease_out(gfx, from, to, ms)?;
    let v3: IDCompositionVisual3 = v.cast()?;
    unsafe { v3.SetOpacity(&anim)? };
    gfx.commit()
}

/// Jumps to a resting position/opacity without animating; `inner` (the glass backdrop) gets
/// the opposite offset so it stays put on screen.
pub fn set_offset_opacity(gfx: &Gfx, v: &IDCompositionVisual2, inner: Option<&IDCompositionVisual2>, y: f32, opacity: f32) -> Result<()> {
    let v3: IDCompositionVisual3 = v.cast()?;
    unsafe {
        v.SetOffsetY2(y)?;
        v3.SetOpacity2(opacity)?;
        if let Some(inner) = inner {
            inner.SetOffsetY2(-y)?;
        }
    }
    gfx.commit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ease_out_curve() {
        assert_eq!(ease_out_progress(0.0), 0.0);
        assert_eq!(ease_out_progress(1.0), 1.0);
        assert_eq!(ease_out_progress(0.5), 0.875);
        assert_eq!(ease_out_progress(-1.0), 0.0);
        assert_eq!(ease_out_progress(3.0), 1.0);
    }
}
