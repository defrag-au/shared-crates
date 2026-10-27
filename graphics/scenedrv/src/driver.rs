use sceneanim::{Channel, Clip, Ease, Interp, Key, Track, Wrap};
use scenegraph::{Kind, Node, Scene, SurfaceBind, Vec3};

use crate::error::Error;
use crate::op::{Fan, Op, Orbit, To};

#[derive(Clone, Debug, PartialEq)]
pub struct Driver {
    pub name: String,
    pub wrap: Wrap,
    pub ops: Vec<Op>,
}

impl Driver {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            wrap: Wrap::Once,
            ops: Vec::new(),
        }
    }

    pub fn wrap(mut self, wrap: Wrap) -> Self {
        self.wrap = wrap;
        self
    }

    pub fn op(mut self, op: impl Into<Op>) -> Self {
        self.ops.push(op.into());
        self
    }

    /// Compile to a clip over `scene` rest. Does not write the scene.
    pub fn clip(&self, scene: &Scene) -> Result<Clip, Error> {
        if self.ops.is_empty() {
            return Err(Error::EmptyDriver);
        }
        let mut channels = Vec::new();
        let mut fan_open: Vec<(String, Vec3)> = Vec::new();
        for op in &self.ops {
            match op {
                Op::Fan(fan) => compile_fan(scene, fan, &mut channels, &mut fan_open)?,
                Op::Orbit(orbit) => compile_orbit(scene, orbit, &fan_open, &mut channels)?,
                Op::To(to) => compile_to(scene, to, &mut channels)?,
            }
        }
        if channels.is_empty() {
            return Err(Error::EmptyDriver);
        }
        Ok(Clip {
            name: self.name.clone(),
            wrap: self.wrap,
            length: None,
            channels,
        })
    }
}

impl From<Fan> for Op {
    fn from(f: Fan) -> Self {
        Op::Fan(f)
    }
}

impl From<Orbit> for Op {
    fn from(o: Orbit) -> Self {
        Op::Orbit(o)
    }
}

impl From<To> for Op {
    fn from(t: To) -> Self {
        Op::To(t)
    }
}

fn compile_fan(
    scene: &Scene,
    fan: &Fan,
    channels: &mut Vec<Channel>,
    fan_open: &mut Vec<(String, Vec3)>,
) -> Result<(), Error> {
    let mut nodes: Vec<_> = scene.tagged(&fan.tag).collect();
    if nodes.is_empty() {
        return Err(Error::EmptyTag(fan.tag.clone()));
    }
    nodes.sort_by(|a, b| {
        a.xf.t
            .z
            .total_cmp(&b.xf.t.z)
            .then_with(|| a.name.cmp(&b.name))
    });
    let cam_z = if let Some(name) = fan.fit.as_deref() {
        match scene.get(name).map(|n| &n.kind) {
            Some(Kind::Camera { eye, .. }) => Some(eye.length().max(1.0)),
            Some(_) => return Err(Error::NotCamera(name.to_string())),
            None => return Err(Error::UnknownCamera(name.to_string())),
        }
    } else {
        None
    };
    fan_open.clear();
    for (i, n) in nodes.iter().enumerate() {
        let rest = n.xf.t;
        let open = Vec3::new(
            rest.x + i as f32 * fan.spread.x,
            rest.y + i as f32 * fan.spread.y,
            rest.z + i as f32 * fan.spread.z,
        );
        fan_open.push((n.name.clone(), open));
        channels.push(chan(
            &n.name,
            Track::Translate,
            hold_keys(fan.open, rest, open),
            Ease::Linear,
        ));
        if let Some(c) = cam_z {
            let s0 = Vec3::ONE;
            let s1 = fit_scale(c, open.z);
            channels.push(chan(
                &n.name,
                Track::Scale,
                hold_keys(fan.open, s0, s1),
                Ease::Linear,
            ));
        }
    }
    Ok(())
}

