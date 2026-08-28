//! Traced surface mesh (Catmull-Rom control grid) + CPU warp/mask/composite.
//!
//! A [`SurfaceMesh`] is a hand-traced/auto-traced control grid mapping a 2D
//! canvas region (the printable face of an object — originally a mug) to a
//! `0..1` UV space. This crate carries the CPU side of the machinery: spline
//! tessellation of the control grid, decal placement ([`DecalPlacement`]),
//! CPU rasterization of art onto the warped surface ([`cpu_warp`]), silhouette
//! clipping and alpha masking, plus the straight-alpha composite blends
//! ([`composite_over`], [`composite_multiply`]) needed to build a full
//! "post-it on a surface" composite from this crate alone.
//!
//! Extracted verbatim from the hodlcroft compositor's `mugz-mesh` crate
//! (struct renamed `MugzSurfaceMesh` → `SurfaceMesh`; serde field names are
//! unchanged, so the mesh JSON format is identical). The GPU implementations
//! of the same operations remain host-side in the compositor. Consumers here
//! are wasm frontends (egui / macroquad) and CPU-only workers, so the crate is
//! pure: `image` (in-memory `RgbaImage` only, no codecs) + `serde`, no I/O,
//! no wgpu.

use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

pub mod spline;

/// Errors are plain strings; the extracted code has no failure modes beyond
/// what a caller-facing message can carry, and this keeps the crate free of
/// error-handling dependencies.
pub type Result<T, E = String> = std::result::Result<T, E>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurfaceMesh {
    pub version: u32,
    pub canvas: CanvasSize,
    pub source: MeshSource,
    pub generation: MeshGeneration,
    pub vertices: Vec<MeshVertex>,
    pub triangles: Vec<[usize; 3]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshSource {
    pub mug: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshGeneration {
    pub alpha_threshold: u8,
    pub rows: u32,
    pub cols: u32,
    pub top_fraction: f32,
    pub bottom_fraction: f32,
    pub side_inset_px: u32,
    pub alpha_bbox: [u32; 4],
    /// How far the printed texture wraps around the visible front of the mug, in
    /// degrees. 180 = full front semicircle (maximum edge foreshortening); small
    /// values approach a flat label with little horizontal compression.
    #[serde(default = "default_wrap_degrees")]
    pub wrap_degrees: f32,
    /// Vertical bow of each row as a fraction of that row's half-width, modelling
    /// the downward ellipse arc of a cylinder viewed slightly from above.
    /// 0 = flat horizontal rows.
    #[serde(default = "default_curve_strength")]
    pub curve_strength: f32,
    /// Extra base compression beyond the geometric arc-length row distribution
    /// (1 = none). See [`DEFAULT_BASE_BIAS`].
    #[serde(default = "default_base_bias")]
    pub base_bias: f32,
    /// Whether the top row was snapped to the detected front-lip edge.
    #[serde(default)]
    pub snap_edges: bool,
}

// Full visible front of the cylinder (sides land on the silhouette tangent).
// Reduce below 180 to wrap the print over only a centred sub-arc of the front.
pub const DEFAULT_WRAP_DEGREES: f32 = 180.0;
pub const DEFAULT_CURVE_STRENGTH: f32 = 0.16;

/// Extra base compression beyond the geometric (arc-length) row distribution.
/// 1 = pure arc-length (uniform body, compression only where the base curves);
/// >1 adds extra squeeze toward the base.
pub const DEFAULT_BASE_BIAS: f32 = 1.0;
pub const DEFAULT_ALPHA_THRESHOLD: u8 = 16;
/// Output cells per control cell when smoothing the mesh for rasterization.
pub const DEFAULT_SUBDIV: u32 = 8;
/// Pixels to erode the mug silhouette inward when clipping the print, so it stays
/// inside the dark outline. See [`clip_to_silhouette`].
pub const DEFAULT_CLIP_ERODE: u32 = 6;

fn default_wrap_degrees() -> f32 {
    DEFAULT_WRAP_DEGREES
}

fn default_curve_strength() -> f32 {
    DEFAULT_CURVE_STRENGTH
}

fn default_base_bias() -> f32 {
    DEFAULT_BASE_BIAS
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshVertex {
    pub id: usize,
    pub screen: [f32; 2],
    pub uv: [f32; 2],
}

pub struct RasterOutputs {
    pub warped: RgbaImage,
    pub uv_map: RgbaImage,
    pub surface_mask: RgbaImage,
}

/// Multiply a canvas-space layer's alpha by `mask`'s alpha.
///
/// Both are full-canvas images in the same space, so this is a straight
/// per-pixel product — anything the mask leaves transparent is erased from the
/// layer. Backs the host's `Warp::mask`: a sticker design is bounded to
/// the drawn printable face rather than to the mesh's polygon approximation of
/// it. Canvas-space, so it must run AFTER the warp.
///
/// Where the mask is smaller than the layer, the layer is erased — the mask
/// defines where the layer may exist at all.
pub fn apply_alpha_mask(mut layer: RgbaImage, mask: &RgbaImage) -> RgbaImage {
    for (x, y, px) in layer.enumerate_pixels_mut() {
        let a = if x < mask.width() && y < mask.height() {
            mask.get_pixel(x, y).0[3] as u32
        } else {
            0
        };
        px.0[3] = (px.0[3] as u32 * a / 255) as u8;
    }
    layer
}

/// Modulate `target`'s colour by a warped texture: rgb 0.5 = neutral, brighter /
/// darker values scale the target's rgb ×(2·mask), weighted by the mask's alpha.
/// The multiply half of the synth-target modes — bakes surface texture (brushed
/// metal) that TRACKS the mesh warp exactly like a pattern fill, unlike any
/// screen-space shader term. Alpha is left untouched.
pub fn multiply_rgb_by_mask(target: &mut RgbaImage, mask: &RgbaImage) {
    let w = target.width().min(mask.width()) as usize;
    let h = target.height().min(mask.height()) as usize;
    let (tw, mw) = (target.width() as usize, mask.width() as usize);
    let mbuf: &[u8] = mask;
    let tbuf: &mut [u8] = &mut *target;
    for y in 0..h {
        for x in 0..w {
            let mi = (y * mw + x) * 4;
            let ma = mbuf[mi + 3] as f32 / 255.0;
            if ma == 0.0 {
                continue;
            }
            let ti = (y * tw + x) * 4;
            if tbuf[ti + 3] == 0 {
                continue;
            }
            for c in 0..3 {
                let m = 2.0 * mbuf[mi + c] as f32 / 255.0; // 1.0 = neutral
                let scale = 1.0 + ma * (m - 1.0);
                tbuf[ti + c] = (tbuf[ti + c] as f32 * scale).clamp(0.0, 255.0) as u8;
            }
        }
    }
}

pub fn erase_alpha_by_mask(target: &mut RgbaImage, mask: &RgbaImage) {
    let w = target.width().min(mask.width()) as usize;
    let h = target.height().min(mask.height()) as usize;
    let (tw, mw) = (target.width() as usize, mask.width() as usize);
    let mbuf: &[u8] = mask;
    let tbuf: &mut [u8] = &mut *target;
    for y in 0..h {
        for x in 0..w {
            let ma = mbuf[(y * mw + x) * 4 + 3];
            if ma == 0 {
                continue;
            }
            let ti = (y * tw + x) * 4 + 3;
            tbuf[ti] = ((tbuf[ti] as u32 * (255 - ma as u32)) / 255) as u8;
        }
    }
}

pub fn rebuild_mesh_topology(mesh: &mut SurfaceMesh) {
    mesh.triangles.clear();
    rebuild_triangles(
        &mut mesh.triangles,
        mesh.generation.rows,
        mesh.generation.cols,
    );
    for (id, vertex) in mesh.vertices.iter_mut().enumerate() {
        vertex.id = id;
    }
}

/// Smoothly subdivide the control grid using Catmull-Rom splines in both
/// directions, returning a denser mesh for rasterization. Control points are
/// preserved exactly at grid nodes, so straight control edges become curves
/// without moving the points the artist placed. `subdiv` is the number of output
/// cells per control cell (1 = no smoothing, returns a clone). Edge segments
/// clamp their phantom control points to the endpoints, limiting overshoot.
pub fn tessellate(mesh: &SurfaceMesh, subdiv: u32) -> SurfaceMesh {
    let rows = mesh.generation.rows as usize;
    let cols = mesh.generation.cols as usize;
    let subdiv = subdiv.max(1) as usize;
    if subdiv == 1 || rows < 2 || cols < 2 {
        return mesh.clone();
    }
    let out_rows = (rows - 1) * subdiv + 1;
    let out_cols = (cols - 1) * subdiv + 1;

    let screen = |r: usize, c: usize| mesh.vertices[r * cols + c].screen;
    let uv = |r: usize, c: usize| mesh.vertices[r * cols + c].uv;

    let mut vertices = Vec::with_capacity(out_rows * out_cols);
    for orow in 0..out_rows {
        let fr = orow as f32 / subdiv as f32;
        let i = (fr.floor() as usize).min(rows - 2);
        let t = fr - i as f32;
        for ocol in 0..out_cols {
            let fc = ocol as f32 / subdiv as f32;
            let j = (fc.floor() as usize).min(cols - 2);
            let s = fc - j as f32;
            let screen_p = spline::surface_point(&screen, rows, cols, i, j, t, s);
            let uv_p = spline::surface_point(&uv, rows, cols, i, j, t, s);
            vertices.push(MeshVertex {
                id: vertices.len(),
                screen: screen_p,
                uv: [uv_p[0].clamp(0.0, 1.0), uv_p[1].clamp(0.0, 1.0)],
            });
        }
    }

    let mut out = SurfaceMesh {
        version: mesh.version,
        canvas: mesh.canvas.clone(),
        source: mesh.source.clone(),
        generation: MeshGeneration {
            rows: out_rows as u32,
            cols: out_cols as u32,
            ..mesh.generation.clone()
        },
        vertices,
        triangles: Vec::new(),
    };
    rebuild_mesh_topology(&mut out);
    out
}

/// What gets warped onto the mug surface. Modelled as a type rather than a
/// mode flag so callers (notably collection generation) can place any number of
/// decals without boolean toggles.
pub enum SurfacePrint<'a> {
    /// Full-surface material wrap: the texture's UV range stretches across the
    /// whole surface. Useful for all-over patterns and the checker stress test.
    Fill(&'a RgbaImage),
    /// Decals placed on the surface, composited back-to-front (last on top).
    /// Each keeps its own size/aspect and conforms to the curvature.
    Decals(&'a [PlacedDecal<'a>]),
}

/// A sticker image paired with where it sits on the surface.
pub struct PlacedDecal<'a> {
    pub sticker: &'a RgbaImage,
    pub placement: DecalPlacement,
}

/// Rasterize whatever `print` describes onto the mesh, producing the warped
/// layer, UV map, and surface mask. The surface mask always covers the full
/// mesh; the warped layer only contains the print's pixels.
pub fn rasterize_print(mesh: &SurfaceMesh, print: &SurfacePrint) -> Result<RasterOutputs> {
    match print {
        SurfacePrint::Fill(texture) => rasterize_with(mesh, &|u, v| sample_bilinear(texture, u, v)),
        SurfacePrint::Decals(decals) => {
            let rects: Vec<[f32; 4]> = decals
                .iter()
                .map(|d| {
                    d.placement
                        .uv_rect(mesh, d.sticker.width(), d.sticker.height())
                })
                .collect();
            rasterize_with(mesh, &|u, v| {
                let mut acc = [0u8; 4];
                for (d, rect) in decals.iter().zip(&rects) {
                    let uw = (rect[2] - rect[0]).max(f32::EPSILON);
                    let vh = (rect[3] - rect[1]).max(f32::EPSILON);
                    let (mut su, mut sv) = rotate_uv(
                        (u - rect[0]) / uw,
                        (v - rect[1]) / vh,
                        -d.placement.rotation,
                    );
                    // Tiled fill: wrap the (seamless) sticker across the rect.
                    // Keep in sync with the GPU path (layering warp.wgsl).
                    let [rx, ry] = d.placement.tile;
                    if rx > 1.0001 || ry > 1.0001 {
                        su = (su * rx.max(1.0)).fract();
                        sv = (sv * ry.max(1.0)).fract();
                    }
                    if (0.0..=1.0).contains(&su) && (0.0..=1.0).contains(&sv) {
                        let texel = sample_bilinear(d.sticker, su, sv).0;
                        if texel[3] > 0 {
                            acc = alpha_over(acc, texel).0;
                        }
                    }
                }
                Rgba(acc)
            })
        }
    }
}

/// Rotate a sticker-UV coord `(u, v)` by `a` radians around the sticker centre
/// `(0.5, 0.5)`. Used to sample a rotated decal (pass `-rotation`). Keep this in sync
/// with the GPU equivalent in `mugz-surface/shaders/warp.wgsl`.
fn rotate_uv(u: f32, v: f32, a: f32) -> (f32, f32) {
    if a == 0.0 {
        return (u, v);
    }
    let (du, dv) = (u - 0.5, v - 0.5);
    let (s, c) = a.sin_cos();
    (0.5 + du * c - dv * s, 0.5 + du * s + dv * c)
}

/// Convenience for the full-surface material case (see [`SurfacePrint::Fill`]).
pub fn rasterize_outputs(mesh: &SurfaceMesh, texture: &RgbaImage) -> Result<RasterOutputs> {
    rasterize_print(mesh, &SurfacePrint::Fill(texture))
}

/// Convenience for a single placed decal (see [`SurfacePrint::Decals`]).
pub fn rasterize_decal(
    mesh: &SurfaceMesh,
    sticker: &RgbaImage,
    placement: &DecalPlacement,
) -> Result<RasterOutputs> {
    rasterize_print(
        mesh,
        &SurfacePrint::Decals(&[PlacedDecal {
            sticker,
            placement: *placement,
        }]),
    )
}

fn rasterize_with(
    mesh: &SurfaceMesh,
    sample: &dyn Fn(f32, f32) -> Rgba<u8>,
) -> Result<RasterOutputs> {
    let mut uv_map = RgbaImage::new(mesh.canvas.width, mesh.canvas.height);
    let mut warped = RgbaImage::new(mesh.canvas.width, mesh.canvas.height);
    let mut surface_mask = RgbaImage::new(mesh.canvas.width, mesh.canvas.height);
    rasterize_mesh(mesh, sample, &mut warped, &mut uv_map, &mut surface_mask)?;
    Ok(RasterOutputs {
        warped,
        uv_map,
        surface_mask,
    })
}

/// Placement of a sticker decal in the mesh's UV space. `center` is the decal
/// centre in `0..1` surface coords, `scale` is the decal width as a fraction of
/// the surface width, and `keep_aspect` preserves the sticker's pixel aspect
/// ratio (using the surface's screen extent) so a square sticker stays square.
/// Serializable so generation can pick `center`/`scale` from a random range.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DecalPlacement {
    pub center: [f32; 2],
    pub scale: f32,
    pub keep_aspect: bool,
    /// Decal rotation in RADIANS, applied around the sticker centre during sampling.
    /// (Correct for `keep_aspect` decals — the UV→surface mapping is ~square there;
    /// a non-keep-aspect decal would shear. Large angles clip at the UV-rect corners.)
    #[serde(default)]
    pub rotation: f32,
    /// Repeats of the (seamless) sticker inside its UV rect, `[1, 1]` = none.
    /// Aspect-true pattern fills tile horizontally to cancel the band's
    /// centre-latitude stretch.
    #[serde(default = "default_tile")]
    pub tile: [f32; 2],
    /// `true` = `scale` sizes the HEIGHT (fraction of the printable band) and the
    /// WIDTH is solved from the art aspect — the inverse of the default, for
    /// full-height prints (a PFP filling the mug band, never stretched). Requires
    /// `keep_aspect`; ignored otherwise. See [`Self::uv_rect`].
    #[serde(default)]
    pub fit_height: bool,
}

