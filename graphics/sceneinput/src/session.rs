use scenegraph::{Hit, HitFace, Vec3};

use crate::tool::Tool;

/// Last successful pick. Session state — not a node tag.
#[derive(Clone, Debug, PartialEq)]
pub struct Sel {
    pub name: String,
    pub face: HitFace,
    pub uv: [f32; 2],
    pub point: Vec3,
}

impl Sel {
    pub fn from_hit(h: Hit<'_>) -> Self {
        Self {
            name: h.name.to_string(),
            face: h.face,
            uv: h.uv,
            point: h.point,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Grab {
    pub name: String,
    pub plane_origin: Vec3,
    pub plane_normal: Vec3,
    pub rest_world: Vec3,
}

/// Ephemeral pointer session. Dump (`in1`) is the score; the rest scene is separate.
#[derive(Clone, Debug)]
pub struct Session {
    pub tool: Tool,
    pub ndc: [f32; 2],
    pub aspect: f32,
    pub sel: Option<Sel>,
    pub(crate) dragging: bool,
    pub(crate) grab: Option<Grab>,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    pub fn new() -> Self {
        Self {
            tool: Tool::Pick,
            ndc: [0.0, 0.0],
            aspect: 1.0,
            sel: None,
            dragging: false,
            grab: None,
        }
    }

    pub fn with_tool(mut self, tool: Tool) -> Self {
        self.set_tool(tool);
        self
    }

    pub fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
        self.dragging = false;
        self.grab = None;
    }

    pub fn dragging(&self) -> bool {
        self.dragging
    }
}
