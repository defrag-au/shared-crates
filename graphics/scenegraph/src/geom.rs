//! Interned triangle blobs. The graph stores geometry *keys*; verts live here.

use std::collections::HashMap;

use crate::error::Error;
use crate::material::Surface;
use crate::math::{compact_f, Vec3};
use crate::mesh::{tet, LocalVert};

#[derive(Clone, Debug)]
pub struct MeshBlob {
    pub verts: Vec<LocalVert>,
    pub indices: Vec<u16>,
}

impl MeshBlob {
    /// Triangle list. Empty and leftover indices are separate from "divisible
    /// by 3" — `0.is_multiple_of(3)` is true.
    pub fn triangles(verts: Vec<LocalVert>, indices: Vec<u16>) -> Option<Self> {
        let (tris, leftover) = indices.as_chunks::<3>();
        if !leftover.is_empty() {
            return None;
        }
        if tris.is_empty() {
            return None;
        }
        if verts.is_empty() {
            return None;
        }
        if tris.iter().flatten().any(|&i| (i as usize) >= verts.len()) {
            return None;
        }
        Some(Self { verts, indices })
    }
}

/// One part of a model: a triangle mesh in the model's own frame (its glTF
/// node transforms baked in) and the surface it was authored with. Parts
/// split by surface, and by the 16-bit index limit.
#[derive(Clone, Debug)]
pub struct ModelPart {
    pub tess: std::sync::Arc<crate::mesh::Tessellation>,
    pub surface: Surface,
    /// Triangles whose UVs span more than one texture tile. The backends
    /// clamp, so these draw smeared; every other triangle was moved into
    /// the first tile at import. Non-zero shows in [`Geometry::dump`].
    pub tiled: u32,
    /// A rigged model's part: its vertices as authored and what moves each
    /// ([`crate::rig`]). `tess` is then the rig at rest.
    pub skin: Option<std::sync::Arc<crate::rig::PartSkin>>,
}

#[derive(Clone, Debug, Default)]
pub struct Geometry {
    map: HashMap<String, MeshBlob>,
    /// Whole models by key: a `Mesh Geom g=KEY` node draws every part, and
    /// picks as that one node.
    models: HashMap<String, Vec<ModelPart>>,
    /// A rigged model's skeleton and clips, by its model key.
    rigs: HashMap<String, std::sync::Arc<crate::rig::Rig>>,
}

