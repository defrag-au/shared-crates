use scenegraph::{
    mat4_inverse, transform_point, unproject, DrawList, Hit, Images, Kind, Scene, Vec3,
};

use crate::error::Error;
use crate::session::{Grab, Sel, Session};
use crate::tool::{Event, Tool};

const YAW_PER_NDC: f32 = 1.5;
const PITCH_PER_NDC: f32 = 1.5;
const PITCH_LIMIT: f32 = 1.35;
const DIST_MIN: f32 = 0.5;
const DIST_MAX: f32 = 40.0;
const WALK_PER_WHEEL: f32 = 8.0;
const WALL_PAD: f32 = 0.45;

/// Apply `event` with `session.tool`. `list` is what the pointer sees (posed);
/// `scene` is rest pose. Pick writes the session only; orbit/drag clone and
/// write the rest scene.
pub fn step(
    scene: &Scene,
    list: &DrawList,
    session: &Session,
    event: Event,
) -> Result<(Scene, Session, bool), Error> {
    step_with(scene, list, session, event, &Images::new())
}

/// Like [`step`], but pick/drag sample interned images. Transparent texels
/// (≤ [`scenegraph::ALPHA_CUTOFF`]) do not occupy the ray.
pub fn step_with(
    scene: &Scene,
    list: &DrawList,
    session: &Session,
    event: Event,
    images: &Images,
) -> Result<(Scene, Session, bool), Error> {
    let mut session = session.clone();
    let was_dragging = session.dragging;
    let had_grab = session.grab.is_some();
    let dndc = [
        event.ndc()[0] - session.ndc[0],
        event.ndc()[1] - session.ndc[1],
    ];
    session.aspect = event.aspect();
    let mut out = scene.clone();

    match event {
        Event::Down { ndc, aspect } => {
            session.dragging = true;
            match session.tool {
                Tool::Pick => pick(&mut session, &out, list, ndc, aspect, images),
                Tool::Orbit | Tool::Walk => {}
                Tool::Drag => drag_down(&mut session, &out, list, ndc, aspect, images),
            }
        }
        Event::Move { ndc, aspect } => match session.tool {
            Tool::Pick => {}
            Tool::Orbit if session.dragging => orbit(&mut out, dndc, 0.0),
            Tool::Orbit => {}
            Tool::Walk if session.dragging => walk_look(&mut out, dndc),
            Tool::Walk => {}
            Tool::Drag if session.dragging => {
                drag_move(&mut out, &session, list, ndc, aspect);
            }
            Tool::Drag => {}
        },
        Event::Up { .. } => {
            session.dragging = false;
            session.grab = None;
        }
        Event::Wheel { delta, .. } => match session.tool {
            Tool::Orbit => orbit(&mut out, [0.0, 0.0], delta),
            Tool::Walk => walk_stride(&mut out, list, -delta * WALK_PER_WHEEL, 0.0),
            Tool::Pick | Tool::Drag => {}
        },
    }

    session.ndc = event.ndc();
    let wrote = match event {
        Event::Move { .. } if session.tool == Tool::Orbit && was_dragging => true,
        Event::Move { .. } if session.tool == Tool::Walk && was_dragging => true,
        Event::Move { .. } if session.tool == Tool::Drag && had_grab => true,
        Event::Wheel { .. } if session.tool == Tool::Orbit || session.tool == Tool::Walk => true,
        _ => false,
    };
    Ok((out, session, wrote))
}

/// Translate the active camera in the look XZ plane. `forward` is along
/// flattened `(at-eye)`; `strafe` is to the right. Collision is `list.hit`.
pub fn stride(scene: &Scene, list: &DrawList, forward: f32, strafe: f32) -> Scene {
    let mut out = scene.clone();
    walk_stride(&mut out, list, forward, strafe);
    out
}

impl Session {
    pub fn step(
        &self,
        scene: &Scene,
        list: &DrawList,
        event: Event,
    ) -> Result<(Scene, Session, bool), Error> {
        step(scene, list, self, event)
    }

    pub fn step_with(
        &self,
        scene: &Scene,
        list: &DrawList,
        event: Event,
        images: &Images,
    ) -> Result<(Scene, Session, bool), Error> {
        step_with(scene, list, self, event, images)
    }
}

