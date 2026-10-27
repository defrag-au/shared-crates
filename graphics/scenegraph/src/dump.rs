use crate::error::Error;
use crate::kind::{
    Kind, KindOf, Lamp, LampOf, Prim, DEFAULT_FOV, DEFAULT_RANGE, DEFAULT_SLAB_DEPTH,
    DEFAULT_SLAB_RADIUS,
};
use crate::material::{Materials, Shade, Surface, SurfaceBind, VERSION as MAT_VERSION};
use crate::math::Vec3;
use crate::node::{Node, Transform};
use crate::scene::Scene;

pub const VERSION: &str = "sg1";

/// Closed dump keys. Unknown tokens are [`Error::UnknownKey`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DumpKey {
    Parent,
    Translate,
    Rotate,
    Scale,
    Image,
    Color,
    Tag,
    Eye,
    At,
    Up,
    Fov,
    Dir,
    Intensity,
    Range,
    Active,
    Geom,
    Depth,
    Radius,
    ImageBack,
    Shade,
    Mat,
    Font,
    Size,
    Pose,
    Front,
}

impl DumpKey {
    fn as_str(self) -> &'static str {
        match self {
            DumpKey::Parent => "p",
            DumpKey::Translate => "t",
            DumpKey::Rotate => "r",
            DumpKey::Scale => "s",
            DumpKey::Image => "m",
            DumpKey::Color => "c",
            DumpKey::Tag => "tag",
            DumpKey::Eye => "eye",
            DumpKey::At => "at",
            DumpKey::Up => "up",
            DumpKey::Fov => "fov",
            DumpKey::Dir => "dir",
            DumpKey::Intensity => "i",
            DumpKey::Range => "range",
            DumpKey::Active => "active",
            DumpKey::Geom => "g",
            DumpKey::Depth => "d",
            DumpKey::Radius => "rad",
            DumpKey::ImageBack => "mb",
            DumpKey::Shade => "sh",
            DumpKey::Mat => "mat",
            DumpKey::Font => "font",
            DumpKey::Size => "size",
            DumpKey::Pose => "pose",
            DumpKey::Front => "front",
        }
    }

    fn parse(token: &str) -> Result<Self, Error> {
        match token {
            "p" => Ok(DumpKey::Parent),
            "t" => Ok(DumpKey::Translate),
            "r" => Ok(DumpKey::Rotate),
            "s" => Ok(DumpKey::Scale),
            "m" => Ok(DumpKey::Image),
            "c" => Ok(DumpKey::Color),
            "tag" => Ok(DumpKey::Tag),
            "eye" => Ok(DumpKey::Eye),
            "at" => Ok(DumpKey::At),
            "up" => Ok(DumpKey::Up),
            "fov" => Ok(DumpKey::Fov),
            "dir" => Ok(DumpKey::Dir),
            "i" => Ok(DumpKey::Intensity),
            "range" => Ok(DumpKey::Range),
            "active" => Ok(DumpKey::Active),
            "g" => Ok(DumpKey::Geom),
            "d" => Ok(DumpKey::Depth),
            "rad" => Ok(DumpKey::Radius),
            "mb" => Ok(DumpKey::ImageBack),
            "sh" => Ok(DumpKey::Shade),
            "mat" => Ok(DumpKey::Mat),
            "font" => Ok(DumpKey::Font),
            "size" => Ok(DumpKey::Size),
            "pose" => Ok(DumpKey::Pose),
            "front" => Ok(DumpKey::Front),
            other => Err(Error::UnknownKey(other.to_string())),
        }
    }
}

