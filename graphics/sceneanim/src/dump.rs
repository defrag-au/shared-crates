use scenegraph::Vec3;

use crate::clip::{Channel, Clip, Interp, Key, Track, Wrap};
use crate::ease::Ease;
use crate::error::Error;

pub const VERSION: &str = "an1";

impl Clip {
    pub fn dump(&self) -> String {
        let mut out = String::from(VERSION);
        out.push('\n');
        out.push_str(&self.name);
        if self.wrap != Wrap::Once {
            out.push_str(" wrap=");
            out.push_str(self.wrap.as_str());
        }
        if let Some(len) = self.length {
            if (len - self.last_key_time()).abs() > 1e-4 {
                out.push_str(" len=");
                compact_f(len, &mut out);
            }
        }
        out.push('\n');
        for ch in &self.channels {
            out.push_str(&ch.node);
            out.push(' ');
            out.push_str(ch.track.as_str());
            if ch.interp != Interp::Linear {
                out.push(' ');
                out.push_str(ch.interp.as_str());
            }
            if ch.ease != Ease::Linear {
                out.push_str(" ease=");
                out.push_str(ch.ease.as_str());
            }
            for k in &ch.keys {
                out.push(' ');
                compact_f(k.time, &mut out);
                out.push(':');
                compact_f(k.value.x, &mut out);
                out.push(',');
                compact_f(k.value.y, &mut out);
                out.push(',');
                compact_f(k.value.z, &mut out);
            }
            out.push('\n');
        }
        out
    }

    fn last_key_time(&self) -> f32 {
        self.channels
            .iter()
            .flat_map(|c| c.keys.iter().map(|k| k.time))
            .fold(0.0f32, f32::max)
    }

    pub fn from_dump(src: &str) -> Result<Self, Error> {
        let mut lines = src.lines();
        let ver = lines
            .next()
            .ok_or_else(|| Error::Parse("empty dump".into()))?
            .trim();
        if ver != VERSION {
            return Err(Error::Parse(format!("expected {VERSION}, got {ver}")));
        }
        let header = lines
            .next()
            .ok_or_else(|| Error::Parse("missing clip name".into()))?
            .trim();
        let mut wrap = Wrap::Once;
        let mut length = None;
        let mut name = None;
        for tok in header.split_whitespace() {
            if let Some((k, v)) = tok.split_once('=') {
                match k {
                    "wrap" => wrap = Wrap::parse(v)?,
                    "len" => {
                        length = Some(
                            v.parse()
                                .map_err(|_| Error::Parse(format!("bad len '{v}'")))?,
                        )
                    }
                    other => return Err(Error::UnknownKey(other.to_string())),
                }
            } else if name.is_none() {
                name = Some(tok.to_string());
            } else {
                return Err(Error::Parse(format!("bad header token '{tok}'")));
            }
        }
        let name = name.ok_or_else(|| Error::Parse("missing clip name".into()))?;
        let mut channels = Vec::new();
        for (i, line) in lines.enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            channels.push(parse_channel(line).map_err(|e| match e {
                Error::Parse(m) => Error::Parse(format!("line {}: {m}", i + 3)),
                other => other,
            })?);
        }
        if channels.is_empty() {
            return Err(Error::EmptyClip);
        }
        Ok(Clip {
            name,
            wrap,
            length,
            channels,
        })
    }
}

fn parse_channel(line: &str) -> Result<Channel, Error> {
    let mut parts = line.split_whitespace();
    let node = parts
        .next()
        .ok_or_else(|| Error::Parse("missing node".into()))?;
    let track = Track::parse(
        parts
            .next()
            .ok_or_else(|| Error::Parse("missing track".into()))?,
    )?;
    let rest: Vec<&str> = parts.collect();
    let mut interp = Interp::Linear;
    let mut ease = Ease::Linear;
    let mut keys = Vec::new();
    for tok in rest {
        if let Some((k, v)) = tok.split_once('=') {
            match k {
                "ease" => ease = Ease::parse(v)?,
                other => return Err(Error::UnknownKey(other.to_string())),
            }
            continue;
        }
        if !tok.contains(':') {
            interp = Interp::parse(tok)?;
            continue;
        }
        let (ts, vs) = tok
            .split_once(':')
            .ok_or_else(|| Error::Parse(format!("bad key '{tok}'")))?;
        let time: f32 = ts
            .parse()
            .map_err(|_| Error::Parse(format!("bad time '{ts}'")))?;
        let value = parse_vec3(vs)?;
        if keys
            .last()
            .is_some_and(|k: &Key| (k.time - time).abs() < 1e-8)
        {
            return Err(Error::Parse(format!("duplicate time {time}")));
        }
        keys.push(Key { time, value });
    }
    if keys.is_empty() {
        return Err(Error::Parse("channel has no keys".into()));
    }
    keys.sort_by(|a, b| a.time.total_cmp(&b.time));
    Ok(Channel {
        node: node.to_string(),
        track,
        interp,
        ease,
        keys,
    })
}

fn parse_vec3(s: &str) -> Result<Vec3, Error> {
    let mut it = s.split(',');
    let x = parse_f(it.next().ok_or_else(|| Error::Parse("vec3 x".into()))?)?;
    let y = parse_f(it.next().ok_or_else(|| Error::Parse("vec3 y".into()))?)?;
    let z = parse_f(it.next().ok_or_else(|| Error::Parse("vec3 z".into()))?)?;
    if it.next().is_some() {
        return Err(Error::Parse("vec3 extra".into()));
    }
    Ok(Vec3::new(x, y, z))
}

fn parse_f(s: &str) -> Result<f32, Error> {
    s.parse()
        .map_err(|_| Error::Parse(format!("bad float '{s}'")))
}

fn compact_f(v: f32, out: &mut String) {
    if v == 0.0 {
        out.push('0');
        return;
    }
    let s = format!("{v:.4}");
    out.push_str(s.trim_end_matches('0').trim_end_matches('.'));
}
