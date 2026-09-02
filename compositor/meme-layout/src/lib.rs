//! Meme template box definitions, and the CPU compositor that fills them.
//!
//! A [`TemplateSpec`] says where text goes on a template image and what each
//! area *means*; [`render`] turns a spec plus some text into a picture. Both
//! halves live here on purpose: the editor previews with this code and the
//! service renders with this code, so what someone places is what gets posted.
//! A second compositor anywhere would drift silently, and "silently" is the
//! problem — nobody notices a caption two pixels off until it is in a channel.
//!
//! ## Boxes, not captions
//!
//! The thing this replaces is a hardcoded top/bottom captioner, which is why
//! meme catalogues get filtered to two-box templates: a four-panel image
//! captioned top-and-bottom is recognisably wrong and nothing can detect it.
//! A [`TemplateBox`] is a *region* with wrapping and shrink-to-fit, so a panel
//! is expressible and Drake renders correctly.
//!
//! ## The role is the point
//!
//! Public catalogues expose a box *count* and nothing else, so a model
//! captioning Drake has no idea panel one is the thing being rejected.
//! [`TemplateBox::role`] carries that in words, which is what lets a tool
//! schema describe itself to a caller that has never seen the picture.
//!
//! ## Datum-shaped
//!
//! The encoding is deliberately small, versioned and deterministic — struct
//! field order is the serialisation order, and defaults are skipped — so a
//! spec can later become a CIP-68 datum without a schema rewrite. Nothing here
//! knows about chains; it just declines to make that impossible.
//!
//! Pure: `image` (in-memory `RgbaImage`, no codecs), `serde` and `ab_glyph`.
//! No I/O, no egui, no worker bindings — the consumers are a wasm editor and a
//! CPU worker.

use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

mod layout;

pub use layout::{FittedText, fit_text};

/// Bumped when a stored spec would be read wrongly by old code.
///
/// Carried in the encoding rather than inferred, because the first consumer of
/// an old spec will be a renderer that has to decide whether it can be trusted.
pub const SCHEMA_VERSION: u16 = 1;

// ─── Geometry ────────────────────────────────────────────────────────────────

/// A rectangle in **normalized** image space: `0.0..1.0` of width and height.
///
/// Normalized rather than pixels so a spec survives the image being re-encoded,
/// resized or served at a different derivative — which it will be, because the
/// editor works on a scaled preview and the renderer works on the original.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Smallest side [`Rect::sanitized`] will produce. A zero-width box would make
/// the fit search run to its floor on every candidate size and draw nothing.
const MIN_EXTENT: f32 = 0.01;

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// Project onto a concrete image size.
    pub fn to_pixels(self, width: u32, height: u32) -> PixelRect {
        PixelRect {
            x: self.x * width as f32,
            y: self.y * height as f32,
            w: self.w * width as f32,
            h: self.h * height as f32,
        }
    }

    /// Clamp into the image and give it a non-zero extent.
    ///
    /// A box dragged past the edge in an editor is a normal thing to do, and a
    /// zero-width one would make the fit search never terminate.
    pub fn sanitized(self) -> Self {
        // Capped below 1.0 rather than at it: an origin *on* the far edge
        // leaves no room for the minimum extent, and `clamp(0.01, 0.0)` is a
        // panic, not a small rectangle.
        let x = self.x.clamp(0.0, 1.0 - MIN_EXTENT);
        let y = self.y.clamp(0.0, 1.0 - MIN_EXTENT);
        // …and the remaining room is floored again, because the subtraction
        // does not round the way the cap above implies: `1.0f32 - 0.99f32` is
        // 0.00999999, just under MIN_EXTENT, which is another `min > max`.
        Self {
            x,
            y,
            w: self.w.clamp(MIN_EXTENT, (1.0 - x).max(MIN_EXTENT)),
            h: self.h.clamp(MIN_EXTENT, (1.0 - y).max(MIN_EXTENT)),
        }
    }
}

/// A [`Rect`] resolved against a real image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VAlign {
    Top,
    #[default]
    Middle,
    Bottom,
}

// ─── Style ───────────────────────────────────────────────────────────────────

