//! Minimal Direct2D / DirectWrite / DirectComposition toolkit for NewWin.
//!
//! Retained, on-demand rendering: callers redraw a `Layer` only when something changed;
//! animations are DirectComposition property animations run by DWM.

mod anim;
pub mod backdrop;
mod geom;
mod glass;
mod gfx;
mod layer;
mod paint;
pub mod popup;
mod text;
pub mod theme;

pub use anim::{ease_out_progress, fade, set_offset_opacity, slide_y, slide_y_both};
pub use geom::{Color, PointF, RectF};
pub use glass::{glass_tint, paint_glass};
pub use gfx::Gfx;
pub use layer::{Backdrop, Layer};
pub use paint::Painter;
pub use text::{TextCache, TextLayout, TextStyle};
