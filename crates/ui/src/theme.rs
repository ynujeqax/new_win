//! Visual constants agreed on the mockups (spec §2). Lengths are logical px at 96 DPI.

use crate::Color;

/// Dark base under the glass overlay so white text stays readable over light app content.
pub const TINT: Color = Color::rgba(18, 18, 20, 0.88);
pub const BORDER: Color = Color::rgba(255, 255, 255, 0.08);
pub const TEXT: Color = Color::rgba(0xec, 0xec, 0xec, 1.0);
pub const TEXT_STRONG: Color = Color::rgba(0xff, 0xff, 0xff, 1.0);
pub const TEXT_INACTIVE: Color = Color::rgba(0x9a, 0x9a, 0x9a, 1.0);
pub const TEXT_DISABLED: Color = Color::rgba(0x6f, 0x6f, 0x6f, 1.0);
pub const ACTIVE_PILL: Color = Color::rgba(255, 255, 255, 0.11);
pub const HOVER_PILL: Color = Color::rgba(255, 255, 255, 0.07);
pub const SEPARATOR: Color = Color::rgba(255, 255, 255, 0.12);

pub const PILL_HEIGHT: f32 = 22.0;
pub const PILL_RADIUS: f32 = 7.0;
pub const PILL_PAD_X: f32 = 9.0;
pub const ITEM_GAP: f32 = 2.0;

pub const POPUP_BG: Color = Color::rgba(28, 28, 30, 0.78);
pub const POPUP_RADIUS: f32 = 12.0;
pub const POPUP_PAD: f32 = 6.0;
pub const POPUP_ITEM_HEIGHT: f32 = 30.0;
pub const POPUP_ITEM_RADIUS: f32 = 8.0;
pub const POPUP_ITEM_PAD_X: f32 = 10.0;
pub const POPUP_HOVER: Color = Color::rgba(255, 255, 255, 0.10);

pub const WEIGHT_REGULAR: u16 = 400;
pub const WEIGHT_SEMIBOLD: u16 = 600;

pub const REVEAL_MS: u32 = 150;
pub const HIDE_MS: u32 = 120;
