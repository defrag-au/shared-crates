//! Ordered image quads — a CNFT token (or any layer stack) as a scene.
//!
//! Generation, rarity, and chain metadata stay in the compositor. This
//! crate only names the fulfilled layers. Rest pose keeps layers a hair
//! of world Z apart so the frontal reads as one print. An exploded view
//! is a sibling driver crate (`drv1` ops → `an1`). This crate names rest.
//! Node names are dump-safe (`01-backgrounds`).

use crate::error::Error;
use crate::material::{Materials, Shade, Surface};
use crate::math::Vec3;
use crate::node::Node;
use crate::scene::Scene;

/// Default print height in world units. Width follows the pixel aspect.
pub const HEIGHT: f32 = 2.0;
/// Rest gap along +Z per `z` unit. Tiny — registration, not a card stack.
pub const GAP: f32 = 0.001;
/// Explode `an1` (layer-only): world Z per compositor `z` unit.
pub const SPREAD_Z: f32 = 0.55;
/// Explode `an1`: world X fan.
pub const SPREAD_X: f32 = 0.12;
/// Explode `an1`: world Y stair.
pub const SPREAD_Y: f32 = 0.08;

#[derive(Clone, Debug, PartialEq)]
pub struct StackLayer {
    pub name: String,
    pub image: String,
    pub z: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stack {
    pub group: String,
    pub width: f32,
    pub height: f32,
    pub gap: f32,
    pub layers: Vec<StackLayer>,
}

impl Stack {
    pub fn new(group: impl Into<String>, width: f32, height: f32) -> Self {
        Self {
            group: group.into(),
            width: width.max(1e-4),
            height: height.max(1e-4),
            gap: GAP,
            layers: Vec::new(),
        }
    }

    /// Square canvas from pixel size. Height is [`HEIGHT`]; width follows aspect.
    pub fn from_pixels(group: impl Into<String>, px_w: u32, px_h: u32) -> Self {
        let h = HEIGHT;
        let w = if px_h == 0 {
            h
        } else {
            h * px_w as f32 / px_h as f32
        };
        Self::new(group, w, h)
    }

    pub fn push(&mut self, slot: &str, image: impl Into<String>, z: u32) {
        let mut name = dump_id(slot);
        if name.is_empty() {
            name = format!("layer-{z}");
        }
        let mut unique = name.clone();
        let mut n = 2u32;
        while self.layers.iter().any(|l| l.name == unique) {
            unique = format!("{name}-{n}");
            n += 1;
        }
        self.layers.push(StackLayer {
            name: unique,
            image: image.into(),
            z,
        });
    }

    pub fn rest_t(&self, z: u32) -> Vec3 {
        Vec3::new(0.0, 0.0, z as f32 * self.gap)
    }

    pub fn explode_t(&self, z: u32) -> Vec3 {
        let zf = z as f32;
        Vec3::new(zf * SPREAD_X, zf * SPREAD_Y, zf * SPREAD_Z)
    }

    /// Dead-on camera distance. Far enough that rest Z gaps are a slight scale.
    pub fn camera_z(&self) -> f32 {
        (self.height * 3.0).max(6.0)
    }

    /// Perspective register: a slab at `z_world` matches the z=0 print on axis.
    /// Closer layers look bigger; shrink them by (C − z) / C.
    pub fn scale_at(&self, z_world: f32) -> Vec3 {
        let c = self.camera_z();
        let s = ((c - z_world) / c).clamp(0.1, 2.0);
        Vec3::new(s, s, 1.0)
    }

    pub fn scene(&self) -> Result<(Scene, Materials), Error> {
        if self.layers.is_empty() {
            return Err(Error::Parse("stack has no layers".into()));
        }
        let mut scene = Scene::new();
        let mut mats = Materials::new();
        let g = if self.group.is_empty() {
            "token"
        } else {
            &self.group
        };
        scene.insert(Node::group(g))?;
        let mut layers = self.layers.clone();
        layers.sort_by_key(|l| l.z);
        for layer in &layers {
            if mats.get(&layer.name).is_none() {
                mats.insert(
                    layer.name.clone(),
                    Surface {
                        shade: Shade::Unlit,
                        color: [255, 255, 255, 255],
                        image: Some(layer.image.clone()),
                        image_back: None,
                    },
                )?;
            }
            scene.insert(
                Node::quad(&layer.name, self.width, self.height)
                    .parent(g)
                    .mat(&layer.name)
                    .at(self.rest_t(layer.z))
                    .tag("layer"),
            )?;
        }
        scene.insert(Node::camera(
            "cam",
            Vec3::new(0.0, 0.0, self.camera_z()),
            Vec3::ZERO,
        ))?;
        Ok((scene, mats))
    }

    /// Layer-only `an1`: rest → staircase. Driver: `pose(scene, clip, t)`.
    pub fn explode_dump(&self) -> String {
        let mut out = String::from("an1\nexplode wrap=once\n");
        self.write_layer_channels(&mut out, 1.0);
        out
    }