/// Which font a box wants, named rather than embedded.
///
/// A datum carries a *choice*, never font bytes; the caller supplies the faces
/// in [`Fonts`]. Two roles rather than a font name because that is the real
/// distinction on a meme: white-on-picture caption lettering, or ordinary text
/// sitting inside a panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FontRole {
    /// Heavy condensed display face — Impact and its free equivalents.
    #[default]
    Display,
    /// Ordinary body text, for panel captions that are not shouting.
    Body,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Outline {
    pub color: [u8; 4],
    /// Thickness as a fraction of font size, so it scales with the text.
    pub width: f32,
}

impl Default for Outline {
    fn default() -> Self {
        Self {
            color: [0, 0, 0, 255],
            width: 0.08,
        }
    }
}

/// How the text in a box is drawn.
///
/// # Every skipped field defaults to the value that means "absent"
///
/// Load-bearing, and easy to get wrong: `skip_serializing_if` plus a
/// `#[serde(default)]` that disagrees with it *loses data silently*. An
/// `outline: None` that is skipped on write and defaults to `Some` on read
/// comes back as a different style, and nobody notices until a panel caption
/// renders with a black halo. So the derived default here is the plain,
/// unstyled case, and the interesting registers are constructors
/// ([`Self::caption`], [`Self::panel`]) that encode their fields explicitly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BoxStyle {
    pub font: FontRole,
    /// Always encoded. It could be skipped when white, but then the derived
    /// default would have to be white *and* the skip predicate would have to
    /// agree — and a colour that defaults to transparent draws nothing at all,
    /// which is the worst way to discover the rule above.
    pub color: [u8; 4],
    /// `None` is plain text — a panel caption. `Some` is the classic
    /// white-with-black-edges look that survives being drawn over a photo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outline: Option<Outline>,
    /// Extra tracking as a fraction of font size.
    #[serde(skip_serializing_if = "is_zero")]
    pub letter_spacing: f32,
    /// Upper-case the text when drawing. A *presentation* choice — the text is
    /// stored as the user wrote it.
    #[serde(skip_serializing_if = "is_false")]
    pub uppercase: bool,
}

impl Default for BoxStyle {
    fn default() -> Self {
        Self {
            font: FontRole::Display,
            // Visible on a photograph, unlike the derived all-zero default.
            color: [255, 255, 255, 255],
            outline: None,
            letter_spacing: 0.0,
            uppercase: false,
        }
    }
}

impl BoxStyle {
    /// The classic look: white, shouted, with a black edge so it survives being
    /// drawn over a photograph.
    pub fn caption() -> Self {
        Self {
            font: FontRole::Display,
            color: [255, 255, 255, 255],
            outline: Some(Outline::default()),
            letter_spacing: 0.0,
            uppercase: true,
        }
    }

    /// Plain dark text with no outline, for text that sits *inside* a panel
    /// rather than over a picture — Drake's right-hand side, Expanding Brain.
    pub fn panel() -> Self {
        Self {
            font: FontRole::Body,
            color: [16, 16, 16, 255],
            outline: None,
            letter_spacing: 0.0,
            uppercase: false,
        }
    }
}

fn is_zero(value: &f32) -> bool {
    *value == 0.0
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Bounds on the search [`fit_text`] runs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fit {
    /// Largest font size to try, as a fraction of **image height**.
    pub max: f32,
    /// Smallest size worth drawing, same units. Below this the text is
    /// unreadable and overflowing is the more honest failure.
    pub min: f32,
    /// Hard cap on wrapped lines. Zero means "whatever fits the height".
    #[serde(skip_serializing_if = "is_zero_u8")]
    pub max_lines: u8,
}

impl Default for Fit {
    fn default() -> Self {
        Self {
            max: 0.16,
            min: 0.03,
            max_lines: 0,
        }
    }
}

fn is_zero_u8(value: &u8) -> bool {
    *value == 0
}

// ─── The spec ────────────────────────────────────────────────────────────────

/// One place text goes, and what it is for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateBox {
    /// Stable, model-facing name — `top`, `bottom`, `rejects`, `prefers`.
    /// Referenced by a fill, so renaming one breaks stored requests.
    pub id: String,
    /// What this area is *for*, in words, for whoever (or whatever) is writing
    /// the text. "The thing being rejected" is the difference between a
    /// rendered Drake and a correct one.
    pub role: String,
    pub rect: Rect,
    #[serde(default, skip_serializing_if = "is_default_align")]
    pub align: Align,
    #[serde(default, skip_serializing_if = "is_default_valign")]
    pub valign: VAlign,
    #[serde(default)]
    pub style: BoxStyle,
    #[serde(default)]
    pub fit: Fit,
    /// Whether the template reads as broken with this box empty.
    #[serde(default, skip_serializing_if = "is_false")]
    pub required: bool,
}

