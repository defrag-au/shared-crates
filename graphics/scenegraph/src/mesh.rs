//! Local tessellation of [`Prim`]. World transform is applied at flatten.

use crate::kind::Prim;
use crate::math::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct LocalVert {
    pub pos: Vec3,
    pub nrm: Vec3,
    pub uv: [f32; 2],
    /// Multiplies the mesh colour. Rim stock is not white so prints stay off the edge.
    pub rgb: [f32; 3],
}

impl LocalVert {
    pub const WHITE: [f32; 3] = [1.0, 1.0, 1.0];
    /// Closed cardboard for slab rims. Not a dump key.
    pub const STOCK: [f32; 3] = [0.58, 0.44, 0.32];

    pub fn new(pos: Vec3, nrm: Vec3, uv: [f32; 2]) -> Self {
        Self {
            pos,
            nrm,
            uv,
            rgb: Self::WHITE,
        }
    }

    fn stock(pos: Vec3, nrm: Vec3) -> Self {
        Self {
            pos,
            nrm,
            uv: [0.0, 0.0],
            rgb: Self::STOCK,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Tessellation {
    pub verts: Vec<LocalVert>,
    pub indices: Vec<u16>,
    /// First index of rim (edge) triangles, if the prim splits print / stock.
    pub rim_index: Option<u32>,
    /// First index of the back-face triangles, if the prim has a separate back.
    pub back_index: Option<u32>,
}

pub fn tessellate(prim: Prim) -> Tessellation {
    match prim {
        Prim::Plane => pack(plane()),
        Prim::Box => pack(cube()),
        Prim::Geom => Tessellation {
            verts: Vec::new(),
            indices: Vec::new(),
            rim_index: None,
            back_index: None,
        },
        Prim::Sphere => pack(sphere()),
        Prim::Slab {
            w,
            h,
            depth,
            radius,
        } => slab(w, h, depth, radius),
    }
}

/// The key of a stock prim's shape: its variant and its parameters' bits.
fn shape_key(prim: Prim) -> ShapeKey {
    match prim {
        Prim::Plane => (0, [0; 4]),
        Prim::Box => (1, [0; 4]),
        Prim::Geom => (2, [0; 4]),
        Prim::Sphere => (3, [0; 4]),
        Prim::Slab {
            w,
            h,
            depth,
            radius,
        } => (
            4,
            [w.to_bits(), h.to_bits(), depth.to_bits(), radius.to_bits()],
        ),
    }
}

type ShapeKey = (u8, [u32; 4]);
type Shapes = std::collections::HashMap<ShapeKey, std::sync::Arc<Tessellation>>;

thread_local! {
    /// Tessellations by shape. A shape is the same local mesh every time, so
    /// flattening a posed scene each frame re-uses it instead of rebuilding
    /// rounded corners per card per frame. Bounded by the distinct shapes a
    /// thread has flattened (a handful per app).
    static SHAPES: std::cell::RefCell<Shapes> = Default::default();
}

/// [`tessellate`], shared: one tessellation per distinct shape per thread.
pub fn tessellate_shared(prim: Prim) -> std::sync::Arc<Tessellation> {
    let key = shape_key(prim);
    SHAPES.with(|s| {
        s.borrow_mut()
            .entry(key)
            .or_insert_with(|| std::sync::Arc::new(tessellate(prim)))
            .clone()
    })
}

fn pack(pair: (Vec<LocalVert>, Vec<u16>)) -> Tessellation {
    Tessellation {
        verts: pair.0,
        indices: pair.1,
        rim_index: None,
        back_index: None,
    }
}

const CORNER_SEGS: usize = 8;

fn slab(w: f32, h: f32, depth: f32, radius: f32) -> Tessellation {
    let hw = w.abs() * 0.5;
    let hh = h.abs() * 0.5;
    let hz = depth.abs() * 0.5;
    let r = radius.max(0.0).min(hw).min(hh);
    let ring = rounded_ring(hw, hh, r);
    let n = ring.len();
    if n < 3 {
        return pack(plane());
    }
    let mut verts = Vec::new();
    let mut idx = Vec::new();
    let front_n = Vec3::new(0.0, 0.0, 1.0);
    let back_n = Vec3::new(0.0, 0.0, -1.0);
    let front_c = verts.len() as u16;
    verts.push(LocalVert::new(Vec3::new(0.0, 0.0, hz), front_n, [0.5, 0.5]));
    let front_ring = verts.len() as u16;
    for p in &ring {
        verts.push(LocalVert::new(
            Vec3::new(p.x, p.y, hz),
            front_n,
            face_uv(p.x, p.y, hw, hh),
        ));
    }
    for i in 0..n {
        let a = front_ring + i as u16;
        let b = front_ring + ((i + 1) % n) as u16;
        idx.extend_from_slice(&[front_c, a, b]);
    }
    let rim_index = idx.len() as u32;
    for i in 0..n {
        let p0 = ring[i];
        let p1 = ring[(i + 1) % n];
        let edge = Vec3::new(p1.x - p0.x, p1.y - p0.y, 0.0);
        if edge.length() < 1e-5 {
            continue;
        }
        let nrm = Vec3::new(edge.y, -edge.x, 0.0).normalized();
        let base = verts.len() as u16;
        verts.push(LocalVert::stock(Vec3::new(p0.x, p0.y, hz), nrm));
        verts.push(LocalVert::stock(Vec3::new(p1.x, p1.y, hz), nrm));
        verts.push(LocalVert::stock(Vec3::new(p1.x, p1.y, -hz), nrm));
        verts.push(LocalVert::stock(Vec3::new(p0.x, p0.y, -hz), nrm));
        idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    let back_index = idx.len() as u32;
    let back_c = verts.len() as u16;
    verts.push(LocalVert::new(Vec3::new(0.0, 0.0, -hz), back_n, [0.5, 0.5]));
    let back_ring = verts.len() as u16;
    for p in &ring {
        verts.push(LocalVert::new(
            Vec3::new(p.x, p.y, -hz),
            back_n,
            face_uv(-p.x, p.y, hw, hh),
        ));
    }
    for i in 0..n {
        let a = back_ring + i as u16;
        let b = back_ring + ((i + 1) % n) as u16;
        idx.extend_from_slice(&[back_c, b, a]);
    }
    Tessellation {
        verts,
        indices: idx,
        rim_index: Some(rim_index),
        back_index: Some(back_index),
    }
}

fn close(a: Vec3, b: Vec3) -> bool {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    let dz = a.z - b.z;
    dx * dx + dy * dy + dz * dz < 1e-10
}

fn face_uv(x: f32, y: f32, hw: f32, hh: f32) -> [f32; 2] {
    [
        (x + hw) / (2.0 * hw).max(1e-8),
        1.0 - (y + hh) / (2.0 * hh).max(1e-8),
    ]
}

fn rounded_ring(hw: f32, hh: f32, r: f32) -> Vec<Vec3> {
    if r < 1e-4 {
        return vec![
            Vec3::new(-hw, -hh, 0.0),
            Vec3::new(hw, -hh, 0.0),
            Vec3::new(hw, hh, 0.0),
            Vec3::new(-hw, hh, 0.0),
        ];
    }
    let corners = [
        (
            Vec3::new(-hw + r, -hh + r, 0.0),
            std::f32::consts::PI,
            std::f32::consts::PI * 1.5,
        ),
        (
            Vec3::new(hw - r, -hh + r, 0.0),
            std::f32::consts::PI * 1.5,
            std::f32::consts::TAU,
        ),
        (
            Vec3::new(hw - r, hh - r, 0.0),
            0.0,
            std::f32::consts::FRAC_PI_2,
        ),
        (
            Vec3::new(-hw + r, hh - r, 0.0),
            std::f32::consts::FRAC_PI_2,
            std::f32::consts::PI,
        ),
    ];
    let mut ring = Vec::with_capacity(4 * (CORNER_SEGS + 1));
    for (centre, a0, a1) in corners {
        for i in 0..=CORNER_SEGS {
            let t = i as f32 / CORNER_SEGS as f32;
            let a = a0 + (a1 - a0) * t;
            let p = Vec3::new(centre.x + r * a.cos(), centre.y + r * a.sin(), 0.0);
            if let Some(&last) = ring.last() {
                if close(last, p) {
                    continue;
                }
            }
            ring.push(p);
        }
    }
    if ring.len() >= 2 && close(ring[ring.len() - 1], ring[0]) {
        ring.pop();
    }
    ring
}

/// Base on y=0, apex at y=1. Origin-centred in XZ.
pub fn tet() -> (Vec<LocalVert>, Vec<u16>) {
    let a = Vec3::new(-0.5, 0.0, -0.5);
    let b = Vec3::new(0.5, 0.0, -0.5);
    let c = Vec3::new(0.0, 0.0, 0.5);
    let d = Vec3::new(0.0, 1.0, 0.0);
    let mut verts = Vec::with_capacity(12);
    let mut idx = Vec::with_capacity(12);
    tri(&mut verts, &mut idx, a, c, b);
    tri(&mut verts, &mut idx, a, b, d);
    tri(&mut verts, &mut idx, b, c, d);
    tri(&mut verts, &mut idx, c, a, d);
    (verts, idx)
}

fn tri(verts: &mut Vec<LocalVert>, idx: &mut Vec<u16>, p0: Vec3, p1: Vec3, p2: Vec3) {
    let e1 = Vec3::new(p1.x - p0.x, p1.y - p0.y, p1.z - p0.z);
    let e2 = Vec3::new(p2.x - p0.x, p2.y - p0.y, p2.z - p0.z);
    let n = e1.cross(e2).normalized();
    let base = verts.len() as u16;
    for (p, uv) in [(p0, [0.0, 1.0]), (p1, [1.0, 1.0]), (p2, [0.5, 0.0])] {
        verts.push(LocalVert::new(p, n, uv));
    }
    idx.extend_from_slice(&[base, base + 1, base + 2]);
}

const SPHERE_STACKS: usize = 12;
const SPHERE_SLICES: usize = 16;
const SPHERE_R: f32 = 0.5;

fn sphere() -> (Vec<LocalVert>, Vec<u16>) {
    let mut verts = Vec::new();
    for stack in 0..=SPHERE_STACKS {
        let v = stack as f32 / SPHERE_STACKS as f32;
        let phi = v * std::f32::consts::PI;
        let y = SPHERE_R * phi.cos();
        let ring = SPHERE_R * phi.sin();
        for slice in 0..=SPHERE_SLICES {
            let u = slice as f32 / SPHERE_SLICES as f32;
            let theta = u * std::f32::consts::TAU;
            let pos = Vec3::new(ring * theta.sin(), y, ring * theta.cos());
            let nrm = pos.normalized();
            verts.push(LocalVert::new(
                pos,
                if nrm.length() < 1e-8 {
                    Vec3::new(0.0, if v < 0.5 { 1.0 } else { -1.0 }, 0.0)
                } else {
                    nrm
                },
                [u, v],
            ));
        }
    }
    let stride = SPHERE_SLICES + 1;
    let mut idx = Vec::new();
    for stack in 0..SPHERE_STACKS {
        for slice in 0..SPHERE_SLICES {
            let a = (stack * stride + slice) as u16;
            let b = a + stride as u16;
            let a1 = a + 1;
            let b1 = b + 1;
            if stack != 0 {
                idx.extend_from_slice(&[a, b, a1]);
            }
            if stack != SPHERE_STACKS - 1 {
                idx.extend_from_slice(&[a1, b, b1]);
            }
        }
    }
    (verts, idx)
}

fn plane() -> (Vec<LocalVert>, Vec<u16>) {
    let n = Vec3::new(0.0, 0.0, 1.0);
    let verts = vec![
        LocalVert::new(Vec3::new(-0.5, -0.5, 0.0), n, [0.0, 1.0]),
        LocalVert::new(Vec3::new(0.5, -0.5, 0.0), n, [1.0, 1.0]),
        LocalVert::new(Vec3::new(0.5, 0.5, 0.0), n, [1.0, 0.0]),
        LocalVert::new(Vec3::new(-0.5, 0.5, 0.0), n, [0.0, 0.0]),
    ];
    (verts, vec![0, 1, 2, 0, 2, 3])
}

fn cube() -> (Vec<LocalVert>, Vec<u16>) {
    let faces: [(Vec3, Vec3, Vec3); 6] = [
        (
            Vec3::new(0.0, 0.0, 0.5),
            Vec3::new(0.5, 0.0, 0.0),
            Vec3::new(0.0, 0.5, 0.0),
        ),
        (
            Vec3::new(0.0, 0.0, -0.5),
            Vec3::new(-0.5, 0.0, 0.0),
            Vec3::new(0.0, 0.5, 0.0),
        ),
        (
            Vec3::new(0.0, 0.5, 0.0),
            Vec3::new(0.5, 0.0, 0.0),
            Vec3::new(0.0, 0.0, -0.5),
        ),
        (
            Vec3::new(0.0, -0.5, 0.0),
            Vec3::new(0.5, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 0.5),
        ),
        (
            Vec3::new(0.5, 0.0, 0.0),
            Vec3::new(0.0, 0.0, -0.5),
            Vec3::new(0.0, 0.5, 0.0),
        ),
        (
            Vec3::new(-0.5, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 0.5),
            Vec3::new(0.0, 0.5, 0.0),
        ),
    ];
    let mut verts = Vec::with_capacity(24);
    let mut idx = Vec::with_capacity(36);
    for (centre, ax, ay) in faces {
        let n = ax.cross(ay).normalized();
        let base = verts.len() as u16;
        let corners = [
            (
                [0.0, 1.0],
                Vec3::new(-ax.x - ay.x, -ax.y - ay.y, -ax.z - ay.z),
            ),
            ([1.0, 1.0], Vec3::new(ax.x - ay.x, ax.y - ay.y, ax.z - ay.z)),
            ([1.0, 0.0], Vec3::new(ax.x + ay.x, ax.y + ay.y, ax.z + ay.z)),
            (
                [0.0, 0.0],
                Vec3::new(-ax.x + ay.x, -ax.y + ay.y, -ax.z + ay.z),
            ),
        ];
        for (uv, off) in corners {
            verts.push(LocalVert::new(
                Vec3::new(centre.x + off.x, centre.y + off.y, centre.z + off.z),
                n,
                uv,
            ));
        }
        idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    (verts, idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prim_counts() {
        let t = tessellate(Prim::Plane);
        assert_eq!(t.verts.len(), 4);
        assert_eq!(t.indices.len(), 6);
        let t = tessellate(Prim::Box);
        assert_eq!(t.verts.len(), 24);
        assert_eq!(t.indices.len(), 36);
        let t = tessellate(Prim::Sphere);
        assert_eq!(t.verts.len(), (SPHERE_STACKS + 1) * (SPHERE_SLICES + 1));
        assert!(t.indices.len().is_multiple_of(3));
        assert!(t.indices.iter().all(|&j| (j as usize) < t.verts.len()));
        for tri in t.indices.as_chunks::<3>().0 {
            let a = t.verts[tri[0] as usize].pos;
            let b = t.verts[tri[1] as usize].pos;
            let c = t.verts[tri[2] as usize].pos;
            let n = (b - a).cross(c - a);
            let mid = Vec3::new(
                (a.x + b.x + c.x) / 3.0,
                (a.y + b.y + c.y) / 3.0,
                (a.z + b.z + c.z) / 3.0,
            );
            assert!(n.dot(mid) > 0.0, "inward tri {tri:?} n={n:?} mid={mid:?}");
        }
        let (v, i) = tet();
        assert_eq!(v.len(), 12);
        assert_eq!(i.len(), 12);
        let t = tessellate(Prim::Slab {
            w: 2.2,
            h: 3.1,
            depth: 0.05,
            radius: 0.12,
        });
        assert!(t.verts.len() > 24, "{}", t.verts.len());
        assert!(t.indices.len() > 36, "{}", t.indices.len());
        assert!(t.indices.len().is_multiple_of(3));
        assert!(t.indices.iter().all(|&j| (j as usize) < t.verts.len()));
        let ri = t.rim_index.expect("slab rim");
        let bi = t.back_index.expect("slab back");
        assert!(ri > 0 && ri < bi && (bi as usize) < t.indices.len());
        let rim_i = t.indices[ri as usize] as usize;
        assert!(
            t.verts[rim_i].rgb != LocalVert::WHITE,
            "rim must not sample the print"
        );
        assert_eq!(t.verts[0].rgb, LocalVert::WHITE);
        let sharp = tessellate(Prim::Slab {
            w: 1.5,
            h: 1.5,
            depth: 0.05,
            radius: 0.0,
        });
        assert!(sharp.verts.len() < t.verts.len(), "rad=0 is a rectangle");
    }
}