fn default_tile() -> [f32; 2] {
    [1.0, 1.0]
}

impl Default for DecalPlacement {
    fn default() -> Self {
        Self {
            center: [0.5, 0.5],
            scale: 0.6,
            keep_aspect: true,
            rotation: 0.0,
            tile: [1.0, 1.0],
            fit_height: false,
        }
    }
}

impl DecalPlacement {
    /// Compute the `[u0, v0, u1, v1]` UV rectangle the sticker maps into.
    ///
    /// With `keep_aspect`, the UV height is solved so the decal's SCREEN
    /// footprint matches the art's pixel aspect: the footprint width is the
    /// exact screen span of the rect's U extent (endpoint difference — i.e.
    /// the integral of dx/du, not a single-point differential, which
    /// overestimates width by ~30% for a decal spanning half the surface
    /// because its outer regions foreshorten), and the height is fixed-point
    /// solved on the same measure over V. Both spans are measured on the
    /// reference column/row through `u = 0.5`, so the result is independent
    /// of horizontal placement: moving a decal sideways repositions it and
    /// then foreshortens naturally via the mesh warp, never resizes it.
    pub fn uv_rect(&self, mesh: &SurfaceMesh, sticker_w: u32, sticker_h: u32) -> [f32; 4] {
        let aspect = if self.keep_aspect && sticker_w > 0 {
            sticker_h as f32 / sticker_w as f32
        } else {
            0.0
        };
        // Fallback initial ratio (dx/du)/(dy/dv): the legacy centre-point
        // differential (small-decal approximation), used only to seed the
        // fixed-point / when the span probes fall off the mesh.
        let ratio = || {
            local_screen_aspect(mesh, [0.5, 0.5])
                .or_else(|| {
                    let (sw, sh) = surface_extent(mesh);
                    (sh > f32::EPSILON).then_some(sw / sh)
                })
                .unwrap_or(1.0)
        };
        let cv = self.center[1];
        let (uv_w, uv_h) = if aspect <= 0.0 {
            // No aspect lock — square in UV.
            (self.scale, self.scale)
        } else if self.fit_height {
            // FIT HEIGHT: `scale` is the band-height fraction; solve the WIDTH so
            // the decal's on-screen footprint keeps the art aspect (the inverse of
            // the width path below). The target width is the exact screen height of
            // the V span divided by the art aspect; uv_w converges the same way.
            let uv_h = self.scale;
            let mut uv_w = (uv_h / (ratio() * aspect)).min(MAX_UV_H);
            let target_w_px =
                screen_span_y(mesh, cv - uv_h * 0.5, cv + uv_h * 0.5).map(|h_px| h_px / aspect);
            if let Some(target) = target_w_px {
                for _ in 0..4 {
                    let Some(w_px) = screen_span_x(mesh, 0.5 - uv_w * 0.5, 0.5 + uv_w * 0.5, cv)
                    else {
                        break;
                    };
                    if w_px <= f32::EPSILON {
                        break;
                    }
                    uv_w = (uv_w * target / w_px).min(MAX_UV_H);
                }
            }
            (uv_w, uv_h)
        } else {
            // FIT WIDTH (default): `scale` is the width fraction; solve the height.
            let uv_w = self.scale;
            let mut uv_h = (uv_w * ratio() * aspect).min(MAX_UV_H);
            let target_h_px = screen_span_x(mesh, 0.5 - uv_w * 0.5, 0.5 + uv_w * 0.5, cv)
                .map(|w_px| w_px * aspect);
            if let Some(target) = target_h_px {
                // h_px grows monotonically (near-linearly) with uv_h, so a
                // multiplicative update converges in a few rounds.
                for _ in 0..4 {
                    let Some(h_px) = screen_span_y(mesh, cv - uv_h * 0.5, cv + uv_h * 0.5) else {
                        break;
                    };
                    if h_px <= f32::EPSILON {
                        break;
                    }
                    uv_h = (uv_h * target / h_px).min(MAX_UV_H);
                }
            }
            (uv_w, uv_h)
        };
        [
            self.center[0] - uv_w * 0.5,
            self.center[1] - uv_h * 0.5,
            self.center[0] + uv_w * 0.5,
            self.center[1] + uv_h * 0.5,
        ]
    }
}

