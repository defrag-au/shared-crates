use std::collections::{HashMap, HashSet};

use crate::error::Error;
use crate::kind::{Kind, KindOf};
use crate::math::{mat4_mul, mat4_trs, Mat4};
use crate::node::{Node, Transform};

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Scene {
    nodes: Vec<Node>,
    /// `None` means the first Camera in visit order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    active_camera: Option<String>,
}

impl Scene {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            active_camera: None,
        }
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn insert(&mut self, node: Node) -> Result<(), Error> {
        if self.nodes.iter().any(|n| n.name == node.name) {
            return Err(Error::DuplicateName(node.name));
        }
        if let Some(p) = &node.parent {
            if !self.nodes.iter().any(|n| n.name == *p) {
                return Err(Error::UnknownParent(p.clone()));
            }
        }
        self.nodes.push(node);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.name == name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.name == name)
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub fn tagged<'a>(&'a self, tag: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.nodes
            .iter()
            .filter(move |n| n.tags.iter().any(|t| t == tag))
    }

    pub fn kind(&self, of: KindOf) -> impl Iterator<Item = &Node> {
        self.nodes.iter().filter(move |n| n.kind.of() == of)
    }

    pub fn first_camera_name(&self) -> Option<&str> {
        self.visit_order().into_iter().find_map(|n| match n.kind {
            Kind::Camera { .. } => Some(n.name.as_str()),
            _ => None,
        })
    }

    /// Camera used by [`Scene::draw_list`].
    pub fn active_camera_name(&self) -> Option<&str> {
        if let Some(name) = self.active_camera.as_deref() {
            if matches!(self.get(name).map(|n| &n.kind), Some(Kind::Camera { .. })) {
                return Some(name);
            }
        }
        self.first_camera_name()
    }

    pub fn set_active_camera(&mut self, name: &str) -> Result<(), Error> {
        let of = self
            .get(name)
            .ok_or_else(|| Error::UnknownNode(name.to_string()))?
            .kind
            .of();
        if of != KindOf::Camera {
            return Err(Error::KindKey(of, "active"));
        }
        self.active_camera = if self.first_camera_name() == Some(name) {
            None
        } else {
            Some(name.to_string())
        };
        Ok(())
    }

    pub fn set_parent(&mut self, name: &str, parent: Option<&str>) -> Result<(), Error> {
        if !self.nodes.iter().any(|n| n.name == name) {
            return Err(Error::UnknownNode(name.to_string()));
        }
        if let Some(p) = parent {
            if !self.nodes.iter().any(|n| n.name == p) {
                return Err(Error::UnknownParent(p.to_string()));
            }
            if self.would_cycle(name, p) {
                return Err(Error::Cycle(name.to_string()));
            }
        }
        let n = self.get_mut(name).expect("checked");
        n.parent = parent.map(str::to_string);
        Ok(())
    }

    pub fn set_xf(&mut self, name: &str, xf: Transform) -> Result<(), Error> {
        let n = self
            .get_mut(name)
            .ok_or_else(|| Error::UnknownNode(name.to_string()))?;
        n.xf = xf;
        Ok(())
    }

    /// Remove `name`. Children are reparented to the removed node's parent.
    pub fn remove(&mut self, name: &str) -> Option<Node> {
        let idx = self.nodes.iter().position(|n| n.name == name)?;
        let removed = self.nodes.remove(idx);
        for n in &mut self.nodes {
            if n.parent.as_deref() == Some(name) {
                n.parent.clone_from(&removed.parent);
            }
        }
        if self.active_camera.as_deref() == Some(name) {
            self.active_camera = None;
        }
        Some(removed)
    }

    fn would_cycle(&self, name: &str, new_parent: &str) -> bool {
        let mut walk = Some(new_parent);
        let mut seen = HashSet::new();
        while let Some(p) = walk {
            if p == name {
                return true;
            }
            if !seen.insert(p) {
                return true;
            }
            walk = self.get(p).and_then(|n| n.parent.as_deref());
        }
        false
    }

    pub fn world_matrix(&self, name: &str) -> Result<Mat4, Error> {
        let mut chain = Vec::new();
        let mut walk = Some(name);
        let mut seen = HashSet::new();
        while let Some(n) = walk {
            if !seen.insert(n) {
                return Err(Error::Cycle(n.to_string()));
            }
            let node = self
                .get(n)
                .ok_or_else(|| Error::UnknownNode(n.to_string()))?;
            chain.push(node);
            walk = node.parent.as_deref();
        }
        let mut m = crate::math::mat4_ident();
        for node in chain.iter().rev() {
            let local = mat4_trs(node.xf.t, node.xf.r, node.xf.s);
            m = mat4_mul(m, local);
        }
        Ok(m)
    }

    /// Parent-before-child world matrices for one flatten. Avoids walking
    /// the chain (and allocating) once per node.
    pub(crate) fn world_matrices(&self) -> Result<HashMap<String, Mat4>, Error> {
        let mut out = HashMap::with_capacity(self.nodes.len());
        for n in self.visit_order() {
            let local = mat4_trs(n.xf.t, n.xf.r, n.xf.s);
            let world = match n.parent.as_deref() {
                None => local,
                Some(p) => {
                    let parent = out
                        .get(p)
                        .copied()
                        .ok_or_else(|| Error::UnknownNode(p.to_string()))?;
                    mat4_mul(parent, local)
                }
            };
            out.insert(n.name.clone(), world);
        }
        Ok(out)
    }

    /// Roots first (spawn order), then children — parent-before-child for dumps.
    pub fn visit_order(&self) -> Vec<&Node> {
        let mut out = Vec::with_capacity(self.nodes.len());
        let mut by_parent: HashMap<Option<&str>, Vec<&Node>> = HashMap::new();
        for n in &self.nodes {
            by_parent.entry(n.parent.as_deref()).or_default().push(n);
        }
        fn walk<'a>(
            parent: Option<&str>,
            by_parent: &HashMap<Option<&str>, Vec<&'a Node>>,
            out: &mut Vec<&'a Node>,
        ) {
            if let Some(kids) = by_parent.get(&parent) {
                for k in kids {
                    out.push(k);
                    walk(Some(k.name.as_str()), by_parent, out);
                }
            }
        }
        walk(None, &by_parent, &mut out);
        out
    }
}
