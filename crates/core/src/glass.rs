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

/// Colors under the glass get more vivid, as in macOS Liquid Glass.
pub const SATURATION: f32 = 1.7;
/// Width of the lens band inside the glass edge, desktop px.
pub const LENS_BAND: f32 = 12.0;
/// How far from outside the very edge of the glass samples, desktop px.
pub const LENS_MAX: f32 = 8.0;

/// Scales each pixel's distance from its own luminance by `s` (1 = unchanged).
pub fn saturate(img: &mut Image, s: f32) {
    for p in img.bgra.chunks_mut(4) {
        let (b, g, r) = (f32::from(p[0]), f32::from(p[1]), f32::from(p[2]));
        let l = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        for c in &mut p[..3] {
            *c = (l + (f32::from(*c) - l) * s).round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Signed distance from (`x`, `y`) to a rounded rectangle (negative inside), desktop px.
fn rounded_rect_distance(x: f32, y: f32, r: PxRect, radius: f32) -> f32 {
    let (cx, cy) = ((r.left + r.right) as f32 / 2.0, (r.top + r.bottom) as f32 / 2.0);
    let radius = radius.min(r.width() as f32 / 2.0).min(r.height() as f32 / 2.0);
    let qx = (x - cx).abs() - (r.width() as f32 / 2.0 - radius);
    let qy = (y - cy).abs() - (r.height() as f32 / 2.0 - radius);
    let outside = qx.max(0.0).hypot(qy.max(0.0));
    outside + qx.max(qy).min(0.0) - radius
}

/// Bilinear sample of `img` at small-pixel coordinates (centers at .5), clamped to the edges.
fn sample(img: &Image, x: f32, y: f32) -> [f32; 3] {
    let x = (x - 0.5).clamp(0.0, (img.width - 1) as f32);
    let y = (y - 0.5).clamp(0.0, (img.height - 1) as f32);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(img.width - 1), (y0 + 1).min(img.height - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let at = |x: usize, y: usize, c: usize| f32::from(img.bgra[(y * img.width + x) * 4 + c]);
    std::array::from_fn(|c| {
        let top = at(x0, y0, c) * (1.0 - fx) + at(x1, y0, c) * fx;
        let bottom = at(x0, y1, c) * (1.0 - fx) + at(x1, y1, c) * fx;
        top * (1.0 - fy) + bottom * fy
    })
}

/// The lens of Liquid Glass: inside a [`LENS_BAND`]-wide band along the edge of `window` (a
/// rounded rectangle with `radius`), each pixel shows the image from up to [`LENS_MAX`] px further
/// out, fading quadratically to nothing at the inner side of the band. `img` covers `area`.
pub fn refract(img: &mut Image, area: PxRect, window: PxRect, radius: f32) {
    if img.width == 0 || img.height == 0 {
        return;
    }
    let src = img.clone();
    let s = SCALE as f32;
    for y in 0..img.height {
        for x in 0..img.width {
            let (dx, dy) = (area.left as f32 + (x as f32 + 0.5) * s, area.top as f32 + (y as f32 + 0.5) * s);
            let d = rounded_rect_distance(dx, dy, window, radius);
            if d >= 0.0 || -d >= LENS_BAND {
                continue;
            }
            let t = 1.0 - (-d) / LENS_BAND;
            let push = LENS_MAX * t * t;
            // Outward normal from the distance field's gradient.
            let e = 0.5;
            let nx = rounded_rect_distance(dx + e, dy, window, radius) - rounded_rect_distance(dx - e, dy, window, radius);
            let ny = rounded_rect_distance(dx, dy + e, window, radius) - rounded_rect_distance(dx, dy - e, window, radius);
            let len = nx.hypot(ny);
            if len == 0.0 {
                continue;
            }
            let (sx, sy) = (dx + nx / len * push, dy + ny / len * push);
            let v = sample(&src, (sx - area.left as f32) / s, (sy - area.top as f32) / s);
            let o = (y * img.width + x) * 4;
            for (dst, c) in img.bgra[o..o + 3].iter_mut().zip(v) {
                *dst = c.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// Everything the glass shows from a captured strip: blur, saturation, lens.
pub fn glass_image(src: &[u8], pitch: usize, area: PxRect, window: PxRect, radius: f32) -> Image {
    let mut img = blur_strip(src, pitch, area.width() as usize, area.height() as usize);
    saturate(&mut img, SATURATION);
    refract(&mut img, area, window, radius);
    img
}

/// Pause before rebuilding a failed capture: 0.25 s doubling up to 5 s (`failures` in a row).
pub fn retry_delay_ms(failures: u32) -> u64 {
    match failures {
        0 => 0,
        n => (250u64 << (n - 1).min(5)).min(5000),
    }
}

/// Whether `area` (in frame coordinates) lies inside a `width` x `height` frame and is non-empty.
pub fn fits(area: PxRect, width: u32, height: u32) -> bool {
    area.left >= 0
        && area.top >= 0
        && area.width() > 0
        && area.height() > 0
        && area.right as i64 <= i64::from(width)
        && area.bottom as i64 <= i64::from(height)
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