/// Sanity cap for the solved UV height (a decal can legitimately span more
/// than the band, but a runaway on a degenerate mesh must not).
const MAX_UV_H: f32 = 1.5;

/// Exact screen width of a U span at latitude `v`: the x-difference of the
/// mapped endpoints (the integral of dx/du over the span). Probes are clamped
/// onto the mesh and the result rescaled linearly for any clamped-off part.
fn screen_span_x(mesh: &SurfaceMesh, u0: f32, u1: f32, v: f32) -> Option<f32> {
    let vc = v.clamp(0.02, 0.98);
    let a = u0.clamp(0.02, 0.98);
    let b = u1.clamp(0.02, 0.98);
    if b - a < 1e-4 {
        return None;
    }
    let pa = uv_to_screen(mesh, [a, vc])?;
    let pb = uv_to_screen(mesh, [b, vc])?;
    Some((pb[0] - pa[0]).abs() * ((u1 - u0) / (b - a)))
}

/// Exact screen height of a V span on the reference column `u = 0.5` (see
/// [`screen_span_x`]).
fn screen_span_y(mesh: &SurfaceMesh, v0: f32, v1: f32) -> Option<f32> {
    let a = v0.clamp(0.02, 0.98);
    let b = v1.clamp(0.02, 0.98);
    if b - a < 1e-4 {
        return None;
    }
    let pa = uv_to_screen(mesh, [0.5, a])?;
    let pb = uv_to_screen(mesh, [0.5, b])?;
    Some((pb[1] - pa[1]).abs() * ((v1 - v0) / (b - a)))
}

