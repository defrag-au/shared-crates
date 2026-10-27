//! Problems a glance misses, read from the document alone. The gamekit's
//! capture report leads with text off its surface, too small or overlapping;
//! this is the same idea for a scene, with no pixels: every check reads the
//! [`Scene`] and its flattened [`DrawList`].
//!
//! | Kind | What it means |
//! |---|---|
//! | `off-print` | A Label on a Quad or Slab spills past its face, or sits behind or inside it. |
//! | `missing` | A Label has codepoints its face cannot draw (`.notdef`). |
//! | `off-camera` | A drawable is wholly outside the view. |
//! | `small` | A Label's em is under [`MIN_TEXT`] on a [`REF_HEIGHT`]-unit-tall stage. |
//! | `z-fight` | Two quads or labels are coplanar and overlap, so depth cannot order them. |
//! | `covered` | A control (a node with one of the given tags) cannot be picked anywhere on its face. |
//! | `contrast` | A Label on an untextured face is under [`MIN_CONTRAST`] against it (WCAG ratio). |
//! | `cropped` | A Label on screen runs past the edge of the view. |
//! | `backwards` | A Label on screen faces away from the eye, so it reads mirrored. |
//! | `scale` | A model (`Mesh Geom`) is under [`MIN_MODEL`] or over [`MAX_MODEL`] metres on its longest side. |
//! | `off-origin` | A model is drawn away from its node's origin, so rotating or scaling the node swings it. |
//! | `floating` | A model's base is just above what is under it: over [`REST_TOLERANCE`], up to [`FLOAT_MAX`]. |
//! | `sunk` | A model's base is below the surface under it. |
//! | `heading` | A node with `front=` moved more than [`MAX_HEADING`]° off it ([`headings`], between two moments). |
//! | `heavy` | The scene draws more than [`HEAVY_TRIS`] triangles. |
//!
//! Text size is measured the way the gamekit measures it: design units on a
//! stage [`REF_HEIGHT`] tall (`scene-mq`'s design), against the same 8-unit
//! minimum as `macroquad-text`.

use crate::draw::{DrawList, WorldLabel, WorldQuad};
use crate::images::Images;
use crate::kind::{Kind, Prim};
use crate::math::{mat4_inverse, transform_point, transform_vec4, Mat4, Vec3};
use crate::scene::Scene;

/// Reference stage height in design units: `scene-mq`'s 960×640 design.
pub const REF_HEIGHT: f32 = 640.0;
/// Reference aspect: the same design. A pane of another shape sees more or
/// less at the sides, so `off-camera` and `covered` are judged here.
pub const REF_ASPECT: f32 = 1.5;
/// Below this em, in design units on a [`REF_HEIGHT`] stage, a Label is too
/// small to read. Same as `macroquad-text`'s `MIN_TEXT`.
pub const MIN_TEXT: f32 = 8.0;

/// Under this WCAG contrast ratio, a Label is hard to read on its face. 3:1
/// is WCAG's floor for large text, which every scene Label is.
pub const MIN_CONTRAST: f32 = 3.0;

/// WCAG relative luminance of an sRGB colour (alpha ignored).
pub fn luminance(c: [u8; 4]) -> f32 {
    let lin = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
}

/// WCAG contrast ratio of two colours, 1 (same) to 21 (black on white).
pub fn contrast(a: [u8; 4], b: [u8; 4]) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Dark ink on a light face, white on a dark one: whichever contrasts more.
pub fn ink_for(face: [u8; 4]) -> [u8; 4] {
    const DARK: [u8; 4] = [0x1a, 0x12, 0x10, 255];
    const LIGHT: [u8; 4] = [255, 255, 255, 255];
    if contrast(DARK, face) >= contrast(LIGHT, face) {
        DARK
    } else {
        LIGHT
    }
}

const PLANE_EPS: f32 = 1e-4;
const EDGE_EPS: f32 = 1e-3;

#[derive(Clone, Debug, PartialEq)]
pub enum Problem {
    OffPrint {
        label: String,
        parent: String,
        why: &'static str,
    },
    Missing {
        label: String,
        codepoints: Vec<u32>,
    },
    OffCamera {
        name: String,
    },
    Small {
        label: String,
        em: f32,
    },
    ZFight {
        a: String,
        b: String,
    },
    Covered {
        name: String,
        by: String,
    },
    Contrast {
        label: String,
        parent: String,
        ratio: f32,
    },
    /// A Label partly past the edge of the view: on screen, but cut off.
    Cropped {
        label: String,
    },
    /// A Label on screen whose face points away from the eye: it is seen
    /// from behind, mirrored.
    Backwards {
        label: String,
    },
    /// An interned mesh (a model) whose drawn size is not a plausible
    /// object in a metric scene: authored in centimetres, or never scaled.
    Scale {
        name: String,
        size: f32,
    },
    /// A model drawn away from its node's origin: rotating or scaling the
    /// node swings it around a point that is not on it.
    OffOrigin {
        name: String,
        distance: f32,
    },
    /// A model just above the surface under it: placed by arithmetic and
    /// missed, not hung on purpose (that is higher than [`FLOAT_MAX`]).
    Floating {
        name: String,
        gap: f32,
    },
    /// A model whose base is below the surface under it.
    Sunk {
        name: String,
        depth: f32,
        into: String,
    },
    /// A node with a declared `front=` moved across the ground at an angle
    /// to it: walking backwards or sideways, or turned the wrong way.
    Heading {
        name: String,
        angle: f32,
        front: &'static str,
    },
    /// More triangles than a browser draws comfortably every frame.
    Heavy {
        tris: usize,
    },
}

