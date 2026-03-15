use egui::Pos2;
use std::f32::consts::TAU;

use super::overlay::CardMask;

/// Badge geometry constants.
pub const BADGE_H: f32 = 22.0;
pub const BADGE_W_FRAC: f32 = 0.45;
const BADGE_OVERLAP: f32 = 0.4;
const BADGE_ARC_SEGS: u32 = 8;

/// Vertices for a regular polygon centered at `center` with given `radius`.
pub fn regular_polygon_vertices(center: Pos2, radius: f32, sides: u32, rotation: f32) -> Vec<Pos2> {
    (0..sides)
        .map(|i| {
            let angle = rotation + (i as f32 / sides as f32) * TAU;
            Pos2::new(
                center.x + angle.cos() * radius,
                center.y + angle.sin() * radius,
            )
        })
        .collect()
}

/// Vertices tracing a rounded rectangle path.
pub fn rounded_rect_vertices(
    center: Pos2,
    half_w: f32,
    half_h: f32,
    radius: f32,
    segments_per_corner: u32,
) -> Vec<Pos2> {
    let r = radius.min(half_w).min(half_h);
    let mut verts = Vec::new();

    let quarter = TAU / 4.0;
    let corners = [
        (center.x + half_w - r, center.y - half_h + r, -quarter),
        (center.x + half_w - r, center.y + half_h - r, 0.0),
        (center.x - half_w + r, center.y + half_h - r, quarter),
        (center.x - half_w + r, center.y - half_h + r, 2.0 * quarter),
    ];

    for &(cx, cy, start_angle) in &corners {
        for i in 0..=segments_per_corner {
            let t = i as f32 / segments_per_corner as f32;
            let angle = start_angle + t * quarter;
            verts.push(Pos2::new(cx + angle.cos() * r, cy + angle.sin() * r));
        }
    }

    verts
}

/// Expand a closed outline outward by `thickness` using miter normals.
pub fn expand_outline(inner: &[Pos2], thickness: f32) -> Vec<Pos2> {
    let n = inner.len();
    if n < 3 {
        return inner.to_vec();
    }
    let mut outer = Vec::with_capacity(n);
    for i in 0..n {
        let prev = inner[(i + n - 1) % n];
        let curr = inner[i];
        let next = inner[(i + 1) % n];

        let e1x = curr.x - prev.x;
        let e1y = curr.y - prev.y;
        let e2x = next.x - curr.x;
        let e2y = next.y - curr.y;

        let len1 = (e1x * e1x + e1y * e1y).sqrt().max(0.001);
        let n1x = e1y / len1;
        let n1y = -e1x / len1;
        let len2 = (e2x * e2x + e2y * e2y).sqrt().max(0.001);
        let n2x = e2y / len2;
        let n2y = -e2x / len2;

        let mx = n1x + n2x;
        let my = n1y + n2y;
        let mlen = (mx * mx + my * my).sqrt().max(0.001);
        let mx = mx / mlen;
        let my = my / mlen;

        let cos_half = (mx * n1x + my * n1y).max(0.2);
        let miter_len = thickness / cos_half;

        outer.push(Pos2::new(curr.x + mx * miter_len, curr.y + my * miter_len));
    }
    outer
}

/// Cumulative arc-lengths for a closed polygon path.
pub fn cumulative_lengths(path: &[Pos2]) -> Vec<f32> {
    let n = path.len();
    let mut cum = Vec::with_capacity(n + 1);
    cum.push(0.0);
    for i in 0..n {
        let j = (i + 1) % n;
        let dx = path[j].x - path[i].x;
        let dy = path[j].y - path[i].y;
        cum.push(cum[i] + (dx * dx + dy * dy).sqrt());
    }
    cum
}

/// Sample a position along a closed polygon path at parameter `t` (0..1).
pub fn sample_path(path: &[Pos2], cum: &[f32], t: f32) -> Pos2 {
    let total = *cum.last().unwrap_or(&1.0);
    let target = (t.fract() + 1.0).fract() * total;
    let seg = match cum.binary_search_by(|v| v.partial_cmp(&target).unwrap()) {
        Ok(i) => i.min(path.len() - 1),
        Err(i) => (i - 1).min(path.len() - 1),
    };
    let seg_start = cum[seg];
    let seg_end = cum[seg + 1];
    let seg_len = seg_end - seg_start;
    let frac = if seg_len > 0.001 {
        (target - seg_start) / seg_len
    } else {
        0.0
    };
    let a = path[seg];
    let b = path[(seg + 1) % path.len()];
    Pos2::new(a.x + (b.x - a.x) * frac, a.y + (b.y - a.y) * frac)
}

