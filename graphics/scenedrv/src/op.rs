use sceneanim::Ease;
use scenegraph::Vec3;

use crate::error::Error;

pub const OPEN: f32 = 0.4;
pub const SPREAD_X: f32 = 0.12;
pub const SPREAD_Y: f32 = 0.08;
pub const SPREAD_Z: f32 = 0.55;
pub const AZ: f32 = 0.85;
pub const EL: f32 = 0.32;
/// Inspect dest in the hand parent frame. World ≈ camera `at`, closer +Z.
/// hand rest `t=0,-2.12,0.16` + this → `(0,0.2,2.36)`.
pub const LIFT_T: Vec3 = Vec3 {
    x: 0.0,
    y: 2.32,
    z: 2.2,
};
pub const LIFT_S: Vec3 = Vec3 {
    x: 1.6,
    y: 1.6,
    z: 1.0,
};
pub const LIFT_OPEN: f32 = 0.7;
pub const FADE_OPEN: f32 = 0.22;

#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    Fan(Fan),
    Orbit(Orbit),
    To(To),
}

impl Op {
    pub fn as_str(&self) -> &'static str {
        match self {
            Op::Fan(_) => "fan",
            Op::Orbit(_) => "orbit",
            Op::To(_) => "to",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Fan {
    pub tag: String,
    pub open: f32,
    pub spread: Vec3,
    pub fit: Option<String>,
}

impl Fan {
    pub fn tag(tag: impl Into<String>) -> Self {
        Self {
            tag: tag.into(),
            open: OPEN,
            spread: Vec3::new(SPREAD_X, SPREAD_Y, SPREAD_Z),
            fit: None,
        }
    }

    pub fn fit(mut self, cam: impl Into<String>) -> Self {
        self.fit = Some(cam.into());
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Orbit {
    pub cam: String,
    pub from: f32,
    pub az: f32,
    pub el: f32,
    pub dist: Option<f32>,
}

impl Orbit {
    pub fn cam(cam: impl Into<String>) -> Self {
        Self {
            cam: cam.into(),
            from: OPEN,
            az: AZ,
            el: EL,
            dist: None,
        }
    }
}

/// Move one named node from rest `t`/`r`/`s` to a dest. Compile is a clip;
/// it does not insert nodes.
#[derive(Clone, Debug, PartialEq)]
pub struct To {
    pub node: String,
    pub open: f32,
    pub t: Option<Vec3>,
    pub r: Option<Vec3>,
    pub s: Option<Vec3>,
    pub a: Option<f32>,
    pub ease: Ease,
}

impl To {
    pub fn node(node: impl Into<String>) -> Self {
        Self {
            node: node.into(),
            open: OPEN,
            t: None,
            r: None,
            s: None,
            a: None,
            ease: Ease::Linear,
        }
    }

    /// Highlight pose: dest `t`/`s`, flatten `r`, full travel, in-out cubic.
    pub fn lift(node: impl Into<String>) -> Self {
        Self::node(node)
            .open(LIFT_OPEN)
            .translate(LIFT_T)
            .rotate(Vec3::ZERO)
            .scale(LIFT_S)
            .ease(Ease::InOutCubic)
    }

    /// Fade a Label (or other inline surface) in. Dest alpha 1.
    pub fn fade(node: impl Into<String>) -> Self {
        Self::node(node)
            .open(FADE_OPEN)
            .alpha(1.0)
            .ease(Ease::InOutCubic)
    }

    pub fn open(mut self, open: f32) -> Self {
        self.open = open;
        self
    }

    pub fn translate(mut self, t: Vec3) -> Self {
        self.t = Some(t);
        self
    }

    pub fn rotate(mut self, r: Vec3) -> Self {
        self.r = Some(r);
        self
    }

    pub fn scale(mut self, s: Vec3) -> Self {
        self.s = Some(s);
        self
    }

    pub fn alpha(mut self, a: f32) -> Self {
        self.a = Some(a);
        self
    }

    pub fn ease(mut self, ease: Ease) -> Self {
        self.ease = ease;
        self
    }
}

pub fn parse_op(line: &str) -> Result<Op, Error> {
    let mut parts = line.split_whitespace();
    let kind = parts
        .next()
        .ok_or_else(|| Error::Parse("empty op".into()))?;
    match kind {
        "fan" => parse_fan(parts).map(Op::Fan),
        "orbit" => parse_orbit(parts).map(Op::Orbit),
        "to" => parse_to(parts).map(Op::To),
        other => Err(Error::UnknownOp(other.to_string())),
    }
}

fn parse_fan<'a>(parts: impl Iterator<Item = &'a str>) -> Result<Fan, Error> {
    let mut fan = Fan::tag("");
    let mut have_tag = false;
    let mut have_spread = false;
    for tok in parts {
        let (k, v) = tok
            .split_once('=')
            .ok_or_else(|| Error::Parse(format!("bad token '{tok}'")))?;
        match k {
            "tag" => {
                fan.tag = v.to_string();
                have_tag = true;
            }
            "open" => fan.open = parse_f(v)?,
            "x" => {
                fan.spread.x = parse_f(v)?;
                have_spread = true;
            }
            "y" => {
                fan.spread.y = parse_f(v)?;
                have_spread = true;
            }
            "z" => {
                fan.spread.z = parse_f(v)?;
                have_spread = true;
            }
            "fit" => fan.fit = Some(v.to_string()),
            other => return Err(Error::UnknownKey(other.to_string())),
        }
    }
    if !have_tag || fan.tag.is_empty() {
        return Err(Error::FanNeedsTag);
    }
    if !have_spread {
        fan.spread = Vec3::new(SPREAD_X, SPREAD_Y, SPREAD_Z);
    }
    Ok(fan)
}

fn parse_orbit<'a>(parts: impl Iterator<Item = &'a str>) -> Result<Orbit, Error> {
    let mut orbit = Orbit::cam("");
    let mut have_cam = false;
    for tok in parts {
        let (k, v) = tok
            .split_once('=')
            .ok_or_else(|| Error::Parse(format!("bad token '{tok}'")))?;
        match k {
            "cam" => {
                orbit.cam = v.to_string();
                have_cam = true;
            }
            "from" => orbit.from = parse_f(v)?,
            "az" => orbit.az = parse_f(v)?,
            "el" => orbit.el = parse_f(v)?,
            "dist" => orbit.dist = Some(parse_f(v)?),
            other => return Err(Error::UnknownKey(other.to_string())),
        }
    }
    if !have_cam || orbit.cam.is_empty() {
        return Err(Error::OrbitNeedsCam);
    }
    Ok(orbit)
}

fn parse_to<'a>(parts: impl Iterator<Item = &'a str>) -> Result<To, Error> {
    let mut to = To::node("");
    let mut have_node = false;
    for tok in parts {
        let (k, v) = tok
            .split_once('=')
            .ok_or_else(|| Error::Parse(format!("bad token '{tok}'")))?;
        match k {
            "node" => {
                to.node = v.to_string();
                have_node = true;
            }
            "open" => to.open = parse_f(v)?,
            "t" => to.t = Some(parse_vec3(v)?),
            "r" => to.r = Some(parse_vec3(v)?),
            "s" => to.s = Some(parse_vec3(v)?),
            "a" => to.a = Some(parse_f(v)?),
            "ease" => to.ease = Ease::parse(v)?,
            other => return Err(Error::UnknownKey(other.to_string())),
        }
    }
    if !have_node || to.node.is_empty() {
        return Err(Error::ToNeedsNode);
    }
    if to.t.is_none() && to.r.is_none() && to.s.is_none() && to.a.is_none() {
        return Err(Error::ToNeedsDest);
    }
    Ok(to)
}

fn parse_vec3(s: &str) -> Result<Vec3, Error> {
    let mut it = s.split(',');
    let x = parse_f(it.next().ok_or_else(|| Error::Parse("vec3 x".into()))?)?;
    let y = parse_f(it.next().ok_or_else(|| Error::Parse("vec3 y".into()))?)?;
    let z = parse_f(it.next().ok_or_else(|| Error::Parse("vec3 z".into()))?)?;
    if it.next().is_some() {
        return Err(Error::Parse("vec3 extra".into()));
    }
    Ok(Vec3::new(x, y, z))
}

fn parse_f(s: &str) -> Result<f32, Error> {
    s.parse()
        .map_err(|_| Error::Parse(format!("bad float '{s}'")))
}
