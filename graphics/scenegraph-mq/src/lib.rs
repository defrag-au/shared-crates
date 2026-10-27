//! macroquad / miniquad backend — draws a [`DrawList`], nothing else.
//!
//! Two steps. [`plan`] is pure (no GL context): the exact meshes this backend
//! submits, with lighting baked into vertex colour. [`Renderer`] resolves the
//! plan's texture keys to cached GPU textures and calls `draw_mesh` with this
//! backend's programs. `Shade` is document data; the programs live here, as
//! WGSL lives in `scenegraph-wgpu`. The document never contains GLSL.
//!
//! Parity with the wgpu backend, and where it is approximate:
//!
//! - **Projection** is [`DrawList::view_proj`], the same matrix `hit` uses
//!   ([`view_proj_glam`]). Picks land where pixels are drawn.
//! - **Triangles** per texture key match `scenegraph_wgpu::census` exactly
//!   (pinned by the tests below). An unprinted slab back (no `mb=`) is skipped by both,
//!   as `hit_with` skips it.
//! - **Lambert** is the WGSL model (ambient 0.18, first 4 `Dir`, first 4
//!   `Point`, `(1 - d/range)²` falloff) evaluated per vertex on the CPU in
//!   linear light, then encoded to sRGB into the vertex colour. Exact for
//!   flat faces under `Dir` lights; `Point` lights and curved meshes are
//!   per-vertex here and per-fragment on wgpu. Unlit colour is untouched.
//! - **Texture × colour** multiplies in sRGB space (no sRGB target on
//!   WebGL1), where wgpu multiplies in linear. Untextured surfaces are
//!   exact; textured Lambert is slightly darker in the mid-tones.
//! - **Alpha cutoff**: texels with alpha < 0.031 are discarded, as the WGSL
//!   does, so raster holes match [`scenegraph::ALPHA_CUTOFF`] picking. Blending is
//!   in sRGB space; target alpha accumulates towards opaque.
//! - **MSDF** labels take the median of RGB with a fixed smoothing band
//!   ([`MSDF_BAND`]): GLSL ES 100 has no `fwidth` without
//!   `OES_standard_derivatives`, so edges are softer when a label is tiny on
//!   screen and crisper when it is large.
//! - No mipmaps: textures are linear-filtered at one level.
//!
//! If the GLSL programs cannot be built (a Metal backend), drawing falls back
//! to macroquad's default material: no discard, and labels draw as raw atlas.

use std::collections::HashMap;

use macroquad::camera::{pop_camera_state, push_camera_state, set_camera, Camera};
use macroquad::color::Color;
use macroquad::material::{gl_use_default_material, gl_use_material, load_material};
use macroquad::material::{Material, MaterialParams};
use macroquad::math::{vec2, vec3, Mat4 as GMat4, Vec4};
use macroquad::miniquad::{
    BlendFactor, BlendState, BlendValue, Comparison, Equation, PipelineParams, ShaderSource,
};
use macroquad::models::{draw_mesh, Mesh, Vertex};
use macroquad::texture::{FilterMode, RenderPass, RenderTarget, Texture2D};
use macroquad::window::{clear_background, screen_height, screen_width};

use scenegraph::{
    DrawList, Fonts, Geometry, Images, Materials, Scene, Shade, SubmitCensus, Vec3, WorldLamp,
    WorldMesh, MAX_DIR_LIGHTS, MAX_POINT_LIGHTS,
};

/// MSDF smoothing band in distance units (atlas range is 0..1 around 0.5).
pub const MSDF_BAND: f32 = 0.08;
/// Same constant as the WGSL `base.a < 0.031` discard.
pub const DISCARD_ALPHA: f32 = 0.031;
/// Under macroquad's per-draw-call capacity (10000 verts, 5000 indices), so a
/// large mesh is split rather than silently clamped.
const MAX_DRAW_INDICES: usize = 4800;
const AMBIENT: f32 = 0.18;

/// One mesh the backend submits.
#[derive(Clone, Debug)]
pub struct PlanDraw {
    /// Node name in the draw list.
    pub name: String,
    /// Image key (`m=` / `mb=`), or the font key's atlas when `sdf`. None is
    /// untextured.
    pub key: Option<String>,
    /// MSDF glyph program.
    pub sdf: bool,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
}