fn is_default_align(align: &Align) -> bool {
    *align == Align::default()
}

fn is_default_valign(valign: &VAlign) -> bool {
    *valign == VAlign::default()
}

/// The image a spec describes.
///
/// The digest is what lets a renderer confirm it is drawing on the picture the
/// boxes were placed against. Boxes are normalized, so a *resized* image is
/// still correct — but a *replaced* one silently puts text in the wrong place,
/// and that is the case this catches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageRef {
    /// Where the bytes live, in whatever namespace the caller uses.
    pub key: String,
    /// Lower-case hex digest of the image bytes. Algorithm is the caller's
    /// choice; this crate only compares.
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub digest: String,
}

/// A template: an image, and where the words go.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateSpec {
    /// See [`SCHEMA_VERSION`].
    pub schema: u16,
    /// Slug. What a caller types to ask for this template.
    pub name: String,
    /// Human title, for saying what was made.
    pub title: String,
    pub image: ImageRef,
    pub boxes: Vec<TemplateBox>,
}

impl TemplateSpec {
    /// The default two-box top/bottom captioner.
    ///
    /// Every imported template gets this without anyone authoring anything, so
    /// a catalogue import is immediately renderable and hand-placed boxes are
    /// an *improvement* rather than a prerequisite. It is also exactly what the
    /// old hardcoded captioner did, which is what makes adopting this a no-op
    /// for existing templates.
    pub fn classic(
        name: impl Into<String>,
        title: impl Into<String>,
        key: impl Into<String>,
    ) -> Self {
        Self {
            schema: SCHEMA_VERSION,
            name: name.into(),
            title: title.into(),
            image: ImageRef {
                key: key.into(),
                digest: String::new(),
            },
            boxes: vec![
                TemplateBox {
                    id: "top".to_string(),
                    role: "The set-up line, across the top".to_string(),
                    rect: Rect::new(0.04, 0.02, 0.92, 0.28),
                    align: Align::Center,
                    valign: VAlign::Top,
                    style: BoxStyle::caption(),
                    fit: Fit::default(),
                    required: false,
                },
                TemplateBox {
                    id: "bottom".to_string(),
                    role: "The punchline, across the bottom".to_string(),
                    rect: Rect::new(0.04, 0.70, 0.92, 0.28),
                    align: Align::Center,
                    valign: VAlign::Bottom,
                    style: BoxStyle::caption(),
                    fit: Fit::default(),
                    required: false,
                },
            ],
        }
    }

    pub fn box_by_id(&self, id: &str) -> Option<&TemplateBox> {
        self.boxes.iter().find(|b| b.id == id)
    }

    /// Ids that must be filled for this template to read correctly.
    pub fn required_ids(&self) -> Vec<&str> {
        self.boxes
            .iter()
            .filter(|b| b.required)
            .map(|b| b.id.as_str())
            .collect()
    }

    /// Problems that would make this spec render wrongly.
    ///
    /// Checked rather than trusted because a spec can arrive from an editor, a
    /// hand-written file, or one day a datum — and the failure it prevents
    /// (duplicate ids silently shadowing each other) is invisible in the output.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut problems = Vec::new();

        if self.schema > SCHEMA_VERSION {
            problems.push(format!(
                "spec is schema {} but this build understands {SCHEMA_VERSION}",
                self.schema
            ));
        }
        if self.name.trim().is_empty() {
            problems.push("template has no name".to_string());
        }
        if self.boxes.is_empty() {
            problems.push("template has no boxes, so nothing can be written on it".to_string());
        }

        for (n, area) in self.boxes.iter().enumerate() {
            if area.id.trim().is_empty() {
                problems.push(format!("box {n} has no id"));
            }
            if self.boxes.iter().filter(|b| b.id == area.id).count() > 1 {
                problems.push(format!("more than one box is called \"{}\"", area.id));
            }
            if area.fit.min > area.fit.max {
                problems.push(format!(
                    "box \"{}\" has a minimum size above its maximum",
                    area.id
                ));
            }
        }

        problems.sort();
        problems.dedup();

        if problems.is_empty() {
            Ok(())
        } else {
            Err(problems)
        }
    }
}

