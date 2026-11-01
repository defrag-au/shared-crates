//! `Phosphor` — the Phosphor icon font, subset to the glyphs a game uses,
//! drawn by its ink box: the honest alternative to hand-drawn shape icons.
//!
//! A macroquad client has no icon font of its own, and `Icon` draws a small set
//! of shapes. That is fine for a heart or a coin, and wrong for anything with a
//! silhouette of its own: a cart drawn from lines and circles is a guess at one
//! designer's drawing, and reads as a new one at every size.
//!
//! This is the same icon set the egui side uses, cut down to the glyphs a
//! macroquad client names. The asset is a **subset** of
//! `ui/egui-widgets/fonts/Phosphor.ttf` (Phosphor, MIT), so a game ships two
//! kilobytes rather than 478. Regenerate after changing the list:
//!
//! ```sh
//! python3 -m fontTools.subset ui/egui-widgets/fonts/Phosphor.ttf \
//!     --unicodes=U+E41E,U+E420 \
//!     --output-file=gaming/macroquad-gamekit/assets/phosphor-icons.ttf \
//!     --layout-features= --no-hinting --name-IDs='*' --name-legacy --name-languages='*'
//! ```
//!
//! The glyph is drawn through [`macroquad_text::draw_run`], so it is held by the
//! glyph gate like any other run, and recorded in a capture report under its
//! Phosphor name at its own ink box — the reporter's line box comes from
//! measuring `Mg`, which a two-glyph subset has not got.
//!
//! Placement is by the glyph's ink box, measured through fontdue — the same
//! rasteriser macroquad itself uses — so an icon fills its rect with no em
//! padding and no per-icon fudge.

use fontdue::Font as Measured;
use macroquad::prelude::*;
use macroquad_text::{TextStyle, draw_run, report};
use macroquad_widgets::Painter;

/// The icons this subset carries: a cart, and its simpler sibling.
///
/// Adding one is a line here plus its codepoint in the regeneration command in
/// the module docs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PhosphorIcon {
    /// Phosphor `shopping-cart` (U+E41E).
    Cart,
    /// Phosphor `shopping-cart-simple` (U+E420): fewer strokes, for small sizes.
    CartSimple,
}

impl PhosphorIcon {
    pub const ALL: [Self; 2] = [Self::Cart, Self::CartSimple];

    /// The glyph's codepoint in the Phosphor font.
    pub fn character(self) -> char {
        match self {
            Self::Cart => '\u{e41e}',
            Self::CartSimple => '\u{e420}',
        }
    }

    /// Phosphor's own name for it, for a story's label.
    pub fn name(self) -> &'static str {
        match self {
            Self::Cart => "shopping-cart",
            Self::CartSimple => "shopping-cart-simple",
        }
    }
}

/// The icon font: load once, keep, draw through.
///
/// `new` parses the embedded subset; where that fails it says so once and
/// [`draw`](Self::draw) draws nothing, so a host falls back to its own figure
/// rather than a blank tile.
pub struct Phosphor {
    font: Option<Font>,
    measured: Option<Measured>,
}

/// The embedded subset, so the guard test can measure it without a context.
const SUBSET: &[u8] = include_bytes!("../assets/phosphor-icons.ttf");

impl Default for Phosphor {
    fn default() -> Self {
        Self::new()
    }
}

impl Phosphor {
    pub fn new() -> Self {
        let font = load_ttf_font_from_bytes(SUBSET).ok();
        let measured = Measured::from_bytes(SUBSET, fontdue::FontSettings::default()).ok();
        if font.is_none() || measured.is_none() {
            eprintln!("macroquad-gamekit: the Phosphor subset did not load; icons are omitted");
        }
        Self { font, measured }
    }

    /// Whether the font parsed, so a host can choose a fallback up front.
    pub fn available(&self) -> bool {
        self.font.is_some() && self.measured.is_some()
    }

    /// Draw `icon` filling `rect`, its ink box fitted to the rect and tinted
    /// `color`.
    ///
    /// Under the glyph gate a glyph that is not cached yet draws next frame
    /// instead, so this reports nothing; a host wanting its own fallback asks
    /// [`available`](Self::available) once, at startup.
    pub fn draw(&self, p: &Painter, icon: PhosphorIcon, rect: Rect, color: Color) {
        let (Some(font), Some(measured)) = (&self.font, &self.measured) else {
            return;
        };
        // Measure at a reference size: the ink box in pixels, whose ratio to the
        // size is what sets the glyph's scale. Phosphor's icons carry about 8%
        // padding inside the em, so fitting the em would draw them small.
        const REF: f32 = 256.0;
        let m = measured.metrics(icon.character(), REF);
        if m.width == 0 || m.height == 0 {
            return;
        }
        let size = (rect.w * REF / m.width as f32).min(rect.h * REF / m.height as f32);
        let k = size / REF;
        let ink = vec2(m.width as f32 * k, m.height as f32 * k);
        // fontdue's xmin/ymin are the ink's left and bottom from the pen origin
        // and the baseline. Place the INK box's centre on the rect's, and the
        // pen follows.
        let x = rect.center().x - ink.x * 0.5 - m.xmin as f32 * k;
        let baseline = rect.center().y + (m.ymin as f32 + m.height as f32 * 0.5) * k;
        let text = icon.character().to_string();
        let drew = report::muted(|| {
            draw_run(
                &text,
                &TextStyle::new(Some(font.clone()), size, color),
                vec2(x, baseline),
                p.scale(),
                1.0,
            )
        });
        if drew && report::on() {
            report::line(
                icon.name(),
                Rect::new(
                    rect.center().x - ink.x * 0.5,
                    rect.center().y - ink.y * 0.5,
                    ink.x,
                    ink.y,
                ),
                size,
                1.0,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_subset_carries_every_icon_it_names() {
        // Measured straight from the embedded bytes: `Phosphor::new` loads the
        // font through macroquad, which needs a live context a unit test has
        // not got. The font is embedded, so this is still a build-time
        // guarantee — a codepoint added to the enum without the matching
        // `--unicodes` entry fails here rather than drawing nothing on a page.
        let measured = Measured::from_bytes(SUBSET, fontdue::FontSettings::default())
            .expect("the subset parses");
        for icon in PhosphorIcon::ALL {
            let m = measured.metrics(icon.character(), 64.0);
            assert!(
                m.width > 0 && m.height > 0,
                "{} has no glyph in the subset",
                icon.name()
            );
        }
    }
}
