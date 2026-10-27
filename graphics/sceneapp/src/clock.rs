//! Shell playhead (`tk1`). Never a node in `sg1`. Tests pass `dt`; the pane
//! passes frame dt. No wall clock inside apply.

use crate::act::{apply, Act};
use crate::error::Error;
use scenegraph::Scene;

pub const VERSION: &str = "tk1";
/// Fixed step tests use when they do not pass a dt.
pub const DT: f32 = 1.0 / 60.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clock {
    pub t: f32,
    pub dt: f32,
    /// Gauntlet-only: apply [`propose_tick`] this many times with [`Self::dt`].
    pub n: u32,
    /// Live gravity: seconds of playhead per game tick. 0 = no auto-tick.
    pub every: f32,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            t: 0.0,
            dt: DT,
            n: 0,
            every: 0.0,
        }
    }
}

impl Clock {
    /// Advance by an explicit dt (tests and the pane both pass one).
    pub fn step(&mut self, dt: f32) {
        self.t += dt;
    }

    pub fn dump(&self) -> String {
        let mut out = String::from(VERSION);
        out.push('\n');
        if (self.dt - DT).abs() > 1e-6 {
            out.push_str("dt=");
            compact_f(self.dt, &mut out);
            out.push('\n');
        }
        if self.t.abs() > 1e-6 {
            out.push_str("t=");
            compact_f(self.t, &mut out);
            out.push('\n');
        }
        if self.n > 0 {
            out.push_str("n=");
            out.push_str(&self.n.to_string());
            out.push('\n');
        }
        if self.every > 1e-6 {
            out.push_str("every=");
            compact_f(self.every, &mut out);
            out.push('\n');
        }
        out
    }

    pub fn from_dump(src: &str) -> Result<Self, Error> {
        let mut lines = src.lines();
        let ver = lines
            .next()
            .ok_or_else(|| Error::ActParse("empty dump".into()))?
            .trim();
        if ver != VERSION {
            return Err(Error::ActParse(format!("expected {VERSION}, got {ver}")));
        }
        let mut clock = Clock::default();
        for (i, line) in lines.enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            parse_line(&mut clock, line).map_err(|e| match e {
                Error::ActParse(m) => Error::ActParse(format!("line {}: {m}", i + 2)),
                other => other,
            })?;
        }
        Ok(clock)
    }
}

/// Autonomous rest writes for this playhead. Empty until a game proposes tick ops.
pub fn propose_tick(_scene: &Scene, _clock: &Clock) -> Act {
    Act::new()
}

/// Drive `clock.n` steps with `clock.dt`. Tests never pass wall time.
pub fn drive(scene: &Scene, clock: &Clock) -> Result<(Scene, Clock), Error> {
    let mut scene = scene.clone();
    let mut clock = *clock;
    for _ in 0..clock.n {
        let act = propose_tick(&scene, &clock);
        scene = apply(&scene, &act)?;
        clock.step(clock.dt);
    }
    Ok((scene, clock))
}

fn parse_line(clock: &mut Clock, line: &str) -> Result<(), Error> {
    for tok in line.split_whitespace() {
        let Some((k, v)) = tok.split_once('=') else {
            return Err(Error::ActParse(format!("bad token '{tok}'")));
        };
        match k {
            "dt" => {
                clock.dt = v
                    .parse()
                    .map_err(|_| Error::ActParse(format!("bad dt '{v}'")))?
            }
            "t" => {
                clock.t = v
                    .parse()
                    .map_err(|_| Error::ActParse(format!("bad t '{v}'")))?
            }
            "n" => {
                clock.n = v
                    .parse()
                    .map_err(|_| Error::ActParse(format!("bad n '{v}'")))?
            }
            "every" => {
                clock.every = v
                    .parse()
                    .map_err(|_| Error::ActParse(format!("bad every '{v}'")))?
            }
            other => return Err(Error::UnknownKey(other.to_string())),
        }
    }
    Ok(())
}

fn compact_f(v: f32, out: &mut String) {
    if v == 0.0 {
        out.push('0');
        return;
    }
    let s = format!("{v:.6}");
    out.push_str(s.trim_end_matches('0').trim_end_matches('.'));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dump_omits_defaults() {
        assert_eq!(Clock::default().dump(), "tk1\n");
    }

    #[test]
    fn dump_roundtrip_n() {
        let src = "tk1\nn=10\n";
        let c = Clock::from_dump(src).unwrap();
        assert_eq!(c.n, 10);
        assert!((c.dt - DT).abs() < 1e-6);
        assert_eq!(c.dump(), src);
    }

    #[test]
    fn dump_roundtrip_every() {
        let src = "tk1\nn=1\nevery=0.4\n";
        let c = Clock::from_dump(src).unwrap();
        assert_eq!(c.n, 1);
        assert!((c.every - 0.4).abs() < 1e-6);
        assert_eq!(c.dump(), src);
    }

    #[test]
    fn step_is_explicit_dt() {
        let mut c = Clock::default();
        c.step(0.05);
        c.step(0.05);
        assert!((c.t - 0.1).abs() < 1e-6);
    }
}
