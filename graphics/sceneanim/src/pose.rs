use scenegraph::{Kind, Scene};

use crate::clip::{Clip, Track};
use crate::error::Error;
use crate::interp;

/// Evaluate `clip` at `t` onto a clone of `scene`. Unknown targets error.
pub fn pose(scene: &Scene, clip: &Clip, t: f32) -> Result<Scene, Error> {
    pose_layers(scene, &[(clip, t)])
}

/// Overlay clips in order. Later layers overwrite the same node+track.
/// Each clip samples from rest keys; this is not additive blending.
/// An empty list returns a clone of `scene`.
pub fn pose_layers(scene: &Scene, layers: &[(&Clip, f32)]) -> Result<Scene, Error> {
    for (clip, _) in layers {
        if clip.channels.is_empty() {
            return Err(Error::EmptyClip);
        }
    }
    let mut out = scene.clone();
    for (clip, t) in layers {
        apply(&mut out, clip, *t)?;
    }
    Ok(out)
}

fn apply(out: &mut Scene, clip: &Clip, t: f32) -> Result<(), Error> {
    let t = clip.wrap_time(t);
    for ch in &clip.channels {
        let Some(v) = interp::sample(&ch.keys, ch.interp, ch.ease, ch.track, t) else {
            continue;
        };
        let node = out
            .get_mut(&ch.node)
            .ok_or_else(|| Error::UnknownTarget(ch.node.clone()))?;
        match ch.track {
            Track::Translate => node.xf.t = v,
            Track::Rotate => node.xf.r = v,
            Track::Scale => node.xf.s = v,
            Track::Eye => match &mut node.kind {
                Kind::Camera { eye, .. } => *eye = v,
                _ => return Err(Error::BadTrack(ch.node.clone(), ch.track)),
            },
            Track::At => match &mut node.kind {
                Kind::Camera { at, .. } => *at = v,
                _ => return Err(Error::BadTrack(ch.node.clone(), ch.track)),
            },
            Track::Alpha => {
                let a = (v.x.clamp(0.0, 1.0) * 255.0).round() as u8;
                match node.kind.inline_surface_mut() {
                    Some(s) => s.color[3] = a,
                    None => return Err(Error::BadTrack(ch.node.clone(), ch.track)),
                }
            }
            Track::Pose => match node.kind.pose_mut() {
                Some(Some(p)) => p.t = v.x,
                _ => return Err(Error::BadTrack(ch.node.clone(), ch.track)),
            },
        }
    }
    Ok(())
}

impl Clip {
    pub fn pose(&self, scene: &Scene, t: f32) -> Result<Scene, Error> {
        pose(scene, self, t)
    }
}
