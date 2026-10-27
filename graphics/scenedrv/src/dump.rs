use sceneanim::{Ease, Wrap};
use scenegraph::Vec3;

use crate::driver::Driver;
use crate::error::Error;
use crate::op::{parse_op, Fan, Op, Orbit, To, AZ, EL, OPEN, SPREAD_X, SPREAD_Y, SPREAD_Z};

pub const VERSION: &str = "drv1";

impl Driver {
    pub fn dump(&self) -> String {
        let mut out = String::from(VERSION);
        out.push('\n');
        out.push_str(&self.name);
        if self.wrap != Wrap::Once {
            out.push_str(" wrap=");
            out.push_str(self.wrap.as_str());
        }
        out.push('\n');
        for op in &self.ops {
            match op {
                Op::Fan(f) => dump_fan(f, &mut out),
                Op::Orbit(o) => dump_orbit(o, &mut out),
                Op::To(t) => dump_to(t, &mut out),
            }
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
        let header = lines
            .next()
            .ok_or_else(|| Error::Parse("missing driver name".into()))?
            .trim();
        let mut wrap = Wrap::Once;
        let mut name = None;
        for tok in header.split_whitespace() {
            if let Some((k, v)) = tok.split_once('=') {
                match k {
                    "wrap" => {
                        wrap = Wrap::parse(v).map_err(|_| Error::UnknownWrap(v.to_string()))?
                    }
                    other => return Err(Error::UnknownKey(other.to_string())),
                }
            } else if name.is_none() {
                name = Some(tok.to_string());
            } else {
                return Err(Error::Parse(format!("bad header token '{tok}'")));
            }
        }
        let name = name.ok_or_else(|| Error::Parse("missing driver name".into()))?;
        let mut ops = Vec::new();
        for (i, line) in lines.enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            ops.push(parse_op(line).map_err(|e| match e {
                Error::Parse(m) => Error::Parse(format!("line {}: {m}", i + 3)),
                other => other,
            })?);
        }
        if ops.is_empty() {
            return Err(Error::EmptyDriver);
        }
        Ok(Driver { name, wrap, ops })
    }
}

fn dump_fan(f: &Fan, out: &mut String) {
    out.push_str("fan tag=");
    out.push_str(&f.tag);
    if (f.open - OPEN).abs() > 1e-4 {
        out.push_str(" open=");
        compact_f(f.open, out);
    }
    let def = Vec3::new(SPREAD_X, SPREAD_Y, SPREAD_Z);
    if f.spread != def {
        if (f.spread.x - def.x).abs() > 1e-4 {
            out.push_str(" x=");
            compact_f(f.spread.x, out);
        }
        if (f.spread.y - def.y).abs() > 1e-4 {
            out.push_str(" y=");
            compact_f(f.spread.y, out);
        }
        if (f.spread.z - def.z).abs() > 1e-4 {
            out.push_str(" z=");
            compact_f(f.spread.z, out);
        }
    }
    if let Some(cam) = &f.fit {
        out.push_str(" fit=");
        out.push_str(cam);
    }
    out.push('\n');
}

fn dump_orbit(o: &Orbit, out: &mut String) {
    out.push_str("orbit cam=");
    out.push_str(&o.cam);
    if (o.from - OPEN).abs() > 1e-4 {
        out.push_str(" from=");
        compact_f(o.from, out);
    }
    if (o.az - AZ).abs() > 1e-4 {
        out.push_str(" az=");
        compact_f(o.az, out);
    }
    if (o.el - EL).abs() > 1e-4 {
        out.push_str(" el=");
        compact_f(o.el, out);
    }
    if let Some(d) = o.dist {
        out.push_str(" dist=");
        compact_f(d, out);
    }
    out.push('\n');
}

fn dump_to(t: &To, out: &mut String) {
    out.push_str("to node=");
    out.push_str(&t.node);
    if (t.open - OPEN).abs() > 1e-4 {
        out.push_str(" open=");
        compact_f(t.open, out);
    }
    if let Some(v) = t.t {
        out.push_str(" t=");
        dump_vec(v, out);
    }
    if let Some(v) = t.r {
        out.push_str(" r=");
        dump_vec(v, out);
    }
    if let Some(v) = t.s {
        out.push_str(" s=");
        dump_vec(v, out);
    }
    if let Some(a) = t.a {
        out.push_str(" a=");
        compact_f(a, out);
    }
    if t.ease != Ease::Linear {
        out.push_str(" ease=");
        out.push_str(t.ease.as_str());
    }
    out.push('\n');
}

fn dump_vec(v: Vec3, out: &mut String) {
    compact_f(v.x, out);
    out.push(',');
    compact_f(v.y, out);
    out.push(',');
    compact_f(v.z, out);
}

fn compact_f(v: f32, out: &mut String) {
    if v == 0.0 {
        out.push('0');
        return;
    }
    let s = format!("{v:.4}");
    out.push_str(s.trim_end_matches('0').trim_end_matches('.'));
}