/// What [`Renderer`] will submit for a [`DrawList`]. Pure.
#[derive(Clone, Debug, Default)]
pub struct Plan {
    pub draws: Vec<PlanDraw>,
    pub quads: u32,
    pub meshes: u32,
    pub labels: u32,
}

impl Plan {
    pub fn census(&self) -> SubmitCensus {
        let mut c = SubmitCensus::new();
        for d in &self.draws {
            c.add(d.key.as_deref(), d.sdf, d.indices.len() as u32);
        }
        c.quads = self.quads;
        c.meshes = self.meshes;
        c.labels = self.labels;
        c
    }
}

/// [`DrawList::view_proj`] as glam, column for column. Draw and pick share it.
pub fn view_proj_glam(list: &DrawList, aspect: f32) -> GMat4 {
    GMat4::from_cols_array_2d(&list.view_proj(aspect))
}

/// World point → NDC through [`view_proj_glam`]. `None` behind the eye.
pub fn project_ndc(list: &DrawList, aspect: f32, p: Vec3) -> Option<(f32, f32)> {
    let c = view_proj_glam(list, aspect) * Vec4::new(p.x, p.y, p.z, 1.0);
    if c.w <= 1e-8 {
        return None;
    }
    Some((c.x / c.w, c.y / c.w))
}

/// Build the submission for `list`. Needs no GL context.
pub fn plan(list: &DrawList, _fonts: &Fonts) -> Plan {
    let lights = Lights::of(list);
    let mut out = Plan::default();
    for q in &list.quads {
        out.quads += 1;
        let color = lights.shade(q.color, [1.0; 3], q.normal, q.centre, q.shade);
        let [p0, p1, p2, p3] = q.corners();
        let verts = [
            vertex(p0, [0.0, 1.0], color, q.normal),
            vertex(p1, [1.0, 1.0], color, q.normal),
            vertex(p2, [1.0, 0.0], color, q.normal),
            vertex(p3, [0.0, 0.0], color, q.normal),
        ];
        push(
            &mut out.draws,
            &q.name,
            q.image.clone(),
            false,
            &verts,
            &[0, 1, 2, 0, 2, 3],
        );
    }
    for mesh in &list.meshes {
        out.meshes += 1;
        let verts: Vec<Vertex> = mesh
            .verts
            .iter()
            .map(|(p, n, uv, rgb)| {
                let c = lights.shade(mesh.color, *rgb, *n, *p, mesh.shade);
                vertex(*p, *uv, c, *n)
            })
            .collect();
        for (key, range) in mesh_ranges(mesh) {
            push(
                &mut out.draws,
                &mesh.name,
                key,
                false,
                &verts,
                &mesh.indices[range],
            );
        }
    }
    for lab in &list.labels {
        out.labels += 1;
        let color = lights.shade(lab.color, [1.0; 3], lab.normal, lab.centre, lab.shade);
        for g in &lab.glyphs {
            let verts = [
                vertex(g.corners[0], g.uv[0], color, lab.normal),
                vertex(g.corners[1], g.uv[1], color, lab.normal),
                vertex(g.corners[2], g.uv[2], color, lab.normal),
                vertex(g.corners[3], g.uv[3], color, lab.normal),
            ];
            push(
                &mut out.draws,
                &lab.name,
                Some(g.atlas.clone()),
                true,
                &verts,
                &[0, 1, 2, 0, 2, 3],
            );
        }
    }
    out
}

/// Index ranges and their texture: print, stock rim, back print. An
/// unprinted back (no `mb=`) is not drawn — same as wgpu and `hit_with`.
fn mesh_ranges(mesh: &WorldMesh) -> Vec<(Option<String>, std::ops::Range<usize>)> {
    let total = mesh.indices.len();
    let rim = mesh
        .rim_index
        .map(|i| i as usize)
        .filter(|&i| i > 0 && i < total);
    let back = mesh
        .back_index
        .map(|i| i as usize)
        .filter(|&i| i > 0 && i < total);
    match (rim, back) {
        (Some(r), Some(b)) if r <= b => {
            let mut v = vec![(mesh.image.clone(), 0..r), (None, r..b)];
            if mesh.image_back.is_some() {
                v.push((mesh.image_back.clone(), b..total));
            }
            v
        }
        (Some(r), None) => vec![(mesh.image.clone(), 0..r), (None, r..total)],
        (None, Some(b)) if mesh.image_back.is_some() => vec![
            (mesh.image.clone(), 0..b),
            (mesh.image_back.clone(), b..total),
        ],
        _ => vec![(mesh.image.clone(), 0..total)],
    }
}

