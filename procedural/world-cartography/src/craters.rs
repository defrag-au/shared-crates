//! War craters, drawn.
//!
//! The craters themselves are world data: the bake's detail stage places
//! them and stamps them into the elevation before erosion
//! (`world_baker::detail::place_craters`, shape
//! [`world_bake::crater_offset`]), so rivers already route around them. The
//! bake records them (`DetailInfo::craters`); this module only draws them.
//!
//! PROPOSED world material ("a war broke the world"), not founder canon.

use glam::Vec3;
pub use world_bake::Crater;

use crate::colour::{Colour, mix, rgb};
use crate::palette::Look;

/// Where a direction sits relative to the nearest crater it falls in.
pub struct Spot {
    /// Distance from the centre in crater radii.
    pub t: f32,
    /// Unit offset from the centre, east and south (screen x and y).
    pub east: f32,
    pub south: f32,
    /// The crater's radius, radians.
    pub radius: f32,
}

/// The crater a direction falls in (out to its ejecta), if any.
pub fn spot(craters: &[Crater], dir: Vec3) -> Option<Spot> {
    craters
        .iter()
        .filter_map(|c| {
            let centre = Vec3::from_array(c.dir);
            let angle = dir.dot(centre).clamp(-1.0, 1.0).acos();
            let t = angle / c.radius;
            (t < 1.6).then(|| {
                let east_axis = Vec3::Y.cross(centre).normalize_or_zero();
                let north_axis = centre.cross(east_axis);
                let off = dir - centre;
                let (e, n) = (off.dot(east_axis), off.dot(north_axis));
                let len = (e * e + n * n).sqrt().max(1e-9);
                Spot {
                    t,
                    east: e / len,
                    south: -n / len,
                    radius: c.radius,
                }
            })
        })
        .min_by(|a, b| a.t.total_cmp(&b.t))
}

/// A crater drawn over `ground` in the cel style: a bare-earth bowl, a raised
/// rim, an ink edge and a faint ejecta skirt. Light comes from the
/// north-west, so the bowl's far (south-east) wall and the rim's near
/// (north-west) slope are lit. `px` is a pixel's size on the ground, radians.
/// Without `shade` no wall faces the light: bowl and rim take the middle
/// earth, for a renderer that lights the crater's real relief.
pub fn draw(ground: Colour, spot: &Spot, px: f32, look: &Look, shade: bool) -> Colour {
    const EARTH: [Colour; 3] = [rgb(84, 58, 40), rgb(124, 92, 60), rgb(166, 130, 86)];
    const FLOOR: f32 = 0.82;
    const RIM: f32 = 1.12;
    // Towards the light: +1 on the north-west, −1 on the south-east.
    let toward = if shade {
        -(spot.east + spot.south) * std::f32::consts::FRAC_1_SQRT_2
    } else {
        0.0
    };
    let ink_width = px / spot.radius; // one pixel, in crater radii
    let t = spot.t;
    if t > RIM {
        // Ejecta: a scuffed skirt of earth fading out by 1.6 radii.
        let skirt = (1.0 - (t - RIM) / (1.6 - RIM)).clamp(0.0, 1.0);
        let outline = (1.0 - (t - RIM) / ink_width).clamp(0.0, 1.0);
        return mix(
            mix(ground, EARTH[1], skirt * skirt * 0.35),
            look.ink,
            outline * 0.8,
        );
    }
    let base = if t < FLOOR {
        // The bowl: dark floor, inner wall lit on the side facing the light
        // (the far side), shadowed under the near rim.
        let wall = (t / FLOOR).powi(2);
        let lit = -toward * wall;
        EARTH[if lit > 0.35 {
            2
        } else if lit < -0.25 {
            0
        } else {
            1
        }]
    } else {
        // The rim: its outer slope faces outward, lit on the near side.
        EARTH[if toward > 0.2 {
            2
        } else if toward < -0.4 {
            0
        } else {
            1
        }]
    };
    // A lighter line where the floor meets the rim's inner wall.
    let lip = (1.0 - (t - FLOOR).abs() / ink_width).clamp(0.0, 1.0);
    mix(base, look.ink, lip * 0.45)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(radius: f32) -> Vec<Crater> {
        vec![Crater {
            dir: [1.0, 0.0, 0.0],
            radius,
            depth: 1.0,
            epoch: 0,
        }]
    }

    #[test]
    fn spots_reach_out_to_the_ejecta_only() {
        let craters = one(0.05);
        let near = spot(&craters, Vec3::new(1.0, 0.0, 0.05).normalize()).expect("inside");
        assert!((near.t - 1.0).abs() < 0.01);
        assert!(spot(&craters, Vec3::new(1.0, 0.0, 0.2).normalize()).is_none());
    }
}
