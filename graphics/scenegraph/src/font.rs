//! Interned faces. The graph stores a font *key*; outlines and the MSDF atlas live here.

use std::collections::HashMap;

use fdsm::bezier::scanline::FillRule;
use fdsm::generate::generate_msdf;
use fdsm::render::correct_sign_msdf;
use fdsm::shape::Shape;
use fdsm::transform::Transform;
use image::RgbImage;
use nalgebra::{Affine2, Similarity2, Vector2};

use crate::error::Error;
use crate::images::RgbaImage;
use crate::math::compact_f;

/// Noto Sans Bold, the stock face (`font=ibm`) a dump folder can always
/// name, and the face every test lays text out with. SIL Open Font
/// License 1.1: `assets/NotoSans-OFL.txt`.
pub const NOTO_SANS_BOLD: &[u8] = include_bytes!("../assets/NotoSans-Bold.ttf");

const ATLAS: u32 = 1024;
const PX: f32 = 64.0;
const RANGE: f64 = 4.0;
const GAP: u32 = 1;

#[derive(Clone, Debug)]
pub struct LayoutGlyph {
    pub atlas: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
}

#[derive(Clone, Debug)]
pub struct Layout {
    pub w: f32,
    pub h: f32,
    pub glyphs: Vec<LayoutGlyph>,
    pub missing: Vec<u32>,
}

impl Layout {
    /// Agent view of a layout — glyph count, bbox, missing codepoints.
    pub fn dump(&self, font: &str, size: f32, text: &str) -> String {
        let mut out = String::from("layout\n");
        out.push_str(font);
        out.push_str(" size=");
        out.push_str(&compact_f(size));
        out.push(' ');
        out.push('"');
        for c in text.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                _ => out.push(c),
            }
        }
        out.push('"');
        out.push('\n');
        out.push_str("glyphs=");
        out.push_str(&self.glyphs.len().to_string());
        out.push_str(" bbox=");
        out.push_str(&compact_f(self.w));
        out.push('x');
        out.push_str(&compact_f(self.h));
        if !self.missing.is_empty() {
            out.push_str(" missing=");
            for (i, cp) in self.missing.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&format!("U+{cp:04X}"));
            }
        }
        out.push('\n');
        out
    }
}

#[derive(Clone, Copy)]
struct Packed {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    xmin: f32,
    ymin: f32,
    advance: f32,
}

struct Face {
    ttf: Vec<u8>,
    fallbacks: Vec<String>,
    atlas: Vec<u8>,
    pack_x: u32,
    pack_y: u32,
    pack_h: u32,
    glyphs: HashMap<u16, Packed>,
    /// `glyph_index` for this face. Avoids `Face::parse` per character.
    cmap: HashMap<char, Option<u16>>,
}

pub struct Fonts {
    faces: HashMap<String, Face>,
}

impl Default for Fonts {
    fn default() -> Self {
        Self::new()
    }
}

impl Fonts {
    pub fn new() -> Self {
        Self {
            faces: HashMap::new(),
        }
    }

    pub fn insert(
        &mut self,
        key: impl Into<String>,
        bytes: &[u8],
        fallbacks: Vec<String>,
    ) -> Result<(), Error> {
        let key = key.into();
        if key.is_empty() {
            return Err(Error::BadFont(key));
        }
        ttf_parser::Face::parse(bytes, 0).map_err(|_| Error::BadFont(key.clone()))?;
        self.faces.insert(
            key,
            Face {
                ttf: bytes.to_vec(),
                fallbacks,
                atlas: vec![0u8; (ATLAS * ATLAS * 4) as usize],
                pack_x: GAP,
                pack_y: GAP,
                pack_h: 0,
                glyphs: HashMap::new(),
                cmap: HashMap::new(),
            },
        );
        Ok(())
    }

    pub fn atlas(&self, key: &str) -> Option<RgbaImage> {
        let f = self.faces.get(key)?;
        Some(RgbaImage {
            width: ATLAS,
            height: ATLAS,
            pixels: f.atlas.clone(),
        })
    }