impl Problem {
    pub fn kind(&self) -> &'static str {
        match self {
            Problem::OffPrint { .. } => "off-print",
            Problem::Missing { .. } => "missing",
            Problem::OffCamera { .. } => "off-camera",
            Problem::Small { .. } => "small",
            Problem::ZFight { .. } => "z-fight",
            Problem::Covered { .. } => "covered",
            Problem::Contrast { .. } => "contrast",
            Problem::Cropped { .. } => "cropped",
            Problem::Backwards { .. } => "backwards",
            Problem::Scale { .. } => "scale",
            Problem::OffOrigin { .. } => "off-origin",
            Problem::Floating { .. } => "floating",
            Problem::Sunk { .. } => "sunk",
            Problem::Heading { .. } => "heading",
            Problem::Heavy { .. } => "heavy",
        }
    }

    /// Whether the problem depends on where the camera is (`off-camera`,
    /// `small`, `covered`, `cropped`, `backwards`), as opposed to the
    /// document alone.
    pub fn of_the_view(&self) -> bool {
        matches!(
            self,
            Problem::OffCamera { .. }
                | Problem::Small { .. }
                | Problem::Covered { .. }
                | Problem::Cropped { .. }
                | Problem::Backwards { .. }
        )
    }

    /// The node the problem is about (the first, for a pair; `scene` for the
    /// whole scene).
    pub fn node(&self) -> &str {
        match self {
            Problem::OffPrint { label, .. }
            | Problem::Missing { label, .. }
            | Problem::Small { label, .. }
            | Problem::Contrast { label, .. }
            | Problem::Cropped { label }
            | Problem::Backwards { label } => label,
            Problem::OffCamera { name }
            | Problem::Covered { name, .. }
            | Problem::Scale { name, .. }
            | Problem::OffOrigin { name, .. }
            | Problem::Floating { name, .. }
            | Problem::Sunk { name, .. }
            | Problem::Heading { name, .. } => name,
            Problem::ZFight { a, .. } => a,
            Problem::Heavy { .. } => "scene",
        }
    }

    /// One line: `KIND NODE detail…`.
    pub fn dump(&self) -> String {
        match self {
            Problem::OffPrint { label, parent, why } => {
                format!("off-print {label} parent={parent} {why}")
            }
            Problem::Missing { label, codepoints } => {
                let cps: Vec<String> = codepoints.iter().map(|c| format!("U+{c:04X}")).collect();
                format!("missing {label} {}", cps.join(","))
            }
            Problem::OffCamera { name } => format!("off-camera {name}"),
            Problem::Small { label, em } => {
                format!("small {label} em={em:.1} min={MIN_TEXT}")
            }
            Problem::ZFight { a, b } => format!("z-fight {a} {b}"),
            Problem::Covered { name, by } => format!("covered {name} by={by}"),
            Problem::Contrast {
                label,
                parent,
                ratio,
            } => format!("contrast {label} parent={parent} ratio={ratio:.2} min={MIN_CONTRAST}"),
            Problem::Cropped { label } => format!("cropped {label} past the edge of the view"),
            Problem::Backwards { label } => {
                format!("backwards {label} is seen from behind (its face points away)")
            }
            Problem::Scale { name, size } => format!(
                "scale {name} is {size:.3} m on its longest side (plausible {MIN_MODEL}–{MAX_MODEL} m)"
            ),
            Problem::OffOrigin { name, distance } => {
                format!("off-origin {name} is drawn {distance:.2} m from its node's origin")
            }
            Problem::Floating { name, gap } => {
                format!("floating {name} is {gap:.2} m above what is under it")
            }
            Problem::Sunk { name, depth, into } => {
                format!("sunk {name} is {depth:.2} m into {into}")
            }
            Problem::Heading { name, angle, front } => {
                format!("heading {name} travels {angle:.0}° away from its front={front}")
            }
            Problem::Heavy { tris } => {
                format!("heavy scene draws {tris} triangles (budget {HEAVY_TRIS})")
            }
        }
    }
}

/// A model's longest side outside this, in metres, is not a plausible object
/// in a metric scene: a centimetre-authored model is 100× too big.
pub const MIN_MODEL: f32 = 0.01;
pub const MAX_MODEL: f32 = 50.0;
/// A browser draws this many triangles a frame comfortably; past it the
/// scene is heavy for the web build.
pub const HEAVY_TRIS: usize = 200_000;
/// A model's base within this of the surface under it rests on it: 2 cm,
/// or 2% of the model's size if that is more.
pub const REST_TOLERANCE: f32 = 0.02;
/// Higher than this above the surface under it, a model is hung or flying
/// on purpose, not `floating`.
pub const FLOAT_MAX: f32 = 0.5;

/// Travel this far off a node's declared front is `heading`: past 60° it
/// is sideways or backwards, not a curve.
pub const MAX_HEADING: f32 = 60.0;
/// Less ground travel than this between two moments is standing still.
const MIN_TRAVEL: f32 = 0.01;

/// Nodes with a declared `front=` that moved across the ground between
/// `before` and `after` (two moments of a clip, or a replay step) at more
/// than [`MAX_HEADING`] to where they face. Facing is read at `after`.
pub fn headings(before: &Scene, after: &Scene) -> Vec<Problem> {
    let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z);
    let mut out = Vec::new();
    for n in after.nodes() {
        let Kind::Mesh {
            front: Some(front), ..
        } = &n.kind
        else {
            continue;
        };
        let (Ok(wa), Ok(wb)) = (before.world_matrix(&n.name), after.world_matrix(&n.name)) else {
            continue;
        };
        let travel = flat(transform_point(wb, Vec3::ZERO) - transform_point(wa, Vec3::ZERO));
        if travel.length() < MIN_TRAVEL {
            continue;
        }
        let facing = flat(crate::math::transform_dir(wb, front.local()));
        if facing.length() < 1e-6 {
            continue;
        }
        let cos = travel
            .normalized()
            .dot(facing.normalized())
            .clamp(-1.0, 1.0);
        let angle = cos.acos().to_degrees();
        if angle > MAX_HEADING {
            out.push(Problem::Heading {
                name: n.name.clone(),
                angle,
                front: front.as_str(),
            });
        }
    }
    out
}

/// What is straight below `from`: the first quad or mesh triangle (not
/// `skip`'s own, not a label) on a ray down, as its name and height.
fn under<'a>(list: &'a DrawList, from: Vec3, skip: &str) -> Option<(&'a str, f32)> {
    let down = Vec3::new(0.0, -1.0, 0.0);
    let mut best: Option<(&str, f32)> = None;
    let mut take = |name: &'a str, a: Vec3, b: Vec3, c: Vec3| {
        if let Some(hit) = crate::math::ray_triangle(from, down, a, b, c) {
            if best.is_none_or(|(_, t)| hit.t < t) {
                best = Some((name, hit.t));
            }
        }
    };
    for q in &list.quads {
        let [p0, p1, p2, p3] = q.corners();
        take(&q.name, p0, p1, p2);
        take(&q.name, p0, p2, p3);
    }
    for m in list.meshes.iter().filter(|m| m.name != skip) {
        for idx in m.indices.as_chunks::<3>().0 {
            let p = |i: u16| m.verts[i as usize].0;
            take(&m.name, p(idx[0]), p(idx[1]), p(idx[2]));
        }
    }
    best.map(|(name, t)| (name, from.y - t))
}

