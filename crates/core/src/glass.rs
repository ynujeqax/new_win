//! Liquid glass image math: which screen area to capture under a window, and how to turn it
//! into the small blurred image the compositor scales back up (spec: liquid glass §3).

/// Extra screen captured around a window so the blur near its edges has real neighbors.
pub const PAD: i32 = 24;
/// Downsample factor; the compositor scales the image back up by the same factor.
pub const SCALE: usize = 4;
pub const BLUR_RADIUS: usize = 2;
/// Three box blurs approximate a gaussian.
pub const BLUR_PASSES: usize = 3;
/// At most one recompute per this many ms for each window.
pub const MIN_INTERVAL_MS: u64 = 33;

/// A rectangle in physical desktop pixels, right/bottom exclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PxRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl PxRect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }

    pub fn intersects(&self, o: &PxRect) -> bool {
        self.left < o.right && o.left < self.right && self.top < o.bottom && o.top < self.bottom
    }
}

/// The window grown by [`PAD`] and clipped to the output; `None` if nothing of it is on the output.
pub fn capture_area(window: PxRect, output: PxRect) -> Option<PxRect> {
    let r = PxRect {
        left: (window.left - PAD).max(output.left),
        top: (window.top - PAD).max(output.top),
        right: (window.right + PAD).min(output.right),
        bottom: (window.bottom + PAD).min(output.bottom),
    };
    (r.width() > 0 && r.height() > 0).then_some(r)
}

/// Opaque BGRA pixels, rows packed (`width * 4` bytes each).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub bgra: Vec<u8>,
}

/// BGRA screen pixels (`pitch` bytes per row) → downsampled by [`SCALE`] (the remainder rows and
/// columns are dropped) → [`BLUR_PASSES`] box blurs of [`BLUR_RADIUS`].
pub fn blur_strip(src: &[u8], pitch: usize, width: usize, height: usize) -> Image {
    let (w, h) = (width / SCALE, height / SCALE);
    let k = (SCALE * SCALE) as f32;
    let mut px = vec![[0f32; 3]; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut s = [0f32; 3];
            for yy in 0..SCALE {
                let row = &src[(y * SCALE + yy) * pitch..];
                for xx in 0..SCALE {
                    let p = &row[(x * SCALE + xx) * 4..];
                    for c in 0..3 {
                        s[c] += f32::from(p[c]);
                    }
                }
            }
            px[y * w + x] = s.map(|v| v / k);
        }
    }
    for _ in 0..BLUR_PASSES {
        box_pass(&mut px, w, h, BLUR_RADIUS, true);
        box_pass(&mut px, w, h, BLUR_RADIUS, false);
    }
    let bgra = px.iter().flat_map(|p| [p[0].round() as u8, p[1].round() as u8, p[2].round() as u8, 255]).collect();
    Image { width: w, height: h, bgra }
}

/// One horizontal or vertical box pass; the window is clamped to the image so edges keep their
/// brightness instead of averaging in black.
fn box_pass(px: &mut [[f32; 3]], w: usize, h: usize, r: usize, horizontal: bool) {
    let (lines, len) = if horizontal { (h, w) } else { (w, h) };
    let at = |line: usize, i: usize| if horizontal { line * w + i } else { i * w + line };
    let mut buf = vec![[0f32; 3]; len];
    for line in 0..lines {
        for (i, out) in buf.iter_mut().enumerate() {
            let (lo, hi) = (i.saturating_sub(r), (i + r).min(len - 1));
            let mut s = [0f32; 3];
            for j in lo..=hi {
                let p = px[at(line, j)];
                for c in 0..3 {
                    s[c] += p[c];
                }
            }
            let n = (hi - lo + 1) as f32;
            *out = s.map(|v| v / n);
        }
        for (i, v) in buf.iter().enumerate() {
            px[at(line, i)] = *v;
        }
    }
}

/// Whether any changed screen rectangle touches `area`.
pub fn any_hit(changed: &[PxRect], area: &PxRect) -> bool {
    changed.iter().any(|r| r.intersects(area))
}

/// Lets an image through only when it differs from the last one let through. Our own redraw
/// marks the area under the window as changed, so without this a window would wake itself up.
#[derive(Default)]
pub struct ChangeFilter {
    last: Option<Image>,
}

impl ChangeFilter {
    pub fn changed(&mut self, img: &Image) -> bool {
        if self.last.as_ref() == Some(img) {
            return false;
        }
        self.last = Some(img.clone());
        true
    }
}