fn compile_orbit(
    scene: &Scene,
    orbit: &Orbit,
    fan_open: &[(String, Vec3)],
    channels: &mut Vec<Channel>,
) -> Result<(), Error> {
    let node = scene
        .get(&orbit.cam)
        .ok_or_else(|| Error::UnknownCamera(orbit.cam.clone()))?;
    let (rest_eye, rest_at) = match &node.kind {
        Kind::Camera { eye, at, .. } => (*eye, *at),
        _ => return Err(Error::NotCamera(orbit.cam.clone())),
    };
    let at_open = if fan_open.is_empty() {
        rest_at
    } else {
        let n = fan_open.len() as f32;
        let mut s = Vec3::ZERO;
        for (_, p) in fan_open {
            s.x += p.x;
            s.y += p.y;
            s.z += p.z;
        }
        Vec3::new(s.x / n, s.y / n, s.z / n)
    };
    let dist = orbit
        .dist
        .unwrap_or_else(|| (rest_eye - rest_at).length().max(1.0));
    let rest_dir = (rest_eye - rest_at).normalized();
    let rest_dir = if rest_dir.length() < 1e-5 {
        Vec3::new(0.0, 0.0, 1.0)
    } else {
        rest_dir
    };
    let eye_open = Vec3::new(
        at_open.x + rest_dir.x * dist,
        at_open.y + rest_dir.y * dist,
        at_open.z + rest_dir.z * dist,
    );
    let (sa, ca) = (orbit.az.sin(), orbit.az.cos());
    let (se, ce) = (orbit.el.sin(), orbit.el.cos());
    let eye_show = Vec3::new(
        at_open.x + dist * sa * ce,
        at_open.y + dist * se,
        at_open.z + dist * ca * ce,
    );
    let mut eye_keys = hold_keys(orbit.from, rest_eye, eye_open);
    if let Some(last) = eye_keys.last_mut() {
        last.value = eye_show;
    }
    channels.push(chan(&orbit.cam, Track::Eye, eye_keys, Ease::Linear));
    channels.push(chan(
        &orbit.cam,
        Track::At,
        hold_keys(orbit.from, rest_at, at_open),
        Ease::Linear,
    ));
    Ok(())
}

fn compile_to(scene: &Scene, to: &To, channels: &mut Vec<Channel>) -> Result<(), Error> {
    let node = scene.get(&to.node);
    if to.t.is_some() || to.r.is_some() || to.s.is_some() {
        let node = node.ok_or_else(|| Error::UnknownNode(to.node.clone()))?;
        if let Some(dest) = to.t {
            channels.push(chan(
                &node.name,
                Track::Translate,
                travel_keys(to.open, node.xf.t, dest),
                to.ease,
            ));
        }
        if let Some(dest) = to.r {
            channels.push(chan(
                &node.name,
                Track::Rotate,
                travel_keys(to.open, node.xf.r, dest),
                to.ease,
            ));
        }
        if let Some(dest) = to.s {
            channels.push(chan(
                &node.name,
                Track::Scale,
                travel_keys(to.open, node.xf.s, dest),
                to.ease,
            ));
        }
    }
    if let Some(dest) = to.a {
        let rest = node.map(node_alpha).unwrap_or(0.0);
        channels.push(chan(
            &to.node,
            Track::Alpha,
            travel_keys(
                to.open,
                Vec3::new(rest, 0.0, 0.0),
                Vec3::new(dest, 0.0, 0.0),
            ),
            to.ease,
        ));
    }
    Ok(())
}

fn node_alpha(n: &Node) -> f32 {
    match n.kind.surface_bind() {
        Some(SurfaceBind::Inline(s)) => s.color[3] as f32 / 255.0,
        _ => 0.0,
    }
}

fn travel_keys(open: f32, a: Vec3, b: Vec3) -> Vec<Key> {
    let t1 = if open < 1e-4 { 1.0 } else { open };
    vec![
        Key {
            time: 0.0,
            value: a,
        },
        Key { time: t1, value: b },
    ]
}

fn hold_keys(open: f32, a: Vec3, b: Vec3) -> Vec<Key> {
    if (open - 1.0).abs() < 1e-4 {
        vec![
            Key {
                time: 0.0,
                value: a,
            },
            Key {
                time: 1.0,
                value: b,
            },
        ]
    } else {
        vec![
            Key {
                time: 0.0,
                value: a,
            },
            Key {
                time: open,
                value: b,
            },
            Key {
                time: 1.0,
                value: b,
            },
        ]
    }
}

fn chan(node: &str, track: Track, keys: Vec<Key>, ease: Ease) -> Channel {
    Channel {
        node: node.to_string(),
        track,
        interp: Interp::Linear,
        ease,
        keys,
    }
}

fn fit_scale(cam_z: f32, z_world: f32) -> Vec3 {
    let s = ((cam_z - z_world) / cam_z).clamp(0.1, 2.0);
    Vec3::new(s, s, 1.0)
}
