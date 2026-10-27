use crate::material::{Surface, SurfaceBind};
use crate::math::Vec3;

/// A model's own clip at a time: `pose=CLIP@SECONDS` on a `Mesh Geom`. The
/// time wraps by the clip's length, so a clock or an `an1` `pose` track can
/// run it without knowing the length. A clip the model does not have is a
/// flatten error.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Pose {
    pub clip: String,
    pub t: f32,
}

impl Pose {
    pub fn new(clip: impl Into<String>, t: f32) -> Self {
        Self {
            clip: clip.into(),
            t,
        }
    }
}

/// A model's facing, as one of its local horizontal axes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Front {
    PlusX,
    MinusX,
    PlusZ,
    MinusZ,
}

impl Front {
    pub fn as_str(self) -> &'static str {
        match self {
            Front::PlusX => "+x",
            Front::MinusX => "-x",
            Front::PlusZ => "+z",
            Front::MinusZ => "-z",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "+x" => Front::PlusX,
            "-x" => Front::MinusX,
            "+z" => Front::PlusZ,
            "-z" => Front::MinusZ,
            _ => return None,
        })
    }

    /// The axis in the node's own frame.
    pub fn local(self) -> Vec3 {
        match self {
            Front::PlusX => Vec3::new(1.0, 0.0, 0.0),
            Front::MinusX => Vec3::new(-1.0, 0.0, 0.0),
            Front::PlusZ => Vec3::new(0.0, 0.0, 1.0),
            Front::MinusZ => Vec3::new(0.0, 0.0, -1.0),
        }
    }
}

/// Closed inventory. A new variant is engine work.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Kind {
    Group,
    /// Local XY plane, origin centre, facing +Z. `w`×`h` in local units.
    Quad {
        w: f32,
        h: f32,
        #[serde(default)]
        bind: SurfaceBind,
    },
    Camera {
        eye: Vec3,
        at: Vec3,
        #[serde(default = "up_y")]
        up: Vec3,
        #[serde(default = "default_fov")]
        fov: f32,
    },
    Light {
        lamp: Lamp,
        #[serde(default = "white")]
        color: [u8; 4],
        #[serde(default = "one")]
        intensity: f32,
    },
    /// Closed primitives. Tessellated at flatten time, or looked up in [`crate::Geometry`].
    Mesh {
        prim: Prim,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        geom: Option<String>,
        #[serde(default)]
        bind: SurfaceBind,
        /// A rigged model posed by one of its own clips (`pose=Run@0.4`).
        /// `Geom` only; `None` draws the model as authored.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pose: Option<Pose>,
        /// Which local axis the model faces (`front=+z`): glTF says +Z,
        /// but files disagree, and nothing in a mesh says which end is its
        /// face. Declared, the gauntlet checks travel against it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        front: Option<Front>,
    },
    /// XY plane, origin at layout bbox centre, facing +Z. Glyphs from [`crate::Fonts`].
    Label {
        font: String,
        /// Em height in world units.
        size: f32,
        text: String,
        #[serde(default)]
        bind: SurfaceBind,
    },
}

/// Closed mesh inventory. A new variant is engine work.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Prim {
    /// Unit cube, origin centre, extents ±0.5.
    Box,
    /// Unit XY plane, origin centre, facing +Z (same pose as a 1×1 Quad).
    Plane,
    /// Named triangle blob in [`crate::Geometry`]. Dump `g=`.
    Geom,
    /// Unit UV sphere, origin centre, radius 0.5 (same extents as [`Prim::Box`]).
    /// Scale the node for ellipsoids. Tessellation is closed (not in the dump).
    Sphere,
    /// Rounded stock: XY face `w`×`h`, thickness along +Z, corner radius.
    /// Card-like fixtures use this, not a zero-width [`Kind::Quad`].
    Slab {
        w: f32,
        h: f32,
        depth: f32,
        radius: f32,
    },
}

/// Closed light inventory. Position for [`Lamp::Point`] is the node transform.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Lamp {
    Dir { dir: Vec3 },
    Point { range: f32 },
}

impl Lamp {
    pub fn as_str(self) -> &'static str {
        LampOf::from(self).as_str()
    }
}

/// Discriminant for the dump `Dir|Point` token on a Light.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LampOf {
    Dir,
    Point,
}

impl LampOf {
    pub fn as_str(self) -> &'static str {
        match self {
            LampOf::Dir => "Dir",
            LampOf::Point => "Point",
        }
    }

    pub fn parse(token: &str) -> Result<Self, crate::error::Error> {
        match token {
            "Dir" => Ok(LampOf::Dir),
            "Point" => Ok(LampOf::Point),
            other => Err(crate::error::Error::UnknownLamp(other.to_string())),
        }
    }
}

impl From<Lamp> for LampOf {
    fn from(lamp: Lamp) -> Self {
        match lamp {
            Lamp::Dir { .. } => LampOf::Dir,
            Lamp::Point { .. } => LampOf::Point,
        }
    }
}

impl Prim {
    pub fn as_str(self) -> &'static str {
        match self {
            Prim::Box => "Box",
            Prim::Plane => "Plane",
            Prim::Geom => "Geom",
            Prim::Sphere => "Sphere",
            Prim::Slab { .. } => "Slab",
        }
    }

    pub fn parse(token: &str) -> Result<Self, crate::error::Error> {
        match token {
            "Box" => Ok(Prim::Box),
            "Plane" => Ok(Prim::Plane),
            "Geom" => Ok(Prim::Geom),
            "Sphere" => Ok(Prim::Sphere),
            "Slab" => Ok(Prim::Slab {
                w: 0.0,
                h: 0.0,
                depth: DEFAULT_SLAB_DEPTH,
                radius: DEFAULT_SLAB_RADIUS,
            }),
            other => Err(crate::error::Error::UnknownPrim(other.to_string())),
        }
    }
}

