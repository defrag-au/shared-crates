//! A model's skeleton and its own clips, and the skinning that poses it.
//! Pure: no glTF here (the importer fills it), no time (a node's
//! `pose=CLIP@T` says when), no GPU (vertices are skinned on the CPU, so a
//! posed model flattens to the same [`crate::DrawList`] meshes every
//! backend, pick and check already reads).
//!
//! Every node of the source file is a rig node, so a rigid part moved by an
//! animation (the milk truck's wheels) and a skinned one (the fox) pose the
//! same way: a rigid vertex is bound wholly to its own node.

use crate::math::{mat4_ident, mat4_mul, transform_dir, transform_point, Mat4, Vec3};
use crate::mesh::LocalVert;

/// One node of the source file's tree, at rest. Parents come before
/// children.
#[derive(Clone, Debug, PartialEq)]
pub struct RigNode {
    pub name: String,
    pub parent: Option<u16>,
    pub t: Vec3,
    /// Rotation quaternion, `[x, y, z, w]`.
    pub r: [f32; 4],
    pub s: Vec3,
}

/// What a vertex's joint index means: a node, and the matrix that takes the
/// vertex from where it was authored into that node's rest frame (a skin's
/// inverse bind matrix; identity for a rigid part, authored in its node).
#[derive(Clone, Debug, PartialEq)]
pub struct RigBind {
    pub node: u16,
    pub inverse_bind: Mat4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RigPath {
    Translate,
    Rotate,
    Scale,
}

/// One animated property of one node: keyframe times and values
/// (`[x, y, z, _]`, or a quaternion for rotation).
#[derive(Clone, Debug, PartialEq)]
pub struct RigChannel {
    pub node: u16,
    pub path: RigPath,
    /// `STEP` holds each key; otherwise linear (slerp for rotation).
    pub step: bool,
    pub times: Vec<f32>,
    pub values: Vec<[f32; 4]>,
}

/// One of the file's animations, by its name.
#[derive(Clone, Debug, PartialEq)]
pub struct RigClip {
    pub name: String,
    /// Seconds: the last key of any channel.
    pub length: f32,
    pub channels: Vec<RigChannel>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rig {
    pub nodes: Vec<RigNode>,
    pub binds: Vec<RigBind>,
    pub clips: Vec<RigClip>,
}

/// A part's vertices as authored, and what moves each: up to four bind
/// indices into [`Rig::binds`] with weights summing to one.
#[derive(Clone, Debug)]
pub struct PartSkin {
    pub base: Vec<LocalVert>,
    pub joints: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
}

impl Rig {
    pub fn clip(&self, name: &str) -> Option<&RigClip> {
        self.clips.iter().find(|c| c.name == name)
    }

