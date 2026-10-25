//! Minimal Direct2D / DirectWrite / DirectComposition toolkit for NewWin.
//!
//! Retained, on-demand rendering: callers redraw a `Layer` only when something changed;
//! animations are DirectComposition property animations run by DWM.

mod anim;
pub mod app_window;
pub mod backdrop;
mod geom;
mod glass;
pub mod icon_bitmaps;
pub mod icons;
mod gfx;
mod layer;
mod paint;
pub mod panel;
pub mod nav;
pub mod popup;
pub mod reorder_list;
pub mod scroll;
pub mod segmented;
pub mod slider;
pub mod spotlight;
mod text;
mod win_loop;
pub mod tooltip;
pub mod tray_list;
pub mod volume_panel;
pub mod text_field;
pub mod theme;
pub mod toggle;

pub use anim::{ease_out_progress, fade, set_offset_opacity, slide_y, slide_y_both};
pub use geom::{Color, PointF, RectF};
pub use glass::{glass_tint, paint_glass};
pub use gfx::Gfx;
pub use layer::{Backdrop, Layer};
pub use paint::Painter;
pub use panel::GlassWindow;
pub use text::{TextCache, TextLayout, TextStyle};
