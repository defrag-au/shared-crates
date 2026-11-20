//! Trees: conifers scattered over vegetated, gentle ground, the way the
//! paintings dot them — clumped in small groups inside forest patches, alone
//! and rare outside them.
//!
//! [`scatter`] decides where trees stand; [`conifer`] draws one; [`plant`]
//! does both onto a sheet. Trees are collected first and stamped back to
//! front (by their base), so a nearer tree overlaps the one behind it. A 3D
//! renderer takes [`scatter`]'s positions instead and stands real trees there.

use glam::Vec3;
use image::{Rgb, RgbImage};

use crate::colour::{Colour, mix, to_u8};
use crate::noise::hash;
use crate::palette::{GREEN, Look};
use crate::sheet::{Cell, TreeCover};
use crate::terrain::Terrain;

/// Where a tree stands on the sheet: its base, in pixels, and its height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tree {
    pub x: f32,
    pub y: f32,
    /// Pixels tall, as drawn on the sheet.
    pub size: f32,
}

/// Scatter trees over `image`. Where they may stand comes from the land
/// analysis: not at sea, on cliffs, on ice, in rivers or on volcanic ground.
/// `width` sets the tree scale (the scatter spacing is `width / 230` px).
pub fn plant(image: &mut RgbImage, cells: &[Cell], terrain: &Terrain, river: &[f32], width: u32) {
    let trees = scatter(image.width(), image.height(), cells, terrain, river, width);
    for tree in trees {
        conifer(image, tree.x, tree.y, tree.size, &GREEN);
    }
}

/// Where [`plant`] would stand trees on a `w × h` sheet, back to front (a
/// tree further down the sheet comes later).
pub fn scatter(
    w: u32,
    h: u32,
    cells: &[Cell],
    terrain: &Terrain,
    river: &[f32],
    width: u32,
) -> Vec<Tree> {
    scatter_spaced(w, h, cells, terrain, river, (width as f32 / 230.0).max(6.0))
}

/// [`scatter`] at a spacing of its own, pixels between scatter points: the
/// chart's icons want few and large, a renderer standing real trees (the
/// globe) many and small.
pub fn scatter_spaced(
    w: u32,
    h: u32,
    cells: &[Cell],
    terrain: &Terrain,
    river: &[f32],
    spacing: f32,
) -> Vec<Tree> {
    let (w, h) = (w as i32, h as i32);
    let can_stand = |x: f32, y: f32| -> Option<usize> {
        let (ix, iy) = (x as i32, y as i32);
        if ix < 0 || iy < 0 || ix >= w || iy >= h {
            return None;
        }
        let i = iy as usize * w as usize + ix as usize;
        // No trees on ice: lone ones read as debris on the sheet.
        let blocked = terrain.class[i] == u32::MAX
            || terrain.is_cliff(i)
            || terrain.frozen[i]
            || river[i] > 0.1
            || cells[i].biome.tree_cover() == TreeCover::None;
        (!blocked).then_some(i)
    };

    // (x, y, size) of every tree, collected before any is drawn.
    let mut stand: Vec<(f32, f32, f32)> = Vec::new();
    let mut y = spacing * 0.5;
    let mut row = 0;
    while y < h as f32 {
        let mut x = spacing * 0.5 + if row % 2 == 1 { spacing * 0.5 } else { 0.0 };
        while x < w as f32 {
            let seed = Vec3::new(x * 0.137, y * 0.311, 7.7);
            let (jx, jy) = (hash(seed) - 0.5, hash(seed + Vec3::ONE) - 0.5);
            let (cx, cy) = (x + jx * spacing * 0.7, y + jy * spacing * 0.7);
            x += spacing;
            let Some(i) = can_stand(cx, cy) else {
                continue;
            };
            // Trees clump in forest patches and stand alone, rarely, outside
            // them. `>=` so a zero chance plants nothing even where the hash
            // rounds to exactly 0.
            let forest = terrain.forest[i];
            let chance = if forest {
                0.6
            } else {
                cells[i].vegetation * 0.12
            };
            if hash(seed + Vec3::splat(3.0)) >= chance {
                continue;
            }
            // A forest point is a clump of one to three trees; a lone tree is
            // one. Sizes vary a little so a clump does not read as stamped.
            let count = if forest {
                1 + (hash(seed + Vec3::splat(5.0)) * 3.0) as usize
            } else {
                1
            };
            for k in 0..count {
                let ks = seed + Vec3::splat(11.0 + k as f32 * 4.0);
                let (ox, oy) = if k == 0 {
                    (0.0, 0.0)
                } else {
                    (
                        (hash(ks) - 0.5) * spacing * 0.8,
                        (hash(ks + Vec3::ONE) - 0.5) * spacing * 0.5,
                    )
                };
                let (tx, ty) = (cx + ox, cy + oy);
                if can_stand(tx, ty).is_some() {
                    let size = spacing * 0.78 * (0.8 + 0.4 * hash(ks + Vec3::splat(2.0)));
                    stand.push((tx, ty, size));
                }
            }
        }
        y += spacing * 0.87;
        row += 1;
    }

    // Back to front: a tree further down the sheet is nearer the viewer.
    stand.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.total_cmp(&b.0)));
    stand
        .into_iter()
        .map(|(x, y, size)| Tree { x, y, size })
        .collect()
}

