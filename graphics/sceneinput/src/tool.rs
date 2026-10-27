use std::fmt;

use crate::error::Error;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    #[default]
    Pick,
    Orbit,
    Drag,
    /// First-person: drag looks, wheel/stride walks. Collision is `hit`.
    Walk,
}

impl Tool {
    pub fn as_str(self) -> &'static str {
        match self {
            Tool::Pick => "pick",
            Tool::Orbit => "orbit",
            Tool::Drag => "drag",
            Tool::Walk => "walk",
        }
    }

    pub fn parse(token: &str) -> Result<Self, Error> {
        match token {
            "pick" => Ok(Tool::Pick),
            "orbit" => Ok(Tool::Orbit),
            "drag" => Ok(Tool::Drag),
            "walk" => Ok(Tool::Walk),
            other => Err(Error::UnknownTool(other.to_string())),
        }
    }
}

impl fmt::Display for Tool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Pointer events in NDC (x,y ∈ [-1, 1], y up). `aspect` is width/height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Down {
        ndc: [f32; 2],
        aspect: f32,
    },
    Move {
        ndc: [f32; 2],
        aspect: f32,
    },
    Up {
        ndc: [f32; 2],
        aspect: f32,
    },
    Wheel {
        ndc: [f32; 2],
        aspect: f32,
        delta: f32,
    },
}

impl Event {
    pub fn down(ndc_x: f32, ndc_y: f32, aspect: f32) -> Self {
        Event::Down {
            ndc: [ndc_x, ndc_y],
            aspect,
        }
    }

    pub fn motion(ndc_x: f32, ndc_y: f32, aspect: f32) -> Self {
        Event::Move {
            ndc: [ndc_x, ndc_y],
            aspect,
        }
    }

    pub fn up(ndc_x: f32, ndc_y: f32, aspect: f32) -> Self {
        Event::Up {
            ndc: [ndc_x, ndc_y],
            aspect,
        }
    }

    pub fn wheel(ndc_x: f32, ndc_y: f32, aspect: f32, delta: f32) -> Self {
        Event::Wheel {
            ndc: [ndc_x, ndc_y],
            aspect,
            delta,
        }
    }

    pub fn ndc(self) -> [f32; 2] {
        match self {
            Event::Down { ndc, .. }
            | Event::Move { ndc, .. }
            | Event::Up { ndc, .. }
            | Event::Wheel { ndc, .. } => ndc,
        }
    }

    pub fn aspect(self) -> f32 {
        match self {
            Event::Down { aspect, .. }
            | Event::Move { aspect, .. }
            | Event::Up { aspect, .. }
            | Event::Wheel { aspect, .. } => aspect,
        }
    }
}