    /// Each bind's skinning matrix with `clip` sampled at `t` (wrapped by
    /// its length), or at rest with none: the node's posed world matrix
    /// times its inverse bind.
    pub fn matrices(&self, pose: Option<(&RigClip, f32)>) -> Vec<Mat4> {
        let mut local: Vec<(Vec3, [f32; 4], Vec3)> =
            self.nodes.iter().map(|n| (n.t, n.r, n.s)).collect();
        if let Some((clip, t)) = pose {
            let t = if clip.length > 1e-6 {
                t.rem_euclid(clip.length)
            } else {
                0.0
            };
            for ch in &clip.channels {
                let Some(v) = sample(ch, t) else {
                    continue;
                };
                let Some(l) = local.get_mut(ch.node as usize) else {
                    continue;
                };
                match ch.path {
                    RigPath::Translate => l.0 = Vec3::new(v[0], v[1], v[2]),
                    RigPath::Rotate => l.1 = v,
                    RigPath::Scale => l.2 = Vec3::new(v[0], v[1], v[2]),
                }
            }
        }
        let mut world: Vec<Mat4> = Vec::with_capacity(self.nodes.len());
        for (n, (t, r, s)) in self.nodes.iter().zip(&local) {
            let m = trs_quat(*t, *r, *s);
            let w = match n.parent {
                Some(p) => mat4_mul(world[p as usize], m),
                None => m,
            };
            world.push(w);
        }
        self.binds
            .iter()
            .map(|b| {
                world
                    .get(b.node as usize)
                    .map_or(mat4_ident(), |w| mat4_mul(*w, b.inverse_bind))
            })
            .collect()
    }
}

/// `skin`'s vertices moved by `mats` ([`Rig::matrices`]): each a weighted
/// blend of its joints' matrices. Normals go through the same blend and are
/// renormalised.
pub fn skin(skin: &PartSkin, mats: &[Mat4]) -> Vec<LocalVert> {
    skin.base
        .iter()
        .zip(&skin.joints)
        .zip(&skin.weights)
        .map(|((v, j), w)| {
            let mut pos = Vec3::ZERO;
            let mut nrm = Vec3::ZERO;
            for k in 0..4 {
                if w[k] <= 0.0 {
                    continue;
                }
                let Some(m) = mats.get(j[k] as usize) else {
                    continue;
                };
                pos = pos + transform_point(*m, v.pos) * w[k];
                nrm = nrm + transform_dir(*m, v.nrm) * w[k];
            }
            LocalVert {
                pos,
                nrm: nrm.normalized(),
                ..*v
            }
        })
        .collect()
}

/// A channel's value at `t`: before the first key, the first; after the
/// last, the last.
fn sample(ch: &RigChannel, t: f32) -> Option<[f32; 4]> {
    let n = ch.times.len().min(ch.values.len());
    if n == 0 {
        return None;
    }
    let i = ch.times[..n].partition_point(|&k| k <= t);
    if i == 0 {
        return Some(ch.values[0]);
    }
    if i >= n || ch.step {
        return Some(ch.values[i.min(n) - 1]);
    }
    let (t0, t1) = (ch.times[i - 1], ch.times[i]);
    let u = if t1 - t0 > 1e-8 {
        ((t - t0) / (t1 - t0)).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (a, b) = (ch.values[i - 1], ch.values[i]);
    Some(match ch.path {
        RigPath::Rotate => slerp(a, b, u),
        _ => [
            a[0] + (b[0] - a[0]) * u,
            a[1] + (b[1] - a[1]) * u,
            a[2] + (b[2] - a[2]) * u,
            0.0,
        ],
    })
}

fn slerp(a: [f32; 4], b: [f32; 4], u: f32) -> [f32; 4] {
    let mut dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    let b = if dot < 0.0 {
        dot = -dot;
        [-b[0], -b[1], -b[2], -b[3]]
    } else {
        b
    };
    let (wa, wb) = if dot > 0.9995 {
        (1.0 - u, u)
    } else {
        let theta = dot.clamp(-1.0, 1.0).acos();
        let sin = theta.sin();
        (((1.0 - u) * theta).sin() / sin, (u * theta).sin() / sin)
    };
    let q = [
        a[0] * wa + b[0] * wb,
        a[1] * wa + b[1] * wb,
        a[2] * wa + b[2] * wb,
        a[3] * wa + b[3] * wb,
    ];
    let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if len < 1e-8 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    q.map(|c| c / len)
}

/// Translation, quaternion rotation and scale, column-major like
/// [`crate::math::mat4_trs`].
pub fn trs_quat(t: Vec3, q: [f32; 4], s: Vec3) -> Mat4 {
    let [x, y, z, w] = q;
    let (xx, yy, zz) = (x * x, y * y, z * z);
    let (xy, xz, yz) = (x * y, x * z, y * z);
    let (wx, wy, wz) = (w * x, w * y, w * z);
    [
        [
            (1.0 - 2.0 * (yy + zz)) * s.x,
            2.0 * (xy + wz) * s.x,
            2.0 * (xz - wy) * s.x,
            0.0,
        ],
        [
            2.0 * (xy - wz) * s.y,
            (1.0 - 2.0 * (xx + zz)) * s.y,
            2.0 * (yz + wx) * s.y,
            0.0,
        ],
        [
            2.0 * (xz + wy) * s.z,
            2.0 * (yz - wx) * s.z,
            (1.0 - 2.0 * (xx + yy)) * s.z,
            0.0,
        ],
        [t.x, t.y, t.z, 1.0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A two-node arm: a root, and a child one unit up it that a clip turns
    /// a quarter round Z over one second. A vertex bound to the child at its
    /// tip swings from up to left.
    fn arm() -> (Rig, PartSkin) {
        let q90 = [
            0.0,
            0.0,
            std::f32::consts::FRAC_PI_4.sin(),
            std::f32::consts::FRAC_PI_4.cos(),
        ];
        let rig = Rig {
            nodes: vec![
                RigNode {
                    name: "root".into(),
                    parent: None,
                    t: Vec3::ZERO,
                    r: [0.0, 0.0, 0.0, 1.0],
                    s: Vec3::new(1.0, 1.0, 1.0),
                },
                RigNode {
                    name: "bone".into(),
                    parent: Some(0),
                    t: Vec3::new(0.0, 1.0, 0.0),
                    r: [0.0, 0.0, 0.0, 1.0],
                    s: Vec3::new(1.0, 1.0, 1.0),
                },
            ],
            // The vertex is authored in model space at (0,2,0): the bone's
            // inverse bind takes it into the bone's frame, (0,1,0).
            binds: vec![RigBind {
                node: 1,
                inverse_bind: trs_quat(
                    Vec3::new(0.0, -1.0, 0.0),
                    [0.0, 0.0, 0.0, 1.0],
                    Vec3::new(1.0, 1.0, 1.0),
                ),
            }],
            clips: vec![RigClip {
                name: "Wave".into(),
                length: 1.0,
                channels: vec![RigChannel {
                    node: 1,
                    path: RigPath::Rotate,
                    step: false,
                    times: vec![0.0, 1.0],
                    values: vec![[0.0, 0.0, 0.0, 1.0], q90],
                }],
            }],
        };
        let skin = PartSkin {
            base: vec![LocalVert::new(
                Vec3::new(0.0, 2.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                [0.0, 0.0],
            )],
            joints: vec![[0, 0, 0, 0]],
            weights: vec![[1.0, 0.0, 0.0, 0.0]],
        };
        (rig, skin)
    }

    #[test]
    fn rest_skins_to_where_it_was_authored() {
        let (rig, part) = arm();
        let v = skin(&part, &rig.matrices(None))[0];
        assert!(v.pos.is_near(Vec3::new(0.0, 2.0, 0.0), 1e-5), "{:?}", v.pos);
    }

    #[test]
    fn a_clip_turns_the_bone_and_wraps_by_its_length() {
        let (rig, part) = arm();
        let clip = rig.clip("Wave").unwrap();
        let end = skin(&part, &rig.matrices(Some((clip, 0.999_99))))[0];
        assert!(
            end.pos.is_near(Vec3::new(-1.0, 1.0, 0.0), 1e-3),
            "{:?}",
            end.pos
        );
        assert!(
            end.nrm.is_near(Vec3::new(-1.0, 0.0, 0.0), 1e-3),
            "{:?}",
            end.nrm
        );
        let half = skin(&part, &rig.matrices(Some((clip, 0.5))))[0];
        let s = std::f32::consts::FRAC_PI_4.sin();
        assert!(
            half.pos.is_near(Vec3::new(-s, 1.0 + s, 0.0), 1e-4),
            "{:?}",
            half.pos
        );
        let wrapped = skin(&part, &rig.matrices(Some((clip, 1.5))))[0];
        assert!(wrapped.pos.is_near(half.pos, 1e-5), "1.5 s is 0.5 s again");
    }
}