/// What a pick at `ndc` selects, without a session: the `Tool::Pick` rule
/// (a Label hit selects its parent). For a pane that picks while another
/// tool owns the pointer — a walk's crosshair picks at `[0, 0]`.
pub fn pick_ndc(
    scene: &Scene,
    list: &DrawList,
    ndc: [f32; 2],
    aspect: f32,
    images: &Images,
) -> Option<Sel> {
    list.hit_ndc_with(ndc[0], ndc[1], aspect, images)
        .map(|h| sel_from_hit(scene, h))
}

fn pick(
    session: &mut Session,
    scene: &Scene,
    list: &DrawList,
    ndc: [f32; 2],
    aspect: f32,
    images: &Images,
) {
    session.sel = pick_ndc(scene, list, ndc, aspect, images);
}

/// A Label is chrome on a control. The session selects the parent (card, button, pile).
fn pick_target(scene: &Scene, name: &str) -> String {
    match scene.get(name) {
        Some(n) if matches!(n.kind, Kind::Label { .. }) => n
            .parent
            .clone()
            .filter(|p| scene.get(p).is_some())
            .unwrap_or_else(|| name.to_string()),
        _ => name.to_string(),
    }
}

fn sel_from_hit(scene: &Scene, h: Hit<'_>) -> Sel {
    let mut sel = Sel::from_hit(h);
    sel.name = pick_target(scene, h.name);
    sel
}

fn drag_down(
    session: &mut Session,
    scene: &Scene,
    list: &DrawList,
    ndc: [f32; 2],
    aspect: f32,
    images: &Images,
) {
    let Some(hit) = list.hit_ndc_with(ndc[0], ndc[1], aspect, images) else {
        session.sel = None;
        session.grab = None;
        return;
    };
    let name = pick_target(scene, hit.name);
    if !draggable(scene, &name) {
        session.sel = Some(sel_from_hit(scene, hit));
        session.grab = None;
        session.dragging = false;
        return;
    }
    let rest_world = world_origin(scene, &name);
    let plane_normal = (list.camera.at - list.camera.eye).normalized();
    session.sel = Some(Sel {
        name: name.clone(),
        face: hit.face,
        uv: hit.uv,
        point: hit.point,
    });
    session.grab = Some(Grab {
        name,
        plane_origin: hit.point,
        plane_normal,
        rest_world,
    });
}

fn drag_move(scene: &mut Scene, session: &Session, list: &DrawList, ndc: [f32; 2], aspect: f32) {
    let Some(grab) = &session.grab else {
        return;
    };
    let Some(hit_pt) = ray_plane_ndc(list, ndc, aspect, grab.plane_origin, grab.plane_normal)
    else {
        return;
    };
    let delta = hit_pt - grab.plane_origin;
    let new_world = Vec3::new(
        grab.rest_world.x + delta.x,
        grab.rest_world.y + delta.y,
        grab.rest_world.z + delta.z,
    );
    let local = world_to_local(scene, grab.name.as_str(), new_world);
    if let Some(node) = scene.get_mut(&grab.name) {
        node.xf.t = local;
    }
}

fn draggable(scene: &Scene, name: &str) -> bool {
    match scene.get(name).map(|n| &n.kind) {
        Some(Kind::Camera { .. } | Kind::Light { .. }) => false,
        Some(_) => true,
        None => false,
    }
}

fn world_origin(scene: &Scene, name: &str) -> Vec3 {
    scene
        .world_matrix(name)
        .map(|m| transform_point(m, Vec3::ZERO))
        .unwrap_or(Vec3::ZERO)
}

fn world_to_local(scene: &Scene, name: &str, world: Vec3) -> Vec3 {
    let Some(node) = scene.get(name) else {
        return world;
    };
    let Some(parent) = node.parent.as_deref() else {
        return world;
    };
    let Ok(pw) = scene.world_matrix(parent) else {
        return world;
    };
    let Some(inv) = mat4_inverse(pw) else {
        return world;
    };
    transform_point(inv, world)
}

fn ray_plane_ndc(
    list: &DrawList,
    ndc: [f32; 2],
    aspect: f32,
    origin: Vec3,
    normal: Vec3,
) -> Option<Vec3> {
    let (orig, dir) = ndc_ray(list, ndc, aspect)?;
    ray_plane(orig, dir, origin, normal)
}

