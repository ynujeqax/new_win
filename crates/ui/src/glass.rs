//! Painting the liquid glass over a window's backdrop (spec: liquid glass §2, variant B).

use crate::{Color, Painter, RectF, theme};

/// The tint over the blurred backdrop, or the stronger one when there is no backdrop.
pub fn glass_tint(with_backdrop: bool) -> Color {
    if with_backdrop { theme::GLASS_TINT } else { theme::GLASS_TINT_SOLID }
}

/// The tint alone: no rim or glow along the edge, the blur itself shapes the glass.
pub fn paint_glass(p: &mut Painter, r: RectF, radius: f32, with_backdrop: bool) {
    p.fill_round_rect(r, radius, glass_tint(with_backdrop));
}