/// Generate a pill (stadium) polygon as a Vec of [f32; 2] points.
fn pill_polygon(cx: f32, cy: f32, width: f32, height: f32, segs: u32) -> Vec<[f32; 2]> {
    let r = height / 2.0;
    let half_straight = (width / 2.0 - r).max(0.0);
    let mut pts = Vec::new();

    for i in 0..=segs {
        let t = i as f32 / segs as f32;
        let a = -std::f32::consts::FRAC_PI_2 + t * std::f32::consts::PI;
        pts.push([cx + half_straight + r * a.cos(), cy + r * a.sin()]);
    }

    for i in 0..=segs {
        let t = i as f32 / segs as f32;
        let a = std::f32::consts::FRAC_PI_2 + t * std::f32::consts::PI;
        pts.push([cx - half_straight + r * a.cos(), cy + r * a.sin()]);
    }

    pts
}

/// Build a unified outline path that merges the card border and badge popout
/// into a single continuous silhouette using boolean union.
pub fn unified_outline(center: Pos2, half: f32, mask: CardMask) -> Vec<Pos2> {
    use i_overlay::core::fill_rule::FillRule;
    use i_overlay::core::overlay_rule::OverlayRule;
    use i_overlay::float::single::SingleFloatOverlay;

    match mask {
        CardMask::Hex { radius } => regular_polygon_vertices(center, radius, 6, -TAU / 4.0),
        CardMask::Square => {
            let left = center.x - half;
            let right = center.x + half;
            let top = center.y - half;
            let bottom = center.y + half;

            let card: Vec<[f32; 2]> =
                vec![[left, top], [right, top], [right, bottom], [left, bottom]];

            let badge_w = half * 2.0 * BADGE_W_FRAC;
            let badge_top = bottom - BADGE_H * BADGE_OVERLAP;
            let badge_cy = badge_top + BADGE_H / 2.0;
            let pill = pill_polygon(center.x, badge_cy, badge_w, BADGE_H, BADGE_ARC_SEGS);

            let result = card.overlay(&pill, OverlayRule::Union, FillRule::EvenOdd);

            if let Some(shape) = result.first() {
                if let Some(contour) = shape.first() {
                    return contour.iter().map(|p| Pos2::new(p[0], p[1])).collect();
                }
            }

            vec![
                Pos2::new(left, top),
                Pos2::new(right, top),
                Pos2::new(right, bottom),
                Pos2::new(left, bottom),
            ]
        }
        CardMask::RoundedSquare { corner_radius } => {
            let r = corner_radius.min(half);
            let left = center.x - half;
            let right = center.x + half;
            let top = center.y - half;
            let bottom = center.y + half;

            let segs = 8_u32;
            let quarter = TAU / 4.0;
            let mut card: Vec<[f32; 2]> = Vec::new();

            let cx_tr = right - r;
            let cy_tr = top + r;
            for i in 0..=segs {
                let t = i as f32 / segs as f32;
                let a = -quarter + t * quarter;
                card.push([cx_tr + a.cos() * r, cy_tr + a.sin() * r]);
            }

            let cx_br = right - r;
            let cy_br = bottom - r;
            for i in 0..=segs {
                let t = i as f32 / segs as f32;
                let a = t * quarter;
                card.push([cx_br + a.cos() * r, cy_br + a.sin() * r]);
            }

            let cx_bl = left + r;
            let cy_bl = bottom - r;
            for i in 0..=segs {
                let t = i as f32 / segs as f32;
                let a = quarter + t * quarter;
                card.push([cx_bl + a.cos() * r, cy_bl + a.sin() * r]);
            }

            let cx_tl = left + r;
            let cy_tl = top + r;
            for i in 0..=segs {
                let t = i as f32 / segs as f32;
                let a = 2.0 * quarter + t * quarter;
                card.push([cx_tl + a.cos() * r, cy_tl + a.sin() * r]);
            }

            let badge_w = half * 2.0 * BADGE_W_FRAC;
            let badge_top = bottom - BADGE_H * BADGE_OVERLAP;
            let badge_cy = badge_top + BADGE_H / 2.0;
            let pill = pill_polygon(center.x, badge_cy, badge_w, BADGE_H, BADGE_ARC_SEGS);

            let result = card.overlay(&pill, OverlayRule::Union, FillRule::EvenOdd);

            if let Some(shape) = result.first() {
                if let Some(contour) = shape.first() {
                    return contour.iter().map(|p| Pos2::new(p[0], p[1])).collect();
                }
            }

            card.iter().map(|p| Pos2::new(p[0], p[1])).collect()
        }
    }
}