// ─── Rendering ───────────────────────────────────────────────────────────────

/// The faces a render draws with, supplied by the caller.
///
/// Not embedded in this crate: fonts are large, licensing varies, and a worker
/// already has to load them from somewhere. [`FontRole`] is the indirection.
pub struct Fonts {
    pub display: ab_glyph::FontArc,
    pub body: ab_glyph::FontArc,
}

impl Fonts {
    /// Parse both faces from their bytes.
    ///
    /// Exists so a consumer never has to name `ab_glyph` — the crate is an
    /// implementation detail of the compositor, and a worker that had to add it
    /// as a direct dependency could drift to a different major and stop being
    /// able to build a `Fonts` at all.
    pub fn new(display: &[u8], body: &[u8]) -> Result<Self, String> {
        Ok(Self {
            display: ab_glyph::FontArc::try_from_vec(display.to_vec())
                .map_err(|e| format!("display font failed to parse: {e}"))?,
            body: ab_glyph::FontArc::try_from_vec(body.to_vec())
                .map_err(|e| format!("body font failed to parse: {e}"))?,
        })
    }

    fn get(&self, role: FontRole) -> &ab_glyph::FontArc {
        match role {
            FontRole::Display => &self.display,
            FontRole::Body => &self.body,
        }
    }
}

/// Draw text into a template's boxes.
///
/// `fills` maps box id to text. An id with no fill is skipped, an unknown id is
/// ignored — a caller passing a stale id gets a picture with a missing caption
/// rather than an error, which is the better failure when the alternative is
/// posting nothing.
///
/// The base image is not modified; the composite is returned.
pub fn render(
    base: &RgbaImage,
    spec: &TemplateSpec,
    fills: &[(&str, &str)],
    fonts: &Fonts,
) -> RgbaImage {
    let mut canvas = base.clone();
    let (width, height) = (canvas.width(), canvas.height());

    for area in &spec.boxes {
        let Some((_, text)) = fills.iter().find(|(id, _)| *id == area.id) else {
            continue;
        };
        let text = text.trim();
        if text.is_empty() {
            continue;
        }

        let text = if area.style.uppercase {
            text.to_uppercase()
        } else {
            text.to_string()
        };

        let rect = area.rect.sanitized().to_pixels(width, height);
        let font = fonts.get(area.style.font);
        let Some(fitted) = fit_text(
            font,
            &text,
            rect,
            &area.fit,
            area.style.letter_spacing,
            height as f32,
        ) else {
            continue;
        };

        draw_fitted(&mut canvas, font, &fitted, area, rect);
    }

    canvas
}

fn draw_fitted(
    canvas: &mut RgbaImage,
    font: &ab_glyph::FontArc,
    fitted: &FittedText,
    area: &TemplateBox,
    rect: PixelRect,
) {
    use ab_glyph::{Font, ScaleFont};

    let scale = ab_glyph::PxScale::from(fitted.font_size);
    let scaled = font.as_scaled(scale);
    let spacing = area.style.letter_spacing * fitted.font_size;
    let pen = Pen {
        font,
        scale,
        spacing,
    };

    let block_height = fitted.line_height * fitted.lines.len() as f32;
    let top = match area.valign {
        VAlign::Top => rect.y,
        VAlign::Middle => rect.y + (rect.h - block_height) * 0.5,
        VAlign::Bottom => rect.y + rect.h - block_height,
    };

    for (n, line) in fitted.lines.iter().enumerate() {
        let line_width = layout::measure(font, scale, line, spacing);
        let x = match area.align {
            Align::Left => rect.x,
            Align::Center => rect.x + (rect.w - line_width) * 0.5,
            Align::Right => rect.x + rect.w - line_width,
        };
        // Baseline, not the top of the line box.
        let y = top + fitted.line_height * n as f32 + scaled.ascent();

        if let Some(outline) = area.style.outline {
            let radius = (outline.width * fitted.font_size).max(1.0);
            // A ring of offsets rather than a true stroke: eight passes is
            // indistinguishable at meme sizes and needs no path geometry.
            for (dx, dy) in OUTLINE_RING {
                draw_line(
                    canvas,
                    &pen,
                    line,
                    x + dx * radius,
                    y + dy * radius,
                    outline.color,
                );
            }
        }

        draw_line(canvas, &pen, line, x, y, area.style.color);
    }
}