/// Interned meshes (models) by node: drawn size and placement. Stock prims
/// are sized by their node and never judged here.
fn models(scene: &Scene, list: &DrawList, out: &mut Vec<Problem>) {
    let mut seen: Vec<&str> = Vec::new();
    for m in list.meshes.iter().filter(|m| m.prim == Prim::Geom) {
        if seen.contains(&m.name.as_str()) {
            continue;
        }
        seen.push(&m.name);
        let (mut lo, mut hi) = (
            Vec3::new(f32::MAX, f32::MAX, f32::MAX),
            Vec3::new(f32::MIN, f32::MIN, f32::MIN),
        );
        for part in list.meshes.iter().filter(|p| p.name == m.name) {
            for (p, _, _, _) in &part.verts {
                lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
                hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
            }
        }
        if lo.x > hi.x {
            continue;
        }
        let ext = hi - lo;
        let size = ext.x.max(ext.y).max(ext.z);
        if !(MIN_MODEL..=MAX_MODEL).contains(&size) {
            out.push(Problem::Scale {
                name: m.name.clone(),
                size,
            });
        }
        // The origin should be on the model or near it (feet are fine): the
        // bounds grown by a tenth of its size, at least 5 cm.
        let Ok(world) = scene.world_matrix(&m.name) else {
            continue;
        };
        let o = transform_point(world, Vec3::ZERO);
        let pad = (size * 0.1).max(0.05);
        let gap = |v: f32, a: f32, b: f32| (a - pad - v).max(v - b - pad).max(0.0);
        let d = Vec3::new(
            gap(o.x, lo.x, hi.x),
            gap(o.y, lo.y, hi.y),
            gap(o.z, lo.z, hi.z),
        );
        let distance = d.length();
        if distance > 0.0 {
            out.push(Problem::OffOrigin {
                name: m.name.clone(),
                distance,
            });
        }
        // Resting: straight down from the top of its bounds, at the middle of
        // its footprint, the first thing that is not itself. Nothing under it
        // (a model in the air of an empty scene) is not judged.
        let top = Vec3::new((lo.x + hi.x) * 0.5, hi.y + 1e-3, (lo.z + hi.z) * 0.5);
        if let Some((support, y)) = under(list, top, &m.name) {
            let tol = REST_TOLERANCE.max(size * 0.02);
            let gap = lo.y - y;
            if gap > tol && gap <= FLOAT_MAX {
                out.push(Problem::Floating {
                    name: m.name.clone(),
                    gap,
                });
            } else if gap < -tol {
                out.push(Problem::Sunk {
                    name: m.name.clone(),
                    depth: -gap,
                    into: support.to_string(),
                });
            }
        }
    }
    let tris = list.quads.len() * 2
        + list
            .meshes
            .iter()
            .map(|m| m.indices.len() / 3)
            .sum::<usize>()
        + list
            .labels
            .iter()
            .map(|l| l.glyphs.len() * 2)
            .sum::<usize>();
    if tris > HEAVY_TRIS {
        out.push(Problem::Heavy { tris });
    }
}

/// Labels on screen but partly past its edge: in front of the eye, not
/// wholly outside (that is `off-camera`), and a corner beyond ±1 ndc.
fn cropped(list: &DrawList, vp: Mat4, out: &mut Vec<Problem>) {
    for l in &list.labels {
        let corners = l.corners();
        let projected: Option<Vec<(f32, f32)>> = corners.iter().map(|c| ndc(vp, *c)).collect();
        let Some(projected) = projected else {
            continue;
        };
        if outside(vp, &corners) {
            continue;
        }
        if projected
            .iter()
            .any(|(x, y)| x.abs() > 1.0 || y.abs() > 1.0)
        {
            out.push(Problem::Cropped {
                label: l.name.clone(),
            });
        }
    }
}

/// Labels on screen facing away from the eye. A label with no visible
/// glyphs or fading out is skipped, as for contrast.
fn backwards(list: &DrawList, vp: Mat4, out: &mut Vec<Problem>) {
    let eye = list.camera.eye;
    for l in &list.labels {
        if l.glyphs.is_empty() || l.color[3] < 128 || outside(vp, &l.corners()) {
            continue;
        }
        if l.normal.dot(eye - l.centre) <= 0.0 {
            out.push(Problem::Backwards {
                label: l.name.clone(),
            });
        }
    }
}

/// Every problem in `scene` as drawn at `aspect`. `controls` are the tags a
/// pane treats as pickable controls (`cell`, `hand`); each must be pickable
/// somewhere on its face. Order is stable: by kind, then dump order.
pub fn problems(scene: &Scene, list: &DrawList, aspect: f32, controls: &[&str]) -> Vec<Problem> {
    let vp = list.view_proj(aspect);
    let mut out = Vec::new();
    off_print(scene, list, &mut out);
    for lab in &list.labels {
        if !lab.missing.is_empty() {
            out.push(Problem::Missing {
                label: lab.name.clone(),
                codepoints: lab.missing.clone(),
            });
        }
    }
    off_camera(list, vp, &mut out);
    small(scene, list, vp, aspect, &mut out);
    z_fight(list, &mut out);
    covered(scene, list, vp, aspect, controls, &mut out);
    low_contrast(scene, list, &mut out);
    cropped(list, vp, &mut out);
    backwards(list, vp, &mut out);
    models(scene, list, &mut out);
    out
}

/// A visible Label against the face it sits on, when that face is a plain
/// colour (a texture's pixels are not in the document, so they are skipped).
fn low_contrast(scene: &Scene, list: &DrawList, out: &mut Vec<Problem>) {
    for lab in &list.labels {
        // Fading in or out: an animation state, not a resting look.
        if lab.color[3] < 128 {
            continue;
        }
        let Some(parent) = scene.get(&lab.name).and_then(|n| n.parent.as_deref()) else {
            continue;
        };
        let face = list
            .quads
            .iter()
            .find(|q| q.name == parent)
            .map(|q| (q.color, q.image.is_some()))
            .or_else(|| {
                list.meshes
                    .iter()
                    .find(|m| m.name == parent)
                    .map(|m| (m.color, m.image.is_some()))
            });
        let Some((face, textured)) = face else {
            continue;
        };
        if textured {
            continue;
        }
        let ratio = contrast(lab.color, face);
        if ratio < MIN_CONTRAST {
            out.push(Problem::Contrast {
                label: lab.name.clone(),
                parent: parent.to_string(),
                ratio,
            });
        }
    }
}

