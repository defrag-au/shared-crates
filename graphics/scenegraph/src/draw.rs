/// Phase timer for [`FlattenCost`]. `wasm32-unknown-unknown` has no clock
/// (`Instant::now` panics there, on the first flatten), so phases read 0 in
/// the browser. The cost is a pane metric, never part of a dump.
struct Stopwatch(#[cfg(not(target_arch = "wasm32"))] std::time::Instant);

impl Stopwatch {
    #[cfg(not(target_arch = "wasm32"))]
    fn start() -> Self {
        Self(std::time::Instant::now())
    }

    #[cfg(target_arch = "wasm32")]
    fn start() -> Self {
        Self()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn us(&self) -> u64 {
        self.0.elapsed().as_micros() as u64
    }

    #[cfg(target_arch = "wasm32")]
    fn us(&self) -> u64 {
        0
    }
}

use crate::error::Error;
use crate::font::Fonts;
use crate::geom::Geometry;
use crate::images::Images;
use crate::kind::{Kind, Lamp, Prim, DEFAULT_FOV};
use crate::material::{Materials, Shade};
use crate::math::{
    compact_f, lerp_uv, mat4_inverse, mat4_look_at, mat4_mul, mat4_perspective, ray_triangle,
    transform_dir, transform_point, unproject, Mat4, NormalXf, Vec3,
};
use crate::scene::Scene;

#[derive(Clone, Debug)]
pub struct CameraParams {
    pub eye: Vec3,
    pub at: Vec3,
    pub up: Vec3,
    pub fov: f32,
    pub name: String,
}

impl CameraParams {
    /// Rest-pose look. Up is +Y, fov is [`DEFAULT_FOV`].
    pub fn look(name: impl Into<String>, eye: Vec3, at: Vec3) -> Self {
        Self {
            eye,
            at,
            up: Vec3::Y,
            fov: DEFAULT_FOV,
            name: name.into(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum WorldLamp {
    Dir { dir: Vec3 },
    Point { pos: Vec3, range: f32 },
}

#[derive(Clone, Debug)]
pub struct LightParams {
    pub lamp: WorldLamp,
    pub color: [u8; 4],
    pub intensity: f32,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct WorldQuad {
    pub name: String,
    pub w: f32,
    pub h: f32,
    pub shade: Shade,
    pub color: [u8; 4],
    pub image: Option<String>,
    /// World-space centre.
    pub centre: Vec3,
    /// World-space +X, +Y axes of the quad (unit, already scaled? length = half-extent).
    pub axis_x: Vec3,
    pub axis_y: Vec3,
    pub normal: Vec3,
}

impl WorldQuad {
    pub fn corners(&self) -> [Vec3; 4] {
        let (c, x, y) = (self.centre, self.axis_x, self.axis_y);
        [
            Vec3::new(c.x - x.x - y.x, c.y - x.y - y.y, c.z - x.z - y.z),
            Vec3::new(c.x + x.x - y.x, c.y + x.y - y.y, c.z + x.z - y.z),
            Vec3::new(c.x + x.x + y.x, c.y + x.y + y.y, c.z + x.z + y.z),
            Vec3::new(c.x - x.x + y.x, c.y - x.y + y.y, c.z - x.z + y.z),
        ]
    }

    pub fn colour_f(&self) -> [f32; 4] {
        rgba_f(self.color)
    }
}

#[derive(Clone, Debug)]
pub struct WorldMesh {
    pub name: String,
    pub prim: Prim,
    pub geom: Option<String>,
    pub shade: Shade,
    pub color: [u8; 4],
    pub image: Option<String>,
    pub centre: Vec3,
    /// World-space local +X / +Y half-extents (flatten only; not in the dump).
    pub axis_x: Vec3,
    pub axis_y: Vec3,
    /// World-space local +Z. Slab/quad stock: Front vs Back vs Edge.
    pub axis_z: Vec3,
    pub image_back: Option<String>,
    /// First index of rim triangles (print vs stock). Slab only.
    pub rim_index: Option<u32>,
    /// First index of the back-face triangles, if the prim has a separate back.
    pub back_index: Option<u32>,
    /// World pos, normal, uv, rgb tint.
    pub verts: Vec<(Vec3, Vec3, [f32; 2], [f32; 3])>,
    pub indices: Vec<u16>,
}

impl WorldMesh {
    pub fn colour_f(&self) -> [f32; 4] {
        rgba_f(self.color)
    }
}

#[derive(Clone, Debug)]
pub struct WorldGlyph {
    pub corners: [Vec3; 4],
    pub uv: [[f32; 2]; 4],
    pub atlas: String,
}

#[derive(Clone, Debug)]
pub struct WorldLabel {
    pub name: String,
    pub w: f32,
    pub h: f32,
    pub shade: Shade,
    pub color: [u8; 4],
    pub centre: Vec3,
    pub axis_x: Vec3,
    pub axis_y: Vec3,
    pub normal: Vec3,
    pub glyphs: Vec<WorldGlyph>,
    pub missing: Vec<u32>,
}

impl WorldLabel {
    pub fn colour_f(&self) -> [f32; 4] {
        rgba_f(self.color)
    }

    pub fn corners(&self) -> [Vec3; 4] {
        let (c, x, y) = (self.centre, self.axis_x, self.axis_y);
        [
            Vec3::new(c.x - x.x - y.x, c.y - x.y - y.y, c.z - x.z - y.z),
            Vec3::new(c.x + x.x - y.x, c.y + x.y - y.y, c.z + x.z - y.z),
            Vec3::new(c.x + x.x + y.x, c.y + x.y + y.y, c.z + x.z + y.z),
            Vec3::new(c.x - x.x + y.x, c.y - x.y + y.y, c.z - x.z + y.z),
        ]
    }
}

fn rgba_f(c: [u8; 4]) -> [f32; 4] {
    [
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
        c[3] as f32 / 255.0,
    ]
}

/// Which surface of the hit node. Not triangle winding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitFace {
    /// Local +Z (card print, quad front).
    Front,
    /// Local −Z (card back).
    Back,
    /// Thickness / rim (slab sides, rounded corners).
    Edge,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit<'a> {
    pub name: &'a str,
    pub t: f32,
    pub point: Vec3,
    pub uv: [f32; 2],
    pub face: HitFace,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FlattenCost {
    pub matrix_us: u64,
    pub prims_us: u64,
    pub labels_us: u64,
}

impl FlattenCost {
    /// Phase times inside one `draw_list_with`. Not part of `draw` dump.
    pub fn dump(&self) -> String {
        format!(
            "cost\nmatrix={} prims={} labels={}\n",
            fmt_us(self.matrix_us),
            fmt_us(self.prims_us),
            fmt_us(self.labels_us),
        )
    }
}

fn fmt_us(us: u64) -> String {
    if us >= 1000 {
        format!("{:.2}ms", us as f32 / 1000.0)
    } else {
        format!("{us}us")
    }
}

#[derive(Clone, Debug)]
pub struct DrawList {
    pub camera: CameraParams,
    pub lights: Vec<LightParams>,
    pub quads: Vec<WorldQuad>,
    pub meshes: Vec<WorldMesh>,
    pub labels: Vec<WorldLabel>,
    pub cost: FlattenCost,
}

fn camera_of(scene: &Scene) -> Result<CameraParams, Error> {
    let cam_name = scene.active_camera_name().ok_or(Error::NoCamera)?;
    let cam_node = scene.get(cam_name).ok_or(Error::NoCamera)?;
    match &cam_node.kind {
        Kind::Camera { eye, at, up, fov } => Ok(CameraParams {
            eye: *eye,
            at: *at,
            up: *up,
            fov: *fov,
            name: cam_node.name.clone(),
        }),
        _ => Err(Error::NoCamera),
    }
}

impl Scene {
    pub fn camera_params(&self) -> Result<CameraParams, Error> {
        camera_of(self)
    }

    /// Write a live view onto the camera node. Inserts `cam` if missing.
    /// Flatten and orbit read this node; the pane owns the values.
    pub fn put_camera(&mut self, cam: &CameraParams) -> Result<(), Error> {
        let name = if cam.name.is_empty() {
            "cam"
        } else {
            cam.name.as_str()
        };
        if let Some(node) = self.get_mut(name) {
            match &mut node.kind {
                Kind::Camera { eye, at, up, fov } => {
                    *eye = cam.eye;
                    *at = cam.at;
                    *up = cam.up;
                    *fov = cam.fov;
                    return Ok(());
                }
                _ => return Err(Error::KindKey(node.kind.of(), "camera")),
            }
        }
        let mut node = crate::node::Node::camera(name, cam.eye, cam.at);
        if let Kind::Camera { up, fov, .. } = &mut node.kind {
            *up = cam.up;
            *fov = cam.fov;
        }
        self.insert(node)?;
        Ok(())
    }

    pub fn draw_list(&self) -> Result<DrawList, Error> {
        self.draw_list_with(&Geometry::new(), &Materials::new(), &mut Fonts::new())
    }

    pub fn draw_list_with(
        &self,
        geometry: &Geometry,
        materials: &Materials,
        fonts: &mut Fonts,
    ) -> Result<DrawList, Error> {
        let camera = camera_of(self)?;
        let t_matrix = Stopwatch::start();
        let worlds = self.world_matrices()?;
        let matrix_us = t_matrix.us();

        let mut lights = Vec::new();
        let mut quads = Vec::new();
        let mut meshes = Vec::new();
        let mut labels = Vec::new();
        let t_prims = Stopwatch::start();
        for n in self.nodes() {
            let m = *worlds
                .get(&n.name)
                .ok_or_else(|| Error::UnknownNode(n.name.clone()))?;
            match &n.kind {
                Kind::Light {
                    lamp,
                    color,
                    intensity,
                } => {
                    let world = match lamp {
                        Lamp::Dir { dir } => WorldLamp::Dir {
                            dir: transform_dir(m, *dir),
                        },
                        Lamp::Point { range } => WorldLamp::Point {
                            pos: transform_point(m, Vec3::ZERO),
                            range: *range,
                        },
                    };
                    lights.push(LightParams {
                        lamp: world,
                        color: *color,
                        intensity: *intensity,
                        name: n.name.clone(),
                    });
                }
                Kind::Quad { w, h, bind } => {
                    let surf = bind.resolve(materials)?;
                    let hx = *w * 0.5;
                    let hy = *h * 0.5;
                    let centre = transform_point(m, Vec3::ZERO);
                    let axis_x = Vec3::new(
                        transform_point(m, Vec3::new(hx, 0.0, 0.0)).x - centre.x,
                        transform_point(m, Vec3::new(hx, 0.0, 0.0)).y - centre.y,
                        transform_point(m, Vec3::new(hx, 0.0, 0.0)).z - centre.z,
                    );
                    let axis_y = Vec3::new(
                        transform_point(m, Vec3::new(0.0, hy, 0.0)).x - centre.x,
                        transform_point(m, Vec3::new(0.0, hy, 0.0)).y - centre.y,
                        transform_point(m, Vec3::new(0.0, hy, 0.0)).z - centre.z,
                    );
                    let normal = axis_x.cross(axis_y).normalized();
                    quads.push(WorldQuad {
                        name: n.name.clone(),
                        w: *w,
                        h: *h,
                        shade: surf.shade,
                        color: surf.color,
                        image: surf.image.clone(),
                        centre,
                        axis_x,
                        axis_y,
                        normal,
                    });
                }
                Kind::Mesh {
                    prim,
                    geom,
                    bind,
                    pose,
                    ..
                } => {
                    let surf = bind.resolve(materials)?;
                    // Stock prims are tessellated once per shape and shared:
                    // a pose moves a mesh, it never changes its local verts.
                    // A model is every part it was authored with, each in
                    // its own surface — unless the node names one (`c=`,
                    // `m=`, `mat=`), which then dresses every part.
                    type Part = (std::sync::Arc<crate::mesh::Tessellation>, crate::Surface);
                    let parts: Vec<Part> = match prim {
                        Prim::Geom => {
                            let key = geom.as_deref().ok_or(Error::MeshNeedsGeom)?;
                            if let Some(blob) = geometry.get(key) {
                                vec![(
                                    std::sync::Arc::new(crate::mesh::Tessellation {
                                        verts: blob.verts.clone(),
                                        indices: blob.indices.clone(),
                                        rim_index: None,
                                        back_index: None,
                                    }),
                                    surf.clone(),
                                )]
                            } else if let Some(model) = geometry.model(key) {
                                let own = !matches!(
                                    bind,
                                    crate::SurfaceBind::Inline(s) if s.is_default()
                                );
                                // Posed: the rig's clip at the node's time,
                                // each part skinned on the CPU.
                                let mats = match pose {
                                    Some(p) => {
                                        let unknown =
                                            || Error::UnknownPose(n.name.clone(), p.clip.clone());
                                        let rig = geometry.rig(key).ok_or_else(unknown)?;
                                        let clip = rig.clip(&p.clip).ok_or_else(unknown)?;
                                        Some(rig.matrices(Some((clip, p.t))))
                                    }
                                    None => None,
                                };
                                model
                                    .iter()
                                    .map(|p| {
                                        let s = if own { surf.clone() } else { p.surface.clone() };
                                        let tess = match (&mats, &p.skin) {
                                            (Some(mats), Some(skin)) => {
                                                std::sync::Arc::new(crate::mesh::Tessellation {
                                                    verts: crate::rig::skin(skin, mats),
                                                    ..(*p.tess).clone()
                                                })
                                            }
                                            _ => p.tess.clone(),
                                        };
                                        (tess, s)
                                    })
                                    .collect()
                            } else {
                                return Err(Error::UnknownGeom(key.to_string()));
                            }
                        }
                        Prim::Box | Prim::Plane | Prim::Sphere | Prim::Slab { .. } => {
                            vec![(crate::mesh::tessellate_shared(*prim), surf.clone())]
                        }
                    };
                    for (tess, surf) in parts {
                        let centre = transform_point(m, Vec3::ZERO);
                        let (hx, hy) = match prim {
                            Prim::Slab { w, h, .. } => (*w * 0.5, *h * 0.5),
                            _ => (0.5, 0.5),
                        };
                        let axis_x = transform_point(m, Vec3::new(hx, 0.0, 0.0)) - centre;
                        let axis_y = transform_point(m, Vec3::new(0.0, hy, 0.0)) - centre;
                        let axis_z = transform_dir(m, Vec3::new(0.0, 0.0, 1.0));
                        // One inverse per mesh, not per vertex.
                        let nxf = NormalXf::of(m);
                        let verts = tess
                            .verts
                            .iter()
                            .map(|v| (transform_point(m, v.pos), nxf.apply(v.nrm), v.uv, v.rgb))
                            .collect();
                        meshes.push(WorldMesh {
                            name: n.name.clone(),
                            prim: *prim,
                            geom: geom.clone(),
                            shade: surf.shade,
                            color: surf.color,
                            image: surf.image.clone(),
                            image_back: surf.image_back.clone(),
                            rim_index: tess.rim_index,
                            back_index: tess.back_index,
                            centre,
                            axis_x,
                            axis_y,
                            axis_z,
                            verts,
                            indices: tess.indices.clone(),
                        });
                    }
                }
                Kind::Label { .. } | Kind::Group | Kind::Camera { .. } => {}
            }
        }
        let prims_us = t_prims.us();

        let t_labels = Stopwatch::start();
        for n in self.nodes() {
            let Kind::Label {
                font,
                size,
                text,
                bind,
            } = &n.kind
            else {
                continue;
            };
            let m = *worlds
                .get(&n.name)
                .ok_or_else(|| Error::UnknownNode(n.name.clone()))?;
            let surf = bind.resolve(materials)?;
            let layout = fonts.layout(font, *size, text)?;
            let hx = layout.w * 0.5;
            let hy = layout.h * 0.5;
            let centre = transform_point(m, Vec3::ZERO);
            let axis_x = transform_point(m, Vec3::new(hx, 0.0, 0.0)) - centre;
            let axis_y = transform_point(m, Vec3::new(0.0, hy, 0.0)) - centre;
            let normal = axis_x.cross(axis_y).normalized();
            let mut glyphs = Vec::new();
            for g in &layout.glyphs {
                let p0 = transform_point(m, Vec3::new(g.x, g.y, 0.0));
                let p1 = transform_point(m, Vec3::new(g.x + g.w, g.y, 0.0));
                let p2 = transform_point(m, Vec3::new(g.x + g.w, g.y + g.h, 0.0));
                let p3 = transform_point(m, Vec3::new(g.x, g.y + g.h, 0.0));
                glyphs.push(WorldGlyph {
                    corners: [p0, p1, p2, p3],
                    uv: [[g.u0, g.v1], [g.u1, g.v1], [g.u1, g.v0], [g.u0, g.v0]],
                    atlas: g.atlas.clone(),
                });
            }
            labels.push(WorldLabel {
                name: n.name.clone(),
                w: layout.w,
                h: layout.h,
                shade: surf.shade,
                color: surf.color,
                centre,
                axis_x,
                axis_y,
                normal,
                glyphs,
                missing: layout.missing,
            });
        }
        let labels_us = t_labels.us();

        Ok(DrawList {
            camera,
            lights,
            quads,
            meshes,
            labels,
            cost: FlattenCost {
                matrix_us,
                prims_us,
                labels_us,
            },
        })
    }
}

impl DrawList {
    /// Walk/orbit only move the camera. World prims stay; retarget view.
    pub fn set_camera_from(&mut self, scene: &Scene) -> Result<(), Error> {
        self.camera = camera_of(scene)?;
        Ok(())
    }

    pub fn set_camera(&mut self, cam: CameraParams) {
        self.camera = cam;
    }

    /// Counts only — flatten/present cost without dumping every prim.
    pub fn census(&self) -> String {
        let mesh_verts: usize = self.meshes.iter().map(|m| m.verts.len()).sum();
        let mesh_idx: usize = self.meshes.iter().map(|m| m.indices.len()).sum();
        let glyphs: usize = self.labels.iter().map(|l| l.glyphs.len()).sum();
        format!(
            "census\nquads={} meshes={} mesh_verts={} mesh_idx={} labels={} glyphs={} lights={}\n",
            self.quads.len(),
            self.meshes.len(),
            mesh_verts,
            mesh_idx,
            self.labels.len(),
            glyphs,
            self.lights.len(),
        )
    }

    /// Tight agent view of what will be drawn — no matrices.
    pub fn dump(&self) -> String {
        let mut out = String::from("draw\n");
        out.push_str(&format!(
            "cam {} eye={},{},{} at={},{},{} fov={}\n",
            self.camera.name,
            compact_f(self.camera.eye.x),
            compact_f(self.camera.eye.y),
            compact_f(self.camera.eye.z),
            compact_f(self.camera.at.x),
            compact_f(self.camera.at.y),
            compact_f(self.camera.at.z),
            compact_f(self.camera.fov),
        ));
        for l in &self.lights {
            match l.lamp {
                WorldLamp::Dir { dir } => {
                    out.push_str(&format!(
                        "lit {} Dir dir={},{},{} i={}\n",
                        l.name,
                        compact_f(dir.x),
                        compact_f(dir.y),
                        compact_f(dir.z),
                        compact_f(l.intensity),
                    ));
                }
                WorldLamp::Point { pos, range } => {
                    out.push_str(&format!(
                        "lit {} Point at={},{},{} range={} i={}\n",
                        l.name,
                        compact_f(pos.x),
                        compact_f(pos.y),
                        compact_f(pos.z),
                        compact_f(range),
                        compact_f(l.intensity),
                    ));
                }
            }
        }
        for q in &self.quads {
            out.push_str(&format!(
                "quad {} {}x{} at={},{},{}",
                q.name,
                compact_f(q.w),
                compact_f(q.h),
                compact_f(q.centre.x),
                compact_f(q.centre.y),
                compact_f(q.centre.z),
            ));
            if let Some(m) = &q.image {
                out.push_str(" m=");
                out.push_str(m);
            }
            if q.shade != Shade::Lambert {
                out.push_str(" sh=");
                out.push_str(q.shade.as_str());
            }
            out.push('\n');
        }
        for mesh in &self.meshes {
            out.push_str(&format!(
                "mesh {} {} at={},{},{}",
                mesh.name,
                mesh.prim.as_str(),
                compact_f(mesh.centre.x),
                compact_f(mesh.centre.y),
                compact_f(mesh.centre.z),
            ));
            if let Some(g) = &mesh.geom {
                out.push_str(" g=");
                out.push_str(g);
            }
            if let Some(m) = &mesh.image {
                out.push_str(" m=");
                out.push_str(m);
            }
            if let Some(m) = &mesh.image_back {
                out.push_str(" mb=");
                out.push_str(m);
            }
            if mesh.shade != Shade::Lambert {
                out.push_str(" sh=");
                out.push_str(mesh.shade.as_str());
            }
            out.push('\n');
        }
        for lab in &self.labels {
            out.push_str(&format!(
                "label {} {}x{} at={},{},{} glyphs={}",
                lab.name,
                compact_f(lab.w),
                compact_f(lab.h),
                compact_f(lab.centre.x),
                compact_f(lab.centre.y),
                compact_f(lab.centre.z),
                lab.glyphs.len(),
            ));
            if lab.shade != Shade::Unlit {
                out.push_str(" sh=");
                out.push_str(lab.shade.as_str());
            }
            if !lab.missing.is_empty() {
                out.push_str(" missing=");
                for (i, cp) in lab.missing.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&format!("U+{cp:04X}"));
                }
            }
            out.push('\n');
        }
        out
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = mat4_look_at(self.camera.eye, self.camera.at, self.camera.up);
        let proj = mat4_perspective(self.camera.fov, aspect, 0.05, 80.0);
        mat4_mul(proj, view)
    }

    /// Where to aim at `name`: the centre of the bounds of every mesh it
    /// draws (a model's origin is often at its feet, so aiming there hits
    /// the floor), else its world origin (a quad's or label's centre).
    pub fn aim_point(&self, scene: &Scene, name: &str) -> Option<Vec3> {
        let mut lo = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut hi = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
        let mut any = false;
        for m in self.meshes.iter().filter(|m| m.name == name) {
            for (p, _, _, _) in &m.verts {
                lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
                hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
                any = true;
            }
        }
        if any {
            return Some((lo + hi) * 0.5);
        }
        let world = scene.world_matrix(name).ok()?;
        Some(transform_point(world, Vec3::ZERO))
    }

    /// Where world point `p` is drawn, as ndc (y up), through
    /// [`Self::view_proj`] — the matrix both backends draw with and
    /// [`Self::hit_ndc`] picks with. `None` behind the eye.
    pub fn project_ndc(&self, p: Vec3, aspect: f32) -> Option<(f32, f32)> {
        let c = crate::math::transform_vec4(self.view_proj(aspect), [p.x, p.y, p.z, 1.0]);
        if c[3] <= 1e-6 {
            return None;
        }
        Some((c[0] / c[3], c[1] / c[3]))
    }

    /// Closest two-sided geometry. Does not sample images — a transparent
    /// PNG still occupies its whole quad/slab. Prefer [`Self::hit_with`]
    /// when pixels are interned (CNFT stacks, punched cards).
    pub fn hit(&self, origin: Vec3, dir: Vec3) -> Option<Hit<'_>> {
        self.hit_filtered(origin, dir, None)
    }

    /// Closest **opaque** hit. Front samples `m=`, back samples `mb=`
    /// only (omitted `mb=` is unprinted stock, not a flipped front).
    /// Rim is solid, labels use the layout bbox. Missing image keys are
    /// opaque so a dump without [`Images`] still picks. Texels at or
    /// below [`ALPHA_CUTOFF`] are skipped so the ray continues.
    pub fn hit_with(&self, origin: Vec3, dir: Vec3, images: &Images) -> Option<Hit<'_>> {
        self.hit_filtered(origin, dir, Some(images))
    }

    fn hit_filtered<'a>(
        &'a self,
        origin: Vec3,
        dir: Vec3,
        images: Option<&Images>,
    ) -> Option<Hit<'a>> {
        let dir = dir.normalized();
        if dir.length() < 1e-8 {
            return None;
        }
        let mut hits: Vec<Hit<'a>> = Vec::new();
        for q in &self.quads {
            let [p0, p1, p2, p3] = q.corners();
            // Same UVs as the wgpu path.
            let corners = [
                (p0, [0.0, 1.0]),
                (p1, [1.0, 1.0]),
                (p2, [1.0, 0.0]),
                (p3, [0.0, 0.0]),
            ];
            for idx in [[0, 1, 2], [0, 2, 3]] {
                let (a, uv0) = corners[idx[0]];
                let (b, uv1) = corners[idx[1]];
                let (c, uv2) = corners[idx[2]];
                if let Some(tri) = ray_triangle(origin, dir, a, b, c) {
                    hits.push(Hit {
                        name: q.name.as_str(),
                        t: tri.t,
                        point: origin + dir * tri.t,
                        uv: lerp_uv(uv0, uv1, uv2, tri.u, tri.v),
                        face: if tri.front {
                            HitFace::Front
                        } else {
                            HitFace::Back
                        },
                    });
                }
            }
        }
        for mesh in &self.meshes {
            for (k, idx) in mesh.indices.as_chunks::<3>().0.iter().enumerate() {
                let start = (k * 3) as u32;
                let (a, _, uv0, _) = mesh.verts[idx[0] as usize];
                let (b, _, uv1, _) = mesh.verts[idx[1] as usize];
                let (c, _, uv2, _) = mesh.verts[idx[2] as usize];
                if let Some(tri) = ray_triangle(origin, dir, a, b, c) {
                    hits.push(Hit {
                        name: mesh.name.as_str(),
                        t: tri.t,
                        point: origin + dir * tri.t,
                        uv: lerp_uv(uv0, uv1, uv2, tri.u, tri.v),
                        face: mesh_face_at(mesh, start, a, b, c),
                    });
                }
            }
        }
        for lab in &self.labels {
            let [p0, p1, p2, p3] = lab.corners();
            let corners = [
                (p0, [0.0, 1.0]),
                (p1, [1.0, 1.0]),
                (p2, [1.0, 0.0]),
                (p3, [0.0, 0.0]),
            ];
            for idx in [[0, 1, 2], [0, 2, 3]] {
                let (a, uv0) = corners[idx[0]];
                let (b, uv1) = corners[idx[1]];
                let (c, uv2) = corners[idx[2]];
                if let Some(tri) = ray_triangle(origin, dir, a, b, c) {
                    hits.push(Hit {
                        name: lab.name.as_str(),
                        t: tri.t,
                        point: origin + dir * tri.t,
                        uv: lerp_uv(uv0, uv1, uv2, tri.u, tri.v),
                        face: if tri.front {
                            HitFace::Front
                        } else {
                            HitFace::Back
                        },
                    });
                }
            }
        }
        hits.sort_by(|a, b| a.t.total_cmp(&b.t));
        hits.into_iter()
            .find(|h| images.is_none_or(|img| self.opaque_at(h, img)))
    }

    fn opaque_at(&self, hit: &Hit<'_>, images: &Images) -> bool {
        if hit.face == HitFace::Edge {
            return true;
        }
        if self.labels.iter().any(|l| l.name == hit.name) {
            return true;
        }
        let key = if let Some(q) = self.quads.iter().find(|q| q.name == hit.name) {
            q.image.as_deref()
        } else if let Some(m) = self.meshes.iter().find(|m| m.name == hit.name) {
            match hit.face {
                HitFace::Back => m.image_back.as_deref(),
                _ => m.image.as_deref(),
            }
        } else {
            return true;
        };
        let Some(key) = key else {
            // No texture: unprinted slab back does not occupy the ray (matches
            // the GPU skipping that batch). Front/edge without m= stay solid.
            return hit.face != HitFace::Back;
        };
        images.alpha_at(key, hit.uv) > ALPHA_CUTOFF
    }

    /// NDC x,y in [-1, 1], y up. Uses the same view-projection as rasterise.
    pub fn hit_ndc(&self, ndc_x: f32, ndc_y: f32, aspect: f32) -> Option<Hit<'_>> {
        self.hit_ndc_filtered(ndc_x, ndc_y, aspect, None)
    }

    pub fn hit_ndc_with(
        &self,
        ndc_x: f32,
        ndc_y: f32,
        aspect: f32,
        images: &Images,
    ) -> Option<Hit<'_>> {
        self.hit_ndc_filtered(ndc_x, ndc_y, aspect, Some(images))
    }

    fn hit_ndc_filtered(
        &self,
        ndc_x: f32,
        ndc_y: f32,
        aspect: f32,
        images: Option<&Images>,
    ) -> Option<Hit<'_>> {
        if aspect <= 0.0 {
            return None;
        }
        let inv = mat4_inverse(self.view_proj(aspect))?;
        let far = unproject(inv, Vec3::new(ndc_x, ndc_y, 1.0))?;
        let dir = far - self.camera.eye;
        self.hit_filtered(self.camera.eye, dir, images)
    }
}

/// Texels at or below this alpha do not occupy the pick ray (matches GPU discard).
pub const ALPHA_CUTOFF: u8 = 8;

fn mesh_face_at(mesh: &WorldMesh, start: u32, a: Vec3, b: Vec3, c: Vec3) -> HitFace {
    match (mesh.rim_index, mesh.back_index) {
        (_, Some(back)) if start >= back => HitFace::Back,
        (Some(r), _) if start >= r => HitFace::Edge,
        (Some(_), _) => HitFace::Front,
        _ => mesh_face(mesh.prim, a, b, c, mesh.axis_z),
    }
}

fn mesh_face(prim: Prim, a: Vec3, b: Vec3, c: Vec3, axis_z: Vec3) -> HitFace {
    if prim == Prim::Sphere {
        return HitFace::Front;
    }
    let n = (b - a).cross(c - a).normalized();
    let align = n.dot(axis_z);
    if align > 0.5 {
        HitFace::Front
    } else if align < -0.5 {
        HitFace::Back
    } else {
        HitFace::Edge
    }
}