/// Ratio of horizontal to vertical screen scale of the uv→screen mapping at
/// `center` (i.e. `(dx/du) / (dy/dv)`), via central differences. `None` if the
/// sample points fall off the mesh.
///
/// LIMITATION: a single-point sample (tuned for a uniform cylinder). On a heavily
/// contorted, non-cylindrical mesh the local aspect varies across the surface, so this
/// one measurement mis-scales the decal (visible size/stretch wobble). Acceptable for
/// rigid mugs; revisit (footprint-averaged or conformal UV) for cloth/t-shirt
/// materials. See docs/layered-processor-architecture.md "Known limitations".
fn local_screen_aspect(mesh: &SurfaceMesh, center: [f32; 2]) -> Option<f32> {
    let eps = 0.02;
    let left = uv_to_screen(mesh, [center[0] - eps, center[1]])?;
    let right = uv_to_screen(mesh, [center[0] + eps, center[1]])?;
    let top = uv_to_screen(mesh, [center[0], center[1] - eps])?;
    let bottom = uv_to_screen(mesh, [center[0], center[1] + eps])?;
    let dx_du = (right[0] - left[0]).abs();
    let dy_dv = (bottom[1] - top[1]).abs();
    (dy_dv > f32::EPSILON).then_some(dx_du / dy_dv)
}

