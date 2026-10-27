use std::collections::HashSet;

use scenegraph::Scene;

use crate::clip::Clip;
use crate::error::Error;
use crate::pose::pose_layers;

pub const VERSION: &str = "mx1";

/// One clip with its own playhead. `rate` is +1 forward, -1 reverse, 0 parked.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub clip: Clip,
    pub time: f32,
    pub rate: f32,
    /// Forward waits until this layer parks. Reverse plays this layer first;
    /// when it drops, `after` retreats.
    pub after: Option<String>,
}

/// Several clips, independent times. Pose overlays in push order.
///
/// Inspect uses this: one layer per card, reverse to send a card home
/// while another layer lifts. Follow-on layers (`after=`) are a reversible
/// chain — fade in after a lift parks; fade out before the lift returns.
/// The graph still has no playhead.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mixer {
    pub layers: Vec<Layer>,
}

impl Mixer {
    pub fn new() -> Self {
        Self { layers: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    /// Play `clip` forward. Same `clip.name` resumes toward the end
    /// from the current time (does not reset).
    pub fn go(&mut self, clip: Clip) {
        let name = clip.name.clone();
        if let Some(layer) = self.layers.iter_mut().find(|l| l.clip.name == name) {
            layer.rate = 1.0;
            return;
        }
        self.layers.push(Layer {
            clip,
            time: 0.0,
            rate: 1.0,
            after: None,
        });
    }

    /// Play `clip` after `after` parks. Reverse plays this clip first.
    pub fn then(&mut self, clip: Clip, after: impl Into<String>) {
        let name = clip.name.clone();
        let after = after.into();
        if let Some(layer) = self.layers.iter_mut().find(|l| l.clip.name == name) {
            layer.rate = 1.0;
            layer.after = Some(after);
            return;
        }
        self.layers.push(Layer {
            clip,
            time: 0.0,
            rate: 1.0,
            after: Some(after),
        });
    }

    /// Play named layer backward. Dropped by [`Self::tick`] at time 0.
    pub fn retreat(&mut self, name: &str) {
        if let Some(layer) = self.layers.iter_mut().find(|l| l.clip.name == name) {
            layer.rate = -1.0;
        }
    }

    /// Reverse chain tips. Follow-ons that never started are dropped so the
    /// predecessor can reverse immediately.
    pub fn retreat_all(&mut self) {
        self.layers
            .retain(|l| !(l.after.is_some() && l.time <= 1e-4 && l.rate >= 0.0));
        let followed: HashSet<String> =
            self.layers.iter().filter_map(|l| l.after.clone()).collect();
        for layer in &mut self.layers {
            if !followed.contains(&layer.clip.name) {
                layer.rate = -1.0;
            }
        }
    }

    pub fn pose(&self, scene: &Scene) -> Result<Scene, Error> {
        let layers = self.active();
        pose_layers(scene, &layers)
    }

    /// Layers that currently write the pose. Unstarted follow-ons are omitted.
    pub fn active(&self) -> Vec<(&Clip, f32)> {
        self.layers
            .iter()
            .filter(|l| !unstarted_follow(l))
            .map(|l| (&l.clip, l.time))
            .collect()
    }

    /// Parents (`after=`) of follow-ons that have started — flavor cards.
    pub fn fading(&self) -> Vec<&str> {
        self.layers
            .iter()
            .filter(|l| l.after.is_some() && l.time > 1e-4)
            .filter_map(|l| l.after.as_deref())
            .collect()
    }

    fn parked(&self) -> HashSet<String> {
        self.layers
            .iter()
            .filter(|l| l.rate.abs() < 1e-8 && l.time >= l.clip.duration() - 1e-4)
            .map(|l| l.clip.name.clone())
            .collect()
    }

    /// Advance playheads. Returns true if a time or membership changed.
    /// Forward clips park at duration (`rate=0`). Reverse clips drop at 0
    /// and then retreat their `after` predecessor.
    pub fn tick(&mut self, dt: f32) -> bool {
        let parked = self.parked();
        let mut changed = false;
        let mut i = 0;
        let mut retreat_pred: Vec<String> = Vec::new();
        while i < self.layers.len() {
            let rate = self.layers[i].rate;
            if waiting(&self.layers[i], &parked) {
                i += 1;
                continue;
            }
            if rate.abs() > 1e-8 {
                self.layers[i].time += rate * dt;
                changed = true;
            }
            let d = self.layers[i].clip.duration();
            if rate > 0.0 && self.layers[i].time >= d {
                self.layers[i].time = d;
                self.layers[i].rate = 0.0;
            } else if rate < 0.0 && self.layers[i].time <= 0.0 {
                if let Some(pred) = self.layers[i].after.clone() {
                    retreat_pred.push(pred);
                }
                self.layers.remove(i);
                changed = true;
                continue;
            }
            i += 1;
        }
        for name in retreat_pred {
            self.retreat(&name);
            changed = true;
        }
        changed
    }

    /// Playheads only. Clips are not in the dump — pose tests use `an1`.
    pub fn dump(&self) -> String {
        let mut out = String::from(VERSION);
        out.push('\n');
        for layer in &self.layers {
            out.push_str(&layer.clip.name);
            if layer.time.abs() > 1e-4 {
                out.push_str(" t=");
                compact_f(layer.time, &mut out);
            }
            if (layer.rate - 1.0).abs() > 1e-4 {
                out.push_str(" rate=");
                compact_f(layer.rate, &mut out);
            }
            if let Some(a) = &layer.after {
                out.push_str(" after=");
                out.push_str(a);
            }
            out.push('\n');
        }
        out
    }
}

fn unstarted_follow(layer: &Layer) -> bool {
    layer.after.is_some() && layer.time <= 1e-4 && layer.rate >= 0.0
}

fn waiting(layer: &Layer, parked: &HashSet<String>) -> bool {
    match &layer.after {
        Some(name) if unstarted_follow(layer) => !parked.contains(name),
        _ => false,
    }
}

fn compact_f(v: f32, out: &mut String) {
    if v == 0.0 {
        out.push('0');
        return;
    }
    let s = format!("{v:.4}");
    out.push_str(s.trim_end_matches('0').trim_end_matches('.'));
}