/// Unit offsets for the outline passes, scaled by radius at draw time.
const OUTLINE_RING: [(f32, f32); 8] = [
    (-1.0, -1.0),
    (0.0, -1.0),
    (1.0, -1.0),
    (-1.0, 0.0),
    (1.0, 0.0),
    (-1.0, 1.0),
    (0.0, 1.0),
    (1.0, 1.0),
];

/// The face, size and tracking a run of lines shares.
///
/// Grouped because the outline ring and the fill differ only in colour, so
/// threading five identical arguments through nine calls was both noisy and
/// the kind of thing that goes wrong by one transposed parameter.
struct Pen<'a> {
    font: &'a ab_glyph::FontArc,
    scale: ab_glyph::PxScale,
    spacing: f32,
}

fn draw_line(
    canvas: &mut RgbaImage,
    pen: &Pen<'_>,
    text: &str,
    x: f32,
    baseline: f32,
    color: [u8; 4],
) {
    use ab_glyph::{Font, ScaleFont};

    let Pen {
        font,
        scale,
        spacing,
    } = *pen;
    let scaled = font.as_scaled(scale);
    let mut caret = x;

    for ch in text.chars() {
        let id = scaled.glyph_id(ch);
        let glyph = id.with_scale_and_position(scale, ab_glyph::point(caret, baseline));

        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, coverage| {
                let px = bounds.min.x + gx as f32;
                let py = bounds.min.y + gy as f32;
                if px < 0.0 || py < 0.0 {
                    return;
                }
                let (px, py) = (px as u32, py as u32);
                if px >= canvas.width() || py >= canvas.height() {
                    return;
                }
                blend(canvas.get_pixel_mut(px, py), color, coverage);
            });
        }

        caret += scaled.h_advance(id) + spacing;
    }
}

