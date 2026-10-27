//! `ScenePane` — a scene as one rect on a gamekit stage.
//!
//! The pane draws whatever scene it is shown (rest, or a posed copy from
//! `sceneanim` / `scenedrv`) through `scenegraph-mq` into a depth target the
//! rect's size at device pixels, then composites that texture into the rect.
//! Everything else on the stage — a HUD, baked faces, effects — draws around
//! it the gamekit way, so 2D chrome and the 3D scene never share a camera.
//!
//! ```ignore
//! pane.show(posed);                    // each frame, or only on change
//! pane.draw(rect, frame.px);           // inside Game::draw
//! let hit = pane.hit(tap, rect);       // a node name, or "-"
//! pane.report_labels(rect);            // scene text into the capture report
//! ```
//!
//! [`PaneStats`] times each phase of the last frame (flatten, plan, submit)
//! from macroquad's clock, which the browser has too.

use macroquad::prelude::*;
use macroquad_gamekit::harness::report;
use scenegraph_mq::{self as mq, Renderer};
use scenegraph::{DrawList, Fonts, Geometry, Images, Kind, Materials, Scene};
use sceneinput::{Event, Session, step};

/// What the last [`ScenePane::draw`] cost, in microseconds, and what it drew.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PaneStats {
    /// Scene → draw list. Zero on a frame that reused the list.
    pub flatten_us: f64,
    /// Inside flatten: world matrices, prims (tessellation), labels (layout).
    pub matrix_us: f64,
    pub prims_us: f64,
    pub labels_us: f64,
    /// Draw list → the meshes the backend submits.
    pub plan_us: f64,
    /// Queueing those meshes (the GPU runs them at the frame's end).
    pub submit_us: f64,
    pub nodes: usize,
    pub tris: usize,
    pub draws: usize,
    /// The list was rebuilt this frame.
    pub flattened: bool,
}

pub struct ScenePane {
    pub geom: Geometry,
    pub materials: Materials,
    pub fonts: Fonts,
    pub images: Images,
    pub background: Color,
    scene: Scene,
    list: Option<DrawList>,
    renderer: Renderer,
    /// Held, never dropped while its texture is drawn (a dropped target frees
    /// its texture id for reuse).
    target: Option<(RenderTarget, u32, u32)>,
    pub stats: PaneStats,
    pub status: String,
}

fn now_us() -> f64 {
    get_time() * 1e6
}

impl ScenePane {
    pub fn new(geom: Geometry, materials: Materials, fonts: Fonts) -> Self {
        Self {
            geom,
            materials,
            fonts,
            images: Images::new(),
            background: Color::new(0.07, 0.07, 0.09, 1.0),
            scene: Scene::new(),
            list: None,
            renderer: Renderer::new(),
            target: None,
            stats: PaneStats::default(),
            status: String::new(),
        }
    }

    /// Show `scene` from the next draw. The list is rebuilt on demand.
    pub fn show(&mut self, scene: Scene) {
        self.scene = scene;
        self.list = None;
    }

    /// The scene shown.
    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// The shown scene, flattened; rebuilt only after [`Self::show`].
    pub fn list(&mut self) -> Option<&DrawList> {
        if self.list.is_none() {
            let t = now_us();
            match self
                .scene
                .draw_list_with(&self.geom, &self.materials, &mut self.fonts)
            {
                Ok(l) => {
                    self.stats.matrix_us = l.cost.matrix_us as f64;
                    self.stats.prims_us = l.cost.prims_us as f64;
                    self.stats.labels_us = l.cost.labels_us as f64;
                    self.list = Some(l);
                }
                Err(e) => self.status = e.to_string(),
            }
            self.stats.flatten_us = now_us() - t;
            self.stats.flattened = true;
        }
        self.list.as_ref()
    }

