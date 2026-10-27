use scenegraph::{HitFace, Vec3};

use crate::error::Error;
use crate::session::{Sel, Session};
use crate::tool::Tool;

pub const VERSION: &str = "in1";

impl Session {
    pub fn dump(&self) -> String {
        let mut out = String::from(VERSION);
        out.push('\n');
        out.push_str(self.tool.as_str());
        out.push('\n');
        if self.ndc != [0.0, 0.0] {
            out.push_str("ndc=");
            compact_f(self.ndc[0], &mut out);
            out.push(',');
            compact_f(self.ndc[1], &mut out);
            out.push('\n');
        }
        if (self.aspect - 1.0).abs() > 1e-4 {
            out.push_str("aspect=");
            compact_f(self.aspect, &mut out);
            out.push('\n');
        }
        if let Some(sel) = &self.sel {
            out.push_str("sel=");
            out.push_str(&sel.name);
            out.push_str(" face=");
            out.push_str(face_str(sel.face));
            out.push_str(" uv=");
            compact_f(sel.uv[0], &mut out);
            out.push(',');
            compact_f(sel.uv[1], &mut out);
            out.push_str(" at=");
            compact_f(sel.point.x, &mut out);
            out.push(',');
            compact_f(sel.point.y, &mut out);
            out.push(',');
            compact_f(sel.point.z, &mut out);
            out.push('\n');
        }
        out
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
        let tool_tok = lines
            .next()
            .ok_or_else(|| Error::Parse("missing tool".into()))?
            .trim();
        let mut session = Session::new();
        session.tool = Tool::parse(tool_tok)?;
        for (i, line) in lines.enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            parse_session_line(&mut session, line).map_err(|e| match e {
                Error::Parse(m) => Error::Parse(format!("line {}: {m}", i + 3)),
                other => other,
            })?;
        }
        Ok(session)
    }
}

fn parse_session_line(session: &mut Session, line: &str) -> Result<(), Error> {
    if let Some(rest) = line.strip_prefix("sel=") {
        session.sel = Some(parse_sel(rest)?);
        return Ok(());
    }
    let mut ndc = None;
    let mut aspect = None;
    for tok in line.split_whitespace() {
        let Some((k, v)) = tok.split_once('=') else {
            return Err(Error::Parse(format!("bad token '{tok}'")));
        };
        match k {
            "ndc" => ndc = Some(parse_vec2(v)?),
            "aspect" => {
                aspect = Some(
                    v.parse()
                        .map_err(|_| Error::Parse(format!("bad aspect '{v}'")))?,
                )
            }
            other => return Err(Error::UnknownKey(other.to_string())),
        }
    }
    if let Some(v) = ndc {
        session.ndc = v;
    }
    if let Some(v) = aspect {
        session.aspect = v;
    }
    Ok(())
}

fn parse_sel(s: &str) -> Result<Sel, Error> {
    let mut parts = s.split_whitespace();
    let name = parts
        .next()
        .ok_or_else(|| Error::Parse("missing sel name".into()))?;
    let mut face = HitFace::Front;
    let mut uv = [0.5, 0.5];
    let mut point = Vec3::ZERO;
    for tok in parts {
        let Some((k, v)) = tok.split_once('=') else {
            return Err(Error::Parse(format!("bad sel token '{tok}'")));
        };
        match k {
            "face" => face = parse_face(v)?,
            "uv" => uv = parse_vec2(v)?,
            "at" => point = parse_vec3(v)?,
            other => return Err(Error::UnknownKey(other.to_string())),
        }
    }
    Ok(Sel {
        name: name.to_string(),
        face,
        uv,
        point,
    })
}

fn parse_face(s: &str) -> Result<HitFace, Error> {
    match s {
        "front" => Ok(HitFace::Front),
        "back" => Ok(HitFace::Back),
        "edge" => Ok(HitFace::Edge),
        other => Err(Error::UnknownFace(other.to_string())),
    }
}

fn face_str(face: HitFace) -> &'static str {
    match face {
        HitFace::Front => "front",
        HitFace::Back => "back",
        HitFace::Edge => "edge",
    }
}

fn parse_vec2(s: &str) -> Result<[f32; 2], Error> {
    let mut it = s.split(',');
    let x = parse_f(it.next().ok_or_else(|| Error::Parse("vec2 x".into()))?)?;
    let y = parse_f(it.next().ok_or_else(|| Error::Parse("vec2 y".into()))?)?;
    if it.next().is_some() {
        return Err(Error::Parse("vec2 extra".into()));
    }
    Ok([x, y])
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
