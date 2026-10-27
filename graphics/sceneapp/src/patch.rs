//! Named rest delta (`sgd1`). Insert / set / remove by node name — not a text diff.

use scenegraph::Scene;

use crate::error::Error;

pub const VERSION: &str = "sgd1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    Add(String),
    Set(String),
    Remove(String),
}

impl Line {
    /// One `sgd1` body line, without the newline.
    pub fn dump(&self) -> String {
        match self {
            Line::Add(s) => format!("+ {s}"),
            Line::Set(s) => format!("~ {s}"),
            Line::Remove(name) => format!("- {name}"),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Delta {
    pub lines: Vec<Line>,
}

impl Delta {
    pub fn dump(&self) -> String {
        let mut out = String::from(VERSION);
        out.push('\n');
        for line in &self.lines {
            match line {
                Line::Add(s) => {
                    out.push_str("+ ");
                    out.push_str(s);
                    out.push('\n');
                }
                Line::Set(s) => {
                    out.push_str("~ ");
                    out.push_str(s);
                    out.push('\n');
                }
                Line::Remove(name) => {
                    out.push_str("- ");
                    out.push_str(name);
                    out.push('\n');
                }
            }
        }
        out
    }

    pub fn from_dump(src: &str) -> Result<Self, Error> {
        let mut lines_it = src.lines();
        let ver = lines_it
            .next()
            .ok_or_else(|| Error::ActParse("empty dump".into()))?
            .trim();
        if ver != VERSION {
            return Err(Error::ActParse(format!("expected {VERSION}, got {ver}")));
        }
        let mut lines = Vec::new();
        for (i, line) in lines_it.enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parsed = parse_line(line).map_err(|e| match e {
                Error::ActParse(m) => Error::ActParse(format!("line {}: {m}", i + 2)),
                other => other,
            })?;
            lines.push(parsed);
        }
        Ok(Delta { lines })
    }
}

/// Observable named-node delta. Order follows `after` dump, then removals.
pub fn diff(before: &Scene, after: &Scene) -> Delta {
    let old = index_dump(&before.dump());
    let mut lines = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (name, line) in dump_nodes(&after.dump()) {
        seen.insert(name.clone());
        match old.get(&name) {
            None => lines.push(Line::Add(line)),
            Some(prev) if prev != &line => lines.push(Line::Set(line)),
            Some(_) => {}
        }
    }
    let mut gone: Vec<_> = old.keys().filter(|n| !seen.contains(*n)).cloned().collect();
    gone.sort();
    for name in gone {
        lines.push(Line::Remove(name));
    }
    Delta { lines }
}

pub(crate) fn parse_line(line: &str) -> Result<Line, Error> {
    let (tag, rest) = line.split_at(1);
    let rest = rest.trim();
    if rest.is_empty() {
        return Err(Error::ActParse("empty delta line".into()));
    }
    match tag {
        "+" => Ok(Line::Add(rest.to_string())),
        "~" => Ok(Line::Set(rest.to_string())),
        "-" => Ok(Line::Remove(
            rest.split_whitespace().next().unwrap_or(rest).to_string(),
        )),
        other => Err(Error::UnknownOp(other.to_string())),
    }
}

fn dump_nodes(dump: &str) -> Vec<(String, String)> {
    dump.lines()
        .skip(1)
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter_map(|l| {
            let name = l.split_whitespace().next()?;
            Some((name.to_string(), l.to_string()))
        })
        .collect()
}

fn index_dump(dump: &str) -> std::collections::HashMap<String, String> {
    dump_nodes(dump).into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dump_roundtrip() {
        let src = "sgd1\n+ mark-c11 Label font=ibm size=0.55 \"X\" c=1a1210 p=c11 t=0,0,0.05\n~ turn Label font=ibm size=0.14 \"O to play\" p=ttt t=0,-2.15,0.04\n";
        let d = Delta::from_dump(src).unwrap();
        assert_eq!(d.dump(), src);
    }
}
