//! glTF 2.0 → [`Geometry`] + parented [`Scene`]. Optional (`--features gltf`).
//! Not a runtime document: verts stay interned; `sg1` stores names and rest `t/r/s`.
//!
//! Buffers and PNG/JPEG come from the `gltf` crate (`import` / `import_slice`).
//! This module walks the node tree into `sg1` and maps PBR base-color onto
//! [`Materials`] / [`Images`] (`mat=` / `m=`).

use std::path::Path;

use crate::error::Error;
use crate::geom::Geometry;
use crate::images::Images;
use crate::kind::{Kind, KindOf, DEFAULT_FOV};
use crate::material::{Materials, Shade, Surface};
use crate::math::{mat4_ident, mat4_mul, mat4_trs, transform_dir, transform_point, Mat4, Vec3};
use crate::mesh::LocalVert;
use crate::node::{Node, Transform};
use crate::scene::Scene;

/// One imported glTF appearance. glTF itself is not `sg1`.
#[derive(Clone, Debug)]
pub struct GltfImport {
    pub scene: Scene,
    pub geom: Geometry,
    pub materials: Materials,
    pub images: Images,
}

#[derive(Clone, Debug)]
struct PrimSlot {
    geom: String,
    mat: Option<String>,
}

impl Geometry {
    /// Insert triangle primitives. Keys are mesh names (`tri`, `tri_1`, …).
    pub fn insert_gltf(&mut self, bytes: &[u8]) -> Result<Vec<String>, Error> {
        let (g, buffers, _) = gltf::import_slice(bytes).map_err(|e| Error::Gltf(e.to_string()))?;
        let by_mesh = intern_meshes(&g, &buffers, self, &[])?;
        let keys: Vec<String> = by_mesh.into_iter().flatten().map(|p| p.geom).collect();
        if keys.is_empty() {
            return Err(Error::Gltf("no triangle primitives".into()));
        }
        Ok(keys)
    }
}

/// Scene of parented `Group` / `Mesh Geom` nodes plus interned blobs.
/// Node rest `t/r/s` comes from the glTF node tree (radians). A perspective
/// camera is baked to world `eye`/`at` (the graph does not compose Camera `xf`).
/// Missing camera → default `cam` at `(0,1.5,4)`. Missing lights → default `sun`.
/// Base-color factor/texture intern as `mt1` / `Images`.
pub fn scene_from_gltf(bytes: &[u8]) -> Result<GltfImport, Error> {
    let (g, buffers, decoded) =
        gltf::import_slice(bytes).map_err(|e| Error::Gltf(e.to_string()))?;
    from_imported(g, buffers, decoded)
}

/// Like [`scene_from_gltf`], resolving relative buffer/image URIs against the file.
pub fn scene_from_gltf_path(path: &Path) -> Result<GltfImport, Error> {
    let (g, buffers, decoded) =
        gltf::import(path).map_err(|e| Error::Gltf(format!("{}: {e}", path.display())))?;
    from_imported(g, buffers, decoded)
}

fn from_imported(
    g: gltf::Document,
    buffers: Vec<gltf::buffer::Data>,
    decoded: Vec<gltf::image::Data>,
) -> Result<GltfImport, Error> {
    let mut images = Images::new();
    let img_keys = intern_images(&g, &decoded, &mut images)?;
    let mut materials = Materials::new();
    let mat_keys = intern_materials(&g, &img_keys, &mut materials)?;
    let mut geom = Geometry::new();
    let mesh_keys = intern_meshes(&g, &buffers, &mut geom, &mat_keys)?;
    if mesh_keys.iter().all(|k| k.is_empty()) {
        return Err(Error::Gltf("no triangle primitives".into()));
    }
    let mut scene = Scene::new();
    scene.insert(Node::group("gltf"))?;
    let mut used = vec!["gltf".to_string()];
    let mut cameras = Vec::new();
    let roots: Vec<_> = g
        .default_scene()
        .or_else(|| g.scenes().next())
        .map(|s| s.nodes().collect())
        .ok_or_else(|| Error::Gltf("no scene".into()))?;
    for node in roots {
        walk(
            node,
            Some("gltf"),
            mat4_ident(),
            &mesh_keys,
            &mut scene,
            &mut used,
            &mut cameras,
        )?;
    }
    if cameras.is_empty() {
        scene.insert(Node::camera("cam", Vec3::new(0.0, 1.5, 4.0), Vec3::ZERO))?;
        used.push("cam".into());
    } else {
        for (name, eye, at, up, fov) in cameras {
            let name = unique_key(&used, &name);
            used.push(name.clone());
            let mut n = Node::camera(&name, eye, at);
            if let Kind::Camera { up: u, fov: f, .. } = &mut n.kind {
                *u = up;
                *f = fov;
            }
            scene.insert(n)?;
        }
    }
    if scene.kind(KindOf::Light).count() == 0 {
        let name = unique_key(&used, "sun");
        scene.insert(Node::light(name, Vec3::new(-0.4, -1.0, -0.5)))?;
    }
    Ok(GltfImport {
        scene,
        geom,
        materials,
        images,
    })
}

fn intern_meshes(
    g: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    geom: &mut Geometry,
    mat_keys: &[Option<String>],
) -> Result<Vec<Vec<PrimSlot>>, Error> {
    let meshes: Vec<_> = g.meshes().collect();
    let mut by_mesh = vec![Vec::new(); meshes.len()];
    let mut used = Vec::new();
    for mesh in meshes {
        let i = mesh.index();
        if i >= by_mesh.len() {
            by_mesh.resize(i + 1, Vec::new());
        }
        let base = mesh
            .name()
            .map(str::to_string)
            .unwrap_or_else(|| format!("mesh{}", i));
        for (pi, prim) in mesh.primitives().enumerate() {
            let Some((verts, indices)) = read_prim(&prim, buffers, &base)? else {
                continue;
            };
            let mat = prim
                .material()
                .index()
                .and_then(|mi| mat_keys.get(mi).cloned().flatten());
            let stem = if pi == 0 {
                base.clone()
            } else {
                format!("{base}_{pi}")
            };
            for (ci, (verts, indices)) in split_u16(&verts, &indices).into_iter().enumerate() {
                let want = if ci == 0 {
                    stem.clone()
                } else {
                    format!("{stem}~{ci}")
                };
                let key = unique_key(&used, &want);
                geom.insert(&key, verts, indices)?;
                used.push(key.clone());
                by_mesh[i].push(PrimSlot {
                    geom: key,
                    mat: mat.clone(),
                });
            }
        }
    }
    Ok(by_mesh)
}

/// A primitive's vertices and 32-bit triangle indices.
type Triangles = (Vec<LocalVert>, Vec<u32>);

