//! Step IO (`fx1`). Ordered cues for this fold only — not rest, not a node.

use crate::error::Error;

pub const VERSION: &str = "fx1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    Sound { key: String },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fx {
    pub ops: Vec<Op>,
}

impl Fx {
    pub fn empty() -> &'static Self {
        static EMPTY: Fx = Fx { ops: Vec::new() };
        &EMPTY
    }

    pub fn new() -> Self {
        Self { ops: Vec::new() }
    }

    pub fn sound(mut self, key: impl Into<String>) -> Self {
        self.ops.push(Op::Sound { key: key.into() });
        self
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    pub fn dump(&self) -> String {
        let mut out = String::from(VERSION);
        out.push('\n');
        for op in &self.ops {
            match op {
                Op::Sound { key } => {
                    out.push_str("sound ");
                    out.push_str(key);
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
        Ok(Fx { ops })
    }
}

fn parse_op(line: &str) -> Result<Op, Error> {
    let mut parts = line.split_whitespace();
    let op = parts
        .next()
        .ok_or_else(|| Error::ActParse("empty op".into()))?;
    match op {
        "sound" => {
            let key = parts
                .next()
                .ok_or_else(|| Error::ActParse("sound needs a key".into()))?;
            if parts.next().is_some() {
                return Err(Error::ActParse("sound takes one key".into()));
            }
            Ok(Op::Sound {
                key: key.to_string(),
            })
        }
        other => Err(Error::UnknownOp(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dump_roundtrip() {
        let fx = Fx::new().sound("lock").sound("clear");
        let src = "fx1\nsound lock\nsound clear\n";
        assert_eq!(fx.dump(), src);
        assert_eq!(Fx::from_dump(src).unwrap(), fx);
    }

    #[test]
    fn empty_dump() {
        assert_eq!(Fx::empty().dump(), "fx1\n");
        assert_eq!(Fx::from_dump("fx1\n").unwrap(), *Fx::empty());
    }

    #[test]
    fn unknown_op_errors() {
        match Fx::from_dump("fx1\nplay lock\n") {
            Err(Error::UnknownOp(op)) => assert_eq!(op, "play"),
            other => panic!("{other:?}"),
        }
    }
}
