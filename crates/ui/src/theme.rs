//! Visual constants agreed on the mockups (spec §2). Lengths are logical px at 96 DPI.

use crate::Color;

pub const TEXT: Color = Color::rgba(0xec, 0xec, 0xec, 1.0);
pub const TEXT_STRONG: Color = Color::rgba(0xff, 0xff, 0xff, 1.0);
pub const TEXT_INACTIVE: Color = Color::rgba(0x9a, 0x9a, 0x9a, 1.0);
pub const TEXT_DISABLED: Color = Color::rgba(0x6f, 0x6f, 0x6f, 1.0);
pub const ACTIVE_PILL: Color = Color::rgba(255, 255, 255, 0.16);
pub const HOVER_PILL: Color = Color::rgba(255, 255, 255, 0.07);
pub const SEPARATOR: Color = Color::rgba(255, 255, 255, 0.12);

pub const PILL_HEIGHT: f32 = 26.0;
/// Half the pill height: a capsule, like the bar itself.
pub const PILL_RADIUS: f32 = 13.0;
pub const PILL_PAD_X: f32 = 9.0;
pub const ITEM_GAP: f32 = 2.0;

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

/// Liquid glass, variant B "dark glass" (spec: liquid glass §2): tint over the blurred backdrop,
/// and without one.
pub const GLASS_TINT: Color = Color::rgba(8, 8, 10, 0.38);
pub const GLASS_TINT_SOLID: Color = Color::rgba(8, 8, 10, 0.88);
/// 1 px rim, left to right: light catches the rounded ends, the long sides stay almost clear.
pub const GLASS_RIM_H: [(f32, Color); 4] = [
    (0.0, Color::rgba(255, 255, 255, 0.45)),
    (0.18, Color::rgba(255, 255, 255, 0.05)),
    (0.82, Color::rgba(255, 255, 255, 0.03)),
    (1.0, Color::rgba(255, 255, 255, 0.35)),
];
/// Soft inner glow: (inset px, color) strokes inside the edge.
pub const GLASS_GLOW: [(f32, Color); 3] =
    [(1.0, Color::rgba(255, 255, 255, 0.07)), (2.0, Color::rgba(255, 255, 255, 0.04)), (3.0, Color::rgba(255, 255, 255, 0.02))];