    /// Draw into `rect` (stage units) at `px` device pixels per unit. The
    /// caller's camera is left as it was.
    pub fn draw(&mut self, rect: Rect, px: f32) {
        self.stats.flatten_us = 0.0;
        self.stats.flattened = false;
        let w = (rect.w * px).round().max(1.0) as u32;
        let h = (rect.h * px).round().max(1.0) as u32;
        if self
            .target
            .as_ref()
            .is_none_or(|(_, tw, th)| (*tw, *th) != (w, h))
        {
            let rt = render_target_ex(
                w,
                h,
                RenderTargetParams {
                    // 0, never 1: WebGL1 has no multisample resolve.
                    sample_count: 0,
                    depth: true,
                },
            );
            rt.texture.set_filter(FilterMode::Linear);
            self.target = Some((rt, w, h));
        }
        if self.list().is_none() {
            return;
        }
        let (Some(list), Some((rt, _, _))) = (self.list.as_ref(), self.target.as_ref()) else {
            return;
        };
        let t = now_us();
        let plan = mq::plan(list, &self.fonts);
        self.stats.plan_us = now_us() - t;
        let census = plan.census();
        self.stats.tris = census.tris() as usize;
        self.stats.draws = plan.draws.len();
        self.stats.nodes = self.scene.len();
        let t = now_us();
        self.renderer.draw_plan_to(
            &plan,
            list,
            &self.images,
            &self.fonts,
            Some(rt),
            w as f32 / h as f32,
            Some(self.background),
        );
        self.stats.submit_us = now_us() - t;
        draw_texture_ex(
            &rt.texture,
            rect.x,
            rect.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(rect.size()),
                flip_y: true,
                ..Default::default()
            },
        );
    }

    /// The node a pointer at stage point `at` selects in `rect`, or `-`: the
    /// `sceneinput` pick the gauntlet makes (a Label selects its parent).
    pub fn hit(&mut self, at: Vec2, rect: Rect) -> String {
        let (ndc, aspect) = to_ndc(at, rect);
        let scene = self.scene.clone();
        let Some(list) = self.list() else {
            return "-".into();
        };
        match step(
            &scene,
            list,
            &Session::new(),
            Event::down(ndc.x, ndc.y, aspect),
        ) {
            Ok((_, sess, _)) => sess.sel.map(|s| s.name).unwrap_or_else(|| "-".into()),
            Err(e) => {
                self.status = e.to_string();
                "-".into()
            }
        }
    }

    /// Where `node` is drawn — the centre of its meshes, else its origin
    /// ([`DrawList::aim_point`]) — as a stage point in `rect`.
    pub fn drawn_at(&mut self, node: &str, rect: Rect) -> Option<Vec2> {
        let scene = self.scene.clone();
        let list = self.list()?;
        let p = list.aim_point(&scene, node)?;
        let (x, y) = list.project_ndc(p, rect.w / rect.h)?;
        Some(from_ndc(vec2(x, y), rect))
    }

    /// Record every visible scene Label in the gamekit's capture report, as
    /// it lands on the stage: its text, its projected box and em in stage
    /// units. A label under another node (a card under the next in a fan) is
    /// not on screen and is not recorded. Off unless the report is on.
    pub fn report_labels(&mut self, rect: Rect) {
        if !report::on() {
            return;
        }
        let scene = self.scene.clone();
        let Some(list) = self.list() else {
            return;
        };
        let aspect = rect.w / rect.h;
        for lab in &list.labels {
            let Some(Kind::Label { text, size, .. }) = scene.get(&lab.name).map(|n| &n.kind) else {
                continue;
            };
            let corners: Option<Vec<Vec2>> = lab
                .corners()
                .iter()
                .map(|c| {
                    list.project_ndc(*c, aspect)
                        .map(|(x, y)| from_ndc(vec2(x, y), rect))
                })
                .collect();
            let Some(corners) = corners else {
                continue;
            };
            let Some((cx, cy)) = list.project_ndc(lab.centre, aspect) else {
                continue;
            };
            // Visible only if a pick at its centre lands on it (or the
            // control it labels): otherwise something is drawn over it.
            let parent = scene.get(&lab.name).and_then(|n| n.parent.as_deref());
            let on_top = list
                .hit_ndc(cx, cy, aspect)
                .is_some_and(|h| h.name == lab.name || Some(h.name) == parent);
            if !on_top {
                continue;
            }
            let (lo, hi) = corners.iter().fold(
                (vec2(f32::MAX, f32::MAX), vec2(f32::MIN, f32::MIN)),
                |(lo, hi), c| (lo.min(*c), hi.max(*c)),
            );
            // One em, projected: the layout box is `h` local units tall.
            let em_world = size * 2.0 * lab.axis_y.length() / lab.h.max(1e-6);
            let tip = lab.centre + lab.axis_y.normalized() * em_world;
            let em = list
                .project_ndc(tip, aspect)
                .map(|(x, y)| (from_ndc(vec2(x, y), rect) - from_ndc(vec2(cx, cy), rect)).length())
                .unwrap_or(0.0);
            report::line(
                text,
                Rect::new(lo.x, lo.y, hi.x - lo.x, hi.y - lo.y),
                em,
                1.0,
            );
        }
    }
}

/// Stage point (y down) in `rect` → ndc (y up), and the rect's aspect.
pub fn to_ndc(at: Vec2, rect: Rect) -> (Vec2, f32) {
    let ndc = vec2(
        (at.x - rect.x) / rect.w * 2.0 - 1.0,
        1.0 - (at.y - rect.y) / rect.h * 2.0,
    );
    (ndc, rect.w / rect.h)
}

/// Ndc (y up) → stage point in `rect`.
pub fn from_ndc(ndc: Vec2, rect: Rect) -> Vec2 {
    vec2(
        rect.x + (ndc.x + 1.0) * 0.5 * rect.w,
        rect.y + (1.0 - ndc.y) * 0.5 * rect.h,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ndc_and_stage_are_inverse_inside_a_rect() {
        let rect = Rect::new(40.0, 60.0, 880.0, 520.0);
        for at in [vec2(40.0, 60.0), vec2(480.0, 320.0), vec2(900.0, 570.0)] {
            let (ndc, aspect) = to_ndc(at, rect);
            assert!((aspect - 880.0 / 520.0).abs() < 1e-6);
            assert!((from_ndc(ndc, rect) - at).length() < 1e-3, "{at}");
        }
        assert_eq!(to_ndc(vec2(40.0, 60.0), rect).0, vec2(-1.0, 1.0));
    }
}
