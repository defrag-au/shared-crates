use scenegraph::Vec3;

use crate::clip::{Interp, Key, Track};
use crate::ease::Ease;
use crate::quat::Quat;

pub fn sample(keys: &[Key], interp: Interp, ease: Ease, track: Track, t: f32) -> Option<Vec3> {
    if keys.is_empty() {
        return None;
    }
    if keys.len() == 1 || t <= keys[0].time {
        return Some(keys[0].value);
    }
    let last = keys.last()?;
    if t >= last.time {
        return Some(last.value);
    }
    let i = keys.iter().position(|k| k.time > t).unwrap_or(1) - 1;
    let a = keys[i];
    let b = keys[i + 1];
    let span = (b.time - a.time).max(1e-8);
    let u = ease.apply(((t - a.time) / span).clamp(0.0, 1.0));
    Some(match interp {
        Interp::Step => a.value,
        Interp::Linear => lerp_track(track, a.value, b.value, u),
        Interp::Cubic => {
            let p0 = if i == 0 { a.value } else { keys[i - 1].value };
            let p3 = keys.get(i + 2).map(|k| k.value).unwrap_or(b.value);
            cubic_track(track, p0, a.value, b.value, p3, u)
        }
    })
}

fn lerp_track(track: Track, a: Vec3, b: Vec3, u: f32) -> Vec3 {
    if track == Track::Rotate {
        return Quat::from_euler_xyz(a)
            .slerp(Quat::from_euler_xyz(b), u)
            .to_euler_xyz();
    }
    lerp3(a, b, u)
}

fn cubic_track(track: Track, p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, u: f32) -> Vec3 {
    if track == Track::Rotate {
        let u = u * u * (3.0 - 2.0 * u);
        return lerp_track(track, p1, p2, u);
    }
    catmull(p0, p1, p2, p3, u)
}

fn lerp3(a: Vec3, b: Vec3, u: f32) -> Vec3 {
    Vec3::new(
        a.x + (b.x - a.x) * u,
        a.y + (b.y - a.y) * u,
        a.z + (b.z - a.z) * u,
    )
}

fn catmull(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, u: f32) -> Vec3 {
    let u2 = u * u;
    let u3 = u2 * u;
    Vec3::new(
        catmull1(p0.x, p1.x, p2.x, p3.x, u, u2, u3),
        catmull1(p0.y, p1.y, p2.y, p3.y, u, u2, u3),
        catmull1(p0.z, p1.z, p2.z, p3.z, u, u2, u3),
    )
}

fn catmull1(p0: f32, p1: f32, p2: f32, p3: f32, u: f32, u2: f32, u3: f32) -> f32 {
    0.5 * ((2.0 * p1)
        + (-p0 + p2) * u
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * u2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * u3)
}
