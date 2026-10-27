use sceneanim::{pose_layers, Clip, Mixer};
use scenegraph::{Kind, Node, Prim, Scene, SurfaceBind, Vec3};

use crate::error::Error;

pub const FLAVOR_FONT: &str = "ibm";
pub const FLAVOR_SIZE: f32 = 0.12;
pub const FLAVOR_Z: f32 = 0.05;

/// Label name for inspect flavor on `node` (`th-shredder` → `flavor-shredder`).
pub fn flavor_node(node: &str) -> String {
    let stem = node.strip_prefix("th-").unwrap_or(node);
    format!("flavor-{stem}")
}

/// Clone `scene` and insert a flavor Label parented to `name`. Rest is untouched.
///
/// Pose cannot add nodes — this is the scene write. Compile a `to` clip against
/// rest and `pose` the clone.
pub fn inspect(scene: &Scene, name: &str, text: &str) -> Result<Scene, Error> {
    let node = scene
        .get(name)
        .ok_or_else(|| Error::UnknownNode(name.to_string()))?;
    let flavor = flavor_node(name);
    if scene.get(&flavor).is_some() {
        return Err(Error::Scene(scenegraph::Error::DuplicateName(flavor)));
    }
    let t = flavor_local(&node.kind);
    let mut label = Node::label(flavor, FLAVOR_FONT, FLAVOR_SIZE, text)
        .parent(name)
        .at(t);
    if let Some(ink) = ink_on(&node.kind) {
        label = label.color(ink);
    }
    let mut out = scene.clone();
    out.insert(label)?;
    Ok(out)
}

/// Ink that reads on the node's face, when the face is a plain inline
/// colour. A material or a texture keeps the default (white): its pixels are
/// not here to judge. White on a pale card was invisible (`scene-cards`).
fn ink_on(kind: &Kind) -> Option<[u8; 4]> {
    let surface = match kind {
        Kind::Quad {
            bind: SurfaceBind::Inline(s),
            ..
        }
        | Kind::Mesh {
            bind: SurfaceBind::Inline(s),
            ..
        } => s,
        _ => return None,
    };
    if surface.image.is_some() {
        return None;
    }
    Some(scenegraph::problems::ink_for(surface.color))
}

fn flavor_local(kind: &Kind) -> Vec3 {
    let h = match kind {
        Kind::Mesh {
            prim: Prim::Slab { h, .. },
            ..
        } => *h,
        Kind::Quad { h, .. } => *h,
        _ => 0.0,
    };
    Vec3::new(0.0, -h * 0.175, FLAVOR_Z)
}

/// Insert flavor for mixer follow-ons that have started, then pose.
///
/// `underlay` is an optional idle clip (hover, bob) sampled first; mixer
/// layers win the same node+track. Rest is not written. The pane and the
/// gauntlet must call this — not a parallel insert/pose loop.
pub fn present(
    rest: &Scene,
    mixer: &Mixer,
    flavor: impl Fn(&str) -> &'static str,
    underlay: Option<(&Clip, f32)>,
) -> Result<Scene, Error> {
    let mut base = rest.clone();
    for card in mixer.fading() {
        base = inspect(&base, card, flavor(card))?;
    }
    let mut layers: Vec<(&Clip, f32)> = Vec::new();
    if let Some(layer) = underlay {
        layers.push(layer);
    }
    layers.extend(mixer.active());
    Ok(pose_layers(&base, &layers)?)
}