    pub fn glyph_count(&self, key: &str) -> Option<usize> {
        self.faces.get(key).map(|f| f.glyphs.len())
    }

    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    pub fn dump(&self) -> String {
        let mut names: Vec<_> = self.faces.keys().collect();
        names.sort();
        let mut out = String::from("fonts\n");
        for n in names {
            let f = &self.faces[n];
            out.push_str(n);
            out.push_str(&format!(" glyphs={} atlas={ATLAS}x{ATLAS}", f.glyphs.len()));
            if !f.fallbacks.is_empty() {
                out.push_str(" fb=");
                out.push_str(&f.fallbacks.join(","));
            }
            out.push('\n');
        }
        out
    }

    pub fn layout(&mut self, font: &str, size: f32, text: &str) -> Result<Layout, Error> {
        if !self.faces.contains_key(font) {
            return Err(Error::UnknownFont(font.to_string()));
        }
        let px = PX;
        let scale = size / px;
        let mut missing = Vec::new();
        let mut glyphs = Vec::new();
        let mut pen_x = 0.0f32;
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        for ch in text.chars() {
            if ch == '\n' {
                continue;
            }
            let (face_key, gid, is_missing) = self.lookup(font, ch);
            if is_missing {
                missing.push(ch as u32);
            }
            self.ensure_packed(&face_key, gid)?;
            let packed = *self.faces.get(&face_key).unwrap().glyphs.get(&gid).unwrap();
            let w = packed.w as f32 * scale;
            let h = packed.h as f32 * scale;
            let x = pen_x + packed.xmin * scale;
            let y = packed.ymin * scale;
            if w > 0.0 && h > 0.0 {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x + w);
                max_y = max_y.max(y + h);
                let u0 = packed.x as f32 / ATLAS as f32;
                let v0 = packed.y as f32 / ATLAS as f32;
                let u1 = (packed.x + packed.w) as f32 / ATLAS as f32;
                let v1 = (packed.y + packed.h) as f32 / ATLAS as f32;
                glyphs.push(LayoutGlyph {
                    atlas: face_key,
                    x,
                    y,
                    w,
                    h,
                    u0,
                    v0,
                    u1,
                    v1,
                });
            }
            pen_x += packed.advance * scale;
        }
        if glyphs.is_empty() {
            return Ok(Layout {
                w: 0.0,
                h: 0.0,
                glyphs,
                missing,
            });
        }
        let w = (max_x - min_x).max(0.0);
        let h = (max_y - min_y).max(0.0);
        for g in &mut glyphs {
            g.x -= min_x + w * 0.5;
            g.y -= min_y + h * 0.5;
        }
        Ok(Layout {
            w,
            h,
            glyphs,
            missing,
        })
    }

    fn lookup(&mut self, primary: &str, ch: char) -> (String, u16, bool) {
        if let Some(gid) = self.glyph_of(primary, ch) {
            return (primary.to_string(), gid, false);
        }
        let fbs = self
            .faces
            .get(primary)
            .map(|f| f.fallbacks.clone())
            .unwrap_or_default();
        for fb in fbs {
            if let Some(gid) = self.glyph_of(&fb, ch) {
                return (fb, gid, false);
            }
        }
        (primary.to_string(), 0, true)
    }

    fn glyph_of(&mut self, key: &str, ch: char) -> Option<u16> {
        let face = self.faces.get_mut(key)?;
        if let Some(&cached) = face.cmap.get(&ch) {
            return cached;
        }
        let gid = ttf_parser::Face::parse(&face.ttf, 0)
            .ok()
            .and_then(|f| f.glyph_index(ch).map(|g| g.0));
        face.cmap.insert(ch, gid);
        gid
    }

    fn ensure_packed(&mut self, face_key: &str, gid: u16) -> Result<(), Error> {
        if self
            .faces
            .get(face_key)
            .is_some_and(|f| f.glyphs.contains_key(&gid))
        {
            return Ok(());
        }
        let baked = {
            let face = self
                .faces
                .get(face_key)
                .ok_or_else(|| Error::UnknownFont(face_key.to_string()))?;
            bake_msdf(&face.ttf, gid).map_err(|_| Error::BadFont(face_key.to_string()))?
        };
        let face = self
            .faces
            .get_mut(face_key)
            .ok_or_else(|| Error::UnknownFont(face_key.to_string()))?;
        let Some(img) = baked.img else {
            face.glyphs.insert(
                gid,
                Packed {
                    x: 0,
                    y: 0,
                    w: 0,
                    h: 0,
                    xmin: 0.0,
                    ymin: 0.0,
                    advance: baked.advance,
                },
            );
            return Ok(());
        };
        let tw = baked.w;
        let th = baked.h;
        if tw > ATLAS || th > ATLAS {
            return Err(Error::FontAtlasFull(face_key.to_string()));
        }
        if face.pack_x + tw + GAP > ATLAS {
            face.pack_y += face.pack_h + GAP;
            face.pack_x = GAP;
            face.pack_h = 0;
        }
        if face.pack_y + th + GAP > ATLAS {
            return Err(Error::FontAtlasFull(face_key.to_string()));
        }
        let px = face.pack_x;
        let py = face.pack_y;
        face.pack_x += tw + GAP;
        face.pack_h = face.pack_h.max(th);
        blit_msdf(&mut face.atlas, px, py, &img);
        face.glyphs.insert(
            gid,
            Packed {
                x: px,
                y: py,
                w: tw,
                h: th,
                xmin: baked.xmin,
                ymin: baked.ymin,
                advance: baked.advance,
            },
        );
        Ok(())
    }
}