impl Geometry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern a whole model under `key`. A plain blob of the same key is
    /// replaced: one key, one thing to draw.
    pub fn insert_model(
        &mut self,
        key: impl Into<String>,
        parts: Vec<ModelPart>,
    ) -> Result<(), Error> {
        let key = key.into();
        if parts.is_empty() || parts.iter().any(|p| p.tess.indices.is_empty()) {
            return Err(Error::BadGeom(key));
        }
        self.map.remove(&key);
        self.rigs.remove(&key);
        self.models.insert(key, parts);
        Ok(())
    }

    /// Intern a rigged model: its parts, and the rig a `pose=` samples.
    /// Every part must carry its skin.
    pub fn insert_rigged(
        &mut self,
        key: impl Into<String>,
        parts: Vec<ModelPart>,
        rig: crate::rig::Rig,
    ) -> Result<(), Error> {
        let key = key.into();
        if parts.iter().any(|p| p.skin.is_none()) {
            return Err(Error::BadGeom(key));
        }
        self.insert_model(key.clone(), parts)?;
        self.rigs.insert(key, std::sync::Arc::new(rig));
        Ok(())
    }

    /// Intern an imported model under `key`, rigged or not; its images
    /// come back for the caller's [`crate::Images`].
    #[cfg(feature = "gltf")]
    pub fn insert_imported(
        &mut self,
        key: impl Into<String>,
        model: crate::ImportedModel,
    ) -> Result<crate::Images, Error> {
        match model.rig {
            Some(rig) => self.insert_rigged(key, model.parts, rig)?,
            None => self.insert_model(key, model.parts)?,
        }
        Ok(model.images)
    }

    pub fn model(&self, key: &str) -> Option<&[ModelPart]> {
        self.models.get(key).map(Vec::as_slice)
    }

    pub fn rig(&self, key: &str) -> Option<&crate::rig::Rig> {
        self.rigs.get(key).map(|r| r.as_ref())
    }

    pub fn insert(
        &mut self,
        key: impl Into<String>,
        verts: Vec<LocalVert>,
        indices: Vec<u16>,
    ) -> Result<(), Error> {
        let key = key.into();
        let Some(blob) = MeshBlob::triangles(verts, indices) else {
            return Err(Error::BadGeom(key));
        };
        self.map.insert(key, blob);
        Ok(())
    }

    /// Unit tet, base on y=0, apex at y=1. Shared fixture key `tet`.
    pub fn insert_tet(&mut self, key: impl Into<String>) -> Result<(), Error> {
        let (verts, indices) = tet();
        self.insert(key, verts, indices)
    }

    pub fn get(&self, key: &str) -> Option<&MeshBlob> {
        self.map.get(key)
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty() && self.models.is_empty()
    }

    /// Tight census — keys, face/vert counts, local bbox. Not a load format.
    pub fn dump(&self) -> String {
        let mut names: Vec<_> = self.map.keys().collect();
        names.sort();
        let mut out = String::from("geom\n");
        for name in names {
            let blob = &self.map[name];
            let faces = blob.indices.len() / 3;
            let (mn, mx) = bbox(&blob.verts);
            out.push_str(name);
            out.push_str(&format!(
                " {faces}f {}v bbox={},{},{}:{},{},{}\n",
                blob.verts.len(),
                compact_f(mn.x),
                compact_f(mn.y),
                compact_f(mn.z),
                compact_f(mx.x),
                compact_f(mx.y),
                compact_f(mx.z),
            ));
        }
        let mut models: Vec<_> = self.models.keys().collect();
        models.sort();
        for name in models {
            let parts = &self.models[name];
            let faces: usize = parts.iter().map(|p| p.tess.indices.len() / 3).sum();
            let verts: usize = parts.iter().map(|p| p.tess.verts.len()).sum();
            let all: Vec<LocalVert> = parts
                .iter()
                .flat_map(|p| p.tess.verts.iter().copied())
                .collect();
            let (mn, mx) = bbox(&all);
            let tiled: u32 = parts.iter().map(|p| p.tiled).sum();
            let tiled = if tiled > 0 {
                format!(" tiled={tiled}")
            } else {
                String::new()
            };
            // A rig: its node count, and each clip with its length, which
            // is what `pose=CLIP@T` may name.
            let rig = match self.rigs.get(name) {
                Some(r) => {
                    let clips: Vec<String> = r
                        .clips
                        .iter()
                        .map(|c| format!("{}:{}", c.name, compact_f(c.length)))
                        .collect();
                    let clips = if clips.is_empty() {
                        "-".to_string()
                    } else {
                        clips.join(",")
                    };
                    format!(" rig={} clips={clips}", r.nodes.len())
                }
                None => String::new(),
            };
            out.push_str(&format!(
                "{name} model parts={} {faces}f {verts}v bbox={},{},{}:{},{},{}{tiled}{rig}\n",
                parts.len(),
                compact_f(mn.x),
                compact_f(mn.y),
                compact_f(mn.z),
                compact_f(mx.x),
                compact_f(mx.y),
                compact_f(mx.z),
            ));
        }
        out
    }
}

fn bbox(verts: &[LocalVert]) -> (Vec3, Vec3) {
    let mut mn = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut mx = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
    for v in verts {
        mn.x = mn.x.min(v.pos.x);
        mn.y = mn.y.min(v.pos.y);
        mn.z = mn.z.min(v.pos.z);
        mx.x = mx.x.max(v.pos.x);
        mx.y = mx.y.max(v.pos.y);
        mx.z = mx.z.max(v.pos.z);
    }
    (mn, mx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Vec3;

    fn v() -> LocalVert {
        LocalVert::new(Vec3::ZERO, Vec3::Y, [0.0, 0.0])
    }

    #[test]
    fn triangles_are_whole_faces() {
        let verts = vec![v(), v(), v()];
        assert!(MeshBlob::triangles(vec![], vec![0, 1, 2]).is_none());
        assert!(MeshBlob::triangles(verts.clone(), vec![]).is_none());
        assert!(MeshBlob::triangles(verts.clone(), vec![0, 1]).is_none());
        assert!(MeshBlob::triangles(verts.clone(), vec![0, 1, 2, 0]).is_none());
        assert!(MeshBlob::triangles(verts.clone(), vec![0, 1, 9]).is_none());
        assert!(MeshBlob::triangles(verts, vec![0, 1, 2]).is_some());
    }

    #[test]
    fn census_dump_is_sorted() {
        let mut g = Geometry::new();
        g.insert_tet("tet").unwrap();
        g.insert("tri", vec![v(), v(), v()], vec![0, 1, 2]).unwrap();
        let d = g.dump();
        assert!(d.starts_with("geom\n"), "{d}");
        assert!(d.contains("tet 4f 12v bbox=-0.5,0,-0.5:0.5,1,0.5"), "{d}");
        assert!(d.contains("tri 1f 3v"), "{d}");
        assert!(d.find("tet").unwrap() < d.find("tri").unwrap());
    }
}