impl Scene {
    pub fn dump(&self) -> String {
        let mut out = String::from(VERSION);
        out.push('\n');
        let active = self.active_camera_name().map(str::to_string);
        let first_cam = self.first_camera_name().map(str::to_string);
        for n in self.visit_order() {
            dump_node(n, active.as_deref(), first_cam.as_deref(), &mut out);
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
        let mut parsed: Vec<(Node, bool)> = Vec::new();
        for (i, line) in lines.enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            parsed.push(parse_line(line).map_err(|e| match e {
                Error::Parse(m) => Error::Parse(format!("line {}: {m}", i + 2)),
                other => other,
            })?);
        }
        let mut scene = Scene::new();
        let mut pending = Vec::new();
        let mut active: Option<String> = None;
        for (mut n, is_active) in parsed {
            let parent = n.parent.take();
            let name = n.name.clone();
            if is_active {
                if active.is_some() {
                    return Err(Error::MultipleActive);
                }
                active = Some(name.clone());
            }
            scene.insert(n)?;
            if let Some(p) = parent {
                pending.push((name, p));
            }
        }
        for (name, p) in pending {
            scene.set_parent(&name, Some(&p))?;
        }
        if let Some(name) = active {
            scene.set_active_camera(&name)?;
        }
        Ok(scene)
    }
}

fn dump_node(n: &Node, active: Option<&str>, first_cam: Option<&str>, out: &mut String) {
    out.push_str(&n.name);
    out.push(' ');
    out.push_str(n.kind.of().as_str());
    match &n.kind {
        Kind::Quad { w, h, bind } => {
            out.push(' ');
            fmt_f(*w, out);
            out.push('x');
            fmt_f(*h, out);
            dump_bind(bind, false, Shade::Lambert, out);
        }
        Kind::Camera { eye, at, up, fov } => {
            kv_vec(DumpKey::Eye, *eye, out);
            kv_vec(DumpKey::At, *at, out);
            if *up != Vec3::Y {
                kv_vec(DumpKey::Up, *up, out);
            }
            if (*fov - DEFAULT_FOV).abs() > 1e-4 {
                kv_key(DumpKey::Fov, out);
                fmt_f(*fov, out);
            }
            if active == Some(n.name.as_str()) && first_cam != Some(n.name.as_str()) {
                kv_key(DumpKey::Active, out);
                out.push('1');
            }
        }
        Kind::Light {
            lamp,
            color,
            intensity,
        } => {
            out.push(' ');
            out.push_str(lamp.as_str());
            match lamp {
                Lamp::Dir { dir } => kv_vec(DumpKey::Dir, *dir, out),
                Lamp::Point { range } => {
                    if (*range - DEFAULT_RANGE).abs() > 1e-4 {
                        kv_key(DumpKey::Range, out);
                        fmt_f(*range, out);
                    }
                }
            }
            if *color != [255, 255, 255, 255] {
                kv_key(DumpKey::Color, out);
                fmt_color(*color, out);
            }
            if (*intensity - 1.0).abs() > 1e-4 {
                kv_key(DumpKey::Intensity, out);
                fmt_f(*intensity, out);
            }
        }
        Kind::Mesh {
            prim,
            geom,
            bind,
            pose,
            front,
        } => {
            out.push(' ');
            out.push_str(prim.as_str());
            if let Prim::Slab {
                w,
                h,
                depth,
                radius,
            } = prim
            {
                out.push(' ');
                fmt_f(*w, out);
                out.push('x');
                fmt_f(*h, out);
                if (*depth - DEFAULT_SLAB_DEPTH).abs() > 1e-4 {
                    kv_key(DumpKey::Depth, out);
                    fmt_f(*depth, out);
                }
                if (*radius - DEFAULT_SLAB_RADIUS).abs() > 1e-4 {
                    kv_key(DumpKey::Radius, out);
                    fmt_f(*radius, out);
                }
            }
            if let Some(g) = geom {
                kv_key(DumpKey::Geom, out);
                out.push_str(g);
            }
            if let Some(p) = pose {
                kv_key(DumpKey::Pose, out);
                out.push_str(&p.clip);
                out.push('@');
                fmt_f(p.t, out);
            }
            if let Some(f) = front {
                kv_key(DumpKey::Front, out);
                out.push_str(f.as_str());
            }
            dump_bind(bind, true, Shade::Lambert, out);
        }
        Kind::Label {
            font,
            size,
            text,
            bind,
        } => {
            kv_key(DumpKey::Font, out);
            out.push_str(font);
            kv_key(DumpKey::Size, out);
            fmt_f(*size, out);
            out.push(' ');
            fmt_quoted(text, out);
            dump_bind(bind, false, Shade::Unlit, out);
        }
        Kind::Group => {}
    }
    if let Some(p) = &n.parent {
        kv_key(DumpKey::Parent, out);
        out.push_str(p);
    }
    dump_xf(&n.xf, out);
    for t in &n.tags {
        kv_key(DumpKey::Tag, out);
        out.push_str(t);
    }
}

