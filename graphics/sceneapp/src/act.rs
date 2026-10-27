//! Closed rest-write ops (`act1`). Names are identity. Not a concurrent merge.

use std::collections::HashMap;

use scenegraph::{Node, Scene, Transform, Vec3};

use crate::error::Error;

pub const VERSION: &str = "act1";

const MARK_FONT: &str = "ibm";
const MARK_SIZE: f32 = 0.55;
const MARK_T: Vec3 = Vec3 {
    x: 0.0,
    y: 0.0,
    z: 0.05,
};
const MARK_COLOR: [u8; 4] = [0x1a, 0x12, 0x10, 255];

#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    Mark {
        node: String,
        text: String,
        size: f32,
    },
    Set {
        node: String,
        text: String,
    },
    Remove {
        node: String,
    },
    /// Write rest transform. Omitted axes stay.
    To {
        node: String,
        t: Option<Vec3>,
        r: Option<Vec3>,
        s: Option<Vec3>,
    },
}

impl Op {
    pub fn node(&self) -> &str {
        match self {
            Op::Mark { node, .. }
            | Op::Set { node, .. }
            | Op::Remove { node }
            | Op::To { node, .. } => node,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Act {
    pub ops: Vec<Op>,
}

impl Act {
    pub fn new() -> Self {
        Self { ops: Vec::new() }
    }

    pub fn op(mut self, op: Op) -> Self {
        self.ops.push(op);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    pub fn target(&self) -> Option<&str> {
        self.ops.first().map(Op::node)
    }

    pub fn dump(&self) -> String {
        let mut out = String::from(VERSION);
        out.push('\n');
        for op in &self.ops {
            match op {
                Op::Mark { node, text, size } => {
                    out.push_str("mark node=");
                    out.push_str(node);
                    out.push_str(" text=");
                    dump_text(text, &mut out);
                    if (*size - MARK_SIZE).abs() > 1e-4 {
                        out.push_str(" size=");
                        compact_f(*size, &mut out);
                    }
                    out.push('\n');
                }
                Op::Set { node, text } => {
                    out.push_str("set node=");
                    out.push_str(node);
                    out.push_str(" text=");
                    dump_text(text, &mut out);
                    out.push('\n');
                }
                Op::Remove { node } => {
                    out.push_str("remove node=");
                    out.push_str(node);
                    out.push('\n');
                }
                Op::To { node, t, r, s } => {
                    out.push_str("to node=");
                    out.push_str(node);
                    if let Some(v) = t {
                        out.push_str(" t=");
                        compact_vec(*v, &mut out);
                    }
                    if let Some(v) = r {
                        out.push_str(" r=");
                        compact_vec(*v, &mut out);
                    }
                    if let Some(v) = s {
                        out.push_str(" s=");
                        compact_vec(*v, &mut out);
                    }
                    out.push('\n');
                }
            }
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
        let mut ops = Vec::new();
        for (i, line) in lines.enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            ops.push(parse_op(line).map_err(|e| match e {
                Error::ActParse(m) => Error::ActParse(format!("line {}: {m}", i + 2)),
                other => other,
            })?);
        }
        Ok(Act { ops })
    }
}

/// Apply ops to a clone of `rest`. Missing nodes are errors.
pub fn apply(rest: &Scene, act: &Act) -> Result<Scene, Error> {
    let mut out = rest.clone();
    for op in &act.ops {
        match op {
            Op::Mark { node, text, size } => {
                if out.get(node).is_none() {
                    return Err(Error::UnknownActNode(node.clone()));
                }
                let name = format!("mark-{node}");
                if out.get(&name).is_some() {
                    continue;
                }
                out.insert(
                    Node::label(name, MARK_FONT, *size, text)
                        .parent(node)
                        .at(MARK_T)
                        .color(MARK_COLOR),
                )
                .map_err(|e| Error::Scene {
                    path: "act.act1".into(),
                    source: e,
                })?;
            }
            Op::Set { node, text } => {
                let n = out
                    .get_mut(node)
                    .ok_or_else(|| Error::UnknownActNode(node.clone()))?;
                match &mut n.kind {
                    scenegraph::Kind::Label { text: t, .. } => *t = text.clone(),
                    _ => return Err(Error::ActParse(format!("'{node}' is not a Label"))),
                }
            }
            Op::Remove { node } => {
                if out.get(node).is_none() {
                    return Err(Error::UnknownActNode(node.clone()));
                }
                out.remove(node);
            }
            Op::To { node, t, r, s } => {
                let xf = out
                    .get(node)
                    .ok_or_else(|| Error::UnknownActNode(node.clone()))?
                    .xf;
                let xf = Transform {
                    t: t.unwrap_or(xf.t),
                    r: r.unwrap_or(xf.r),
                    s: s.unwrap_or(xf.s),
                };
                out.set_xf(node, xf).map_err(|e| Error::Scene {
                    path: "act.act1".into(),
                    source: e,
                })?;
            }
        }
    }
    Ok(out)
}

fn parse_op(line: &str) -> Result<Op, Error> {
    let (kind, kv) = split_op(line)?;
    match kind.as_str() {
        "mark" => {
            let node = kv
                .get("node")
                .cloned()
                .ok_or(Error::MarkNeedsNode)?;
            let text = kv.get("text").cloned().ok_or(Error::MarkNeedsText)?;
            let size = match kv.get("size") {
                Some(s) => s
                    .parse()
                    .map_err(|_| Error::ActParse(format!("bad size '{s}'")))?,
                None => MARK_SIZE,
            };
            Ok(Op::Mark { node, text, size })
        }
        "set" => {
            let node = kv.get("node").cloned().ok_or(Error::SetNeedsNode)?;
            let text = kv.get("text").cloned().ok_or(Error::SetNeedsText)?;
            Ok(Op::Set { node, text })
        }
        "remove" => {
            let node = kv.get("node").cloned().ok_or(Error::RemoveNeedsNode)?;
            Ok(Op::Remove { node })
        }
        "to" => {
            let node = kv.get("node").cloned().ok_or(Error::ToNeedsNode)?;
            let t = kv.get("t").map(|v| parse_vec3(v)).transpose()?;
            let r = kv.get("r").map(|v| parse_vec3(v)).transpose()?;
            let s = kv.get("s").map(|v| parse_vec3(v)).transpose()?;
            if t.is_none() && r.is_none() && s.is_none() {
                return Err(Error::ToNeedsDest);
            }
            Ok(Op::To { node, t, r, s })
        }
        other => Err(Error::UnknownOp(other.to_string())),
    }
}

fn split_op(line: &str) -> Result<(String, HashMap<String, String>), Error> {
    let mut it = line.chars().peekable();
    let kind = read_word(&mut it)?;
    skip_ws(&mut it);
    let mut kv = HashMap::new();
    while it.peek().is_some() {
        let key = read_until(&mut it, '=')?;
        if key.is_empty() {
            return Err(Error::ActParse("bad token".into()));
        }
        if it.next() != Some('=') {
            return Err(Error::UnknownKey(key));
        }
        let val = read_value(&mut it)?;
        kv.insert(key, val);
        skip_ws(&mut it);
    }
    Ok((kind, kv))
}

fn read_word(it: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Result<String, Error> {
    skip_ws(it);
    let mut s = String::new();
    while let Some(&c) = it.peek() {
        if c.is_whitespace() {
            break;
        }
        s.push(c);
        it.next();
    }
    if s.is_empty() {
        return Err(Error::ActParse("missing op".into()));
    }
    Ok(s)
}

fn read_until(
    it: &mut std::iter::Peekable<std::str::Chars<'_>>,
    stop: char,
) -> Result<String, Error> {
    let mut s = String::new();
    while let Some(&c) = it.peek() {
        if c == stop || c.is_whitespace() {
            break;
        }
        s.push(c);
        it.next();
    }
    Ok(s)
}

fn read_value(it: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Result<String, Error> {
    match it.peek() {
        Some('"') => {
            it.next();
            let mut s = String::new();
            loop {
                match it.next() {
                    None => return Err(Error::ActParse("unterminated string".into())),
                    Some('\\') => match it.next() {
                        Some('"') => s.push('"'),
                        Some('\\') => s.push('\\'),
                        Some(o) => s.push(o),
                        None => return Err(Error::ActParse("unterminated string".into())),
                    },
                    Some('"') => return Ok(s),
                    Some(ch) => s.push(ch),
                }
            }
        }
        Some(_) => {
            let mut s = String::new();
            while let Some(&c) = it.peek() {
                if c.is_whitespace() {
                    break;
                }
                s.push(c);
                it.next();
            }
            Ok(s)
        }
        None => Err(Error::ActParse("missing value".into())),
    }
}

fn skip_ws(it: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    while it.peek().is_some_and(|c| c.is_whitespace()) {
        it.next();
    }
}

fn dump_text(s: &str, out: &mut String) {
    if s.chars().any(|c| c.is_whitespace() || matches!(c, '"' | '\\')) {
        out.push('"');
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                _ => out.push(c),
            }
        }
        out.push('"');
    } else {
        out.push_str(s);
    }
}

fn parse_vec3(s: &str) -> Result<Vec3, Error> {
    let mut it = s.split(',');
    let x = parse_f(it.next().ok_or_else(|| Error::ActParse("vec3 x".into()))?)?;
    let y = parse_f(it.next().ok_or_else(|| Error::ActParse("vec3 y".into()))?)?;
    let z = parse_f(it.next().ok_or_else(|| Error::ActParse("vec3 z".into()))?)?;
    if it.next().is_some() {
        return Err(Error::ActParse("vec3 extra".into()));
    }
    Ok(Vec3::new(x, y, z))
}

fn parse_f(s: &str) -> Result<f32, Error> {
    s.parse()
        .map_err(|_| Error::ActParse(format!("bad float '{s}'")))
}

fn compact_vec(v: Vec3, out: &mut String) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dump_roundtrip() {
        let src =
            "act1\nmark node=c11 text=X\nset node=turn text=\"O to play\"\nto node=ball t=0.1,0,0\n";
        let a = Act::from_dump(src).unwrap();
        assert_eq!(a.dump(), src);
        assert_eq!(a.target(), Some("c11"));
    }

    #[test]
    fn unknown_op_errors() {
        match Act::from_dump("act1\nwiggle node=c11\n") {
            Err(Error::UnknownOp(s)) => assert_eq!(s, "wiggle"),
            other => panic!("{other:?}"),
        }
    }
}
