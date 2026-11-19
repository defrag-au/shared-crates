//! Where plates meet, and how: spreading apart, closing together or sliding
//! past, from the bake's plate layer and the plates' motions
//! ([`crate::LayerName::Plate`], [`crate::WorldManifest::plates`]).

use crate::{Layer, PlateMotion, cross, normalize};

/// A plate boundary seen from a point near it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Meeting {
    /// The plate here, and the one across the boundary.
    pub here: usize,
    pub there: usize,
    /// Radians from the point to the boundary, to within the search's step.
    pub distance: f32,
    /// Unit direction along the surface from here toward the boundary.
    pub toward: [f32; 3],
    /// How fast the two plates move apart across the boundary, radians per
    /// million years: positive where they spread, negative where they close.
    pub opening: f32,
    /// How fast they slide past each other along it, radians per million
    /// years, never negative.
    pub sliding: f32,
}

/// What kind of boundary a meeting is: whichever of its opening, closing or
/// sliding is fastest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Boundary {
    /// Plates moving apart: a rift on land, a ridge at sea.
    Spreading,
    /// Plates moving together: a trench where one dives, mountains where
    /// neither will.
    Closing,
    /// Plates sliding past: a transform fault.
    Sliding,
}

impl Meeting {
    pub fn boundary(&self) -> Boundary {
        if self.sliding > self.opening.abs() {
            Boundary::Sliding
        } else if self.opening > 0.0 {
            Boundary::Spreading
        } else {
            Boundary::Closing
        }
    }

    /// How fast the plates move against each other here, in all, radians per
    /// million years.
    pub fn rate(&self) -> f32 {
        self.opening.hypot(self.sliding)
    }
}

/// The nearest plate boundary to unit direction `dir` within `reach`
/// radians, if any: searched outward in `rings` rings of 16 directions, so
/// `distance` is good to `reach / rings`.
pub fn meeting(
    plates: &Layer,
    motions: &[PlateMotion],
    dir: [f32; 3],
    reach: f32,
    rings: u32,
) -> Option<Meeting> {
    let d = normalize(dir);
    let here = plates.index(d);
    let up = if d[1].abs() < 0.9 {
        [0.0, 1.0, 0.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let u = normalize(cross(d, up));
    let v = cross(d, u);
    if here >= motions.len() {
        return None;
    }
    for ring in 1..=rings {
        let r = reach * ring as f32 / rings as f32;
        // The first ring to reach another plate: toward it is the mean of
        // the directions that crossed into the first plate found.
        let mut there = None;
        let mut sum = [0.0f32; 3];
        for k in 0..16 {
            let (s, c) = (k as f32 * std::f32::consts::TAU / 16.0).sin_cos();
            let t = [
                u[0] * c + v[0] * s,
                u[1] * c + v[1] * s,
                u[2] * c + v[2] * s,
            ];
            let p = normalize([d[0] + t[0] * r, d[1] + t[1] * r, d[2] + t[2] * r]);
            let plate = plates.index(p);
            if plate == here || plate >= motions.len() || there.is_some_and(|o| o != plate) {
                continue;
            }
            there = Some(plate);
            sum = [sum[0] + t[0], sum[1] + t[1], sum[2] + t[2]];
        }
        let Some(there) = there else { continue };
        let t = normalize(sum);
        // Relative motion at the boundary: the far plate's velocity seen
        // from this one's.
        let (a, b) = (motions[here].velocity(d), motions[there].velocity(d));
        let rel = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let opening = rel[0] * t[0] + rel[1] * t[1] + rel[2] * t[2];
        let along = [
            rel[0] - opening * t[0],
            rel[1] - opening * t[1],
            rel[2] - opening * t[2],
        ];
        return Some(Meeting {
            here,
            there,
            distance: r,
            toward: t,
            opening,
            sliding: (along[0] * along[0] + along[1] * along[1] + along[2] * along[2]).sqrt(),
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LayerInfo, LayerName, octahedral};

    /// Two plates split at the equator (0 north, 1 south).
    fn hemispheres() -> Layer {
        let size = 128u32;
        let values = (0..size * size)
            .map(|i| {
                let d = octahedral::uv_to_dir(
                    ((i % size) as f32 + 0.5) / size as f32,
                    ((i / size) as f32 + 0.5) / size as f32,
                );
                if d[1] >= 0.0 { 0.0 } else { 1.0 }
            })
            .collect();
        let info = LayerInfo {
            name: LayerName::Plate,
            file: String::new(),
            units: String::new(),
            min: 0.0,
            max: 1.0,
            size: None,
        };
        Layer::from_values(info, size, values)
    }

    /// Plates turning about the x axis in opposite senses: at the equator
    /// where x = 0 they move straight north and south.
    fn apart(sign: f32) -> Vec<PlateMotion> {
        // About +x, ground at +z moves toward −y... so the sign picks
        // whether the northern plate heads north or south there.
        vec![
            PlateMotion {
                axis: [1.0, 0.0, 0.0],
                rate: -0.01 * sign,
            },
            PlateMotion {
                axis: [1.0, 0.0, 0.0],
                rate: 0.01 * sign,
            },
        ]
    }

    #[test]
    fn plates_moving_apart_spread_and_together_close() {
        let plates = hemispheres();
        // Just north of the equator, on the z axis.
        let here = normalize([0.0, 0.02, 1.0]);
        let spreading = meeting(&plates, &apart(1.0), here, 0.1, 20).unwrap();
        assert_eq!((spreading.here, spreading.there), (0, 1));
        assert!(spreading.distance <= 0.03, "{spreading:?}");
        assert_eq!(spreading.boundary(), Boundary::Spreading, "{spreading:?}");
        assert!((spreading.rate() - 0.02).abs() < 2e-3, "{spreading:?}");

        let closing = meeting(&plates, &apart(-1.0), here, 0.1, 20).unwrap();
        assert_eq!(closing.boundary(), Boundary::Closing, "{closing:?}");
    }

    #[test]
    fn plates_turning_about_the_pole_slide_past() {
        let plates = hemispheres();
        let here = normalize([0.0, 0.02, 1.0]);
        let motions = vec![
            PlateMotion {
                axis: [0.0, 1.0, 0.0],
                rate: 0.01,
            },
            PlateMotion {
                axis: [0.0, 1.0, 0.0],
                rate: -0.01,
            },
        ];
        let m = meeting(&plates, &motions, here, 0.1, 20).unwrap();
        assert_eq!(m.boundary(), Boundary::Sliding, "{m:?}");
    }

    #[test]
    fn far_from_any_boundary_there_is_none() {
        let plates = hemispheres();
        assert_eq!(meeting(&plates, &apart(1.0), [0.0, 1.0, 0.0], 0.1, 10), None);
    }
}