fn ndc_ray(list: &DrawList, ndc: [f32; 2], aspect: f32) -> Option<(Vec3, Vec3)> {
    if aspect <= 0.0 {
        return None;
    }
    let inv = mat4_inverse(list.view_proj(aspect))?;
    let far = unproject(inv, Vec3::new(ndc[0], ndc[1], 1.0))?;
    let dir = (far - list.camera.eye).normalized();
    if dir.length() < 1e-8 {
        return None;
    }
    Some((list.camera.eye, dir))
}

fn ray_plane(orig: Vec3, dir: Vec3, p0: Vec3, n: Vec3) -> Option<Vec3> {
    let d = dir.dot(n);
    if d.abs() < 1e-8 {
        return None;
    }
    let t = (p0 - orig).dot(n) / d;
    Some(Vec3::new(
        orig.x + dir.x * t,
        orig.y + dir.y * t,
        orig.z + dir.z * t,
    ))
}

fn orbit(scene: &mut Scene, dndc: [f32; 2], zoom: f32) {
    let Some(name) = scene.active_camera_name().map(str::to_string) else {
        return;
    };
    let Some(node) = scene.get_mut(&name) else {
        return;
    };
    let Kind::Camera { eye, at, .. } = &mut node.kind else {
        return;
    };
    let dx = eye.x - at.x;
    let dy = eye.y - at.y;
    let dz = eye.z - at.z;
    let dist = (dx * dx + dy * dy + dz * dz).sqrt().max(0.2);
    let mut yaw = dx.atan2(dz);
    let mut pitch = (dy / dist).clamp(-1.0, 1.0).asin();
    // Grab-the-world: ndc x+ (drag right) decreases yaw; ndc y+ (drag up)
    // decreases pitch because egui-down used to increase it.
    yaw -= dndc[0] * YAW_PER_NDC;
    pitch = (pitch - dndc[1] * PITCH_PER_NDC).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    let dist = (dist * (1.0 + zoom)).clamp(DIST_MIN, DIST_MAX);
    let cp = pitch.cos();
    *eye = Vec3::new(
        at.x + dist * yaw.sin() * cp,
        at.y + dist * pitch.sin(),
        at.z + dist * yaw.cos() * cp,
    );
}

fn walk_look(scene: &mut Scene, dndc: [f32; 2]) {
    let Some(name) = scene.active_camera_name().map(str::to_string) else {
        return;
    };
    let Some(node) = scene.get_mut(&name) else {
        return;
    };
    let Kind::Camera { eye, at, .. } = &mut node.kind else {
        return;
    };
    let dx = at.x - eye.x;
    let dy = at.y - eye.y;
    let dz = at.z - eye.z;
    let dist = (dx * dx + dy * dy + dz * dz).sqrt().max(0.2);
    let mut yaw = dx.atan2(dz);
    let mut pitch = (dy / dist).clamp(-1.0, 1.0).asin();
    // FPS: ndc x+ (mouse right) looks right; ndc y+ (mouse up) looks up.
    yaw -= dndc[0] * YAW_PER_NDC;
    pitch = (pitch + dndc[1] * PITCH_PER_NDC).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    let cp = pitch.cos();
    *at = Vec3::new(
        eye.x + dist * yaw.sin() * cp,
        eye.y + dist * pitch.sin(),
        eye.z + dist * yaw.cos() * cp,
    );
}

fn walk_stride(scene: &mut Scene, list: &DrawList, forward: f32, strafe: f32) {
    let Some(name) = scene.active_camera_name().map(str::to_string) else {
        return;
    };
    let Some(node) = scene.get_mut(&name) else {
        return;
    };
    let Kind::Camera { eye, at, .. } = &mut node.kind else {
        return;
    };
    let mut look = Vec3::new(at.x - eye.x, 0.0, at.z - eye.z);
    let len = look.length();
    if len < 1e-6 {
        look = Vec3::new(0.0, 0.0, -1.0);
    } else {
        look = look * (1.0 / len);
    }
    let right = look.cross(Vec3::Y);
    let mut desired = look * forward + right * strafe;
    let want = desired.length();
    if want < 1e-8 {
        return;
    }
    if let Some(hit) = list.hit(*eye, desired) {
        let dist = (hit.point - *eye).length();
        let allowed = (dist - WALL_PAD).max(0.0);
        if allowed < want {
            desired = desired * (allowed / want);
        }
    }
    eye.x += desired.x;
    eye.z += desired.z;
    at.x += desired.x;
    at.z += desired.z;
}
