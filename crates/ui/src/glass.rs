//! Painting the liquid glass over a window's backdrop (spec: liquid glass §2, variant B).

use crate::{Color, Painter, RectF, theme};

/// The tint over the blurred backdrop, or the stronger one when there is no backdrop.
pub fn glass_tint(with_backdrop: bool) -> Color {
    if with_backdrop { theme::GLASS_TINT } else { theme::GLASS_TINT_SOLID }
}

/// The tint, a little lighter at the top and deeper at the bottom, so the glass reads as a
/// rounded body lit from above; no rim or glow along the edge.
pub fn paint_glass(p: &mut Painter, r: RectF, radius: f32, with_backdrop: bool) {
    let tint = glass_tint(with_backdrop);
    let d = if with_backdrop { theme::GLASS_DEPTH } else { theme::GLASS_DEPTH / 2.0 };
    p.fill_round_rect_v(r, radius, tint.with_alpha(tint.a - d), tint.with_alpha(tint.a + d));
}