fn dump_bind(bind: &SurfaceBind, back: bool, omit_shade: Shade, out: &mut String) {
    match bind {
        SurfaceBind::Name(k) => {
            kv_key(DumpKey::Mat, out);
            out.push_str(k);
        }
        SurfaceBind::Inline(s) => {
            if s.shade != omit_shade {
                kv_key(DumpKey::Shade, out);
                out.push_str(s.shade.as_str());
            }
            if s.color != [255, 255, 255, 255] {
                kv_key(DumpKey::Color, out);
                fmt_color(s.color, out);
            }
            if let Some(m) = &s.image {
                kv_key(DumpKey::Image, out);
                out.push_str(m);
            }
            if back {
                if let Some(m) = &s.image_back {
                    kv_key(DumpKey::ImageBack, out);
                    out.push_str(m);
                }
            }
        }
    }
}

fn dump_xf(xf: &Transform, out: &mut String) {
    if xf.t != Vec3::ZERO {
        kv_vec(DumpKey::Translate, xf.t, out);
    }
    if xf.r != Vec3::ZERO {
        kv_vec(DumpKey::Rotate, xf.r, out);
    }
    if xf.s != Vec3::ONE {
        kv_vec(DumpKey::Scale, xf.s, out);
    }
}

fn fmt_quoted(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    out.push('"');
}

enum Tok {
    Word(String),
    Str(String),
}

fn tokenize(line: &str) -> Result<Vec<Tok>, Error> {
    let mut out = Vec::new();
    let mut it = line.chars().peekable();
    while let Some(&c) = it.peek() {
        if c.is_whitespace() {
            it.next();
            continue;
        }
        if c == '"' {
            it.next();
            let mut s = String::new();
            loop {
                match it.next() {
                    None => return Err(Error::Parse("unterminated string".into())),
                    Some('\\') => match it.next() {
                        Some('"') => s.push('"'),
                        Some('\\') => s.push('\\'),
                        Some('n') => s.push('\n'),
                        Some(o) => s.push(o),
                        None => return Err(Error::Parse("unterminated string".into())),
                    },
                    Some('"') => break,
                    Some(ch) => s.push(ch),
                }
            }
            out.push(Tok::Str(s));
        } else {
            let mut s = String::new();
            while let Some(&ch) = it.peek() {
                if ch.is_whitespace() {
                    break;
                }
                s.push(ch);
                it.next();
            }
            out.push(Tok::Word(s));
        }
    }
    Ok(out)
}

fn kv_key(k: DumpKey, out: &mut String) {
    out.push(' ');
    out.push_str(k.as_str());
    out.push('=');
}

fn kv_vec(k: DumpKey, v: Vec3, out: &mut String) {
    kv_key(k, out);
    fmt_f(v.x, out);
    out.push(',');
    fmt_f(v.y, out);
    out.push(',');
    fmt_f(v.z, out);
}

pub(crate) fn fmt_f(v: f32, out: &mut String) {
    if v == 0.0 {
        out.push('0');
        return;
    }
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    out.push_str(s);
}

