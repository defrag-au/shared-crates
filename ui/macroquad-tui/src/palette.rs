//! Canonical 16-colour EGA palette.
//!
//! Used by games / scenes for visual consistency across the suite.
//! Pure constants — no logic. Terminal applications that want a
//! different palette (e.g. CRT phosphor green) can ignore these and
//! supply their own colours per cell.
//!
//! The RGB values are the *digital* EGA palette — what the card
//! actually outputs at the connector — rather than the perceptual
//! values an NTSC TV would render. The dark colours sit at `0.667`
//! (the IBM `intensity bit unset` level) and the bright ones at `1.0`.
//! Black is true black and white is true white.
//!
//! Colour names follow the original IBM PC convention; `BROWN` is
//! famously the "dark yellow" slot, which on real EGA hardware was
//! gamma-shifted to read as brown rather than mustard.

use macroquad::prelude::Color;

pub const BLACK: Color = Color::new(0.000, 0.000, 0.000, 1.0);
pub const BLUE: Color = Color::new(0.000, 0.000, 0.667, 1.0);
pub const GREEN: Color = Color::new(0.000, 0.667, 0.000, 1.0);
pub const CYAN: Color = Color::new(0.000, 0.667, 0.667, 1.0);
pub const RED: Color = Color::new(0.667, 0.000, 0.000, 1.0);
pub const MAGENTA: Color = Color::new(0.667, 0.000, 0.667, 1.0);
pub const BROWN: Color = Color::new(0.667, 0.333, 0.000, 1.0);
pub const LIGHT_GRAY: Color = Color::new(0.667, 0.667, 0.667, 1.0);

pub const DARK_GRAY: Color = Color::new(0.333, 0.333, 0.333, 1.0);
pub const LIGHT_BLUE: Color = Color::new(0.333, 0.333, 1.000, 1.0);
pub const LIGHT_GREEN: Color = Color::new(0.333, 1.000, 0.333, 1.0);
pub const LIGHT_CYAN: Color = Color::new(0.333, 1.000, 1.000, 1.0);
pub const LIGHT_RED: Color = Color::new(1.000, 0.333, 0.333, 1.0);
pub const LIGHT_MAGENTA: Color = Color::new(1.000, 0.333, 1.000, 1.0);
pub const YELLOW: Color = Color::new(1.000, 1.000, 0.333, 1.0);
pub const WHITE: Color = Color::new(1.000, 1.000, 1.000, 1.0);

/// All 16 colours in canonical IBM PC order. Useful for indexed
/// rendering (an attribute byte's `fg` nibble selects via this).
pub const EGA_16: [Color; 16] = [
    BLACK,
    BLUE,
    GREEN,
    CYAN,
    RED,
    MAGENTA,
    BROWN,
    LIGHT_GRAY,
    DARK_GRAY,
    LIGHT_BLUE,
    LIGHT_GREEN,
    LIGHT_CYAN,
    LIGHT_RED,
    LIGHT_MAGENTA,
    YELLOW,
    WHITE,
];