/// Append a draw with only the vertices its indices use, split under the
/// per-draw-call capacity on whole triangles.
fn push(
    draws: &mut Vec<PlanDraw>,
    name: &str,
    key: Option<String>,
    sdf: bool,
    verts: &[Vertex],
    indices: &[u16],
) {
    // A flat table, not a map: indices are dense in 0..verts.len(). Same
    // first-seen order as before, so the same draws come out.
    const UNSEEN: u16 = u16::MAX;
    let mut remap = vec![UNSEEN; verts.len()];
    for chunk in indices.chunks(MAX_DRAW_INDICES) {
        remap.fill(UNSEEN);
        let mut vertices = Vec::new();
        let mut local = Vec::with_capacity(chunk.len());
        for &i in chunk {
            let slot = &mut remap[i as usize];
            if *slot == UNSEEN {
                vertices.push(verts[i as usize]);
                *slot = (vertices.len() - 1) as u16;
            }
            local.push(*slot);
        }
        draws.push(PlanDraw {
            name: name.to_string(),
            key: key.clone(),
            sdf,
            vertices,
            indices: local,
        });
    }
}

fn vertex(p: Vec3, uv: [f32; 2], color: [u8; 4], n: Vec3) -> Vertex {
    Vertex {
        position: vec3(p.x, p.y, p.z),
        uv: vec2(uv[0], uv[1]),
        color,
        normal: Vec4::new(n.x, n.y, n.z, 0.0),
    }
}

/// The first 4 Dir and first 4 Point lamps, in linear light × intensity.
/// Dir lamps hold the unit vector *towards* the light, normalised once.
struct Lights {
    dirs: Vec<(Vec3, [f32; 3])>,
    points: Vec<(Vec3, f32, [f32; 3])>,
    /// The last base colour made linear: one mesh shades many vertices with
    /// the same colour and tint, and `powf` per channel per vertex was most of
    /// plan's time.
    lin: std::cell::Cell<Option<([u32; 3], [f32; 3])>>,
}

impl Lights {
    fn of(list: &DrawList) -> Self {
        let mut dirs = Vec::new();
        let mut points = Vec::new();
        for l in &list.lights {
            let rgb = [
                srgb_to_lin(l.color[0] as f32 / 255.0) * l.intensity,
                srgb_to_lin(l.color[1] as f32 / 255.0) * l.intensity,
                srgb_to_lin(l.color[2] as f32 / 255.0) * l.intensity,
            ];
            match l.lamp {
                WorldLamp::Dir { dir } if dirs.len() < MAX_DIR_LIGHTS => {
                    dirs.push((Vec3::new(-dir.x, -dir.y, -dir.z).normalized(), rgb))
                }
                WorldLamp::Point { pos, range } if points.len() < MAX_POINT_LIGHTS => {
                    points.push((pos, range, rgb))
                }
                _ => {}
            }
        }
        Self {
            dirs,
            points,
            lin: std::cell::Cell::new(None),
        }
    }

    /// `base` in linear light, remembered for the next vertex of the same
    /// colour.
    fn linear(&self, base: [f32; 3]) -> [f32; 3] {
        let key = base.map(f32::to_bits);
        if let Some((k, lin)) = self.lin.get() {
            if k == key {
                return lin;
            }
        }
        let lin = base.map(srgb_to_lin);
        self.lin.set(Some((key, lin)));
        lin
    }

    /// Vertex colour: `color` × per-vertex `tint`, lit if Lambert.
    fn shade(&self, color: [u8; 4], tint: [f32; 3], n: Vec3, world: Vec3, shade: Shade) -> [u8; 4] {
        let base = [
            color[0] as f32 / 255.0 * tint[0],
            color[1] as f32 / 255.0 * tint[1],
            color[2] as f32 / 255.0 * tint[2],
        ];
        if shade == Shade::Unlit {
            return [
                unit_u8(base[0]),
                unit_u8(base[1]),
                unit_u8(base[2]),
                color[3],
            ];
        }
        let n = n.normalized();
        let mut acc = [AMBIENT; 3];
        for (l, rgb) in &self.dirs {
            let k = n.dot(*l).max(0.0);
            for c in 0..3 {
                acc[c] += rgb[c] * k;
            }
        }
        for (pos, range, rgb) in &self.points {
            let to_l = *pos - world;
            let dist = to_l.length();
            let ldir = to_l * (1.0 / dist.max(1e-4));
            let att = (1.0 - dist / range.max(1e-4)).clamp(0.0, 1.0);
            let k = n.dot(ldir).max(0.0) * att * att;
            for c in 0..3 {
                acc[c] += rgb[c] * k;
            }
        }
        let lin = self.linear(base);
        let lit = |c: usize| unit_u8(lin_to_srgb(lin[c] * acc[c]));
        [lit(0), lit(1), lit(2), color[3]]
    }
}