/// Labels on a textured Quad, against the texels under them: the pixels
/// are on the CPU, so a white sign on a pale photo is judged as a plain face
/// is. The texel colour is tinted by the face's `c=`, as it is drawn;
/// transparent texels are skipped. A face whose image is not in `images` is
/// not judged ([`image_census`] says which).
pub fn textured_contrast(scene: &Scene, list: &DrawList, images: &Images) -> Vec<Problem> {
    let mut out = Vec::new();
    for lab in &list.labels {
        if lab.color[3] < 128 {
            continue;
        }
        let Some(parent) = scene.get(&lab.name).and_then(|n| n.parent.as_deref()) else {
            continue;
        };
        let Some(q) = list.quads.iter().find(|q| q.name == parent) else {
            continue;
        };
        let Some(img) = q.image.as_deref().and_then(|k| images.get(k)) else {
            continue;
        };
        // The label's footprint in the face's UV.
        let (ax2, ay2) = (q.axis_x.dot(q.axis_x), q.axis_y.dot(q.axis_y));
        if ax2 < 1e-12 || ay2 < 1e-12 {
            continue;
        }
        let (mut u0, mut u1, mut v0, mut v1) = (1.0f32, 0.0f32, 1.0f32, 0.0f32);
        for p in lab.corners() {
            let d = p - q.centre;
            let u = ((d.dot(q.axis_x) / ax2 + 1.0) * 0.5).clamp(0.0, 1.0);
            let v = ((1.0 - d.dot(q.axis_y) / ay2) * 0.5).clamp(0.0, 1.0);
            (u0, u1, v0, v1) = (u0.min(u), u1.max(u), v0.min(v), v1.max(v));
        }
        let Some(under) = mean_texel(img, [u0, u1], [v0, v1]) else {
            continue;
        };
        let tint = |c: u8, t: u8| ((c as u32 * t as u32) / 255) as u8;
        let face = [
            tint(under[0], q.color[0]),
            tint(under[1], q.color[1]),
            tint(under[2], q.color[2]),
            255,
        ];
        let ratio = contrast(lab.color, face);
        if ratio < MIN_CONTRAST {
            out.push(Problem::Contrast {
                label: lab.name.clone(),
                parent: parent.to_string(),
                ratio,
            });
        }
    }
    out
}

/// The mean opaque colour of `img` over a UV rectangle, on an 8×8 grid of
/// nearest texels; `None` if all of them are transparent.
fn mean_texel(img: &crate::images::RgbaImage, u: [f32; 2], v: [f32; 2]) -> Option<[u8; 3]> {
    const N: usize = 8;
    let (w, h) = (img.width as usize, img.height as usize);
    if w == 0 || h == 0 {
        return None;
    }
    let mut sum = [0u64; 3];
    let mut n = 0u64;
    for i in 0..N {
        for j in 0..N {
            let uu = u[0] + (u[1] - u[0]) * (i as f32 + 0.5) / N as f32;
            let vv = v[0] + (v[1] - v[0]) * (j as f32 + 0.5) / N as f32;
            let x = ((uu * w as f32) as usize).min(w - 1);
            let y = ((vv * h as f32) as usize).min(h - 1);
            let px = &img.pixels[(y * w + x) * 4..(y * w + x) * 4 + 4];
            if px[3] <= crate::ALPHA_CUTOFF {
                continue;
            }
            for c in 0..3 {
                sum[c] += px[c] as u64;
            }
            n += 1;
        }
    }
    (n > 0).then(|| sum.map(|s| (s / n) as u8))
}

/// Which image keys `scene` names (`m=`, `mb=`, materials), split into
/// those `images` holds, remote asset prints (`policy:hex`, fetched by the
/// pane), and absent ones — drawn untextured unless a client supplies them,
/// and never judged here. One line: `images local=N remote=N absent=a,b`.
pub fn image_census(list: &DrawList, images: &Images) -> String {
    let mut keys: Vec<&str> = list
        .quads
        .iter()
        .flat_map(|q| q.image.as_deref())
        .chain(list.meshes.iter().flat_map(|m| {
            m.image
                .as_deref()
                .into_iter()
                .chain(m.image_back.as_deref())
        }))
        .collect();
    keys.sort();
    keys.dedup();
    let (mut local, mut remote, mut absent) = (0, 0, Vec::new());
    for k in keys {
        if images.get(k).is_some() {
            local += 1;
        } else if k.contains(':') {
            remote += 1;
        } else {
            absent.push(k);
        }
    }
    let absent = if absent.is_empty() {
        "-".to_string()
    } else {
        absent.join(",")
    };
    format!("images local={local} remote={remote} absent={absent}\n")
}

/// What [`problems`] looked at, so `problems 0` can be told apart from
/// "nothing was checked". Adds across scenes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Coverage {
    /// Scenes checked.
    pub scenes: usize,
    /// Labels (`missing`, `small`, `off-camera`, `z-fight`).
    pub labels: usize,
    /// Labels on a Quad or Slab face (`off-print`).
    pub prints: usize,
    /// Faces carrying a control tag (`covered`).
    pub controls: usize,
    /// Quads, meshes and labels (`off-camera`).
    pub drawables: usize,
}

impl Coverage {
    /// `checked scenes=… labels=… prints=… controls=… drawables=…`.
    pub fn dump(&self) -> String {
        format!(
            "checked scenes={} labels={} prints={} controls={} drawables={}\n",
            self.scenes, self.labels, self.prints, self.controls, self.drawables
        )
    }

    pub fn add(&mut self, o: Coverage) {
        self.scenes += o.scenes;
        self.labels += o.labels;
        self.prints += o.prints;
        self.controls += o.controls;
        self.drawables += o.drawables;
    }
}

/// What [`problems`] checks in one scene.
pub fn coverage(scene: &Scene, list: &DrawList, controls: &[&str]) -> Coverage {
    let prints = list
        .labels
        .iter()
        .filter(|l| {
            scene
                .get(&l.name)
                .and_then(|n| n.parent.as_deref())
                .and_then(|p| scene.get(p))
                .is_some_and(|p| {
                    matches!(
                        p.kind,
                        Kind::Quad { .. }
                            | Kind::Mesh {
                                prim: Prim::Slab { .. },
                                ..
                            }
                    )
                })
        })
        .count();
    let is_control = |name: &str| {
        scene
            .get(name)
            .is_some_and(|n| n.tags.iter().any(|t| controls.contains(&t.as_str())))
    };
    let controls = list.quads.iter().filter(|q| is_control(&q.name)).count()
        + list.meshes.iter().filter(|m| is_control(&m.name)).count();
    Coverage {
        scenes: 1,
        labels: list.labels.len(),
        prints,
        controls,
        drawables: list.quads.len() + list.meshes.len() + list.labels.len(),
    }
}

