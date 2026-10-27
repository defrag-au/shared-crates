use crate::kind::{Kind, Lamp, Prim};
use crate::material::{Shade, SurfaceBind};
use crate::math::Vec3;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Transform {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub t: Vec3,
    /// Euler XYZ in **radians**.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub r: Vec3,
    #[serde(default = "one_scale", skip_serializing_if = "is_one")]
    pub s: Vec3,
}

fn is_zero(v: &Vec3) -> bool {
    *v == Vec3::ZERO
}
fn is_one(v: &Vec3) -> bool {
    *v == Vec3::ONE
}
fn one_scale() -> Vec3 {
    Vec3::ONE
}

impl Transform {
    pub const IDENTITY: Self = Self {
        t: Vec3::ZERO,
        r: Vec3::ZERO,
        s: Vec3::ONE,
    };

    pub fn is_identity(&self) -> bool {
        self.t == Vec3::ZERO && self.r == Vec3::ZERO && self.s == Vec3::ONE
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Node {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Transform::is_identity")]
    pub xf: Transform,
    pub kind: Kind,
}

impl Node {
    pub fn group(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            parent: None,
            tags: Vec::new(),
            xf: Transform::IDENTITY,
            kind: Kind::Group,
        }
    }

    pub fn quad(name: impl Into<String>, w: f32, h: f32) -> Self {
        Self {
            name: name.into(),
            parent: None,
            tags: Vec::new(),
            xf: Transform::IDENTITY,
            kind: Kind::quad(w, h),
        }
    }

    pub fn camera(name: impl Into<String>, eye: Vec3, at: Vec3) -> Self {
        Self {
            name: name.into(),
            parent: None,
            tags: Vec::new(),
            xf: Transform::IDENTITY,
            kind: Kind::camera(eye, at),
        }
    }

    pub fn light(name: impl Into<String>, dir: Vec3) -> Self {
        Self {
            name: name.into(),
            parent: None,
            tags: Vec::new(),
            xf: Transform::IDENTITY,
            kind: Kind::light(dir),
        }
    }

    pub fn point(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            parent: None,
            tags: Vec::new(),
            xf: Transform::IDENTITY,
            kind: Kind::point(),
        }
    }

    pub fn mesh(name: impl Into<String>, prim: Prim) -> Self {
        Self {
            name: name.into(),
            parent: None,
            tags: Vec::new(),
            xf: Transform::IDENTITY,
            kind: Kind::mesh(prim),
        }
    }

    pub fn geom(name: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            parent: None,
            tags: Vec::new(),
            xf: Transform::IDENTITY,
            kind: Kind::geom(key),
        }
    }

    pub fn label(
        name: impl Into<String>,
        font: impl Into<String>,
        size: f32,
        text: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            parent: None,
            tags: Vec::new(),
            xf: Transform::IDENTITY,
            kind: Kind::label(font, size, text),
        }
    }

    pub fn slab(name: impl Into<String>, w: f32, h: f32) -> Self {
        Self {
            name: name.into(),
            parent: None,
            tags: Vec::new(),
            xf: Transform::IDENTITY,
            kind: Kind::slab(w, h),
        }
    }

    pub fn parent(mut self, p: impl Into<String>) -> Self {
        self.parent = Some(p.into());
        self
    }

    pub fn tag(mut self, t: impl Into<String>) -> Self {
        self.tags.push(t.into());
        self
    }

    pub fn at(mut self, t: Vec3) -> Self {
        self.xf.t = t;
        self
    }

    pub fn rot(mut self, r: Vec3) -> Self {
        self.xf.r = r;
        self
    }

    pub fn image(mut self, key: impl Into<String>) -> Self {
        if let Some(s) = self.kind.inline_surface_mut() {
            s.image = Some(key.into());
        }
        self
    }

    pub fn image_back(mut self, key: impl Into<String>) -> Self {
        if let Some(s) = self.kind.inline_surface_mut() {
            s.image_back = Some(key.into());
        }
        self
    }

    pub fn color(mut self, rgba: [u8; 4]) -> Self {
        if let Kind::Light { color, .. } = &mut self.kind {
            *color = rgba;
            return self;
        }
        if let Some(s) = self.kind.inline_surface_mut() {
            s.color = rgba;
        }
        self
    }

    pub fn shade(mut self, shade: Shade) -> Self {
        if let Some(s) = self.kind.inline_surface_mut() {
            s.shade = shade;
        }
        self
    }

    pub fn mat(mut self, key: impl Into<String>) -> Self {
        if let Some(bind) = self.kind.surface_bind_mut() {
            *bind = SurfaceBind::Name(key.into());
        }
        self
    }

    pub fn scale(mut self, s: Vec3) -> Self {
        self.xf.s = s;
        self
    }

    /// Slab corner radius. `0` is a sharp rectangle (asset prints).
    pub fn radius(mut self, r: f32) -> Self {
        if let Kind::Mesh {
            prim: Prim::Slab { radius, .. },
            ..
        } = &mut self.kind
        {
            *radius = r.max(0.0);
        }
        self
    }

    pub fn range(mut self, r: f32) -> Self {
        if let Kind::Light {
            lamp: Lamp::Point { range },
            ..
        } = &mut self.kind
        {
            *range = r;
        }
        self
    }

    pub fn depth(mut self, d: f32) -> Self {
        if let Kind::Mesh {
            prim: Prim::Slab { depth, .. },
            ..
        } = &mut self.kind
        {
            *depth = d;
        }
        self
    }
}