/// A triangle primitive's vertices (as authored, in its mesh's frame) and
/// 32-bit indices; `None` for any other mode, or fewer than one triangle.
fn read_prim(
    prim: &gltf::Primitive<'_>,
    buffers: &[gltf::buffer::Data],
    base: &str,
) -> Result<Option<Triangles>, Error> {
    if prim.mode() != gltf::mesh::Mode::Triangles {
        return Ok(None);
    }
    let reader = prim.reader(|buf| buffers.get(buf.index()).map(|d| d.0.as_slice()));
    let positions: Vec<[f32; 3]> = reader
        .read_positions()
        .ok_or_else(|| Error::Gltf(format!("{base}: no POSITION")))?
        .collect();
    // 32-bit here; a primitive over the 16-bit limit is split later.
    let indices: Vec<u32> = if let Some(idx) = reader.read_indices() {
        let raw: Vec<u32> = idx.into_u32().collect();
        if raw.iter().any(|&i| i as usize >= positions.len()) {
            return Err(Error::Gltf(format!("{base}: bad index")));
        }
        raw
    } else {
        if !positions.len().is_multiple_of(3) {
            return Err(Error::Gltf(format!("{base}: unindexed not tris")));
        }
        (0..positions.len() as u32).collect()
    };
    if indices.len() < 3 || !indices.len().is_multiple_of(3) {
        return Ok(None);
    }
    let normals: Vec<Vec3> = if let Some(iter) = reader.read_normals() {
        iter.map(|n| Vec3::new(n[0], n[1], n[2])).collect()
    } else {
        accumulate_normals(&positions, &indices)
    };
    let uvs: Vec<[f32; 2]> = if let Some(iter) = reader.read_tex_coords(0) {
        iter.into_f32().collect()
    } else {
        vec![[0.0, 0.0]; positions.len()]
    };
    if normals.len() != positions.len() || uvs.len() != positions.len() {
        return Err(Error::Gltf(format!("{base}: attribute length")));
    }
    let verts = positions
        .iter()
        .zip(normals)
        .zip(uvs)
        .map(|((p, n), uv)| LocalVert::new(Vec3::new(p[0], p[1], p[2]), n, uv))
        .collect();
    Ok(Some((verts, indices)))
}

/// A vertex that has a texture coordinate [`untile`] may move.
trait Uv: Copy {
    fn uv(&self) -> [f32; 2];
    fn set_uv(&mut self, uv: [f32; 2]);
}

impl Uv for LocalVert {
    fn uv(&self) -> [f32; 2] {
        self.uv
    }
    fn set_uv(&mut self, uv: [f32; 2]) {
        self.uv = uv;
    }
}

/// A rigged model's vertex while it is grouped: as authored (`base`), and
/// what moves it.
#[derive(Clone, Copy, Debug)]
struct SkinVert {
    base: LocalVert,
    joints: [u16; 4],
    weights: [f32; 4],
}

impl Uv for SkinVert {
    fn uv(&self) -> [f32; 2] {
        self.base.uv
    }
    fn set_uv(&mut self, uv: [f32; 2]) {
        self.base.uv = uv;
    }
}

/// Split a triangle list into pieces of at most 65 535 distinct vertices
/// each, on whole triangles, re-indexed to 16 bits. One piece when it fits.
/// Only referenced vertices are kept, in first-use order, so the originals
/// [`untile`] copied away from drop out.
fn split_u16<T: Copy>(verts: &[T], indices: &[u32]) -> Vec<(Vec<T>, Vec<u16>)> {
    const CAP: usize = u16::MAX as usize;
    let mut out = Vec::new();
    let mut remap: Vec<u32> = vec![u32::MAX; verts.len()];
    let mut piece_verts: Vec<T> = Vec::new();
    let mut piece_idx: Vec<u16> = Vec::new();
    for tri in indices.as_chunks::<3>().0 {
        let new = tri
            .iter()
            .filter(|&&i| remap[i as usize] == u32::MAX)
            .count();
        if piece_verts.len() + new > CAP {
            out.push((
                std::mem::take(&mut piece_verts),
                std::mem::take(&mut piece_idx),
            ));
            remap.fill(u32::MAX);
        }
        for &i in tri {
            let slot = &mut remap[i as usize];
            if *slot == u32::MAX {
                *slot = piece_verts.len() as u32;
                piece_verts.push(verts[i as usize]);
            }
            piece_idx.push(*slot as u16);
        }
    }
    if !piece_idx.is_empty() {
        out.push((piece_verts, piece_idx));
    }
    out
}

/// Move each triangle's UVs into the first texture tile, `[0,1]²`, when
/// they all sit in one tile: glTF samplers default to `REPEAT`, and the
/// Khronos BoxTextured lays its six faces out along u = 0..6, but both
/// backends clamp. A vertex two triangles shift differently is copied, so
/// neither moves the other's. Returns the triangles that span more than one
/// tile and so cannot be moved; they are left as authored.
fn untile<T: Uv>(verts: &mut Vec<T>, indices: &mut [u32]) -> u32 {
    const EPS: f32 = 1e-4;
    let mut copies: std::collections::HashMap<(u32, i32, i32), u32> =
        std::collections::HashMap::new();
    let mut tiled = 0;
    for tri in indices.as_chunks_mut::<3>().0 {
        let uv = |i: u32| verts[i as usize].uv();
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for &i in tri.iter() {
            let [u, v] = uv(i);
            lo = [lo[0].min(u), lo[1].min(v)];
            hi = [hi[0].max(u), hi[1].max(v)];
        }
        let tile = [(lo[0] + EPS).floor(), (lo[1] + EPS).floor()];
        if hi[0] - tile[0] > 1.0 + EPS || hi[1] - tile[1] > 1.0 + EPS {
            tiled += 1;
            continue;
        }
        if tile == [0.0, 0.0] {
            continue;
        }
        let shift = (tile[0] as i32, tile[1] as i32);
        for i in tri.iter_mut() {
            *i = *copies.entry((*i, shift.0, shift.1)).or_insert_with(|| {
                let mut v = verts[*i as usize];
                let [u, w] = v.uv();
                v.set_uv([u - tile[0], w - tile[1]]);
                verts.push(v);
                verts.len() as u32 - 1
            });
        }
    }
    tiled
}

/// One glTF file as a model: its parts, its textures (keyed `KEY/NAME`),
/// and, if it has skins or animations, the rig its clips pose.
#[derive(Clone, Debug)]
pub struct ImportedModel {
    pub parts: Vec<crate::geom::ModelPart>,
    pub images: Images,
    pub rig: Option<crate::rig::Rig>,
}

/// A glTF file as one model under `key`: every triangle primitive in the
/// file's default scene, with its node transforms baked in (the model's
/// own frame), grouped by surface and split under the 16-bit index limit.
/// Images come back keyed `KEY/NAME`, and the parts' surfaces name them so.
///
/// A file with skins or animations comes back rigged: its parts carry what
/// moves each vertex, drawn at rest until a node poses them with one of
/// the rig's clips (`pose=CLIP@T`).
pub fn model_from_gltf(key: &str, bytes: &[u8]) -> Result<ImportedModel, Error> {
    let (g, buffers, decoded) =
        gltf::import_slice(bytes).map_err(|e| Error::Gltf(e.to_string()))?;
    if g.skins().next().is_some() || g.animations().next().is_some() {
        return rigged_model(key, &g, &buffers, &decoded);
    }
    let (parts, images) = static_model(key, bytes)?;
    Ok(ImportedModel {
        parts,
        images,
        rig: None,
    })
}

