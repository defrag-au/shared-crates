//! Colours as linear `[r, g, b]` in 0..1, and the small maths drawing uses.

pub type Colour = [f32; 3];

/// An sRGB byte triple as a [`Colour`].
pub const fn rgb(r: u8, g: u8, b: u8) -> Colour {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

/// `a` towards `b` by `t`, clamped to 0..1.
pub fn mix(a: Colour, b: Colour, t: f32) -> Colour {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

pub fn to_u8(c: Colour) -> [u8; 3] {
    [
        (c[0].clamp(0.0, 1.0) * 255.0) as u8,
        (c[1].clamp(0.0, 1.0) * 255.0) as u8,
        (c[2].clamp(0.0, 1.0) * 255.0) as u8,
    ]
}

pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Which of three cel bands (shadow, mid, light) a light level falls in.
pub fn band(lit: f32, low: f32, high: f32) -> usize {
    if lit < low {
        0
    } else if lit > high {
        2
    } else {
        1
    }
}