struct Baked {
    img: Option<RgbImage>,
    xmin: f32,
    ymin: f32,
    w: u32,
    h: u32,
    advance: f32,
}

fn bake_msdf(ttf: &[u8], gid: u16) -> Result<Baked, ()> {
    let face = ttf_parser::Face::parse(ttf, 0).map_err(|_| ())?;
    let gid = ttf_parser::GlyphId(gid);
    let upem = f32::from(face.units_per_em());
    let advance = f32::from(face.glyph_hor_advance(gid).unwrap_or(0)) / upem * PX;
    let Some(bbox) = face.glyph_bounding_box(gid) else {
        return Ok(Baked {
            img: None,
            xmin: 0.0,
            ymin: 0.0,
            w: 0,
            h: 0,
            advance,
        });
    };
    let Some(mut shape) = fdsm_ttf_parser::load_shape_from_face(&face, gid) else {
        return Ok(Baked {
            img: None,
            xmin: 0.0,
            ymin: 0.0,
            w: 0,
            h: 0,
            advance,
        });
    };
    let shrinkage = f64::from(upem) / f64::from(PX);
    let width = ((f64::from(bbox.x_max - bbox.x_min) / shrinkage) + 2.0 * RANGE).ceil() as u32;
    let height = ((f64::from(bbox.y_max - bbox.y_min) / shrinkage) + 2.0 * RANGE).ceil() as u32;
    if width == 0 || height == 0 {
        return Ok(Baked {
            img: None,
            xmin: 0.0,
            ymin: 0.0,
            w: 0,
            h: 0,
            advance,
        });
    }
    let transformation = nalgebra::convert::<_, Affine2<f64>>(Similarity2::new(
        Vector2::new(
            RANGE - f64::from(bbox.x_min) / shrinkage,
            RANGE - f64::from(bbox.y_min) / shrinkage,
        ),
        0.0,
        1.0 / shrinkage,
    ));
    shape.transform(&transformation);
    let colored = Shape::edge_coloring_simple(shape, 0.03, 1);
    let prepared = colored.prepare();
    let mut msdf = RgbImage::new(width, height);
    generate_msdf(&prepared, RANGE, &mut msdf);
    correct_sign_msdf(&mut msdf, &prepared, FillRule::Nonzero);
    Ok(Baked {
        img: Some(msdf),
        xmin: f32::from(bbox.x_min) / shrinkage as f32 - RANGE as f32,
        ymin: f32::from(bbox.y_min) / shrinkage as f32 - RANGE as f32,
        w: width,
        h: height,
        advance,
    })
}