/// A rigged file: every node of its default scene is a rig node; a skinned
/// primitive's vertices are bound to its skin's joints, any other to its own
/// node. Rest is the rig at rest, so a model without `pose=` and one posed
/// at a clip's first frame are drawn by the same skinning.
fn rigged_model(
    key: &str,
    g: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    decoded: &[gltf::image::Data],
) -> Result<ImportedModel, Error> {
    use crate::rig::{PartSkin, Rig, RigBind, RigChannel, RigClip, RigNode, RigPath};
    use std::collections::HashMap;

    let prefixed = |k: &str| format!("{key}/{k}");
    let mut file_images = Images::new();
    let img_keys = intern_images(g, decoded, &mut file_images)?;
    let mut materials = Materials::new();
    let mat_keys = intern_materials(g, &img_keys, &mut materials)?;
    let mut images = Images::new();
    for (k, img) in file_images.iter() {
        images.insert(prefixed(k), img.width, img.height, img.pixels.clone())?;
    }

    // Nodes, parents first.
    let roots: Vec<gltf::Node<'_>> = g
        .default_scene()
        .or_else(|| g.scenes().next())
        .map(|s| s.nodes().collect())
        .ok_or_else(|| Error::Gltf("no scene".into()))?;
    let mut nodes: Vec<RigNode> = Vec::new();
    let mut of_node: HashMap<usize, u16> = HashMap::new();
    let mut order: Vec<gltf::Node<'_>> = Vec::new();
    let mut stack: Vec<(gltf::Node<'_>, Option<u16>)> =
        roots.into_iter().rev().map(|n| (n, None)).collect();
    while let Some((n, parent)) = stack.pop() {
        let (t, r, s) = n.transform().decomposed();
        let idx = u16::try_from(nodes.len())
            .map_err(|_| Error::Gltf(format!("{key}: over 65535 nodes")))?;
        nodes.push(RigNode {
            name: n
                .name()
                .map(str::to_string)
                .unwrap_or_else(|| format!("node{}", n.index())),
            parent,
            t: Vec3::from_array(t),
            r,
            s: Vec3::from_array(s),
        });
        of_node.insert(n.index(), idx);
        for c in n.children().collect::<Vec<_>>().into_iter().rev() {
            stack.push((c, Some(idx)));
        }
        order.push(n);
    }

    // Binds: each skin's joints once, then one per rigid mesh node.
    let mut binds: Vec<RigBind> = Vec::new();
    let mut skin_at: HashMap<usize, u16> = HashMap::new();
    for skin in g.skins() {
        let offset = binds.len() as u16;
        let reader = skin.reader(|b| buffers.get(b.index()).map(|d| d.0.as_slice()));
        let ibms: Vec<Mat4> = reader
            .read_inverse_bind_matrices()
            .map(|it| it.collect())
            .unwrap_or_default();
        for (j, joint) in skin.joints().enumerate() {
            let node = *of_node.get(&joint.index()).ok_or_else(|| {
                Error::Gltf(format!("{key}: a joint is not in the default scene"))
            })?;
            binds.push(RigBind {
                node,
                inverse_bind: ibms.get(j).copied().unwrap_or_else(mat4_ident),
            });
        }
        skin_at.insert(skin.index(), offset);
    }

    // Vertices by surface, each with what moves it.
    let mut groups: Vec<(Surface, Vec<SkinVert>, Vec<u32>)> = Vec::new();
    for n in &order {
        let Some(mesh) = n.mesh() else {
            continue;
        };
        let rig_node = of_node[&n.index()];
        let skin_offset = n.skin().and_then(|s| skin_at.get(&s.index()).copied());
        let rigid = match skin_offset {
            Some(_) => None,
            None => {
                binds.push(RigBind {
                    node: rig_node,
                    inverse_bind: mat4_ident(),
                });
                Some(binds.len() as u16 - 1)
            }
        };
        let base_name = mesh
            .name()
            .map(str::to_string)
            .unwrap_or_else(|| format!("mesh{}", mesh.index()));
        for prim in mesh.primitives() {
            let Some((verts, indices)) = read_prim(&prim, buffers, &base_name)? else {
                continue;
            };
            let reader = prim.reader(|b| buffers.get(b.index()).map(|d| d.0.as_slice()));
            let moved: Vec<([u16; 4], [f32; 4])> = match (skin_offset, rigid) {
                (Some(off), _) => {
                    let joints: Vec<[u16; 4]> = reader
                        .read_joints(0)
                        .map(|j| j.into_u16().collect())
                        .ok_or_else(|| Error::Gltf(format!("{base_name}: skinned, no JOINTS_0")))?;
                    let weights: Vec<[f32; 4]> = reader
                        .read_weights(0)
                        .map(|w| w.into_f32().collect())
                        .ok_or_else(|| {
                            Error::Gltf(format!("{base_name}: skinned, no WEIGHTS_0"))
                        })?;
                    if joints.len() != verts.len() || weights.len() != verts.len() {
                        return Err(Error::Gltf(format!("{base_name}: skin attribute length")));
                    }
                    joints
                        .into_iter()
                        .zip(weights)
                        .map(|(j, w)| {
                            let sum: f32 = w.iter().sum();
                            let w = if sum > 1e-6 {
                                w.map(|x| x / sum)
                            } else {
                                [1.0, 0.0, 0.0, 0.0]
                            };
                            (j.map(|x| off + x), w)
                        })
                        .collect()
                }
                (None, Some(b)) => vec![([b, 0, 0, 0], [1.0, 0.0, 0.0, 0.0]); verts.len()],
                (None, None) => unreachable!("a mesh node is skinned or rigid"),
            };
            let mut surf = match prim
                .material()
                .index()
                .and_then(|mi| mat_keys.get(mi).cloned().flatten())
            {
                Some(m) => materials
                    .get(&m)
                    .cloned()
                    .ok_or(Error::UnknownMaterial(m))?,
                None => Surface::DEFAULT,
            };
            surf.image = surf.image.as_deref().map(prefixed);
            surf.image_back = surf.image_back.as_deref().map(prefixed);
            let gi = match groups.iter().position(|(s, _, _)| *s == surf) {
                Some(i) => i,
                None => {
                    groups.push((surf, Vec::new(), Vec::new()));
                    groups.len() - 1
                }
            };
            let (_, gv, gidx) = &mut groups[gi];
            let first = gv.len() as u32;
            gv.extend(
                verts
                    .into_iter()
                    .zip(moved)
                    .map(|(base, (joints, weights))| SkinVert {
                        base,
                        joints,
                        weights,
                    }),
            );
            gidx.extend(indices.iter().map(|&i| first + i));
        }
    }

    // Clips.
    let mut clips: Vec<RigClip> = Vec::new();
    for (ai, anim) in g.animations().enumerate() {
        let name = anim
            .name()
            .map(str::to_string)
            .unwrap_or_else(|| format!("anim{ai}"));
        let mut channels = Vec::new();
        for ch in anim.channels() {
            let Some(&node) = of_node.get(&ch.target().node().index()) else {
                continue;
            };
            use gltf::animation::util::ReadOutputs;
            use gltf::animation::{Interpolation, Property};
            let path = match ch.target().property() {
                Property::Translation => RigPath::Translate,
                Property::Rotation => RigPath::Rotate,
                Property::Scale => RigPath::Scale,
                Property::MorphTargetWeights => continue,
            };
            let reader = ch.reader(|b| buffers.get(b.index()).map(|d| d.0.as_slice()));
            let times: Vec<f32> = reader
                .read_inputs()
                .map(|t| t.collect())
                .unwrap_or_default();
            let raw: Vec<[f32; 4]> = match reader.read_outputs() {
                Some(ReadOutputs::Translations(it)) | Some(ReadOutputs::Scales(it)) => {
                    it.map(|v| [v[0], v[1], v[2], 0.0]).collect()
                }
                Some(ReadOutputs::Rotations(r)) => r.into_f32().collect(),
                _ => continue,
            };
            let interp = ch.sampler().interpolation();
            // A cubic spline stores in-tangent, value, out-tangent per key:
            // its values are kept and played linearly.
            let values: Vec<[f32; 4]> = match interp {
                Interpolation::CubicSpline => {
                    raw.chunks(3).filter_map(|c| c.get(1).copied()).collect()
                }
                _ => raw,
            };
            if times.is_empty() || values.len() != times.len() {
                return Err(Error::Gltf(format!(
                    "{name}: channel keys and values differ"
                )));
            }
            channels.push(RigChannel {
                node,
                path,
                step: interp == Interpolation::Step,
                times,
                values,
            });
        }
        let length = channels
            .iter()
            .flat_map(|c| c.times.last().copied())
            .fold(0.0f32, f32::max);
        clips.push(RigClip {
            name,
            length,
            channels,
        });
    }

    let rig = Rig {
        nodes,
        binds,
        clips,
    };
    let rest = rig.matrices(None);
    let mut parts = Vec::new();
    for (surface, mut verts, mut indices) in groups {
        let tiled = if surface.image.is_some() || surface.image_back.is_some() {
            untile(&mut verts, &mut indices)
        } else {
            0
        };
        let mut left = tiled;
        for (v, i) in split_u16(&verts, &indices) {
            let skin = PartSkin {
                base: v.iter().map(|s| s.base).collect(),
                joints: v.iter().map(|s| s.joints).collect(),
                weights: v.iter().map(|s| s.weights).collect(),
            };
            let at_rest = crate::rig::skin(&skin, &rest);
            parts.push(crate::geom::ModelPart {
                tiled: std::mem::take(&mut left),
                tess: std::sync::Arc::new(crate::mesh::Tessellation {
                    verts: at_rest,
                    indices: i,
                    rim_index: None,
                    back_index: None,
                }),
                surface: surface.clone(),
                skin: Some(std::sync::Arc::new(skin)),
            });
        }
    }
    if parts.is_empty() {
        return Err(Error::Gltf(format!("{key}: no triangle primitives")));
    }
    Ok(ImportedModel {
        parts,
        images,
        rig: Some(rig),
    })
}

/// A file with no skins and no animations: node transforms baked in.
fn static_model(key: &str, bytes: &[u8]) -> Result<(Vec<crate::geom::ModelPart>, Images), Error> {
    let imported = scene_from_gltf(bytes)?;
    let prefixed = |k: &str| format!("{key}/{k}");
    let mut images = Images::new();
    for (k, img) in imported.images.iter() {
        images.insert(prefixed(k), img.width, img.height, img.pixels.clone())?;
    }
    // Surfaces in first-seen order, each with its merged triangles.
    let mut groups: Vec<(Surface, Vec<LocalVert>, Vec<u32>)> = Vec::new();
    for node in imported.scene.nodes() {
        let Kind::Mesh {
            geom: Some(g),
            bind,
            ..
        } = &node.kind
        else {
            continue;
        };
        let Some(blob) = imported.geom.get(g) else {
            continue;
        };
        let m = imported.scene.world_matrix(&node.name)?;
        let nxf = crate::math::NormalXf::of(m);
        let mut surf = bind.resolve(&imported.materials)?.clone();
        surf.image = surf.image.as_deref().map(prefixed);
        surf.image_back = surf.image_back.as_deref().map(prefixed);
        let i = match groups.iter().position(|(s, _, _)| *s == surf) {
            Some(i) => i,
            None => {
                groups.push((surf, Vec::new(), Vec::new()));
                groups.len() - 1
            }
        };
        let (_, verts, indices) = &mut groups[i];
        let base = verts.len() as u32;
        verts.extend(blob.verts.iter().map(|v| LocalVert {
            pos: transform_point(m, v.pos),
            nrm: nxf.apply(v.nrm),
            ..*v
        }));
        indices.extend(blob.indices.iter().map(|&i| base + i as u32));
    }
    let mut parts = Vec::new();
    for (surface, mut verts, mut indices) in groups {
        let tiled = if surface.image.is_some() || surface.image_back.is_some() {
            untile(&mut verts, &mut indices)
        } else {
            0
        };
        let pieces = split_u16(&verts, &indices);
        let mut left = tiled;
        for (v, i) in pieces {
            // The count rides on the first part; the dump sums them.
            let tiled = std::mem::take(&mut left);
            parts.push(crate::geom::ModelPart {
                tiled,
                tess: std::sync::Arc::new(crate::mesh::Tessellation {
                    verts: v,
                    indices: i,
                    rim_index: None,
                    back_index: None,
                }),
                surface: surface.clone(),
                skin: None,
            });
        }
    }
    if parts.is_empty() {
        return Err(Error::Gltf(format!("{key}: no triangle primitives")));
    }
    Ok((parts, images))
}

fn walk(
    node: gltf::Node<'_>,
    parent: Option<&str>,
    parent_world: Mat4,
    mesh_keys: &[Vec<PrimSlot>],
    scene: &mut Scene,
    used: &mut Vec<String>,
    cameras: &mut Vec<(String, Vec3, Vec3, Vec3, f32)>,
) -> Result<(), Error> {
    let xf = node_xf(&node);
    let world = mat4_mul(parent_world, mat4_trs(xf.t, xf.r, xf.s));
    let prim_keys = node
        .mesh()
        .and_then(|m| mesh_keys.get(m.index()))
        .cloned()
        .unwrap_or_default();
    let children: Vec<_> = node.children().collect();
    let raw = node
        .name()
        .map(str::to_string)
        .unwrap_or_else(|| format!("node{}", node.index()));
    let camera_only = prim_keys.is_empty() && children.is_empty() && node.camera().is_some();
    let scene_name = if camera_only {
        None
    } else {
        let name = unique_key(used, &raw);
        used.push(name.clone());
        insert_node(scene, &name, parent, xf, &prim_keys, used)?;
        Some(name)
    };
    let child_parent = scene_name.as_deref().or(parent);
    for child in children {
        walk(child, child_parent, world, mesh_keys, scene, used, cameras)?;
    }
    if let Some(cam) = node.camera() {
        if let Some((eye, at, up, fov)) = bake_camera(&cam, world) {
            let cam_name = if camera_only {
                raw
            } else if cameras.is_empty() {
                "cam".into()
            } else {
                format!("{raw}-cam")
            };
            cameras.push((cam_name, eye, at, up, fov));
        }
    }
    Ok(())
}

fn insert_node(
    scene: &mut Scene,
    name: &str,
    parent: Option<&str>,
    xf: Transform,
    prim_keys: &[PrimSlot],
    used: &mut Vec<String>,
) -> Result<(), Error> {
    let parent_s = parent.map(str::to_string);
    match prim_keys {
        [] => {
            let mut n = Node::group(name);
            n.parent = parent_s;
            n.xf = xf;
            scene.insert(n)?;
        }
        [slot] => {
            let mut n = Node::geom(name, &slot.geom);
            if let Some(m) = &slot.mat {
                n = n.mat(m);
            }
            n.parent = parent_s;
            n.xf = xf;
            scene.insert(n)?;
        }
        many => {
            let mut n = Node::group(name);
            n.parent = parent_s;
            n.xf = xf;
            scene.insert(n)?;
            for slot in many {
                let child = unique_key(used, &slot.geom);
                used.push(child.clone());
                let mut mesh = Node::geom(child, &slot.geom).parent(name);
                if let Some(m) = &slot.mat {
                    mesh = mesh.mat(m);
                }
                scene.insert(mesh)?;
            }
        }
    }
    Ok(())
}

fn intern_images(
    g: &gltf::Document,
    decoded: &[gltf::image::Data],
    images: &mut Images,
) -> Result<Vec<Option<String>>, Error> {
    let list: Vec<_> = g.images().collect();
    if decoded.len() != list.len() {
        return Err(Error::Gltf(format!(
            "image count {} != {}",
            decoded.len(),
            list.len()
        )));
    }
    let mut keys = vec![None; list.len()];
    let mut used = Vec::new();
    for (img, data) in list.iter().zip(decoded) {
        let i = img.index();
        if i >= keys.len() {
            keys.resize(i + 1, None);
        }
        let Ok(px) = rgba8(data) else {
            continue;
        };
        let raw = img
            .name()
            .map(str::to_string)
            .unwrap_or_else(|| format!("img{i}"));
        let key = unique_key(&used, &raw);
        images.insert(&key, data.width, data.height, px)?;
        used.push(key.clone());
        keys[i] = Some(key);
    }
    Ok(keys)
}

fn rgba8(data: &gltf::image::Data) -> Result<Vec<u8>, Error> {
    use gltf::image::Format;
    let n = data.width as usize * data.height as usize;
    match data.format {
        Format::R8G8B8A8 if data.pixels.len() == n * 4 => Ok(data.pixels.clone()),
        Format::R8G8B8 if data.pixels.len() == n * 3 => Ok(data
            .pixels
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect()),
        Format::R8 if data.pixels.len() == n => {
            Ok(data.pixels.iter().flat_map(|&r| [r, r, r, 255]).collect())
        }
        Format::R8G8 if data.pixels.len() == n * 2 => Ok(data
            .pixels
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[0], c[0], c[1]])
            .collect()),
        other => Err(Error::Gltf(format!("image format {other:?}"))),
    }
}