/// `problems N`, then one line each.
pub fn dump(problems: &[Problem]) -> String {
    let mut out = format!("problems {}\n", problems.len());
    for p in problems {
        out.push_str(&p.dump());
        out.push('\n');
    }
    out
}

fn off_print(scene: &Scene, list: &DrawList, out: &mut Vec<Problem>) {
    for lab in &list.labels {
        let Some(parent) = scene.get(&lab.name).and_then(|n| n.parent.as_deref()) else {
            continue;
        };
        let Some(p) = scene.get(parent) else {
            continue;
        };
        // The face a print sits on: local XY, ±w/2 × ±h/2, front at +z.
        let (w, h, front) = match &p.kind {
            Kind::Quad { w, h, .. } => (*w, *h, 0.0),
            Kind::Mesh {
                prim: Prim::Slab { w, h, depth, .. },
                ..
            } => (*w, *h, depth * 0.5),
            _ => continue,
        };
        let Some(inv) = scene.world_matrix(parent).ok().and_then(mat4_inverse) else {
            continue;
        };
        let local: Vec<Vec3> = lab
            .corners()
            .iter()
            .map(|c| transform_point(inv, *c))
            .collect();
        let why = if local.iter().any(|c| c.z <= front - EDGE_EPS) {
            Some(if local.iter().all(|c| c.z < 0.0) {
                "behind"
            } else {
                "inside the stock"
            })
        } else if local
            .iter()
            .any(|c| c.x.abs() > w * 0.5 + EDGE_EPS || c.y.abs() > h * 0.5 + EDGE_EPS)
        {
            Some("spills past the face")
        } else {
            None
        };
        if let Some(why) = why {
            out.push(Problem::OffPrint {
                label: lab.name.clone(),
                parent: parent.to_string(),
                why,
            });
        }
    }
}

/// `p` in clip space as ndc, or `None` behind the eye.
fn ndc(vp: Mat4, p: Vec3) -> Option<(f32, f32)> {
    let c = transform_vec4(vp, [p.x, p.y, p.z, 1.0]);
    if c[3] <= 1e-6 {
        return None;
    }
    Some((c[0] / c[3], c[1] / c[3]))
}

/// Wholly outside: every point behind the eye, or all past one edge.
fn outside(vp: Mat4, points: &[Vec3]) -> bool {
    let projected: Vec<Option<(f32, f32)>> = points.iter().map(|p| ndc(vp, *p)).collect();
    if projected.iter().all(Option::is_none) {
        return true;
    }
    let front: Vec<(f32, f32)> = projected.into_iter().flatten().collect();
    // Points behind the eye can wrap; only judge by the ones in front when
    // every point is in front.
    if front.len() != points.len() {
        return false;
    }
    front.iter().all(|p| p.0 < -1.0)
        || front.iter().all(|p| p.0 > 1.0)
        || front.iter().all(|p| p.1 < -1.0)
        || front.iter().all(|p| p.1 > 1.0)
}

fn off_camera(list: &DrawList, vp: Mat4, out: &mut Vec<Problem>) {
    for q in &list.quads {
        if outside(vp, &q.corners()) {
            out.push(Problem::OffCamera {
                name: q.name.clone(),
            });
        }
    }
    for m in &list.meshes {
        let pts: Vec<Vec3> = m.verts.iter().map(|v| v.0).collect();
        if !pts.is_empty() && outside(vp, &pts) {
            out.push(Problem::OffCamera {
                name: m.name.clone(),
            });
        }
    }
    for l in &list.labels {
        if outside(vp, &l.corners()) {
            out.push(Problem::OffCamera {
                name: l.name.clone(),
            });
        }
    }
}

fn small(scene: &Scene, list: &DrawList, vp: Mat4, aspect: f32, out: &mut Vec<Problem>) {
    for lab in &list.labels {
        let Some(Kind::Label { size, .. }) = scene.get(&lab.name).map(|n| &n.kind) else {
            continue;
        };
        if lab.h <= 0.0 || outside(vp, &lab.corners()) {
            continue;
        }
        // World em: the layout box is `h` local units tall and `2·|axis_y|`
        // world units tall, so one em is `size · 2|axis_y| / h` along it.
        let up = lab.axis_y;
        let em_world = size * 2.0 * up.length() / lab.h;
        let tip = lab.centre + up.normalized() * em_world;
        let (Some(a), Some(b)) = (ndc(vp, lab.centre), ndc(vp, tip)) else {
            continue;
        };
        let dx = (b.0 - a.0) * REF_HEIGHT * aspect * 0.5;
        let dy = (b.1 - a.1) * REF_HEIGHT * 0.5;
        let em = (dx * dx + dy * dy).sqrt();
        if em < MIN_TEXT {
            out.push(Problem::Small {
                label: lab.name.clone(),
                em,
            });
        }
    }
}

/// A flat rectangle in world space: centre, half-extent axes, unit normal.
struct Flat<'a> {
    name: &'a str,
    centre: Vec3,
    x: Vec3,
    y: Vec3,
    n: Vec3,
}

impl Flat<'_> {
    fn corners(&self) -> [Vec3; 4] {
        let (c, x, y) = (self.centre, self.x, self.y);
        [c - x - y, c + x - y, c + x + y, c - x + y]
    }
}

fn flat_quad(q: &WorldQuad) -> Flat<'_> {
    Flat {
        name: &q.name,
        centre: q.centre,
        x: q.axis_x,
        y: q.axis_y,
        n: q.normal,
    }
}

fn flat_label(l: &WorldLabel) -> Flat<'_> {
    Flat {
        name: &l.name,
        centre: l.centre,
        x: l.axis_x,
        y: l.axis_y,
        n: l.normal,
    }
}

