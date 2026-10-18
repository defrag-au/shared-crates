//! How a run of text looks: its font, size, colour, and an optional outline
//! and shadow.

use macroquad::prelude::*;

/// A line drawn around each glyph, for text over art.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outline {
    pub color: Color,
    pub width: f32,
}

/// A copy drawn underneath, offset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub color: Color,
    pub offset: Vec2,
}

#[derive(Clone, Debug)]
pub struct TextStyle {
    /// `None` draws in macroquad's built-in font.
    pub font: Option<Font>,
    /// Points; resolve through the theme's ramp.
    pub size: f32,
    pub color: Color,
    pub outline: Option<Outline>,
    pub shadow: Option<Shadow>,
}

impl TextStyle {
    pub fn new(font: Option<Font>, size: f32, color: Color) -> Self {
        Self {
            font,
            size,
            color,
            outline: None,
            shadow: None,
        }
    }

    pub fn outline(mut self, color: Color, width: f32) -> Self {
        self.outline = Some(Outline { color, width });
        self
    }

    pub fn shadow(mut self, color: Color, offset: Vec2) -> Self {
        self.shadow = Some(Shadow { color, offset });
        self
    }
}