/// Screen-space bounding box extent (width, height) of the mesh control points.
pub fn surface_extent(mesh: &SurfaceMesh) -> (f32, f32) {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for v in &mesh.vertices {
        min_x = min_x.min(v.screen[0]);
        min_y = min_y.min(v.screen[1]);
        max_x = max_x.max(v.screen[0]);
        max_y = max_y.max(v.screen[1]);
    }
    ((max_x - min_x).max(0.0), (max_y - min_y).max(0.0))
}

/// Map a UV coordinate back to a screen point by locating the triangle that
/// contains it in UV space and interpolating its screen positions. Used to draw
/// the decal footprint conforming to the surface.
pub fn uv_to_screen(mesh: &SurfaceMesh, uv: [f32; 2]) -> Option<[f32; 2]> {
    for triangle in &mesh.triangles {
        let a = &mesh.vertices[triangle[0]];
        let b = &mesh.vertices[triangle[1]];
        let c = &mesh.vertices[triangle[2]];
        let denom = edge(a.uv, b.uv, c.uv);
        if denom.abs() < f32::EPSILON {
            continue;
        }
        let w0 = edge(b.uv, c.uv, uv) / denom;
        let w1 = edge(c.uv, a.uv, uv) / denom;
        let w2 = edge(a.uv, b.uv, uv) / denom;
        if w0 >= -0.001 && w1 >= -0.001 && w2 >= -0.001 {
            return Some([
                w0 * a.screen[0] + w1 * b.screen[0] + w2 * c.screen[0],
                w0 * a.screen[1] + w1 * b.screen[1] + w2 * c.screen[1],
            ]);
        }
    }
    None
}

pub fn clip_to_silhouette(warped: &mut RgbaImage, mug: &RgbaImage, threshold: u8, erode_px: u32) {
    let mask = eroded_alpha_mask(mug, threshold, erode_px);
    let w = warped.width().min(mug.width()) as usize;
    let h = warped.height().min(mug.height()) as usize;
    let mw = mug.width() as usize;
    let ww = warped.width() as usize;
    // Direct buffer access — get_pixel/put_pixel per pixel is far too slow at 2048².
    let buf: &mut [u8] = &mut *warped;
    for y in 0..h {
        let mrow = y * mw;
        let wrow = y * ww * 4;
        for x in 0..w {
            if !mask[mrow + x] {
                let i = wrow + x * 4;
                buf[i] = 0;
                buf[i + 1] = 0;
                buf[i + 2] = 0;
                buf[i + 3] = 0;
            }
        }
    }
}

/// Boolean "inside" mask of the mug alpha, eroded inward by `erode_px` using a
/// separable box erosion (prefix-sum, O(width*height)).
fn eroded_alpha_mask(img: &RgbaImage, threshold: u8, erode_px: u32) -> Vec<bool> {
    let w = img.width() as usize;
    let h = img.height() as usize;
    let inside: Vec<bool> = img
        .as_raw()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| p[3] > threshold)
        .collect();
    if erode_px == 0 {
        return inside;
    }
    let r = erode_px as i64;
    // Horizontal pass: a pixel survives if no "outside" pixel is within r columns.
    let mut tmp = vec![false; w * h];
    let mut prefix = vec![0i32; w + 1];
    for y in 0..h {
        for x in 0..w {
            prefix[x + 1] = prefix[x] + i32::from(!inside[y * w + x]);
        }
        for x in 0..w {
            let lo = (x as i64 - r).max(0) as usize;
            let hi = ((x as i64 + r + 1).min(w as i64)) as usize;
            tmp[y * w + x] = prefix[hi] - prefix[lo] == 0;
        }
    }
    // Vertical pass over the horizontally-eroded mask.
    let mut out = vec![false; w * h];
    let mut col_prefix = vec![0i32; h + 1];
    for x in 0..w {
        for y in 0..h {
            col_prefix[y + 1] = col_prefix[y] + i32::from(!tmp[y * w + x]);
        }
        for y in 0..h {
            let lo = (y as i64 - r).max(0) as usize;
            let hi = ((y as i64 + r + 1).min(h as i64)) as usize;
            out[y * w + x] = col_prefix[hi] - col_prefix[lo] == 0;
        }
    }
    out
}