fn srgb_to_lin(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn lin_to_srgb(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if x <= 0.003_130_8 {
        x * 12.92
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    }
}

fn unit_u8(x: f32) -> u8 {
    (x.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Camera whose matrix is the draw list's own view-projection.
struct ListCamera {
    matrix: GMat4,
    pass: Option<RenderPass>,
}

impl Camera for ListCamera {
    fn matrix(&self) -> GMat4 {
        self.matrix
    }

    fn depth_enabled(&self) -> bool {
        true
    }

    fn render_pass(&self) -> Option<RenderPass> {
        self.pass.clone()
    }

    fn viewport(&self) -> Option<(i32, i32, i32, i32)> {
        None
    }
}

const VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
varying lowp vec4 color;
varying mediump vec2 uv;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1.0);
    color = color0 / 255.0;
    uv = texcoord;
}"#;

const MESH_FRAGMENT: &str = r#"#version 100
precision mediump float;
varying lowp vec4 color;
varying mediump vec2 uv;
uniform sampler2D Texture;
void main() {
    vec4 base = texture2D(Texture, uv) * color;
    if (base.a < 0.031) {
        discard;
    }
    gl_FragColor = base;
}"#;

const MSDF_FRAGMENT: &str = r#"#version 100
precision mediump float;
varying lowp vec4 color;
varying mediump vec2 uv;
uniform sampler2D Texture;
float median3(float a, float b, float c) {
    return max(min(a, b), min(max(a, b), c));
}
void main() {
    vec3 s = texture2D(Texture, uv).rgb;
    float sd = median3(s.r, s.g, s.b) - 0.5;
    float fill = clamp(sd / 0.08 + 0.5, 0.0, 1.0);
    if (fill < 0.01) {
        discard;
    }
    gl_FragColor = vec4(color.rgb, color.a * fill);
}"#;

fn pipeline() -> PipelineParams {
    PipelineParams {
        color_blend: Some(BlendState::new(
            Equation::Add,
            BlendFactor::Value(BlendValue::SourceAlpha),
            BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
        )),
        // Target alpha accumulates towards opaque instead of wearing down.
        alpha_blend: Some(BlendState::new(
            Equation::Add,
            BlendFactor::One,
            BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
        )),
        depth_test: Comparison::LessOrEqual,
        depth_write: true,
        ..Default::default()
    }
}

fn program(fragment: &str) -> Option<Material> {
    let built = load_material(
        ShaderSource::Glsl {
            vertex: VERTEX,
            fragment,
        },
        MaterialParams {
            pipeline_params: pipeline(),
            uniforms: Vec::new(),
            textures: Vec::new(),
        },
    );
    match built {
        Ok(m) => Some(m),
        Err(e) => {
            // Positional: miniquad's log macros print a lone literal verbatim.
            macroquad::logging::warn!("scenegraph mq program: {:?}", e);
            None
        }
    }
}

/// Where image keys resolve: interned pixels (cached here) or textures the
/// caller already uploaded (the free [`draw_list`]).
#[derive(Clone, Copy)]
enum Source<'a> {
    Images(&'a Images),
    Uploaded(&'a HashMap<String, Texture2D>),
}

struct CachedImage {
    tex: Texture2D,
    w: u32,
    h: u32,
    bytes: usize,
}

struct Programs {
    mesh: Option<Material>,
    msdf: Option<Material>,
}

/// GPU-side caches across frames: image textures, font atlases, programs.
/// Create once; [`Renderer::new`] makes no GL calls.
#[derive(Default)]
pub struct Renderer {
    images: HashMap<String, CachedImage>,
    /// Atlas texture and the glyph count it was uploaded at.
    atlases: HashMap<String, (Texture2D, usize)>,
    programs: Option<Programs>,
}

impl Renderer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Draw `list` into `target` (or the window) with its own camera, at
    /// `aspect` (target width / height). Clears colour and depth first when
    /// `clear` is given. The camera state is saved and restored. A target
    /// needs a depth attachment (`render_target_ex` with `depth: true` and
    /// `sample_count: 0` for WebGL1) and draws upright with `flip_y: true`.
    pub fn draw_list_to(
        &mut self,
        list: &DrawList,
        images: &Images,
        fonts: &Fonts,
        target: Option<&RenderTarget>,
        aspect: f32,
        clear: Option<Color>,
    ) {
        let plan = plan(list, fonts);
        self.draw_plan_to(&plan, list, images, fonts, target, aspect, clear);
    }

    /// [`Self::draw_list_to`] with a plan already built (for a census).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_plan_to(
        &mut self,
        plan: &Plan,
        list: &DrawList,
        images: &Images,
        fonts: &Fonts,
        target: Option<&RenderTarget>,
        aspect: f32,
        clear: Option<Color>,
    ) {
        push_camera_state();
        set_camera(&ListCamera {
            matrix: view_proj_glam(list, aspect),
            pass: target.map(|t| t.render_pass.clone()),
        });
        if let Some(c) = clear {
            clear_background(c);
        }
        self.submit(plan, fonts, Source::Images(images));
        pop_camera_state();
    }

    /// Texture for an image key, uploaded once per key + size.
    fn image_texture(&mut self, key: &str, images: &Images) -> Option<Texture2D> {
        let Some(img) = images.get(key) else {
            self.images.remove(key);
            return None;
        };
        let fresh = self
            .images
            .get(key)
            .is_some_and(|c| c.w == img.width && c.h == img.height && c.bytes == img.pixels.len());
        if !fresh {
            let tex = Texture2D::from_rgba8(img.width as u16, img.height as u16, &img.pixels);
            tex.set_filter(FilterMode::Linear);
            self.images.insert(
                key.to_string(),
                CachedImage {
                    tex,
                    w: img.width,
                    h: img.height,
                    bytes: img.pixels.len(),
                },
            );
        }
        self.images.get(key).map(|c| c.tex.clone())
    }

    /// Atlas texture for a font key; re-uploaded only when glyphs were packed.
    fn atlas_texture(&mut self, key: &str, fonts: &Fonts) -> Option<Texture2D> {
        let rev = fonts.glyph_count(key)?;
        if let Some((tex, at)) = self.atlases.get(key) {
            if *at == rev {
                return Some(tex.clone());
            }
            let img = fonts.atlas(key)?;
            tex.update_from_bytes(img.width, img.height, &img.pixels);
            let tex = tex.clone();
            self.atlases.insert(key.to_string(), (tex.clone(), rev));
            return Some(tex);
        }
        let img = fonts.atlas(key)?;
        let tex = Texture2D::from_rgba8(img.width as u16, img.height as u16, &img.pixels);
        tex.set_filter(FilterMode::Linear);
        self.atlases.insert(key.to_string(), (tex.clone(), rev));
        Some(tex)
    }

    fn submit(&mut self, plan: &Plan, fonts: &Fonts, images: Source<'_>) {
        let programs = self.programs.get_or_insert_with(|| Programs {
            mesh: program(MESH_FRAGMENT),
            msdf: program(MSDF_FRAGMENT),
        });
        let (mesh_prog, msdf_prog) = (programs.mesh.clone(), programs.msdf.clone());
        let mut current: Option<bool> = None;
        for d in &plan.draws {
            if current != Some(d.sdf) {
                match if d.sdf { &msdf_prog } else { &mesh_prog } {
                    Some(m) => gl_use_material(m),
                    None => gl_use_default_material(),
                }
                current = Some(d.sdf);
            }
            let texture = match (&d.key, d.sdf) {
                (Some(k), true) => self.atlas_texture(k, fonts),
                (Some(k), false) => match images {
                    Source::Images(i) => self.image_texture(k, i),
                    Source::Uploaded(map) => map.get(k).cloned(),
                },
                (None, _) => None,
            };
            draw_mesh(&Mesh {
                vertices: d.vertices.clone(),
                indices: d.indices.clone(),
                texture,
            });
        }
        gl_use_default_material();
    }
}