pub const DEFAULT_FOV: f32 = std::f32::consts::FRAC_PI_3;
pub const DEFAULT_RANGE: f32 = 6.0;
pub const DEFAULT_SLAB_DEPTH: f32 = 0.05;
pub const DEFAULT_SLAB_RADIUS: f32 = 0.12;
pub const MAX_DIR_LIGHTS: usize = 4;
pub const MAX_POINT_LIGHTS: usize = 4;

fn default_fov() -> f32 {
    DEFAULT_FOV
}
fn up_y() -> Vec3 {
    Vec3::Y
}
fn white() -> [u8; 4] {
    [255, 255, 255, 255]
}
fn one() -> f32 {
    1.0
}

impl Kind {
    pub fn name(&self) -> &'static str {
        self.of().as_str()
    }

    pub fn quad(w: f32, h: f32) -> Self {
        Kind::Quad {
            w,
            h,
            bind: SurfaceBind::Inline(Surface::DEFAULT),
        }
    }

    pub fn camera(eye: Vec3, at: Vec3) -> Self {
        Kind::Camera {
            eye,
            at,
            up: Vec3::Y,
            fov: DEFAULT_FOV,
        }
    }

    pub fn light(dir: Vec3) -> Self {
        Kind::Light {
            lamp: Lamp::Dir { dir },
            color: white(),
            intensity: 1.0,
        }
    }

    pub fn point() -> Self {
        Kind::Light {
            lamp: Lamp::Point {
                range: DEFAULT_RANGE,
            },
            color: white(),
            intensity: 1.0,
        }
    }

    pub fn mesh(prim: Prim) -> Self {
        Kind::Mesh {
            prim,
            geom: None,
            bind: SurfaceBind::Inline(Surface::DEFAULT),
            pose: None,
            front: None,
        }
    }

    pub fn geom(key: impl Into<String>) -> Self {
        Kind::Mesh {
            prim: Prim::Geom,
            geom: Some(key.into()),
            bind: SurfaceBind::Inline(Surface::DEFAULT),
            pose: None,
            front: None,
        }
    }

    /// The pose of a `Mesh Geom`, to write (`an1`'s `pose` track).
    pub fn pose_mut(&mut self) -> Option<&mut Option<Pose>> {
        match self {
            Kind::Mesh {
                prim: Prim::Geom,
                pose,
                ..
            } => Some(pose),
            _ => None,
        }
    }

    pub fn label(font: impl Into<String>, size: f32, text: impl Into<String>) -> Self {
        Kind::Label {
            font: font.into(),
            size,
            text: text.into(),
            bind: SurfaceBind::Inline(Surface {
                shade: crate::material::Shade::Unlit,
                color: [255, 255, 255, 255],
                image: None,
                image_back: None,
            }),
        }
    }

    pub fn slab(w: f32, h: f32) -> Self {
        Kind::Mesh {
            prim: Prim::Slab {
                w,
                h,
                depth: DEFAULT_SLAB_DEPTH,
                radius: DEFAULT_SLAB_RADIUS,
            },
            geom: None,
            bind: SurfaceBind::Inline(Surface::DEFAULT),
            pose: None,
            front: None,
        }
    }

    pub fn surface_bind(&self) -> Option<&SurfaceBind> {
        match self {
            Kind::Quad { bind, .. } | Kind::Mesh { bind, .. } | Kind::Label { bind, .. } => {
                Some(bind)
            }
            _ => None,
        }
    }

    pub fn surface_bind_mut(&mut self) -> Option<&mut SurfaceBind> {
        match self {
            Kind::Quad { bind, .. } | Kind::Mesh { bind, .. } | Kind::Label { bind, .. } => {
                Some(bind)
            }
            _ => None,
        }
    }

    pub fn inline_surface_mut(&mut self) -> Option<&mut Surface> {
        match self.surface_bind_mut()? {
            SurfaceBind::Inline(s) => Some(s),
            SurfaceBind::Name(_) => None,
        }
    }
}

/// Discriminant for [`Scene::kind`] queries and the dump `KIND` token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KindOf {
    Group,
    Quad,
    Camera,
    Light,
    Mesh,
    Label,
}

impl KindOf {
    pub fn as_str(self) -> &'static str {
        match self {
            KindOf::Group => "Group",
            KindOf::Quad => "Quad",
            KindOf::Camera => "Camera",
            KindOf::Light => "Light",
            KindOf::Mesh => "Mesh",
            KindOf::Label => "Label",
        }
    }

    pub fn parse(token: &str) -> Result<Self, crate::error::Error> {
        match token {
            "Group" => Ok(KindOf::Group),
            "Quad" => Ok(KindOf::Quad),
            "Camera" => Ok(KindOf::Camera),
            "Light" => Ok(KindOf::Light),
            "Mesh" => Ok(KindOf::Mesh),
            "Label" => Ok(KindOf::Label),
            other => Err(crate::error::Error::UnknownKind(other.to_string())),
        }
    }
}

impl Kind {
    pub fn of(&self) -> KindOf {
        match self {
            Kind::Group => KindOf::Group,
            Kind::Quad { .. } => KindOf::Quad,
            Kind::Camera { .. } => KindOf::Camera,
            Kind::Light { .. } => KindOf::Light,
            Kind::Mesh { .. } => KindOf::Mesh,
            Kind::Label { .. } => KindOf::Label,
        }
    }
}