/// Straight-alpha source-over of a solid colour at some coverage.
fn blend(pixel: &mut Rgba<u8>, color: [u8; 4], coverage: f32) {
    let alpha = (color[3] as f32 / 255.0) * coverage.clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return;
    }
    for (out, &src) in pixel.0.iter_mut().zip(color.iter()).take(3) {
        let blended = src as f32 * alpha + *out as f32 * (1.0 - alpha);
        *out = blended.round().clamp(0.0, 255.0) as u8;
    }
    let dst_alpha = pixel.0[3] as f32 / 255.0;
    pixel.0[3] = (((alpha + dst_alpha * (1.0 - alpha)) * 255.0).round()).clamp(0.0, 255.0) as u8;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fonts() -> Fonts {
        let face =
            ab_glyph::FontArc::try_from_slice(include_bytes!("../fonts/DejaVuSans.ttf") as &[u8])
                .expect("test font parses");
        Fonts {
            display: face.clone(),
            body: face,
        }
    }

    /// Mid-grey, so both a white fill and a black outline are visible against it.
    fn blank(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_pixel(width, height, Rgba([128, 128, 128, 255]))
    }

    fn touched(image: &RgbaImage, rect: PixelRect) -> bool {
        for y in rect.y as u32..(rect.y + rect.h) as u32 {
            for x in rect.x as u32..(rect.x + rect.w) as u32 {
                if image.get_pixel(x, y).0 != [128, 128, 128, 255] {
                    return true;
                }
            }
        }
        false
    }

    /// The whole point, end to end: text lands inside its box and nowhere else.
    /// A caption bleeding outside its region is the failure the old point-based
    /// overlay had no way to even express.
    #[test]
    fn render_draws_inside_the_box_it_was_given() {
        let spec = TemplateSpec {
            schema: SCHEMA_VERSION,
            name: "t".to_string(),
            title: "T".to_string(),
            image: ImageRef {
                key: "k".to_string(),
                digest: String::new(),
            },
            boxes: vec![TemplateBox {
                id: "only".to_string(),
                role: "the words".to_string(),
                rect: Rect::new(0.0, 0.0, 1.0, 0.5),
                align: Align::Center,
                valign: VAlign::Top,
                style: BoxStyle::caption(),
                fit: Fit::default(),
                required: false,
            }],
        };

        let base = blank(400, 400);
        let out = render(&base, &spec, &[("only", "hello")], &fonts());

        assert_eq!(out.dimensions(), base.dimensions());
        assert!(
            touched(
                &out,
                PixelRect {
                    x: 0.0,
                    y: 0.0,
                    w: 400.0,
                    h: 200.0
                }
            ),
            "nothing was drawn in the box"
        );
        assert!(
            !touched(
                &out,
                PixelRect {
                    x: 0.0,
                    y: 220.0,
                    w: 400.0,
                    h: 180.0
                }
            ),
            "text escaped its box"
        );
    }

    /// An unfilled box, an empty string and a stale id all leave the picture
    /// alone. The last one matters most: a caller with an out-of-date id gets a
    /// meme with a missing caption rather than no meme at all.
    #[test]
    fn nothing_is_drawn_for_empty_or_unknown_fills() {
        let spec = TemplateSpec::classic("t", "T", "k");
        let base = blank(400, 400);
        let whole = PixelRect {
            x: 0.0,
            y: 0.0,
            w: 400.0,
            h: 400.0,
        };

        for fills in [vec![], vec![("top", "   ")], vec![("nonexistent", "hello")]] {
            let out = render(&base, &spec, &fills, &fonts());
            assert!(!touched(&out, whole), "drew something for {fills:?}");
        }
    }

    /// Both halves of a classic template fill independently.
    #[test]
    fn each_box_takes_its_own_fill() {
        let spec = TemplateSpec::classic("t", "T", "k");
        let base = blank(400, 400);

        let out = render(&base, &spec, &[("top", "up"), ("bottom", "down")], &fonts());

        assert!(touched(
            &out,
            PixelRect {
                x: 0.0,
                y: 0.0,
                w: 400.0,
                h: 130.0
            }
        ));
        assert!(touched(
            &out,
            PixelRect {
                x: 0.0,
                y: 280.0,
                w: 400.0,
                h: 120.0
            }
        ));
    }

    /// The outline is what makes white text readable on a light photo, so its
    /// absence has to be observable — panel style must not paint dark pixels
    /// where caption style would.
    #[test]
    fn an_outlined_caption_paints_more_than_a_plain_one() {
        let mut spec = TemplateSpec::classic("t", "T", "k");
        spec.boxes.truncate(1);
        let base = RgbaImage::from_pixel(400, 400, Rgba([255, 255, 255, 255]));

        let outlined = render(&base, &spec, &[("top", "hi")], &fonts());
        spec.boxes[0].style = BoxStyle {
            color: [255, 255, 255, 255],
            ..BoxStyle::default()
        };
        let plain = render(&base, &spec, &[("top", "hi")], &fonts());

        let dark = |img: &RgbaImage| img.pixels().filter(|p| p.0[0] < 128).count();
        assert!(dark(&outlined) > 0, "outline drew nothing");
        assert_eq!(dark(&plain), 0, "white-on-white should be invisible");
    }

    #[test]
    fn a_classic_spec_has_the_two_boxes_the_old_captioner_had() {
        let spec = TemplateSpec::classic("drake", "Drake", "memes/drake.jpg");

        assert_eq!(spec.boxes.len(), 2);
        assert_eq!(spec.box_by_id("top").unwrap().valign, VAlign::Top);
        assert_eq!(spec.box_by_id("bottom").unwrap().valign, VAlign::Bottom);
        assert!(spec.validate().is_ok());
    }

    /// Two boxes with one id means a fill silently lands on whichever the
    /// renderer happened to reach first — invisible in the output, so it is
    /// caught here instead.
    #[test]
    fn duplicate_box_ids_are_rejected() {
        let mut spec = TemplateSpec::classic("x", "X", "k");
        spec.boxes[1].id = "top".to_string();

        let problems = spec.validate().unwrap_err();
        assert!(
            problems.iter().any(|p| p.contains("more than one box")),
            "{problems:?}"
        );
    }

    #[test]
    fn a_spec_from_the_future_is_refused_rather_than_guessed_at() {
        let mut spec = TemplateSpec::classic("x", "X", "k");
        spec.schema = SCHEMA_VERSION + 1;

        let problems = spec.validate().unwrap_err();
        assert!(
            problems.iter().any(|p| p.contains("schema")),
            "{problems:?}"
        );
    }

    /// Boxes are normalized, so the same spec has to land proportionally on a
    /// thumbnail and on the original.
    #[test]
    fn a_rect_projects_proportionally_onto_any_size() {
        let rect = Rect::new(0.25, 0.5, 0.5, 0.25);

        let small = rect.to_pixels(100, 100);
        assert_eq!(
            (small.x, small.y, small.w, small.h),
            (25.0, 50.0, 50.0, 25.0)
        );

        let large = rect.to_pixels(1000, 400);
        assert_eq!(
            (large.x, large.y, large.w, large.h),
            (250.0, 200.0, 500.0, 100.0)
        );
    }

    /// A box dragged off the edge is ordinary editor behaviour, and a
    /// zero-width one would make the fit search never terminate.
    #[test]
    fn an_out_of_bounds_rect_is_clamped_to_something_drawable() {
        let escaped = Rect::new(1.5, -0.5, 2.0, 0.0).sanitized();

        assert!(escaped.x >= 0.0 && escaped.x <= 1.0);
        assert!(escaped.y >= 0.0 && escaped.y <= 1.0);
        assert!(escaped.w > 0.0 && escaped.x + escaped.w <= 1.0);
        assert!(escaped.h > 0.0 && escaped.y + escaped.h <= 1.0);
    }

    /// The encoding is what a datum would carry, so the common case has to stay
    /// small: default style and fit contribute nothing.
    #[test]
    fn defaults_are_omitted_from_the_encoding() {
        let spec = TemplateSpec::classic("drake", "Drake", "memes/drake.jpg");
        let json = serde_json::to_string(&spec).unwrap();

        assert!(!json.contains("letter_spacing"), "{json}");
        assert!(!json.contains("max_lines"), "{json}");
        assert!(!json.contains("required"), "{json}");
        // But what a renderer cannot infer is present.
        assert!(json.contains("\"schema\":1"), "{json}");
        assert!(json.contains("\"role\""), "{json}");
    }

    /// Field order is declaration order, so the same spec encodes to the same
    /// bytes every time — the property a hash (and later, a datum) rests on.
    #[test]
    fn the_encoding_is_deterministic() {
        let spec = TemplateSpec::classic("drake", "Drake", "memes/drake.jpg");
        let once = serde_json::to_vec(&spec).unwrap();
        let twice = serde_json::to_vec(&spec).unwrap();

        assert_eq!(once, twice);
        assert!(
            once.starts_with(br#"{"schema":1,"name":"drake""#),
            "{}",
            String::from_utf8_lossy(&once)
        );
    }

    /// The bug this class of schema invites, caught directly: a field that is
    /// skipped when it holds its "absent" value must deserialize back to that
    /// same value. `panel()`'s `outline: None` was skipped on write and came
    /// back as `Some` because the default disagreed — a black halo on text that
    /// asked for none, visible only in the finished picture.
    ///
    /// Every constructor, because the next one added is where this recurs.
    #[test]
    fn every_style_survives_the_encoding_unchanged() {
        for style in [
            BoxStyle::default(),
            BoxStyle::caption(),
            BoxStyle::panel(),
            BoxStyle {
                letter_spacing: 0.05,
                uppercase: true,
                ..BoxStyle::panel()
            },
        ] {
            let json = serde_json::to_string(&style).unwrap();
            let back: BoxStyle = serde_json::from_str(&json).unwrap();
            assert_eq!(back, style, "lost through {json}");
        }
    }

    #[test]
    fn a_spec_round_trips_through_json() {
        let mut spec = TemplateSpec::classic("drake", "Drake", "memes/drake.jpg");
        spec.boxes[0].style = BoxStyle::panel();
        spec.boxes[0].required = true;
        spec.image.digest = "abc123".to_string();

        let json = serde_json::to_string(&spec).unwrap();
        let back: TemplateSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back, spec);
    }

    #[test]
    fn required_ids_name_what_has_to_be_filled() {
        let mut spec = TemplateSpec::classic("x", "X", "k");
        spec.boxes[1].required = true;

        assert_eq!(spec.required_ids(), vec!["bottom"]);
    }

    /// Panel text is a different register from caption text — no outline, dark,
    /// not shouted. Getting this wrong is the visible difference between Drake
    /// rendered correctly and Drake rendered as a photo caption.
    #[test]
    fn panel_style_is_not_caption_style() {
        let panel = BoxStyle::panel();
        assert!(panel.outline.is_none());
        assert!(!panel.uppercase);
        assert_eq!(panel.font, FontRole::Body);

        let caption = BoxStyle::caption();
        assert!(caption.outline.is_some());
        assert!(caption.uppercase);
        assert_eq!(caption.font, FontRole::Display);
    }
}