/// Draw one conifer with its base at `(x, y)` and `size` px tall: two stacked
/// tiers of foliage over a short trunk. Lit from the left like the rest of
/// the sheet: the left half takes the look's tree colour, the right a shaded
/// one, with a light ink edge and a darker underside to each tier.
pub fn conifer(image: &mut RgbImage, x: f32, y: f32, size: f32, look: &Look) {
    let (w, h) = (image.width() as i32, image.height() as i32);
    let lit = look.tree;
    let shade = mix(look.tree, look.ink, 0.35);
    let under = mix(look.tree, look.ink, 0.55);
    let ink = mix(look.tree, look.ink, 0.8);
    let trunk = mix(look.ink, [0.45, 0.30, 0.18], 0.4);

    // (apex y, base y, half width at base), lower tier first.
    let tiers = [
        (y - size * 0.72, y - size * 0.14, size * 0.36),
        (y - size, y - size * 0.44, size * 0.27),
    ];
    let put = |image: &mut RgbImage, px: i32, py: i32, c: Colour| {
        if px >= 0 && py >= 0 && px < w && py < h {
            image.put_pixel(px as u32, py as u32, Rgb(to_u8(c)));
        }
    };

    // Trunk: a stub below the lower tier.
    let trunk_half = (size * 0.06).max(0.5);
    for py in (y - size * 0.16).floor() as i32..=y.round() as i32 {
        for px in (x - trunk_half).round() as i32..=(x + trunk_half).round() as i32 {
            put(image, px, py, trunk);
        }
    }

    for (apex, base, half) in tiers {
        let height = base - apex;
        for py in apex.floor() as i32..=base.ceil() as i32 {
            let down = (py as f32 + 0.5 - apex) / height;
            if !(0.0..=1.0).contains(&down) {
                continue;
            }
            let reach = half * down;
            for px in (x - half - 1.0).floor() as i32..=(x + half + 1.0).ceil() as i32 {
                let off = px as f32 + 0.5 - x;
                let edge = reach - off.abs();
                if edge < -0.5 {
                    continue;
                }
                let colour = if edge < 0.5 {
                    ink
                } else if down > 0.86 {
                    under
                } else if off > 0.0 {
                    shade
                } else {
                    lit
                };
                put(image, px, py, colour);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_conifer_stays_in_its_footprint() {
        let mut image = RgbImage::new(40, 40);
        conifer(&mut image, 20.0, 30.0, 20.0, &GREEN);
        let mut drawn = 0;
        for (px, py, p) in image.enumerate_pixels() {
            if p.0 != [0, 0, 0] {
                drawn += 1;
                assert!((9..=31).contains(&px), "x {px} outside the crown");
                assert!((9..=31).contains(&py), "y {py} outside the tree");
            }
        }
        assert!(drawn > 60, "a tree covers some pixels");
    }
}