/// fdsm writes y-up (row 0 = glyph bottom). Flip so atlas v=0 is the top.
fn blit_msdf(atlas: &mut [u8], px: u32, py: u32, msdf: &RgbImage) {
    let (tw, th) = (msdf.width(), msdf.height());
    for y in 0..th {
        let src_y = th - 1 - y;
        for x in 0..tw {
            let p = msdf.get_pixel(x, src_y).0;
            let i = (((py + y) * ATLAS + (px + x)) * 4) as usize;
            atlas[i] = p[0];
            atlas[i + 1] = p[1];
            atlas[i + 2] = p[2];
            atlas[i + 3] = 255;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTO: &[u8] = NOTO_SANS_BOLD;

    fn noto() -> Fonts {
        let mut f = Fonts::new();
        f.insert("ibm", NOTO, Vec::new()).unwrap();
        f
    }

    #[test]
    fn layout_shredder() {
        let mut f = noto();
        let l = f.layout("ibm", 0.18, "Shredder").unwrap();
        assert!(l.glyphs.len() >= 7, "{}", l.glyphs.len());
        assert!(l.missing.is_empty(), "{:?}", l.missing);
        assert!(l.w > 0.4 && l.h > 0.1, "bbox {}x{}", l.w, l.h);
        let d = l.dump("ibm", 0.18, "Shredder");
        assert!(d.contains("glyphs="), "{d}");
        assert!(!d.contains("missing="), "{d}");
        let again = f.layout("ibm", 0.18, "Shredder").unwrap();
        assert_eq!(again.glyphs.len(), l.glyphs.len());
        assert!((again.w - l.w).abs() < 1e-5 && (again.h - l.h).abs() < 1e-5);
    }

    #[test]
    fn unicode_covered_has_no_missing() {
        let mut f = noto();
        let l = f.layout("ibm", 0.18, "café Δ Ж").unwrap();
        assert!(l.missing.is_empty(), "{:?}", l.missing);
        assert_eq!(l.glyphs.len(), 6, "{}", l.glyphs.len());
        let d = l.dump("ibm", 0.18, "café Δ Ж");
        assert!(!d.contains("missing="), "{d}");
    }

    #[test]
    fn missing_is_dumped() {
        let mut f = noto();
        let l = f.layout("ibm", 0.18, "HP ☠ 12").unwrap();
        let d = l.dump("ibm", 0.18, "HP ☠ 12");
        assert!(
            l.missing.contains(&0x2620u32) && d.contains("missing=U+2620"),
            "missing={:?} dump={d}",
            l.missing
        );
        assert!(!l.glyphs.is_empty());
    }

    #[test]
    fn fonts_dump_lists_fallbacks() {
        let mut f = Fonts::new();
        f.insert("ibm", NOTO, vec!["sym".into()]).unwrap();
        let d = f.dump();
        assert!(d.starts_with("fonts\n"), "{d}");
        assert!(d.contains("ibm glyphs=0 atlas=1024x1024 fb=sym"), "{d}");
    }

    #[test]
    fn unknown_font() {
        let mut f = Fonts::new();
        match f.layout("nope", 0.2, "A") {
            Err(Error::UnknownFont(k)) => assert_eq!(k, "nope"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn atlas_is_msdf() {
        let mut f = noto();
        f.layout("ibm", 0.18, "A").unwrap();
        let img = f.atlas("ibm").unwrap();
        let varied = img
            .pixels
            .chunks(4)
            .filter(|p| p[0].abs_diff(p[1]) > 8 || p[1].abs_diff(p[2]) > 8)
            .count();
        assert!(
            varied > 20,
            "expected multi-channel distances, varied={varied}"
        );
    }
}