fn rebuild_triangles(triangles: &mut Vec<[usize; 3]>, rows: u32, cols: u32) {
    triangles.reserve(((rows - 1) * (cols - 1) * 2) as usize);
    for row in 0..rows - 1 {
        for col in 0..cols - 1 {
            let a = (row * cols + col) as usize;
            let b = (row * cols + col + 1) as usize;
            let c = ((row + 1) * cols + col) as usize;
            let d = ((row + 1) * cols + col + 1) as usize;
            triangles.push([a, b, d]);
            triangles.push([a, d, c]);
        }
    }
}

fn rasterize_mesh(
    mesh: &SurfaceMesh,
    sample: &dyn Fn(f32, f32) -> Rgba<u8>,
    warped: &mut RgbaImage,
    uv_map: &mut RgbaImage,
    surface_mask: &mut RgbaImage,
) -> Result<()> {
    for triangle in &mesh.triangles {
        let a = &mesh.vertices[triangle[0]];
        let b = &mesh.vertices[triangle[1]];
        let c = &mesh.vertices[triangle[2]];
        rasterize_triangle(a, b, c, sample, warped, uv_map, surface_mask)?;
    }
    Ok(())
}

fn rasterize_triangle(
    a: &MeshVertex,
    b: &MeshVertex,
    c: &MeshVertex,
    sample: &dyn Fn(f32, f32) -> Rgba<u8>,
    warped: &mut RgbaImage,
    uv_map: &mut RgbaImage,
    surface_mask: &mut RgbaImage,
) -> Result<()> {
    let min_x = a.screen[0]
        .min(b.screen[0])
        .min(c.screen[0])
        .floor()
        .max(0.0) as u32;
    let max_x = a.screen[0]
        .max(b.screen[0])
        .max(c.screen[0])
        .ceil()
        .min((warped.width() - 1) as f32) as u32;
    let min_y = a.screen[1]
        .min(b.screen[1])
        .min(c.screen[1])
        .floor()
        .max(0.0) as u32;
    let max_y = a.screen[1]
        .max(b.screen[1])
        .max(c.screen[1])
        .ceil()
        .min((warped.height() - 1) as f32) as u32;

    let denom = edge(a.screen, b.screen, c.screen);
    if denom.abs() < f32::EPSILON {
        return Ok(());
    }

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let p = [x as f32 + 0.5, y as f32 + 0.5];
            let w0 = edge(b.screen, c.screen, p) / denom;
            let w1 = edge(c.screen, a.screen, p) / denom;
            let w2 = edge(a.screen, b.screen, p) / denom;

            if w0 >= -0.0001 && w1 >= -0.0001 && w2 >= -0.0001 {
                let u = w0 * a.uv[0] + w1 * b.uv[0] + w2 * c.uv[0];
                let v = w0 * a.uv[1] + w1 * b.uv[1] + w2 * c.uv[1];
                let texel = sample(u, v);
                warped.put_pixel(x, y, texel);
                uv_map.put_pixel(
                    x,
                    y,
                    Rgba([
                        (u.clamp(0.0, 1.0) * 255.0).round() as u8,
                        (v.clamp(0.0, 1.0) * 255.0).round() as u8,
                        0,
                        255,
                    ]),
                );
                surface_mask.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
    }

    Ok(())
}

fn edge(a: [f32; 2], b: [f32; 2], p: [f32; 2]) -> f32 {
    (p[0] - a[0]) * (b[1] - a[1]) - (p[1] - a[1]) * (b[0] - a[0])
}