    fn write_layer_channels(&self, out: &mut String, open_t: f32) {
        let mut layers = self.layers.clone();
        layers.sort_by_key(|l| l.z);
        let hold = (open_t - 1.0).abs() > 1e-4;
        for layer in &layers {
            let a = self.rest_t(layer.z);
            let b = self.explode_t(layer.z);
            let sa = self.scale_at(a.z);
            let sb = self.scale_at(b.z);
            if hold {
                push_chan(out, &layer.name, "t", &[(0.0, a), (open_t, b), (1.0, b)]);
                push_chan(out, &layer.name, "s", &[(0.0, sa), (open_t, sb), (1.0, sb)]);
            } else {
                push_chan(out, &layer.name, "t", &[(0.0, a), (1.0, b)]);
                push_chan(out, &layer.name, "s", &[(0.0, sa), (1.0, sb)]);
            }
        }
    }
}

/// Dump-safe identity: `01 - backgrounds` → `01-backgrounds`.
pub fn dump_id(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

fn push_vec(out: &mut String, v: Vec3) {
    crate::dump::fmt_f(v.x, out);
    out.push(',');
    crate::dump::fmt_f(v.y, out);
    out.push(',');
    crate::dump::fmt_f(v.z, out);
}

fn push_chan(out: &mut String, node: &str, track: &str, keys: &[(f32, Vec3)]) {
    out.push_str(node);
    out.push(' ');
    out.push_str(track);
    for (t, v) in keys {
        out.push(' ');
        crate::dump::fmt_f(*t, out);
        out.push(':');
        push_vec(out, *v);
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::DrawList;
    use crate::font::Fonts;
    use crate::geom::Geometry;
    use crate::images::Images;
    use crate::ALPHA_CUTOFF;

    fn checker_front() -> (Images, Stack) {
        // 4×4: opaque 2×2 in the centre, transparent rim.
        let mut px = vec![0u8; 4 * 4 * 4];
        for y in 1..3 {
            for x in 1..3 {
                let i = ((y * 4 + x) * 4) as usize;
                px[i] = 200;
                px[i + 1] = 40;
                px[i + 2] = 40;
                px[i + 3] = 255;
            }
        }
        let mut images = Images::new();
        images.insert("hat", 4, 4, px).unwrap();
        let mut skin = vec![0u8; 4 * 4 * 4];
        for p in skin.chunks_mut(4) {
            p[0] = 80;
            p[1] = 60;
            p[2] = 40;
            p[3] = 255;
        }
        images.insert("skin", 4, 4, skin).unwrap();
        let mut stack = Stack::new("token", 2.0, 2.0);
        stack.push("skin", "skin", 0);
        stack.push("hat", "hat", 1);
        (images, stack)
    }

    #[test]
    fn dump_id_strips_slot_noise() {
        assert_eq!(dump_id("01 - backgrounds"), "01-backgrounds");
        assert_eq!(dump_id("02  - skin"), "02-skin");
        assert_eq!(dump_id("14 - headwear"), "14-headwear");
    }

    #[test]
    fn stack_dumps_unlit_quads() {
        let (_, stack) = checker_front();
        let (scene, mats) = stack.scene().unwrap();
        let d = scene.dump();
        assert!(d.contains("skin Quad 2x2"), "{d}");
        assert!(d.contains("mat=skin"), "{d}");
        assert!(d.contains("hat Quad 2x2"), "{d}");
        assert!(d.contains("cam Camera eye=0,0,6 at=0,0,0"), "{d}");
        assert!(!d.contains("Slab"), "{d}");
        assert!(!d.contains(" s="), "rest must not perspective-scale: {d}");
        let md = mats.dump();
        assert!(md.contains("skin Unlit m=skin"), "{md}");
        assert!(md.contains("hat Unlit m=hat"), "{md}");
        Scene::from_dump(&d).unwrap();
        Materials::from_dump(&md).unwrap();
    }

    #[test]
    fn explode_dump_roundtrips() {
        let (_, stack) = checker_front();
        let src = stack.explode_dump();
        assert!(src.starts_with("an1\nexplode wrap=once\n"), "{src}");
        assert!(src.contains("hat t 0:"), "{src}");
        assert!(src.contains("hat s 0:"), "{src}");
        assert!(src.contains(" 1:"));
    }

    #[test]
    fn alpha_pick_punches_through_hat() {
        let (images, stack) = checker_front();
        let (scene, mats) = stack.scene().unwrap();
        let list = scene
            .draw_list_with(&Geometry::new(), &mats, &mut Fonts::new())
            .unwrap();
        let geom = list
            .hit(Vec3::new(0.9, 0.9, 4.0), Vec3::new(0.0, 0.0, -1.0))
            .unwrap();
        assert_eq!(geom.name, "hat", "geometry still occupies the full slab");
        let through = list
            .hit_with(Vec3::new(0.9, 0.9, 4.0), Vec3::new(0.0, 0.0, -1.0), &images)
            .unwrap();
        assert_eq!(
            through.name, "skin",
            "transparent hat rim must yield to skin"
        );
        let centre = list
            .hit_with(Vec3::new(0.0, 0.0, 4.0), Vec3::new(0.0, 0.0, -1.0), &images)
            .unwrap();
        assert_eq!(centre.name, "hat");
        assert!(images.alpha_at("hat", centre.uv) > ALPHA_CUTOFF);
        let _ = list;
    }

    #[test]
    fn hit_without_pixels_stays_geometry() {
        let (_, stack) = checker_front();
        let (scene, mats) = stack.scene().unwrap();
        let list: DrawList = scene
            .draw_list_with(&Geometry::new(), &mats, &mut Fonts::new())
            .unwrap();
        let h = list
            .hit_with(
                Vec3::new(0.9, 0.9, 4.0),
                Vec3::new(0.0, 0.0, -1.0),
                &Images::new(),
            )
            .unwrap();
        assert_eq!(h.name, "hat");
    }

    #[test]
    fn explode_scale_shrinks_front() {
        let (_, stack) = checker_front();
        let rest = stack.scale_at(stack.rest_t(1).z);
        let open = stack.scale_at(stack.explode_t(1).z);
        assert!(open.x < rest.x, "open {} rest {}", open.x, rest.x);
    }
}