/// One-off draw of a scene to the window. Builds a throwaway [`Renderer`];
/// a client that draws every frame should hold one.
pub fn draw(scene: &Scene, images: &HashMap<String, Texture2D>) {
    draw_with(
        scene,
        images,
        &Geometry::new(),
        &Materials::new(),
        &mut Fonts::new(),
    );
}

pub fn draw_with(
    scene: &Scene,
    images: &HashMap<String, Texture2D>,
    geom: &Geometry,
    materials: &Materials,
    fonts: &mut Fonts,
) {
    let Ok(list) = scene.draw_list_with(geom, materials, fonts) else {
        return;
    };
    draw_list(&list, images, fonts);
}

/// Draw `list` to the window with textures the caller already uploaded.
pub fn draw_list(list: &DrawList, images: &HashMap<String, Texture2D>, fonts: &Fonts) {
    let aspect = screen_width() / screen_height().max(1.0);
    let plan = plan(list, fonts);
    let mut r = Renderer::new();
    set_camera(&ListCamera {
        matrix: view_proj_glam(list, aspect),
        pass: None,
    });
    r.submit(&plan, fonts, Source::Uploaded(images));
    macroquad::camera::set_default_camera();
}

/// What this backend submits against what `scenegraph-wgpu` submits for
/// the same lists (a dev-dependency: no macroquad build links wgpu).
#[cfg(test)]
mod parity_tests {
    use super::*;