fn intern_materials(
    g: &gltf::Document,
    img_keys: &[Option<String>],
    materials: &mut Materials,
) -> Result<Vec<Option<String>>, Error> {
    let list: Vec<_> = g.materials().collect();
    let mut keys = vec![None; list.len()];
    let mut used = Vec::new();
    for mat in list {
        let Some(i) = mat.index() else {
            continue;
        };
        if i >= keys.len() {
            keys.resize(i + 1, None);
        }
        let pbr = mat.pbr_metallic_roughness();
        let color = f32_rgba_u8(pbr.base_color_factor());
        let image = pbr.base_color_texture().and_then(|info| {
            img_keys
                .get(info.texture().source().index())
                .cloned()
                .flatten()
        });
        let unlit = mat.unlit();
        if image.is_none() && color == [255, 255, 255, 255] && !unlit {
            continue;
        }
        let raw = mat
            .name()
            .map(str::to_string)
            .unwrap_or_else(|| format!("mat{i}"));
        let key = unique_key(&used, &raw);
        materials.insert(
            &key,
            Surface {
                shade: if unlit { Shade::Unlit } else { Shade::Lambert },
                color,
                image,
                image_back: None,
            },
        )?;
        used.push(key.clone());
        keys[i] = Some(key);
    }
    Ok(keys)
}