fn sample_bilinear(img: &RgbaImage, u: f32, v: f32) -> Rgba<u8> {
    let x = u.clamp(0.0, 1.0) * (img.width().saturating_sub(1)) as f32;
    let y = v.clamp(0.0, 1.0) * (img.height().saturating_sub(1)) as f32;
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(img.width() - 1);
    let y1 = (y0 + 1).min(img.height() - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;

    // Sample in premultiplied space so transparent texels do not bleed their RGB
    // into the visible edge of a sticker (avoids dark/colored halos).
    let p00 = premultiply(img.get_pixel(x0, y0).0);
    let p10 = premultiply(img.get_pixel(x1, y0).0);
    let p01 = premultiply(img.get_pixel(x0, y1).0);
    let p11 = premultiply(img.get_pixel(x1, y1).0);
    let mut acc = [0.0f32; 4];

    for i in 0..4 {
        let top = lerp(p00[i], p10[i], tx);
        let bottom = lerp(p01[i], p11[i], tx);
        acc[i] = lerp(top, bottom, ty);
    }

    unpremultiply(acc)
}

fn premultiply(px: [u8; 4]) -> [f32; 4] {
    let a = px[3] as f32 / 255.0;
    [
        px[0] as f32 * a,
        px[1] as f32 * a,
        px[2] as f32 * a,
        px[3] as f32,
    ]
}

fn unpremultiply(px: [f32; 4]) -> Rgba<u8> {
    let a = px[3];
    let out = if a <= f32::EPSILON {
        [0, 0, 0, 0]
    } else {
        let inv = 255.0 / a;
        [
            (px[0] * inv).round().clamp(0.0, 255.0) as u8,
            (px[1] * inv).round().clamp(0.0, 255.0) as u8,
            (px[2] * inv).round().clamp(0.0, 255.0) as u8,
            a.round().clamp(0.0, 255.0) as u8,
        ]
    };
    Rgba(out)
}

fn alpha_over(dst: [u8; 4], src: [u8; 4]) -> Rgba<u8> {
    let sa = src[3] as f32 / 255.0;
    let da = dst[3] as f32 / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a <= f32::EPSILON {
        return Rgba([0, 0, 0, 0]);
    }

    let mut out = [0; 4];
    for i in 0..3 {
        let sc = src[i] as f32 / 255.0;
        let dc = dst[i] as f32 / 255.0;
        out[i] = (((sc * sa + dc * da * (1.0 - sa)) / out_a) * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8;
    }
    out[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
    Rgba(out)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// CPU-warp `art` onto the mesh at `placement`, returning just the warped
/// layer (the [`rasterize_decal`] output without the UV map / surface mask).
pub fn cpu_warp(
    mesh: &SurfaceMesh,
    art: &RgbaImage,
    placement: &DecalPlacement,
) -> Result<RgbaImage> {
    Ok(rasterize_decal(mesh, art, placement)?.warped)
}

/// [`cpu_warp`], then clip the result to a silhouette when `clip` is
/// `Some((silhouette, alpha_threshold))`. No erosion is applied (matching the
/// host's CPU `warp_clipped` semantics); call [`clip_to_silhouette`] directly
/// with [`DEFAULT_CLIP_ERODE`] when an inset from the outline is wanted.
pub fn cpu_warp_clipped(
    mesh: &SurfaceMesh,
    art: &RgbaImage,
    placement: &DecalPlacement,
    clip: Option<(&RgbaImage, u8)>,
) -> Result<RgbaImage> {
    let mut warped = cpu_warp(mesh, art, placement)?;
    if let Some((mask, threshold)) = clip {
        clip_to_silhouette(&mut warped, mask, threshold, 0);
    }
    Ok(warped)
}

/// Stack multiply blend with straight alpha, matching the GPU compositor's
/// `fs_multiply` pipeline (`out.rgb = factor * dst.rgb`, factor pre-lerped
/// toward white by src alpha):
///
/// `out.rgb = dst.rgb * lerp(1.0, src.rgb, src.a)`
///
/// The destination alpha (coverage) is unchanged, so a multiply layer darkens
/// what is already there and never extends it.
pub fn composite_multiply(dst: &mut RgbaImage, src: &RgbaImage) {
    let w = dst.width().min(src.width()) as usize;
    let h = dst.height().min(src.height()) as usize;
    let (dw, sw) = (dst.width() as usize, src.width() as usize);
    let sbuf: &[u8] = src;
    let dbuf: &mut [u8] = &mut *dst;
    for y in 0..h {
        for x in 0..w {
            let si = (y * sw + x) * 4;
            let sa = sbuf[si + 3] as f32 / 255.0;
            if sa == 0.0 {
                continue;
            }
            let di = (y * dw + x) * 4;
            for c in 0..3 {
                let s = sbuf[si + c] as f32 / 255.0;
                let factor = lerp(1.0, s, sa);
                dbuf[di + c] = (dbuf[di + c] as f32 * factor).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// Plain straight-alpha source-over composite of `src` onto `dst` (the normal
/// layer blend), so a full composite can be built from this crate alone.
pub fn composite_over(dst: &mut RgbaImage, src: &RgbaImage) {
    let w = dst.width().min(src.width()) as usize;
    let h = dst.height().min(src.height()) as usize;
    let (dw, sw) = (dst.width() as usize, src.width() as usize);
    let sbuf: &[u8] = src;
    let dbuf: &mut [u8] = &mut *dst;
    for y in 0..h {
        for x in 0..w {
            let si = (y * sw + x) * 4;
            if sbuf[si + 3] == 0 {
                continue;
            }
            let di = (y * dw + x) * 4;
            let s = [sbuf[si], sbuf[si + 1], sbuf[si + 2], sbuf[si + 3]];
            let d = [dbuf[di], dbuf[di + 1], dbuf[di + 2], dbuf[di + 3]];
            let out = alpha_over(d, s).0;
            dbuf[di..di + 4].copy_from_slice(&out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal 2-row × 3-col control grid (two quads wide) on a 64×64 canvas:
    /// screen rect (10,10)–(50,40), UV spanning the full 0..1 range.
    fn two_quad_mesh() -> SurfaceMesh {
        let mut mesh = SurfaceMesh {
            version: 1,
            canvas: CanvasSize {
                width: 64,
                height: 64,
            },
            source: MeshSource {
                mug: "synthetic".into(),
            },
            generation: MeshGeneration {
                alpha_threshold: DEFAULT_ALPHA_THRESHOLD,
                rows: 2,
                cols: 3,
                top_fraction: 0.0,
                bottom_fraction: 1.0,
                side_inset_px: 0,
                alpha_bbox: [10, 10, 50, 40],
                wrap_degrees: DEFAULT_WRAP_DEGREES,
                curve_strength: 0.0,
                base_bias: DEFAULT_BASE_BIAS,
                snap_edges: false,
            },
            vertices: vec![
                MeshVertex {
                    id: 0,
                    screen: [10.0, 10.0],
                    uv: [0.0, 0.0],
                },
                MeshVertex {
                    id: 1,
                    screen: [30.0, 10.0],
                    uv: [0.5, 0.0],
                },
                MeshVertex {
                    id: 2,
                    screen: [50.0, 10.0],
                    uv: [1.0, 0.0],
                },
                MeshVertex {
                    id: 3,
                    screen: [10.0, 40.0],
                    uv: [0.0, 1.0],
                },
                MeshVertex {
                    id: 4,
                    screen: [30.0, 40.0],
                    uv: [0.5, 1.0],
                },
                MeshVertex {
                    id: 5,
                    screen: [50.0, 40.0],
                    uv: [1.0, 1.0],
                },
            ],
            triangles: Vec::new(),
        };
        rebuild_mesh_topology(&mut mesh);
        mesh
    }

    fn solid(w: u32, h: u32, px: [u8; 4]) -> RgbaImage {
        RgbaImage::from_pixel(w, h, Rgba(px))
    }

    #[test]
    fn tessellate_and_cpu_warp_land_inside_mesh_bbox() {
        let mesh = tessellate(&two_quad_mesh(), 4);
        assert_eq!(mesh.generation.rows, 5);
        assert_eq!(mesh.generation.cols, 9);
        assert_eq!(mesh.vertices.len(), 45);
        assert_eq!(mesh.triangles.len(), 4 * 8 * 2);

        let art = solid(8, 8, [200, 40, 40, 255]);
        let warped = cpu_warp(&mesh, &art, &DecalPlacement::default()).unwrap();
        assert_eq!(warped.dimensions(), (64, 64));

        let mut visible = 0usize;
        for (x, y, px) in warped.enumerate_pixels() {
            if px.0[3] > 0 {
                visible += 1;
                // Everything the warp produced must sit inside the mesh's
                // screen bbox (10,10)-(50,40), +1px for the ceil'd raster edge.
                assert!(
                    (9..=51).contains(&x) && (9..=41).contains(&y),
                    "warped pixel outside mesh bbox at ({x}, {y})"
                );
            }
        }
        assert!(visible > 0, "warp produced no visible pixels");
    }

    #[test]
    fn cpu_warp_clipped_erases_outside_silhouette() {
        let mesh = two_quad_mesh();
        let art = solid(8, 8, [200, 40, 40, 255]);
        // Silhouette only covers the left half of the canvas.
        let mut silhouette = solid(64, 64, [0, 0, 0, 0]);
        for y in 0..64 {
            for x in 0..30 {
                silhouette.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
        let clipped = cpu_warp_clipped(
            &mesh,
            &art,
            &DecalPlacement::default(),
            Some((&silhouette, 0)),
        )
        .unwrap();
        let unclipped = cpu_warp(&mesh, &art, &DecalPlacement::default()).unwrap();
        assert!(
            unclipped
                .enumerate_pixels()
                .any(|(x, _, p)| x >= 30 && p.0[3] > 0)
        );
        assert!(
            clipped
                .enumerate_pixels()
                .all(|(x, _, p)| x < 30 || p.0[3] == 0)
        );
        assert!(clipped.pixels().any(|p| p.0[3] > 0));
    }

    #[test]
    fn apply_alpha_mask_zero_alpha_erases_layer() {
        let layer = solid(4, 4, [10, 20, 30, 255]);
        let mask = solid(4, 4, [255, 255, 255, 0]);
        let out = apply_alpha_mask(layer, &mask);
        assert!(out.pixels().all(|p| p.0[3] == 0));
    }

    #[test]
    fn composite_multiply_matches_gpu_semantics() {
        // White src (any alpha) leaves dst unchanged.
        let mut dst = solid(2, 2, [100, 150, 200, 180]);
        composite_multiply(&mut dst, &solid(2, 2, [255, 255, 255, 255]));
        assert!(dst.pixels().all(|p| p.0 == [100, 150, 200, 180]));

        // Opaque black src multiplies rgb to 0 but keeps dst alpha (coverage).
        composite_multiply(&mut dst, &solid(2, 2, [0, 0, 0, 255]));
        assert!(dst.pixels().all(|p| p.0 == [0, 0, 0, 180]));

        // Fully transparent src is a no-op (factor lerps to 1).
        let mut dst = solid(2, 2, [100, 150, 200, 180]);
        composite_multiply(&mut dst, &solid(2, 2, [0, 0, 0, 0]));
        assert!(dst.pixels().all(|p| p.0 == [100, 150, 200, 180]));
    }

    #[test]
    fn composite_over_source_over() {
        let mut dst = solid(2, 2, [10, 20, 30, 255]);
        composite_over(&mut dst, &solid(2, 2, [200, 100, 50, 255]));
        assert!(dst.pixels().all(|p| p.0 == [200, 100, 50, 255]));

        // Transparent src leaves dst untouched.
        let mut dst = solid(2, 2, [10, 20, 30, 128]);
        composite_over(&mut dst, &solid(2, 2, [200, 100, 50, 0]));
        assert!(dst.pixels().all(|p| p.0 == [10, 20, 30, 128]));
    }

    #[test]
    fn surface_mesh_json_field_names_unchanged() {
        // The rename MugzSurfaceMesh -> SurfaceMesh must not change the wire
        // format: serde field names drive the JSON.
        let mesh = two_quad_mesh();
        let json = serde_json::to_string(&mesh).unwrap();
        for key in [
            "\"version\"",
            "\"canvas\"",
            "\"source\"",
            "\"mug\"",
            "\"generation\"",
            "\"vertices\"",
            "\"triangles\"",
            "\"screen\"",
            "\"uv\"",
            "\"wrap_degrees\"",
            "\"curve_strength\"",
            "\"base_bias\"",
            "\"snap_edges\"",
        ] {
            assert!(json.contains(key), "missing {key} in serialized mesh");
        }
        let back: SurfaceMesh = serde_json::from_str(&json).unwrap();
        assert_eq!(back.vertices.len(), mesh.vertices.len());
        assert_eq!(back.triangles.len(), mesh.triangles.len());
    }
}