/// Separating axis in the shared plane: true when the two rectangles share
/// area (touching edges do not count).
fn overlap_in_plane(a: &Flat<'_>, b: &Flat<'_>) -> bool {
    let u = a.x.normalized();
    let v = a.n.cross(u).normalized();
    let to2 = |p: Vec3| ((p - a.centre).dot(u), (p - a.centre).dot(v));
    let pa: Vec<(f32, f32)> = a.corners().iter().map(|p| to2(*p)).collect();
    let pb: Vec<(f32, f32)> = b.corners().iter().map(|p| to2(*p)).collect();
    for poly in [&pa, &pb] {
        for i in 0..4 {
            let (x0, y0) = poly[i];
            let (x1, y1) = poly[(i + 1) % 4];
            let axis = (y0 - y1, x1 - x0);
            let len = (axis.0 * axis.0 + axis.1 * axis.1).sqrt();
            if len < 1e-9 {
                continue;
            }
            let proj = |pts: &[(f32, f32)]| {
                pts.iter()
                    .map(|p| (p.0 * axis.0 + p.1 * axis.1) / len)
                    .fold((f32::MAX, f32::MIN), |(lo, hi), d| (lo.min(d), hi.max(d)))
            };
            let (alo, ahi) = proj(&pa);
            let (blo, bhi) = proj(&pb);
            if ahi <= blo + EDGE_EPS || bhi <= alo + EDGE_EPS {
                return false;
            }
        }
    }
    true
}

fn z_fight(list: &DrawList, out: &mut Vec<Problem>) {
    let flats: Vec<Flat<'_>> = list
        .quads
        .iter()
        .map(flat_quad)
        .chain(list.labels.iter().map(flat_label))
        .collect();
    for (i, a) in flats.iter().enumerate() {
        for b in &flats[i + 1..] {
            if a.n.dot(b.n).abs() < 0.9999 {
                continue;
            }
            if a.n.dot(b.centre - a.centre).abs() > PLANE_EPS {
                continue;
            }
            if overlap_in_plane(a, b) {
                out.push(Problem::ZFight {
                    a: a.name.to_string(),
                    b: b.name.to_string(),
                });
            }
        }
    }
}

/// The control a pick on `hit` selects: a Label selects its parent, as the
/// pointer session does.
fn selected<'a>(scene: &'a Scene, hit: &'a str) -> &'a str {
    match scene.get(hit) {
        Some(n) if matches!(n.kind, Kind::Label { .. }) => n.parent.as_deref().unwrap_or(hit),
        _ => hit,
    }
}

fn is_self_or_below(scene: &Scene, name: &str, control: &str) -> bool {
    let mut at = Some(name);
    let mut hops = 0;
    while let Some(n) = at {
        if n == control {
            return true;
        }
        hops += 1;
        if hops > scene.len() {
            return false;
        }
        at = scene.get(n).and_then(|node| node.parent.as_deref());
    }
    false
}