fn f32_rgba_u8(c: [f32; 4]) -> [u8; 4] {
    [
        (c[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (c[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (c[2].clamp(0.0, 1.0) * 255.0).round() as u8,
        (c[3].clamp(0.0, 1.0) * 255.0).round() as u8,
    ]
}

fn node_xf(node: &gltf::Node<'_>) -> Transform {
    let (t, q, s) = node.transform().decomposed();
    Transform {
        t: Vec3::new(t[0], t[1], t[2]),
        r: quat_xyzw_to_euler(q[0], q[1], q[2], q[3]),
        s: Vec3::new(s[0], s[1], s[2]),
    }
}

/// glTF quaternion is `[x,y,z,w]`. Euler matches [`crate::math::mat4_trs`]
/// (`R = Rz · Ry · Rx`). Decomposed from the rotation matrix, so it holds at
/// gimbal lock: at ±90° pitch only `z − x` (or `z + x`) is defined, and the
/// textbook per-angle formulas pick a wrong pair there — the Cesium Milk
/// Truck's `Yup2Zup` node came out tipped on its side.
fn quat_xyzw_to_euler(x: f32, y: f32, z: f32, w: f32) -> Vec3 {
    let n = (x * x + y * y + z * z + w * w).sqrt().max(1e-12);
    let (x, y, z, w) = (x / n, y / n, z / n, w / n);
    let r00 = 1.0 - 2.0 * (y * y + z * z);
    let r01 = 2.0 * (x * y - w * z);
    let r02 = 2.0 * (x * z + w * y);
    let r10 = 2.0 * (x * y + w * z);
    let r20 = 2.0 * (x * z - w * y);
    let r21 = 2.0 * (y * z + w * x);
    let r22 = 1.0 - 2.0 * (x * x + y * y);
    if r20.abs() < 0.999_99 {
        Vec3::new(r21.atan2(r22), (-r20).asin(), r10.atan2(r00))
    } else if r20 < 0.0 {
        // Pitch +90°: with z = 0, r01 = sin x and r02 = cos x.
        Vec3::new(r01.atan2(r02), std::f32::consts::FRAC_PI_2, 0.0)
    } else {
        // Pitch −90°: with z = 0, r01 = −sin x and r02 = −cos x.
        Vec3::new((-r01).atan2(-r02), -std::f32::consts::FRAC_PI_2, 0.0)
    }
}

fn bake_camera(cam: &gltf::Camera<'_>, world: Mat4) -> Option<(Vec3, Vec3, Vec3, f32)> {
    let gltf::camera::Projection::Perspective(p) = cam.projection() else {
        return None;
    };
    let eye = transform_point(world, Vec3::ZERO);
    let at = transform_point(world, Vec3::new(0.0, 0.0, -1.0));
    let up = transform_dir(world, Vec3::Y);
    let at = if (at - eye).length() < 1e-5 {
        eye + transform_dir(world, Vec3::new(0.0, 0.0, -1.0))
    } else {
        at
    };
    let fov = p.yfov();
    let fov = if fov > 1e-4 { fov } else { DEFAULT_FOV };
    Some((eye, at, up, fov))
}

fn unique_key(used: &[String], base: &str) -> String {
    if !used.iter().any(|k| k == base) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let k = format!("{base}_{n}");
        if !used.iter().any(|u| u == &k) {
            return k;
        }
        n += 1;
    }
}

fn accumulate_normals(pos: &[[f32; 3]], idx: &[u32]) -> Vec<Vec3> {
    let mut acc = vec![Vec3::ZERO; pos.len()];
    for tri in idx.as_chunks::<3>().0 {
        let p0 = Vec3::from_array(pos[tri[0] as usize]);
        let p1 = Vec3::from_array(pos[tri[1] as usize]);
        let p2 = Vec3::from_array(pos[tri[2] as usize]);
        let n = (p1 - p0).cross(p2 - p0);
        acc[tri[0] as usize] = acc[tri[0] as usize] + n;
        acc[tri[1] as usize] = acc[tri[1] as usize] + n;
        acc[tri[2] as usize] = acc[tri[2] as usize] + n;
    }
    for n in &mut acc {
        *n = n.normalized();
    }
    acc
}

#[cfg(test)]
pub fn triangle_glb() -> Vec<u8> {
    glb_nodes(
        r#"{"nodes":[0]}"#,
        r#"[{"mesh":0,"name":"tri"}]"#,
        &triangle_bin(),
    )
}

#[cfg(test)]
pub fn parented_glb() -> Vec<u8> {
    glb_nodes(
        r#"{"nodes":[0]}"#,
        r#"[{"name":"stand","translation":[0,0.4,0],"children":[1]},{"mesh":0,"name":"tri"}]"#,
        &triangle_bin(),
    )
}

#[cfg(test)]
pub fn colored_glb() -> Vec<u8> {
    glb_doc(
        r#"{"nodes":[0]}"#,
        r#"[{"mesh":0,"name":"tri"}]"#,
        r#"[{"name":"tri","primitives":[{"attributes":{"POSITION":0,"NORMAL":1,"TEXCOORD_0":2},"indices":3,"material":0}]}]"#,
        r#"[{"name":"red","pbrMetallicRoughness":{"baseColorFactor":[1,0,0,1]}}]"#,
        &triangle_bin(),
    )
}

/// 1×1 red PNG in a buffer view, bound as base-color. Proves `gltf::import` decoded
/// the image and we interned it as `m=` rather than hand-slicing buffer 0.
#[cfg(test)]
pub fn textured_glb() -> Vec<u8> {
    let png: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8,
        0xCF, 0xC0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    let mut bin = triangle_bin();
    let png_off = bin.len();
    let png_len = png.len();
    bin.extend_from_slice(png);
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0,"name":"tri"}}],"meshes":[{{"name":"tri","primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1,"TEXCOORD_0":2}},"indices":3,"material":0}}]}}],"materials":[{{"name":"painted","pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}}}}}}],"textures":[{{"source":0}}],"images":[{{"name":"tex","bufferView":4,"mimeType":"image/png"}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","max":[1,1,0],"min":[-1,-1,0]}},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":2,"componentType":5126,"count":3,"type":"VEC2"}},{{"bufferView":3,"componentType":5123,"count":3,"type":"SCALAR"}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":36}},{{"buffer":0,"byteOffset":72,"byteLength":24}},{{"buffer":0,"byteOffset":96,"byteLength":6}},{{"buffer":0,"byteOffset":{png_off},"byteLength":{png_len}}}],"buffers":[{{"byteLength":{}}}]}}"#,
        bin.len()
    );
    pack_glb(json.as_bytes(), &bin)
}