pub(crate) fn fmt_color(c: [u8; 4], out: &mut String) {
    if c[3] == 255 {
        out.push_str(&format!("{:02x}{:02x}{:02x}", c[0], c[1], c[2]));
    } else {
        out.push_str(&format!("{:02x}{:02x}{:02x}{:02x}", c[0], c[1], c[2], c[3]));
    }
}

fn parse_line(line: &str) -> Result<(Node, bool), Error> {
    let tokens = tokenize(line)?;
    let mut words = Vec::new();
    let mut strings = Vec::new();
    for t in tokens {
        match t {
            Tok::Word(w) => words.push(w),
            Tok::Str(s) => strings.push(s),
        }
    }
    let mut parts = words.into_iter();
    let name = parts
        .next()
        .ok_or_else(|| Error::Parse("missing name".into()))?;
    let kind_tok = parts
        .next()
        .ok_or_else(|| Error::Parse("missing kind".into()))?;
    let of = KindOf::parse(&kind_tok)?;
    if of != KindOf::Label && !strings.is_empty() {
        return Err(Error::Parse("quoted string only on Label".into()));
    }
    if strings.len() > 1 {
        return Err(Error::Parse("multiple strings".into()));
    }

    let mut parent = None;
    let mut xf = Transform::IDENTITY;
    let mut tags = Vec::new();
    let mut size = None;
    let mut prim = None;
    let mut lamp_of = None;
    let mut color = [255u8, 255, 255, 255];
    let mut image = None;
    let mut image_back = None;
    let mut eye = Vec3::new(0.0, 1.5, 4.0);
    let mut at = Vec3::ZERO;
    let mut up = Vec3::Y;
    let mut fov = DEFAULT_FOV;
    let mut dir = Vec3::new(0.0, -1.0, -0.3);
    let mut dir_set = false;
    let mut intensity = 1.0;
    let mut range = DEFAULT_RANGE;
    let mut range_set = false;
    let mut camera_active = false;
    let mut geom_key = None;
    let mut slab_depth = DEFAULT_SLAB_DEPTH;
    let mut slab_radius = DEFAULT_SLAB_RADIUS;
    let mut depth_set = false;
    let mut radius_set = false;
    let mut shade = if of == KindOf::Label {
        Shade::Unlit
    } else {
        Shade::Lambert
    };
    let mut shade_set = false;
    let mut mat_key = None;
    let mut font_key = None;
    let mut label_size = None;
    let mut pose = None;
    let mut front = None;

    for tok in parts {
        if let Some((w, h)) = parse_size(&tok) {
            size = Some((w, h));
            continue;
        }
        if !tok.contains('=') {
            match of {
                KindOf::Mesh => {
                    prim = Some(crate::kind::Prim::parse(&tok)?);
                    continue;
                }
                KindOf::Light => {
                    lamp_of = Some(LampOf::parse(&tok)?);
                    continue;
                }
                _ => {
                    if crate::kind::Prim::parse(&tok).is_ok() {
                        return Err(Error::KindKey(of, "Box|Plane|Geom|Slab|Sphere"));
                    }
                    if LampOf::parse(&tok).is_ok() {
                        return Err(Error::KindKey(of, "Dir|Point"));
                    }
                    return Err(Error::Parse(format!("bad token '{tok}'")));
                }
            }
        }
        let (k, v) = tok
            .split_once('=')
            .ok_or_else(|| Error::Parse(format!("bad token '{tok}'")))?;
        match DumpKey::parse(k)? {
            DumpKey::Parent => parent = Some(v.to_string()),
            DumpKey::Translate => xf.t = parse_vec3(v)?,
            DumpKey::Rotate => xf.r = parse_vec3(v)?,
            DumpKey::Scale => xf.s = parse_vec3(v)?,
            DumpKey::Image => image = Some(v.to_string()),
            DumpKey::ImageBack => {
                if of != KindOf::Mesh {
                    return Err(Error::KindKey(of, "mb"));
                }
                image_back = Some(v.to_string());
            }
            DumpKey::Color => color = parse_color(v)?,
            DumpKey::Tag => tags.push(v.to_string()),
            DumpKey::Eye => eye = parse_vec3(v)?,
            DumpKey::At => at = parse_vec3(v)?,
            DumpKey::Up => up = parse_vec3(v)?,
            DumpKey::Fov => fov = parse_f(v)?,
            DumpKey::Dir => {
                dir = parse_vec3(v)?;
                dir_set = true;
            }
            DumpKey::Intensity => intensity = parse_f(v)?,
            DumpKey::Range => {
                range = parse_f(v)?;
                range_set = true;
            }
            DumpKey::Active => {
                if of != KindOf::Camera {
                    return Err(Error::KindKey(of, "active"));
                }
                if v != "1" {
                    return Err(Error::Parse(format!("active wants 1, got '{v}'")));
                }
                camera_active = true;
            }
            DumpKey::Geom => {
                if of != KindOf::Mesh {
                    return Err(Error::KindKey(of, "g"));
                }
                geom_key = Some(v.to_string());
            }
            DumpKey::Depth => {
                if of != KindOf::Mesh {
                    return Err(Error::KindKey(of, "d"));
                }
                slab_depth = parse_f(v)?;
                depth_set = true;
            }
            DumpKey::Radius => {
                if of != KindOf::Mesh {
                    return Err(Error::KindKey(of, "rad"));
                }
                slab_radius = parse_f(v)?;
                radius_set = true;
            }
            DumpKey::Shade => {
                if of != KindOf::Quad && of != KindOf::Mesh && of != KindOf::Label {
                    return Err(Error::KindKey(of, "sh"));
                }
                shade = Shade::parse(v)?;
                shade_set = true;
            }
            DumpKey::Mat => {
                if of != KindOf::Quad && of != KindOf::Mesh && of != KindOf::Label {
                    return Err(Error::KindKey(of, "mat"));
                }
                mat_key = Some(v.to_string());
            }
            DumpKey::Font => {
                if of != KindOf::Label {
                    return Err(Error::KindKey(of, "font"));
                }
                font_key = Some(v.to_string());
            }
            DumpKey::Size => {
                if of != KindOf::Label {
                    return Err(Error::KindKey(of, "size"));
                }
                label_size = Some(parse_f(v)?);
            }
            DumpKey::Pose => {
                if of != KindOf::Mesh {
                    return Err(Error::KindKey(of, "pose"));
                }
                let (clip, t) = v
                    .split_once('@')
                    .filter(|(c, _)| !c.is_empty())
                    .ok_or_else(|| Error::Parse(format!("pose is CLIP@SECONDS, got '{v}'")))?;
                pose = Some(crate::kind::Pose::new(clip, parse_f(t)?));
            }
            DumpKey::Front => {
                if of != KindOf::Mesh {
                    return Err(Error::KindKey(of, "front"));
                }
                front = Some(crate::kind::Front::parse(v).ok_or_else(|| {
                    Error::Parse(format!("front is +x, -x, +z or -z, got '{v}'"))
                })?);
            }
        }
    }

    let kind = match of {
        KindOf::Group => {
            if size.is_some() {
                return Err(Error::KindKey(KindOf::Group, "WxH"));
            }
            Kind::Group
        }
        KindOf::Quad => {
            let (w, h) = size.ok_or(Error::QuadNeedsSize)?;
            Kind::Quad {
                w,
                h,
                bind: surface_bind(mat_key, shade, shade_set, color, image, image_back, false)?,
            }
        }
        KindOf::Camera => Kind::Camera { eye, at, up, fov },
        KindOf::Light => {
            if size.is_some() {
                return Err(Error::KindKey(KindOf::Light, "WxH"));
            }
            let lamp_of = lamp_of.ok_or(Error::LightNeedsLamp)?;
            let lamp = match lamp_of {
                LampOf::Dir => {
                    if range_set {
                        return Err(Error::KindKey(KindOf::Light, "range"));
                    }
                    Lamp::Dir { dir }
                }
                LampOf::Point => {
                    if dir_set {
                        return Err(Error::KindKey(KindOf::Light, "dir"));
                    }
                    Lamp::Point { range }
                }
            };
            Kind::Light {
                lamp,
                color,
                intensity,
            }
        }
        KindOf::Mesh => {
            let prim = prim.ok_or(Error::MeshNeedsPrim)?;
            match prim {
                Prim::Geom => {
                    if size.is_some() {
                        return Err(Error::KindKey(KindOf::Mesh, "WxH"));
                    }
                    if depth_set {
                        return Err(Error::KindKey(KindOf::Mesh, "d"));
                    }
                    if radius_set {
                        return Err(Error::KindKey(KindOf::Mesh, "rad"));
                    }
                    let geom = geom_key.ok_or(Error::MeshNeedsGeom)?;
                    Kind::Mesh {
                        prim,
                        geom: Some(geom),
                        bind: mesh_bind(mat_key, shade, shade_set, color, image, image_back)?,
                        pose,
                        front,
                    }
                }
                _ if pose.is_some() => return Err(Error::KindKey(KindOf::Mesh, "pose")),
                _ if front.is_some() => return Err(Error::KindKey(KindOf::Mesh, "front")),
                Prim::Box | Prim::Plane | Prim::Sphere => {
                    if size.is_some() {
                        return Err(Error::KindKey(KindOf::Mesh, "WxH"));
                    }
                    if geom_key.is_some() {
                        return Err(Error::KindKey(KindOf::Mesh, "g"));
                    }
                    if depth_set {
                        return Err(Error::KindKey(KindOf::Mesh, "d"));
                    }
                    if radius_set {
                        return Err(Error::KindKey(KindOf::Mesh, "rad"));
                    }
                    Kind::Mesh {
                        prim,
                        geom: None,
                        bind: mesh_bind(mat_key, shade, shade_set, color, image, image_back)?,
                        pose: None,
                        front: None,
                    }
                }
                Prim::Slab { .. } => {
                    let (w, h) = size.ok_or(Error::SlabNeedsSize)?;
                    if geom_key.is_some() {
                        return Err(Error::KindKey(KindOf::Mesh, "g"));
                    }
                    Kind::Mesh {
                        prim: Prim::Slab {
                            w,
                            h,
                            depth: slab_depth,
                            radius: slab_radius,
                        },
                        geom: None,
                        bind: mesh_bind(mat_key, shade, shade_set, color, image, image_back)?,
                        pose: None,
                        front: None,
                    }
                }
            }
        }
        KindOf::Label => {
            if size.is_some() {
                return Err(Error::KindKey(KindOf::Label, "WxH"));
            }
            let font = font_key.ok_or(Error::LabelNeedsFont)?;
            let em = label_size.ok_or(Error::LabelNeedsSize)?;
            let text = strings.into_iter().next().ok_or(Error::LabelNeedsText)?;
            Kind::Label {
                font,
                size: em,
                text,
                bind: surface_bind(mat_key, shade, shade_set, color, image, image_back, false)?,
            }
        }
    };

    Ok((
        Node {
            name: name.to_string(),
            parent,
            tags,
            xf,
            kind,
        },
        camera_active,
    ))
}

