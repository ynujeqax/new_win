//! Painting the liquid glass over a window's backdrop (spec: liquid glass §2, variant B).

use crate::{Color, Painter, RectF, theme};

/// The tint over the blurred backdrop, or the stronger one when there is no backdrop.
pub fn glass_tint(with_backdrop: bool) -> Color {
    if with_backdrop { theme::GLASS_TINT } else { theme::GLASS_TINT_SOLID }
}

/// Tint, the soft inner glow and the 1 px rim that lights the rounded ends.
pub fn paint_glass(p: &mut Painter, r: RectF, radius: f32, with_backdrop: bool) {
    p.fill_round_rect(r, radius, glass_tint(with_backdrop));
    for (inset, c) in theme::GLASS_GLOW {
        p.stroke_round_rect(r.inset(inset), (radius - inset).max(0.0), c, 1.0);
    }
    p.stroke_round_rect_h(r, radius, &theme::GLASS_RIM_H, 1.0);
}
