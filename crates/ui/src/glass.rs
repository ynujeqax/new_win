//! Painting the liquid glass over a window's backdrop (spec: liquid glass §2).

use crate::{Color, Painter, RectF, theme};

/// The tint over the blurred backdrop, or the stronger one when there is no backdrop.
pub fn glass_tint(with_backdrop: bool) -> Color {
    if with_backdrop { theme::GLASS_TINT } else { theme::GLASS_TINT_SOLID }
}

/// Tint and the 1 px rim over `r`.
pub fn paint_glass(p: &mut Painter, r: RectF, radius: f32, with_backdrop: bool) {
    p.fill_round_rect(r, radius, glass_tint(with_backdrop));
    p.stroke_round_rect_v(r, radius, &theme::GLASS_RIM, 1.0);
}