fn parse_size(tok: &str) -> Option<(f32, f32)> {
    let (a, b) = tok.split_once('x')?;
    if a.is_empty() || a.contains('=') {
        return None;
    }
    Some((a.parse().ok()?, b.parse().ok()?))
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

pub(crate) fn parse_f(s: &str) -> Result<f32, Error> {
    s.parse()
        .map_err(|_| Error::Parse(format!("bad float '{s}'")))
}

pub(crate) fn parse_color(s: &str) -> Result<[u8; 4], Error> {
    let hex = s.strip_prefix('#').unwrap_or(s);
    match hex.len() {
        6 => {
            let n = u32::from_str_radix(hex, 16)
                .map_err(|_| Error::Parse(format!("bad color '{s}'")))?;
            Ok([
                ((n >> 16) & 0xff) as u8,
                ((n >> 8) & 0xff) as u8,
                (n & 0xff) as u8,
                255,
            ])
        }
        8 => {
            let n = u32::from_str_radix(hex, 16)
                .map_err(|_| Error::Parse(format!("bad color '{s}'")))?;
            Ok([
                ((n >> 24) & 0xff) as u8,
                ((n >> 16) & 0xff) as u8,
                ((n >> 8) & 0xff) as u8,
                (n & 0xff) as u8,
            ])
        }
        _ => Err(Error::Parse(format!("bad color '{s}'"))),
    }
}

fn mesh_bind(
    mat_key: Option<String>,
    shade: Shade,
    shade_set: bool,
    color: [u8; 4],
    image: Option<String>,
    image_back: Option<String>,
) -> Result<SurfaceBind, Error> {
    surface_bind(mat_key, shade, shade_set, color, image, image_back, true)
}

fn surface_bind(
    mat_key: Option<String>,
    shade: Shade,
    shade_set: bool,
    color: [u8; 4],
    image: Option<String>,
    image_back: Option<String>,
    allow_back: bool,
) -> Result<SurfaceBind, Error> {
    if !allow_back && image_back.is_some() {
        return Err(Error::KindKey(KindOf::Quad, "mb"));
    }
    if let Some(k) = mat_key {
        let mixed =
            shade_set || color != [255, 255, 255, 255] || image.is_some() || image_back.is_some();
        if mixed {
            return Err(Error::MatInline);
        }
        return Ok(SurfaceBind::Name(k));
    }
    Ok(SurfaceBind::Inline(Surface {
        shade,
        color,
        image,
        image_back,
    }))
}

impl Materials {
    pub fn dump(&self) -> String {
        let mut out = String::from(MAT_VERSION);
        out.push('\n');
        for name in self.names_sorted() {
            let s = &self.map()[name];
            out.push_str(name);
            if s.shade != Shade::Lambert {
                out.push(' ');
                out.push_str(s.shade.as_str());
            }
            if s.color != [255, 255, 255, 255] {
                out.push_str(" c=");
                fmt_color(s.color, &mut out);
            }
            if let Some(m) = &s.image {
                out.push_str(" m=");
                out.push_str(m);
            }
            if let Some(m) = &s.image_back {
                out.push_str(" mb=");
                out.push_str(m);
            }
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
        if ver != MAT_VERSION {
            return Err(Error::Parse(format!("expected {MAT_VERSION}, got {ver}")));
        }
        let mut mats = Materials::new();
        for (i, line) in lines.enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (name, surface) = parse_material_line(line).map_err(|e| match e {
                Error::Parse(m) => Error::Parse(format!("line {}: {m}", i + 2)),
                other => other,
            })?;
            mats.insert(name, surface)?;
        }
        Ok(mats)
    }
}

fn parse_material_line(line: &str) -> Result<(String, Surface), Error> {
    let mut parts = line.split_whitespace();
    let name = parts
        .next()
        .ok_or_else(|| Error::Parse("missing material name".into()))?;
    let mut surface = Surface::DEFAULT;
    for tok in parts {
        if !tok.contains('=') {
            surface.shade = Shade::parse(tok)?;
            continue;
        }
        let (k, v) = tok
            .split_once('=')
            .ok_or_else(|| Error::Parse(format!("bad token '{tok}'")))?;
        match k {
            "c" => surface.color = parse_color(v)?,
            "m" => surface.image = Some(v.to_string()),
            "mb" => surface.image_back = Some(v.to_string()),
            other => return Err(Error::UnknownKey(other.to_string())),
        }
    }
    Ok((name.to_string(), surface))
}