    fn noto() -> Fonts {
        let mut f = Fonts::new();
        f.insert("ibm", scenegraph::NOTO_SANS_BOLD, Vec::new())
            .unwrap();
        f
    }

    fn goldens() -> Vec<(&'static str, DrawList, Fonts)> {
        let mut geom = Geometry::new();
        geom.insert_tet("tet").unwrap();
        let cases: [(&str, &str, Option<&str>); 4] = [
            (
                "room",
                include_str!("../../scenegraph/tests/golden/room.sg1"),
                None,
            ),
            (
                "cards",
                include_str!("../../scenegraph/tests/golden/cards.sg1"),
                Some(include_str!("../../scenegraph/tests/golden/surfaces.mt1")),
            ),
            (
                "table",
                include_str!("../../scenegraph/tests/golden/table.sg1"),
                Some(include_str!("../../scenegraph/tests/golden/table.mt1")),
            ),
            (
                "gallery",
                include_str!("../../scenegraph/tests/golden/gallery.sg1"),
                Some(include_str!("../../scenegraph/tests/golden/gallery.mt1")),
            ),
        ];
        cases
            .into_iter()
            .map(|(name, sg, mt)| {
                let scene = Scene::from_dump(sg).unwrap();
                let mats = mt
                    .map(|m| Materials::from_dump(m).unwrap())
                    .unwrap_or_default();
                let mut fonts = noto();
                let list = scene.draw_list_with(&geom, &mats, &mut fonts).unwrap();
                (name, list, fonts)
            })
            .chain(yard())
            .collect()
    }

    /// A yard of the four Khronos models, textured and not, posed and not,
    /// through the same comparison as the hand-written goldens.
    fn yard() -> Option<(&'static str, DrawList, Fonts)> {
        macro_rules! glb {
            ($key:literal, $file:literal) => {
                (
                    $key,
                    include_bytes!(concat!(
                        "../../scenegraph/tests/fixtures/khronos/Models/",
                        $file,
                        "/",
                        $file,
                        ".glb"
                    ))
                    .as_slice(),
                )
            };
        }
        let mut geom = Geometry::new();
        let mut images = Images::new();
        for (key, bytes) in [
            glb!("crate", "BoxTextured"),
            glb!("duck", "Duck"),
            glb!("fox", "Fox"),
            glb!("truck", "CesiumMilkTruck"),
        ] {
            let model = scenegraph::model_from_gltf(key, bytes).unwrap();
            images.absorb(geom.insert_imported(key, model).unwrap());
        }
        let scene =
            Scene::from_dump(include_str!("../../scenegraph/tests/golden/yard.sg1")).unwrap();
        let mut fonts = noto();
        let list = scene
            .draw_list_with(&geom, &Materials::new(), &mut fonts)
            .unwrap();
        // Every textured part names an image the import brought.
        for m in &list.meshes {
            if let Some(k) = &m.image {
                assert!(images.get(k).is_some(), "{k}");
            }
        }
        Some(("yard", list, fonts))
    }

