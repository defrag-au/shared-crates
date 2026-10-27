use crate::error::Error;
use crate::hilbert::Print;
use crate::world::{Hang, Wall, World, CELL};

pub const VERSION: &str = "sw1";

impl World {
    pub fn dump(&self) -> String {
        let mut out = String::from(VERSION);
        out.push('\n');
        out.push_str(&self.name);
        if (self.cell - CELL).abs() > 1e-4 {
            out.push_str(" cell=");
            compact_f(self.cell, &mut out);
        }
        if self.span != 0 {
            out.push_str(" span=");
            out.push_str(&self.span.to_string());
        }
        if self.at != (0, 0) {
            out.push_str(" at=");
            dump_i2(self.at, &mut out);
        }
        if let Some(order) = self.order {
            out.push_str(" order=");
            out.push_str(&order.to_string());
        }
        if self.seed != 0 {
            out.push_str(" seed=");
            out.push_str(&self.seed.to_string());
        }
        out.push('\n');
        for p in self.prints() {
            out.push_str("print m=");
            out.push_str(&p.image);
            if !p.title.is_empty() {
                out.push_str(" title=");
                out.push_str(&p.title);
            }
            out.push('\n');
        }
        let mut hangs = self.hangs().to_vec();
        hangs.sort_by(|a, b| {
            a.c.1
                .cmp(&b.c.1)
                .then(a.c.0.cmp(&b.c.0))
                .then(a.wall.dump_rank().cmp(&b.wall.dump_rank()))
                .then(a.name.cmp(&b.name))
        });
        for h in hangs {
            dump_hang(&h, &mut out);
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
            .ok_or_else(|| Error::Parse("missing world name".into()))?
            .trim();
        let mut name = None;
        let mut cell = CELL;
        let mut span = 0u32;
        let mut at = (0, 0);
        let mut order = None;
        let mut seed = 0u32;
        for tok in header.split_whitespace() {
            if let Some((k, v)) = tok.split_once('=') {
                match k {
                    "cell" => cell = parse_f(v)?,
                    "span" => {
                        span = v
                            .parse()
                            .map_err(|_| Error::Parse(format!("bad span '{v}'")))?
                    }
                    "at" => at = parse_i2(v)?,
                    "order" => {
                        order = Some(
                            v.parse()
                                .map_err(|_| Error::Parse(format!("bad order '{v}'")))?,
                        )
                    }
                    "seed" => {
                        seed = v
                            .parse()
                            .map_err(|_| Error::Parse(format!("bad seed '{v}'")))?
                    }
                    other => return Err(Error::UnknownKey(other.to_string())),
                }
            } else if name.is_none() {
                name = Some(tok.to_string());
            } else {
                return Err(Error::Parse(format!("bad header token '{tok}'")));
            }
        }
        let name = name.ok_or_else(|| Error::Parse("missing world name".into()))?;
        let mut world = World::new(name);
        world.cell = cell;
        world.span = span;
        world.at = at;
        world.order = order;
        world.seed = seed;
        if let Some(o) = order {
            crate::hilbert::hilbert_side(o)?;
        }
        for (i, line) in lines.enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            parse_body(line, &mut world).map_err(|e| match e {
                Error::Parse(m) => Error::Parse(format!("line {}: {m}", i + 3)),
                other => other,
            })?;
        }
        Ok(world)
    }
}

fn dump_hang(h: &Hang, out: &mut String) {
    out.push_str("hang c=");
    dump_i2(h.c, out);
    out.push_str(" wall=");
    out.push_str(h.wall.as_str());
    out.push_str(" name=");
    out.push_str(&h.name);
    out.push_str(" m=");
    out.push_str(&h.image);
    if let Some(t) = &h.title {
        out.push_str(" title=");
        out.push_str(t);
    }
    out.push('\n');
}

fn dump_i2(v: (i32, i32), out: &mut String) {
    out.push_str(&v.0.to_string());
    out.push(',');
    out.push_str(&v.1.to_string());
}

fn parse_i2(s: &str) -> Result<(i32, i32), Error> {
    let (a, b) = s
        .split_once(',')
        .ok_or_else(|| Error::Parse(format!("bad pair '{s}'")))?;
    let x = a
        .parse()
        .map_err(|_| Error::Parse(format!("bad pair '{s}'")))?;
    let z = b
        .parse()
        .map_err(|_| Error::Parse(format!("bad pair '{s}'")))?;
    Ok((x, z))
}

fn parse_f(s: &str) -> Result<f32, Error> {
    s.parse()
        .map_err(|_| Error::Parse(format!("bad number '{s}'")))
}

fn parse_body(line: &str, world: &mut World) -> Result<(), Error> {
    let kind = line
        .split_whitespace()
        .next()
        .ok_or_else(|| Error::Parse("empty line".into()))?;
    match kind {
        "hang" => world.push(parse_hang(line)?),
        "print" => world.push_print(parse_print(line)?),
        other => Err(Error::Parse(format!("unknown line '{other}'"))),
    }
}

fn parse_print(line: &str) -> Result<Print, Error> {
    let mut toks = line.split_whitespace();
    let kind = toks
        .next()
        .ok_or_else(|| Error::Parse("empty line".into()))?;
    if kind != "print" {
        return Err(Error::Parse(format!("unknown line '{kind}'")));
    }
    let mut image = None;
    let mut title = String::new();
    for tok in toks {
        let Some((k, v)) = tok.split_once('=') else {
            return Err(Error::Parse(format!("bad token '{tok}'")));
        };
        match k {
            "m" => image = Some(v.to_string()),
            "title" => title = v.to_string(),
            other => return Err(Error::UnknownKey(other.to_string())),
        }
    }
    Ok(Print::new(image.ok_or(Error::HangNeedsImage)?, title))
}

fn parse_hang(line: &str) -> Result<Hang, Error> {
    let mut toks = line.split_whitespace();
    let kind = toks
        .next()
        .ok_or_else(|| Error::Parse("empty line".into()))?;
    if kind != "hang" {
        return Err(Error::Parse(format!("unknown line '{kind}'")));
    }
    let mut c = None;
    let mut wall = None;
    let mut name = None;
    let mut image = None;
    let mut title = None;
    for tok in toks {
        let Some((k, v)) = tok.split_once('=') else {
            return Err(Error::Parse(format!("bad token '{tok}'")));
        };
        match k {
            "c" => c = Some(parse_i2(v)?),
            "wall" => wall = Some(Wall::parse(v)?),
            "name" => name = Some(v.to_string()),
            "m" => image = Some(v.to_string()),
            "title" => title = Some(v.to_string()),
            other => return Err(Error::UnknownKey(other.to_string())),
        }
    }
    let mut hang = Hang::new(
        c.ok_or(Error::HangNeedsCell)?,
        wall.ok_or(Error::HangNeedsWall)?,
        name.ok_or(Error::HangNeedsName)?,
        image.ok_or(Error::HangNeedsImage)?,
    );
    if let Some(t) = title {
        hang = hang.title(t);
    }
    Ok(hang)
}

fn compact_f(v: f32, out: &mut String) {
    if v == 0.0 {
        out.push('0');
        return;
    }
    let s = format!("{v:.4}");
    out.push_str(s.trim_end_matches('0').trim_end_matches('.'));
}
