use std::fmt;

use scenegraph::Vec3;

use crate::ease::Ease;
use crate::error::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Track {
    Translate,
    Rotate,
    Scale,
    Eye,
    At,
    /// Surface alpha 0–1 in `value.x` (yz unused). Inline `c=` only.
    Alpha,
    /// A posed model's time into its own clip, seconds in `value.x` (yz
    /// unused). The node names the clip (`pose=Run@0`); this runs it.
    Pose,
}

impl Track {
    pub fn as_str(self) -> &'static str {
        match self {
            Track::Translate => "t",
            Track::Rotate => "r",
            Track::Scale => "s",
            Track::Eye => "eye",
            Track::At => "at",
            Track::Alpha => "a",
            Track::Pose => "pose",
        }
    }

    pub fn parse(token: &str) -> Result<Self, Error> {
        match token {
            "t" => Ok(Track::Translate),
            "r" => Ok(Track::Rotate),
            "s" => Ok(Track::Scale),
            "eye" => Ok(Track::Eye),
            "at" => Ok(Track::At),
            "a" => Ok(Track::Alpha),
            "pose" => Ok(Track::Pose),
            other => Err(Error::UnknownTrack(other.to_string())),
        }
    }
}

impl fmt::Display for Track {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interp {
    Step,
    Linear,
    Cubic,
}

impl Interp {
    pub fn as_str(self) -> &'static str {
        match self {
            Interp::Step => "Step",
            Interp::Linear => "Linear",
            Interp::Cubic => "Cubic",
        }
    }

    pub fn parse(token: &str) -> Result<Self, Error> {
        match token {
            "Step" => Ok(Interp::Step),
            "Linear" => Ok(Interp::Linear),
            "Cubic" => Ok(Interp::Cubic),
            other => Err(Error::UnknownInterp(other.to_string())),
        }
    }
}

impl fmt::Display for Interp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wrap {
    Once,
    Loop,
    Ping,
}

impl Wrap {
    pub fn as_str(self) -> &'static str {
        match self {
            Wrap::Once => "once",
            Wrap::Loop => "loop",
            Wrap::Ping => "ping",
        }
    }

    pub fn parse(token: &str) -> Result<Self, Error> {
        match token {
            "once" => Ok(Wrap::Once),
            "loop" => Ok(Wrap::Loop),
            "ping" => Ok(Wrap::Ping),
            other => Err(Error::UnknownWrap(other.to_string())),
        }
    }
}

impl fmt::Display for Wrap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Key {
    pub time: f32,
    pub value: Vec3,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Channel {
    pub node: String,
    pub track: Track,
    pub interp: Interp,
    pub ease: Ease,
    pub keys: Vec<Key>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub name: String,
    pub wrap: Wrap,
    /// If set, hold past the last key.
    pub length: Option<f32>,
    pub channels: Vec<Channel>,
}

impl Clip {
    pub fn duration(&self) -> f32 {
        let last = self
            .channels
            .iter()
            .flat_map(|c| c.keys.iter().map(|k| k.time))
            .fold(0.0f32, f32::max);
        self.length.unwrap_or(last).max(last)
    }

    pub fn wrap_time(&self, t: f32) -> f32 {
        let d = self.duration();
        if d <= 1e-8 {
            return 0.0;
        }
        match self.wrap {
            Wrap::Once => t.clamp(0.0, d),
            Wrap::Loop => t.rem_euclid(d),
            Wrap::Ping => {
                let cycle = t.rem_euclid(2.0 * d);
                if cycle > d {
                    2.0 * d - cycle
                } else {
                    cycle
                }
            }
        }
    }
}