#[cfg(test)]
fn triangle_bin() -> Vec<u8> {
    let mut bin = Vec::new();
    for p in [[-1.0f32, -1.0, 0.0], [1.0, -1.0, 0.0], [0.0, 1.0, 0.0]] {
        for c in p {
            bin.extend_from_slice(&c.to_le_bytes());
        }
    }
    for _ in 0..3 {
        for c in [0.0f32, 0.0, 1.0] {
            bin.extend_from_slice(&c.to_le_bytes());
        }
    }
    for uv in [[0.0f32, 1.0], [1.0, 1.0], [0.5, 0.0]] {
        for c in uv {
            bin.extend_from_slice(&c.to_le_bytes());
        }
    }
    for i in [0u16, 1, 2] {
        bin.extend_from_slice(&i.to_le_bytes());
    }
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    bin
}

#[cfg(test)]
fn glb_nodes(scene_json: &str, nodes_json: &str, bin: &[u8]) -> Vec<u8> {
    glb_doc(
        scene_json,
        nodes_json,
        r#"[{"name":"tri","primitives":[{"attributes":{"POSITION":0,"NORMAL":1,"TEXCOORD_0":2},"indices":3}]}]"#,
        "[]",
        bin,
    )
}

#[cfg(test)]
fn glb_doc(
    scene_json: &str,
    nodes_json: &str,
    meshes_json: &str,
    materials_json: &str,
    bin: &[u8],
) -> Vec<u8> {
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{scene_json}],"nodes":{nodes_json},"meshes":{meshes_json},"materials":{materials_json},"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","max":[1,1,0],"min":[-1,-1,0]}},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":2,"componentType":5126,"count":3,"type":"VEC2"}},{{"bufferView":3,"componentType":5123,"count":3,"type":"SCALAR"}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":36}},{{"buffer":0,"byteOffset":72,"byteLength":24}},{{"buffer":0,"byteOffset":96,"byteLength":6}}],"buffers":[{{"byteLength":{}}}]}}"#,
        bin.len()
    );
    pack_glb(json.as_bytes(), bin)
}