    #[test]
    fn mq_submits_what_wgpu_submits() {
        for (name, list, fonts) in goldens() {
            let mq = plan(&list, &fonts).census().dump();
            let wgpu = scenegraph_wgpu::census(&list).dump();
            assert_eq!(mq, wgpu, "{name}\nmq:\n{mq}\nwgpu:\n{wgpu}");
            assert!(mq.contains("tris="), "{name}: {mq}");
            if name == "table" {
                // Labels (MSDF) and printed slabs are both in the comparison.
                assert!(mq.contains("key=ibm sdf=1"), "{mq}");
                assert!(
                    mq.contains("sdf=0") && !mq.contains("draw key=- sdf=0 tris=0"),
                    "{mq}"
                );
                assert!(mq.lines().count() > 4, "{mq}");
            }
            if name == "yard" {
                // Each model's texture is drawn under its `KEY/NAME`; the
                // floor and the truck's two flat parts share key `-`.
                for line in [
                    "draw key=- sdf=0 tris=346",
                    "draw key=crate/img0 sdf=0 tris=12",
                    "draw key=duck/img0 sdf=0 tris=4212",
                    "draw key=fox/img0 sdf=0 tris=576",
                    "draw key=truck/CesiumMilkTruck.jpg sdf=0 tris=3280",
                ] {
                    assert!(mq.lines().any(|l| l == line), "{line}\n{mq}");
                }
            }
        }
    }

    #[test]
    fn view_proj_is_the_pick_matrix() {
        for (name, list, _) in goldens() {
            for aspect in [1.0_f32, 16.0 / 9.0, 0.5] {
                let ours = list.view_proj(aspect);
                let glam = view_proj_glam(&list, aspect).to_cols_array_2d();
                for c in 0..4 {
                    for r in 0..4 {
                        assert!(
                            (ours[c][r] - glam[c][r]).abs() < 1e-6,
                            "{name} aspect={aspect} [{c}][{r}]"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_pick_projects_back_to_its_pixel() {
        let picks = [
            ("room", 0.0, 0.0),
            ("cards", -0.3, 0.1),
            ("table", 0.0, -0.4),
        ];
        let golden = goldens();
        let mut hits = 0;
        for (name, x, y) in picks {
            let (_, list, _) = golden.iter().find(|(n, _, _)| *n == name).unwrap();
            for aspect in [1.0_f32, 16.0 / 9.0] {
                let Some(hit) = list.hit_ndc(x, y, aspect) else {
                    continue;
                };
                hits += 1;
                let (px, py) = project_ndc(list, aspect, hit.point).unwrap();
                assert!(
                    (px - x).abs() < 1e-4 && (py - y).abs() < 1e-4,
                    "{name} {} {:?}: picked at {x},{y}, drawn at {px},{py}",
                    hit.name,
                    hit.face
                );
            }
        }
        assert!(hits >= 4, "expected picks to land, got {hits}");
    }

    #[test]
    fn unlit_colour_is_untouched_and_lambert_is_lit() {
        let src = "sg1\nu Quad 1x1 sh=Unlit c=336699\nl Quad 1x1 c=336699 t=2,0,0\n\
                   cam Camera eye=0,0,4 at=0,0,0\nsun Light Dir dir=0,0,-1\n";
        let list = Scene::from_dump(src).unwrap().draw_list().unwrap();
        let p = plan(&list, &Fonts::new());
        let colour = |n: &str| p.draws.iter().find(|d| d.name == n).unwrap().vertices[0].color;
        assert_eq!(colour("u"), [0x33, 0x66, 0x99, 255]);
        let lit = colour("l");
        // Facing the light head-on: ambient 0.18 + 1.0 brightens past base.
        assert!(lit[2] > 0x99, "{lit:?}");
    }

    #[test]
    fn big_meshes_split_under_draw_call_capacity() {
        let src = "sg1\nball Mesh Sphere s=2,2,2\ncam Camera eye=0,0,4 at=0,0,0\n";
        let list = Scene::from_dump(src).unwrap().draw_list().unwrap();
        let p = plan(&list, &Fonts::new());
        assert!(p.draws.iter().all(|d| d.indices.len() <= MAX_DRAW_INDICES));
        assert!(p
            .draws
            .iter()
            .all(|d| d.indices.iter().all(|&i| (i as usize) < d.vertices.len())));
    }
}