fn covered(
    scene: &Scene,
    list: &DrawList,
    vp: Mat4,
    aspect: f32,
    controls: &[&str],
    out: &mut Vec<Problem>,
) {
    if controls.is_empty() {
        return;
    }
    // A 5×5 grid over a face, inset from the edges.
    const T: [f32; 5] = [-0.8, -0.4, 0.0, 0.4, 0.8];
    let grid = |centre: Vec3, ax: Vec3, ay: Vec3| -> Vec<Vec3> {
        T.iter()
            .flat_map(|&u| T.iter().map(move |&v| centre + ax * u + ay * v))
            .collect()
    };
    // Points to try per control, in first-seen order; a model's parts are
    // one control. A stock face is gridded; an interned mesh (a model) has
    // no face, so its own vertices are sampled, pulled in toward its centre
    // off the silhouette.
    let mut targets: Vec<(String, Vec<Vec3>)> = Vec::new();
    let mut add = |name: &str, points: Vec<Vec3>| match targets.iter_mut().find(|(n, _)| n == name)
    {
        Some((_, ps)) => ps.extend(points),
        None => targets.push((name.to_string(), points)),
    };
    for q in &list.quads {
        add(&q.name, grid(q.centre, q.axis_x, q.axis_y));
    }
    for m in &list.meshes {
        if m.prim != Prim::Geom || m.verts.is_empty() {
            add(&m.name, grid(m.centre, m.axis_x, m.axis_y));
            continue;
        }
        let n = m.verts.len() as f32;
        let mid = m.verts.iter().fold(Vec3::ZERO, |a, v| a + v.0) * (1.0 / n);
        let step = (m.verts.len() / 24).max(1);
        let mut points = vec![mid];
        points.extend(
            m.verts
                .iter()
                .step_by(step)
                .map(|v| v.0 + (mid - v.0) * 0.2),
        );
        add(&m.name, points);
    }
    for (name, points) in &targets {
        let name = name.as_str();
        let tagged = scene
            .get(name)
            .is_some_and(|n| n.tags.iter().any(|t| controls.contains(&t.as_str())));
        if !tagged {
            continue;
        }
        let mut seen_by = None;
        let mut pickable = false;
        let mut on_screen = false;
        for p in points {
            let Some((x, y)) = ndc(vp, *p) else {
                continue;
            };
            if x.abs() > 1.0 || y.abs() > 1.0 {
                continue;
            }
            on_screen = true;
            match list.hit_ndc(x, y, aspect) {
                Some(h) if is_self_or_below(scene, selected(scene, h.name), name) => {
                    pickable = true;
                    break;
                }
                Some(h) => seen_by = seen_by.or_else(|| Some(selected(scene, h.name).to_string())),
                None => {}
            }
        }
        // Wholly off screen is `off-camera`'s to report, not this.
        if on_screen && !pickable {
            out.push(Problem::Covered {
                name: name.to_string(),
                by: seen_by.unwrap_or_else(|| "-".to_string()),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Fonts, Geometry, Materials};

    fn fonts() -> Fonts {
        let mut f = Fonts::new();
        f.insert("ibm", crate::NOTO_SANS_BOLD, Vec::new()).unwrap();
        f
    }

    fn found(src: &str, controls: &[&str]) -> Vec<String> {
        found_with(src, &Geometry::new(), controls)
    }

    fn found_with(src: &str, geom: &Geometry, controls: &[&str]) -> Vec<String> {
        let scene = Scene::from_dump(src).unwrap();
        let mut f = fonts();
        let list = scene
            .draw_list_with(geom, &Materials::new(), &mut f)
            .unwrap();
        problems(&scene, &list, 1.5, controls)
            .iter()
            .map(Problem::dump)
            .collect()
    }

    /// A one-part model: the stock unit cube, moved by `offset` in its own
    /// frame, with `extra` repeats of its triangles (for the budget).
    fn cube_model(offset: Vec3, extra: usize) -> Geometry {
        let cube = crate::mesh::tessellate(Prim::Box);
        let verts = cube
            .verts
            .iter()
            .map(|v| crate::LocalVert {
                pos: v.pos + offset,
                ..*v
            })
            .collect();
        let mut indices = cube.indices.clone();
        for _ in 0..extra {
            indices.extend_from_slice(&cube.indices);
        }
        let mut g = Geometry::new();
        g.insert_model(
            "cube",
            vec![crate::ModelPart {
                tess: std::sync::Arc::new(crate::Tessellation {
                    verts,
                    indices,
                    rim_index: None,
                    back_index: None,
                }),
                surface: crate::Surface::DEFAULT,
                tiled: 0,
                skin: None,
            }],
        )
        .unwrap();
        g
    }

    const MODEL: &str = "sg1
thing Mesh Geom g=cube
cam Camera eye=0,0,6 at=0,0,0
";

    #[test]
    fn a_model_on_its_origin_at_a_sane_size_is_fine() {
        assert_eq!(
            found_with(MODEL, &cube_model(Vec3::ZERO, 0), &[]),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_model_drawn_away_from_its_origin_is_off_origin() {
        let got = found_with(MODEL, &cube_model(Vec3::new(3.0, 0.0, 0.0), 0), &[]);
        assert_eq!(got.len(), 1, "{got:?}");
        assert!(
            got[0].starts_with("off-origin thing is drawn 2.40 m"),
            "{got:?}"
        );
    }

    #[test]
    fn a_centimetre_model_left_unscaled_is_too_big() {
        let src = MODEL.replace("g=cube", "g=cube s=155,155,155");
        let got = found_with(&src, &cube_model(Vec3::ZERO, 0), &[]);
        assert!(
            got.iter()
                .any(|l| l.starts_with("scale thing is 155.000 m")),
            "{got:?}"
        );
    }

    /// The unit cube's base is at y=-0.5 in its own frame: `t=0,0.5,0` sets
    /// it on the floor, a little higher floats, lower sinks, and a lot
    /// higher is hung on purpose.
    #[test]
    fn a_model_rests_on_what_is_under_it() {
        let at = |y: &str| {
            format!(
                "sg1\nfloor Quad 6x6 c=556655 r=-1.5708,0,0\nthing Mesh Geom g=cube t=0,{y},0\ncam Camera eye=0,2,6 at=0,0.5,0\n"
            )
        };
        let geom = cube_model(Vec3::ZERO, 0);
        assert_eq!(found_with(&at("0.5"), &geom, &[]), Vec::<String>::new());
        assert_eq!(found_with(&at("0.51"), &geom, &[]), Vec::<String>::new());
        assert_eq!(
            found_with(&at("0.62"), &geom, &[]),
            ["floating thing is 0.12 m above what is under it"]
        );
        assert_eq!(
            found_with(&at("0.3"), &geom, &[]),
            ["sunk thing is 0.20 m into floor"]
        );
        assert_eq!(found_with(&at("2.5"), &geom, &[]), Vec::<String>::new());
    }

    /// A node facing +Z that moves +Z is walking forward; moving -Z it is
    /// walking backwards, and +X is sideways. Without `front=`, nothing.
    #[test]
    fn travel_against_a_declared_front_is_heading() {
        let at = |front: &str, t: &str| {
            Scene::from_dump(&format!("sg1\nfox Mesh Geom g=fox{front} t={t}\n")).unwrap()
        };
        let found =
            |a: &Scene, b: &Scene| headings(a, b).iter().map(Problem::dump).collect::<Vec<_>>();
        let start = at(" front=+z", "0,0,0");
        let to = |t: &str| at(" front=+z", t);
        assert_eq!(found(&start, &to("0,0,0.5")), Vec::<String>::new());
        assert_eq!(
            found(&start, &to("0,0,-0.5")),
            ["heading fox travels 180° away from its front=+z"]
        );
        assert_eq!(
            found(&start, &to("0.5,0,0")),
            ["heading fox travels 90° away from its front=+z"]
        );
        assert_eq!(
            found(&start, &to("0,0,0.005")),
            Vec::<String>::new(),
            "standing still"
        );
        assert_eq!(
            found(&at("", "0,0,0"), &at("", "0,0,-1")),
            Vec::<String>::new(),
            "no front, no check"
        );
        assert!(
            Scene::from_dump("sg1\nbox Mesh Box front=+z\n").is_err(),
            "front is for models"
        );
    }

    #[test]
    fn too_many_triangles_is_heavy() {
        let got = found_with(MODEL, &cube_model(Vec3::ZERO, 17_000), &[]);
        assert!(
            got.iter()
                .any(|l| l.starts_with("heavy scene draws 204012")),
            "{got:?}"
        );
    }

    #[test]
    fn a_label_past_the_edge_of_the_view_is_cropped() {
        let src = format!("{BOARD}edge Label font=ibm size=0.3 \"Off the edge\" t=2.6,0,0\n");
        let got = found(&src, &[]);
        assert_eq!(got, ["cropped edge past the edge of the view"]);
    }

    /// The same sign from in front reads; turned half round, it is seen
    /// from behind.
    #[test]
    fn a_label_seen_from_behind_is_backwards() {
        let sign =
            |r: &str| format!("{BOARD}sign Label font=ibm size=0.3 \"Exit\" t=0,1,0 r=0,{r},0\n");
        assert_eq!(found(&sign("0"), &[]), Vec::<String>::new());
        assert_eq!(
            found(&sign("3.1416"), &[]),
            ["backwards sign is seen from behind (its face points away)"]
        );
    }

    /// White ink on a white-pixelled face fails; on a dark-pixelled one it
    /// reads. The face's own `c=` is white either way: only the texels say.
    #[test]
    fn a_label_is_judged_against_the_texels_under_it() {
        let mut images = Images::new();
        images.insert_solid("pale", [240, 240, 235]).unwrap();
        images.insert_solid("dark", [30, 30, 40]).unwrap();
        let judge = |key: &str| {
            let scene = Scene::from_dump(&format!(
                "sg1\nsign Quad 2x1 m={key}\nink Label font=ibm size=0.3 \"Exit\" p=sign t=0,0,0.01\ncam Camera eye=0,0,4 at=0,0,0\n"
            ))
            .unwrap();
            let mut f = fonts();
            let list = scene
                .draw_list_with(&Geometry::new(), &Materials::new(), &mut f)
                .unwrap();
            let found: Vec<String> = textured_contrast(&scene, &list, &images)
                .iter()
                .map(Problem::dump)
                .collect();
            (found, image_census(&list, &images))
        };
        let (pale, census) = judge("pale");
        assert_eq!(pale.len(), 1, "{pale:?}");
        assert!(
            pale[0].starts_with("contrast ink parent=sign ratio=1.1"),
            "{pale:?}"
        );
        assert_eq!(census, "images local=1 remote=0 absent=-\n");
        assert_eq!(judge("dark").0, Vec::<String>::new());
        let (unjudged, census) = judge("nowhere");
        assert_eq!(
            unjudged,
            Vec::<String>::new(),
            "absent pixels are not judged"
        );
        assert_eq!(census, "images local=0 remote=0 absent=nowhere\n");
    }

    // A dark cell, so the white test labels on it read (contrast).
    const BOARD: &str = "sg1
b Group
cell Quad 1x1 sh=Unlit c=2a2a2a p=b tag=cell
cam Camera eye=0,0,4 at=0,0,0
";

    #[test]
    fn a_clean_board_has_none() {
        let src = format!("{BOARD}mark Label font=ibm size=0.5 \"X\" p=cell t=0,0,0.05\n");
        assert_eq!(found(&src, &["cell"]), Vec::<String>::new());
    }

    #[test]
    fn a_label_wider_than_its_cell_spills() {
        let src = format!("{BOARD}mark Label font=ibm size=0.5 \"WIDE TEXT\" p=cell t=0,0,0.05\n");
        assert_eq!(
            found(&src, &[]),
            ["off-print mark parent=cell spills past the face"]
        );
    }

    #[test]
    fn a_label_behind_its_cell_is_off_print() {
        let src = format!("{BOARD}mark Label font=ibm size=0.5 \"X\" p=cell t=0,0,-0.05\n");
        assert_eq!(found(&src, &[]), ["off-print mark parent=cell behind"]);
    }

    #[test]
    fn a_label_buried_in_slab_stock_is_off_print() {
        let src = "sg1
card Mesh Slab 2x3 c=2a2a2a d=0.2
title Label font=ibm size=0.3 \"Hi\" p=card t=0,0,0.05
cam Camera eye=0,0,6 at=0,0,0
";
        assert_eq!(
            found(src, &[]),
            ["off-print title parent=card inside the stock"]
        );
    }

    #[test]
    fn missing_glyphs_name_their_codepoints() {
        let src = format!("{BOARD}mark Label font=ibm size=0.5 \"中\" p=cell t=0,0,0.05\n");
        assert!(found(&src, &[]).contains(&"missing mark U+4E2D".to_string()));
    }

    #[test]
    fn a_quad_behind_the_camera_is_off_camera() {
        let src = format!("{BOARD}lost Quad 1x1 t=0,0,9\n");
        assert_eq!(found(&src, &[]), ["off-camera lost"]);
    }

    #[test]
    fn a_partly_visible_backdrop_is_fine() {
        let src = format!("{BOARD}void Quad 40x40 t=0,0,-2\n");
        assert_eq!(found(&src, &[]), Vec::<String>::new());
    }

    #[test]
    fn tiny_far_text_is_small() {
        let src = format!("{BOARD}note Label font=ibm size=0.02 \"hi\" t=0,0.8,0\n");
        let got = found(&src, &[]);
        assert_eq!(got.len(), 1, "{got:?}");
        assert!(got[0].starts_with("small note em="), "{got:?}");
    }

    #[test]
    fn coplanar_overlap_fights_but_a_gap_or_an_edge_does_not() {
        let fight = format!("{BOARD}twin Quad 1x1 t=0.5,0,0\n");
        assert_eq!(found(&fight, &[]), ["z-fight cell twin"]);
        let lifted = format!("{BOARD}twin Quad 1x1 t=0.5,0,0.01\n");
        assert_eq!(found(&lifted, &[]), Vec::<String>::new());
        let edge = format!("{BOARD}twin Quad 1x1 t=1,0,0\n");
        assert_eq!(found(&edge, &[]), Vec::<String>::new());
    }

    #[test]
    fn a_control_under_a_lid_is_covered() {
        let src = format!("{BOARD}lid Quad 2x2 t=0,0,0.1\n");
        assert_eq!(found(&src, &["cell"]), ["covered cell by=lid"]);
        assert_eq!(found(&src, &[]), Vec::<String>::new());
    }

    #[test]
    fn pale_text_on_a_pale_face_is_low_contrast() {
        let card = "sg1
card Mesh Slab 2x3 c=efe6d8
cam Camera eye=0,0,6 at=0,0,0
";
        let white = format!("{card}t Label font=ibm size=0.3 \"Hi\" p=card t=0,0,0.05\n");
        let got = found(&white, &[]);
        assert_eq!(got.len(), 1, "{got:?}");
        assert!(
            got[0].starts_with("contrast t parent=card ratio=1."),
            "{got:?}"
        );
        let dark = format!("{card}t Label font=ibm size=0.3 \"Hi\" c=1a1210 p=card t=0,0,0.05\n");
        assert_eq!(found(&dark, &[]), Vec::<String>::new());
    }

    #[test]
    fn textured_and_fading_labels_are_not_judged() {
        let src = "sg1
card Mesh Slab 2x3 m=art
a Label font=ibm size=0.3 \"Hi\" p=card t=0,0.6,0.05
cam Camera eye=0,0,6 at=0,0,0
";
        assert_eq!(found(src, &[]), Vec::<String>::new());
        let fading = "sg1
card Mesh Slab 2x3 c=efe6d8
a Label font=ibm size=0.3 \"Hi\" c=ffffff40 p=card t=0,0,0.05
cam Camera eye=0,0,6 at=0,0,0
";
        assert_eq!(found(fading, &[]), Vec::<String>::new());
    }

    #[test]
    fn ink_for_picks_the_readable_side() {
        assert_eq!(ink_for([0xef, 0xe6, 0xd8, 255])[0], 0x1a);
        assert_eq!(ink_for([0x1b, 0x3a, 0x2c, 255])[0], 255);
        assert!(contrast([0, 0, 0, 255], [255, 255, 255, 255]) > 20.9);
    }

    #[test]
    fn a_control_partly_under_another_is_still_pickable() {
        let src = format!("{BOARD}lid Quad 1x1 t=0.6,0,0.1\n");
        assert_eq!(found(&src, &["cell"]), Vec::<String>::new());
    }
}