#[cfg(test)]
fn pack_glb(json: &[u8], bin: &[u8]) -> Vec<u8> {
    fn pad4(n: usize) -> usize {
        n.div_ceil(4) * 4
    }
    let json_len = pad4(json.len());
    let bin_len = pad4(bin.len());
    let total = 12 + 8 + json_len + 8 + bin_len;
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&0x4654_6C67u32.to_le_bytes());
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(json_len as u32).to_le_bytes());
    out.extend_from_slice(&0x4E4F_534Au32.to_le_bytes());
    out.extend_from_slice(json);
    out.resize(12 + 8 + json_len, b' ');
    out.extend_from_slice(&(bin_len as u32).to_le_bytes());
    out.extend_from_slice(&0x004E_4942u32.to_le_bytes());
    out.extend_from_slice(bin);
    out.resize(total, 0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::transform_point;

    fn khronos(name: &str) -> Vec<u8> {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/khronos/Models");
        std::fs::read(format!("{root}/{name}/{name}.glb")).unwrap()
    }

    /// A vertex shared by a tile-0 and a tile-2 triangle is copied for the
    /// second, and a triangle spanning two tiles is counted and left alone.
    #[test]
    fn uvs_move_into_the_first_tile_per_triangle() {
        let v = |u: f32, w: f32| LocalVert {
            pos: Vec3::ZERO,
            nrm: Vec3::new(0.0, 0.0, 1.0),
            uv: [u, w],
            rgb: [1.0; 3],
        };
        let mut verts = vec![
            v(0.0, 0.0),
            v(1.0, 0.0),
            v(1.0, 1.0),
            v(2.0, 0.0),
            v(3.0, 1.0),
            v(0.5, 0.0),
        ];
        // tile 0; tile 2 sharing vertex 3 (u=2, which is tile 2's left edge);
        // spans u 0.5..3.0.
        let mut indices = vec![0, 1, 2, 3, 4, 3, 5, 4, 0];
        let tiled = untile(&mut verts, &mut indices);
        assert_eq!(tiled, 1);
        assert_eq!(&indices[..3], &[0, 1, 2], "tile 0 untouched");
        let moved: Vec<[f32; 2]> = indices[3..6]
            .iter()
            .map(|&i| verts[i as usize].uv)
            .collect();
        assert_eq!(moved, vec![[0.0, 0.0], [1.0, 1.0], [0.0, 0.0]]);
        assert_eq!(indices[3], indices[5], "one copy per (vertex, tile)");
        assert_eq!(&indices[6..], &[5, 4, 0], "the spanning triangle is left");
        assert_eq!(verts[3].uv, [2.0, 0.0], "the original is not rewritten");
    }

    /// A posed Fox flattens moved by its own clip: partway through Walk it
    /// is not rest, a whole clip length later it is the same frame again,
    /// and a clip it lacks is an error.
    #[test]
    fn a_pose_skins_the_fox_by_its_own_clip() {
        let mut g = Geometry::new();
        let fox = model_from_gltf("fox", &khronos("Fox")).unwrap();
        let walk = fox.rig.as_ref().unwrap().clip("Walk").unwrap().length;
        g.insert_imported("fox", fox).unwrap();
        let flat = |pose: &str| {
            let src =
                format!("sg1\nfox Mesh Geom g=fox{pose}\ncam Camera eye=0,80,300 at=0,40,0\n");
            let scene = Scene::from_dump(&src).unwrap();
            assert_eq!(scene.dump(), src, "pose round-trips");
            scene
                .draw_list_with(&g, &Materials::new(), &mut crate::Fonts::new())
                .map(|l| l.meshes[0].verts.iter().map(|v| v.0).collect::<Vec<_>>())
        };
        let rest = flat("").unwrap();
        let first = flat(" pose=Walk@0").unwrap();
        let mid = flat(&format!(" pose=Walk@{:.4}", walk * 0.5)).unwrap();
        let again = flat(&format!(" pose=Walk@{:.4}", walk * 1.5)).unwrap();
        let moved = |a: &[Vec3], b: &[Vec3]| {
            a.iter()
                .zip(b)
                .map(|(p, q)| (*p - *q).length())
                .fold(0.0, f32::max)
        };
        assert!(moved(&rest, &mid) > 5.0, "the walk moves the legs");
        assert!(
            moved(&mid, &again) < 1e-2,
            "a clip length later is the same frame"
        );
        // Walk's first frame carries the tail high, so it is not rest, and
        // half a stride later the legs have swapped.
        assert!(moved(&first, &mid) > 5.0, "frames of one clip differ");
        match flat(" pose=Fly@0") {
            Err(Error::UnknownPose(node, clip)) => {
                assert_eq!((node.as_str(), clip.as_str()), ("fox", "Fly"))
            }
            other => panic!("{other:?}"),
        }
    }

    /// The milk truck has no skin: its wheels are rigid parts their own
    /// nodes turn. Posing it turns them about their axles, so the truck
    /// moves but its bounds do not.
    #[test]
    fn a_rigid_part_turns_with_its_node() {
        let mut g = Geometry::new();
        g.insert_imported(
            "truck",
            model_from_gltf("truck", &khronos("CesiumMilkTruck")).unwrap(),
        )
        .unwrap();
        let flat = |pose: &str| {
            let src =
                format!("sg1\ntruck Mesh Geom g=truck{pose}\ncam Camera eye=0,2,8 at=0,1,0\n");
            let list = Scene::from_dump(&src)
                .unwrap()
                .draw_list_with(&g, &Materials::new(), &mut crate::Fonts::new())
                .unwrap();
            list.meshes
                .iter()
                .flat_map(|m| m.verts.iter().map(|v| v.0))
                .collect::<Vec<_>>()
        };
        let (rest, turned) = (flat(""), flat(" pose=Wheels@0.3"));
        let moved = rest
            .iter()
            .zip(&turned)
            .map(|(a, b)| (*a - *b).length())
            .fold(0.0, f32::max);
        assert!(moved > 0.05, "the wheels turn: {moved}");
        let bounds = |v: &[Vec3]| v.iter().fold(f32::MIN, |m, p| m.max(p.y));
        assert!(
            (bounds(&rest) - bounds(&turned)).abs() < 1e-3,
            "the body stays put"
        );
    }

    /// The Khronos fixtures in `tests/fixtures/khronos` import as scenes
    /// and as models; the census is printed with `--nocapture`.
    #[test]
    fn khronos_samples_import_as_models() {
        let mut g = Geometry::new();
        for (name, parts, images) in [
            ("Duck", 1, 1),
            ("Fox", 1, 1),
            // truck and wheels share one textured surface: 3 parts, not 4.
            ("CesiumMilkTruck", 3, 1),
            ("BoxTextured", 1, 1),
        ] {
            let bytes = khronos(name);
            scene_from_gltf(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            let key = name.to_ascii_lowercase();
            let imported = model_from_gltf(&key, &bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            let (model, imgs) = (&imported.parts, &imported.images);
            assert_eq!(model.len(), parts, "{name}: one part per surface");
            assert_eq!(imgs.iter().count(), images, "{name}");
            for p in model {
                if let Some(img) = &p.surface.image {
                    assert!(img.starts_with(&format!("{key}/")), "{img}");
                    assert!(imgs.get(img).is_some(), "{name}: {img} interned");
                    // BoxTextured's u runs 0..6 under REPEAT: all moved in.
                    assert_eq!(p.tiled, 0, "{name}");
                    for v in &p.tess.verts {
                        let ok = |c: f32| (-1e-4..=1.0 + 1e-4).contains(&c);
                        assert!(ok(v.uv[0]) && ok(v.uv[1]), "{name}: uv {:?}", v.uv);
                    }
                }
            }
            g.insert_imported(&key, imported).unwrap();
        }
        let census = g.dump();
        println!("{census}");
        // The Fox is skinned and the truck's wheels are animated: both rigged,
        // with the files' own clips; the duck and the box are not.
        let line = |k: &str| {
            census
                .lines()
                .find(|l| l.starts_with(k))
                .unwrap()
                .to_string()
        };
        assert!(line("fox ").contains(" clips=Survey:"), "{}", line("fox "));
        assert!(line("fox ").contains("Walk:") && line("fox ").contains("Run:"));
        assert!(
            line("cesiummilktruck ").contains(" rig="),
            "{}",
            line("cesiummilktruck ")
        );
        assert!(!line("duck ").contains("rig="));
        assert!(!line("boxtextured ").contains("rig="));
        // The Duck's own node scale (0.01) is baked in: about 1.65 m long.
        let duck = census.lines().find(|l| l.starts_with("duck ")).unwrap();
        assert!(
            duck.contains("bbox=-0.693,0.0993,-0.6133:0.9618,1.6397,0.5393"),
            "{duck}"
        );
        // The Fox's mesh has no node scale: centimetres, 155 units long.
        let fox = census.lines().find(|l| l.starts_with("fox ")).unwrap();
        assert!(fox.contains(":12.5927,78.9072,66.6249"), "{fox}");
    }

    /// Quaternion → our Euler → matrix is the quaternion's own matrix, for
    /// ordinary rotations and at both gimbal locks.
    #[test]
    fn quaternion_euler_survives_gimbal_lock() {
        let qmat = |x: f32, y: f32, z: f32, w: f32| {
            let n = (x * x + y * y + z * z + w * w).sqrt();
            let (x, y, z, w) = (x / n, y / n, z / n, w / n);
            [
                [
                    1.0 - 2.0 * (y * y + z * z),
                    2.0 * (x * y - w * z),
                    2.0 * (x * z + w * y),
                ],
                [
                    2.0 * (x * y + w * z),
                    1.0 - 2.0 * (x * x + z * z),
                    2.0 * (y * z - w * x),
                ],
                [
                    2.0 * (x * z - w * y),
                    2.0 * (y * z + w * x),
                    1.0 - 2.0 * (x * x + y * y),
                ],
            ]
        };
        let h = std::f32::consts::FRAC_1_SQRT_2;
        for q in [
            [0.0, 0.0, 0.0, 1.0],
            [0.1, 0.2, 0.3, 0.9],
            [-0.5, 0.5, 0.5, 0.5],
            [0.5, -0.5, 0.5, 0.5],
            [0.0, h, 0.0, h],
            [0.0, -h, 0.0, h],
            [0.3, h, -0.2, h],
            [0.7, 0.1, -0.6, 0.3],
        ] {
            let e = quat_xyzw_to_euler(q[0], q[1], q[2], q[3]);
            let m = mat4_trs(Vec3::ZERO, e, Vec3::new(1.0, 1.0, 1.0));
            let want = qmat(q[0], q[1], q[2], q[3]);
            for (r, row) in want.iter().enumerate() {
                for (c, w) in row.iter().enumerate() {
                    // mat4_trs is column-major: m[col][row].
                    assert!((m[c][r] - w).abs() < 1e-3, "q={q:?} e={e:?} r{r}c{c}");
                }
            }
        }
    }

    #[test]
    fn a_primitive_over_the_16_bit_limit_splits_on_whole_triangles() {
        let n = 70_000usize;
        let verts: Vec<LocalVert> = (0..n)
            .map(|i| LocalVert::new(Vec3::new(i as f32, 0.0, 0.0), Vec3::Y, [0.0, 0.0]))
            .collect();
        // Triangles that each use three fresh vertices, then one reusing the
        // first three: the split must keep every triangle whole.
        let mut idx: Vec<u32> = (0..(n as u32 / 3 * 3)).collect();
        idx.extend([0, 1, 2]);
        let pieces = split_u16(&verts, &idx);
        assert!(pieces.len() >= 2, "{}", pieces.len());
        let tris: usize = pieces.iter().map(|(_, i)| i.len() / 3).sum();
        assert_eq!(tris, idx.len() / 3);
        for (v, i) in &pieces {
            assert!(v.len() <= u16::MAX as usize);
            assert!(i.iter().all(|&k| (k as usize) < v.len()));
        }
    }

    #[test]
    fn triangle_glb_roundtrip() {
        let bytes = triangle_glb();
        let mut g = Geometry::new();
        let keys = g.insert_gltf(&bytes).unwrap();
        assert_eq!(keys, ["tri"]);
        let blob = g.get("tri").unwrap();
        assert_eq!(blob.verts.len(), 3);
        assert_eq!(blob.indices, [0, 1, 2]);
        let imported = scene_from_gltf(&bytes).unwrap();
        assert!(imported.scene.get("tri").is_some());
        assert_eq!(
            imported.scene.get("tri").unwrap().parent.as_deref(),
            Some("gltf")
        );
        let d = imported
            .scene
            .draw_list_with(
                &imported.geom,
                &imported.materials,
                &mut crate::Fonts::new(),
            )
            .unwrap();
        assert_eq!(d.meshes[0].prim, crate::kind::Prim::Geom);
        assert_eq!(d.meshes[0].verts.len(), 3);
        let dump = imported.scene.dump();
        Scene::from_dump(&dump).unwrap();
        assert!(dump.contains("tri Mesh Geom g=tri p=gltf"), "{dump}");
        assert!(dump.contains("sun Light Dir"), "{dump}");
    }

    #[test]
    fn parented_glb_is_sg1_and_hits() {
        let bytes = parented_glb();
        let imported = scene_from_gltf(&bytes).unwrap();
        let scene = imported.scene;
        let geom = imported.geom;
        assert_eq!(
            geom.dump()
                .lines()
                .filter(|l| l.starts_with("tri "))
                .count(),
            1
        );
        let dump = scene.dump();
        assert!(dump.contains("stand Group p=gltf t=0,0.4,0"), "{dump}");
        assert!(dump.contains("tri Mesh Geom g=tri p=stand"), "{dump}");
        assert_eq!(scene.get("tri").unwrap().parent.as_deref(), Some("stand"));
        let w = transform_point(scene.world_matrix("tri").unwrap(), Vec3::ZERO);
        assert!(
            w.is_near(Vec3::new(0.0, 0.4, 0.0), 1e-4),
            "world origin {w:?}"
        );
        let round = Scene::from_dump(&dump).unwrap();
        assert_eq!(round.dump(), dump);
        let d = scene
            .draw_list_with(&geom, &imported.materials, &mut crate::Fonts::new())
            .unwrap();
        let hit = d
            .hit(Vec3::new(0.0, 0.4, 4.0), Vec3::new(0.0, 0.0, -1.0))
            .expect("hit tri");
        assert_eq!(hit.name, "tri");
        assert_eq!(hit.face, crate::HitFace::Front);
        assert!(d.dump().contains("mesh tri Geom"), "{}", d.dump());
    }

    #[test]
    fn colored_glb_interns_mt1() {
        let bytes = colored_glb();
        let imported = scene_from_gltf(&bytes).unwrap();
        let dump = imported.scene.dump();
        assert!(
            dump.contains("tri Mesh Geom g=tri mat=red p=gltf"),
            "{dump}"
        );
        assert_eq!(imported.materials.dump(), "mt1\nred c=ff0000\n");
        Materials::from_dump(&imported.materials.dump()).unwrap();
        let d = imported
            .scene
            .draw_list_with(
                &imported.geom,
                &imported.materials,
                &mut crate::Fonts::new(),
            )
            .unwrap();
        assert_eq!(d.meshes[0].color, [255, 0, 0, 255]);
        assert!(imported.images.is_empty());
    }

    #[test]
    fn textured_glb_interns_png() {
        let bytes = textured_glb();
        let imported = scene_from_gltf(&bytes).unwrap();
        let dump = imported.scene.dump();
        assert!(
            dump.contains("tri Mesh Geom g=tri mat=painted p=gltf"),
            "{dump}"
        );
        assert_eq!(imported.materials.dump(), "mt1\npainted m=tex\n");
        let tex = imported.images.get("tex").expect("tex");
        assert_eq!((tex.width, tex.height), (1, 1));
        assert_eq!(&tex.pixels[..4], &[255, 0, 0, 255]);
        let d = imported
            .scene
            .draw_list_with(
                &imported.geom,
                &imported.materials,
                &mut crate::Fonts::new(),
            )
            .unwrap();
        assert_eq!(d.meshes[0].image.as_deref(), Some("tex"));
    }
}
