//! `SmartImage` — an image that knows how big it needs to be decoded, and whether it is being drawn or merely warmed.
//!
//! # Why this exists
//!
//! Loading an image and painting it are different costs with different right
//! answers, and every virtualised surface here had rediscovered that
//! separately — badly.
//!
//! - **Painting** is expensive per card and only worth doing for what is on
//!   screen, so grids cull it.
//! - **Asking** for an image is nearly free and wants to happen EARLY, because
//!   the answer needs a fetch, a decode and a texture upload before it can
//!   appear. Cull the asking along with the painting and cards scroll into
//!   view blank.
//!
//! Before this, each surface open-coded `try_load_texture` plus a placeholder
//! plus its own idea of a size hint. The predictable happened:
//! `card_browser::draw_thumbnail` passed `SizeHint::default()`, which is
//! `Scale(1.0)`, which [`crate::image_loader::DecodeSize::for_hint`] maps to
//! `Native` — **full source resolution**. A 140pt card showing 2048² art cost
//! 16MB of texture instead of 256KB, and a grid zoomed out to ~250 thumbnails
//! ran to gigabytes. That blew the retention cap instantly, and what followed
//! was a reload cascade: evict, repaint, refetch, evict. `listing_grid` had
//! been fixed months earlier; nothing propagated the fix, because there was no
//! single place for it to live.
//!
//! So the rule this widget exists to enforce: **the size hint is derived from
//! the rect, once, here.** A warm pass and a paint pass cannot ask for
//! different rungs, because neither of them chooses.
//!
//! # Use
//!
//! ```ignore
//! match SmartImage::new(&url).show(ui, rect, pass) {
//!     ImageState::Loading => CachedSpinner::request_repaint(ui),
//!     _ => {}
//! }
//! ```
//!
//! A caller in a culled grid passes the pass it was given and writes no
//! branch. That is the point: an opt-in `if warm { … return }` at every call
//! site is a thing to forget, and forgetting it silently pays the full paint
//! cost for the whole warm band.

use crate::theme::{Ink, TextSize, ThemeExt, Token};
use egui::{CornerRadius, Rect, TextureOptions, load::SizeHint};

/// Whether this call should draw the image, or only get it on its way.
///
/// A named decision rather than a `bool`, because `show(ui, rect, false)` at a
/// call site says nothing about what `false` means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImagePass {
    /// On screen. Request the texture and paint it (or a placeholder).
    Paint,
    /// Off screen but close. Request the texture and draw NOTHING — anything
    /// painted here is outside the clip rect and thrown away.
    ///
    /// Warming also refreshes the retention LRU (see
    /// `image_loader::schedule::Schedule::release_cold`), so an image about to
    /// be needed is not evicted just before it is needed.
    Warm,
}

/// What became of the image this pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageState {
    /// Texture is up. Painted, unless this was a [`ImagePass::Warm`] pass.
    Ready,
    /// Still fetching or decoding — a caller driving a spinner should keep
    /// asking for frames.
    Loading,
    /// The load failed. **Terminal**: do not request a repaint for this, or a
    /// broken URL becomes a permanent frame loop. The error is cached, so
    /// asking again is cheap and still fails.
    Failed,
    /// No URL was supplied.
    Absent,
}

impl ImageState {
    /// Should the caller keep frames coming for this image?
    ///
    /// Only `Loading`. `Failed` deliberately answers `false` — see its note.
    pub fn wants_repaint(self) -> bool {
        matches!(self, Self::Loading)
    }
}

/// What stands in for an image that is not there yet.
///
/// A named choice rather than an `Option<Ink>`, because the three cases are
/// genuinely different instructions to the reader — "wait", "this is the
/// backing", "say nothing" — and the first is the one that is almost always
/// right for content that is on its way.
// No `Eq`: `Ink` carries a `Color32` variant and does not implement it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Placeholder {
    /// A pulsing [`crate::skeleton::Skeleton`] block. Says CONTENT IS COMING,
    /// which a flat rectangle does not — a grid of empty panels reads as a
    /// collection of broken assets, not a grid that is loading.
    ///
    /// Requests a repaint while it pulses, and stops the moment the last image
    /// lands.
    Skeleton,
    /// A flat fill. For a surface that wants a quiet backing rather than an
    /// animated one.
    Fill(Ink),
    /// Nothing at all. For an image LAYERED over one already drawn, where a
    /// backdrop would hide what is underneath while the top one loads.
    None,
}

/// An image sized to the rect it will occupy.
pub struct SmartImage<'a> {
    url: Option<&'a str>,
    corner_radius: CornerRadius,
    placeholder: Placeholder,
    /// Drawn centred when the load failed, or when there is no URL.
    fallback_glyph: Option<&'a str>,
}

impl<'a> SmartImage<'a> {
    pub fn new(url: &'a str) -> Self {
        Self::from_option(Some(url))
    }

    /// For the common case where the caller may not have a URL — a card whose
    /// asset has no image resolves to [`ImageState::Absent`] rather than
    /// making every call site write the same `if let Some`.
    pub fn from_option(url: Option<&'a str>) -> Self {
        Self {
            url,
            corner_radius: CornerRadius::same(4),
            placeholder: Placeholder::Skeleton,
            fallback_glyph: Some("?"),
        }
    }

    pub fn corner_radius(mut self, corner_radius: impl Into<CornerRadius>) -> Self {
        self.corner_radius = corner_radius.into();
        self
    }

    /// What to show while the image is absent. Defaults to
    /// [`Placeholder::Skeleton`].
    pub fn placeholder(mut self, placeholder: Placeholder) -> Self {
        self.placeholder = placeholder;
        self
    }

    /// Replace or remove the centred glyph shown when there is no image.
    pub fn fallback_glyph(mut self, glyph: Option<&'a str>) -> Self {
        self.fallback_glyph = glyph;
        self
    }

    pub fn show(self, ui: &mut egui::Ui, rect: Rect, pass: ImagePass) -> ImageState {
        let Some(url) = self.url else {
            if pass == ImagePass::Paint {
                self.paint_fallback(ui, rect);
            }
            return ImageState::Absent;
        };

        let poll = ui
            .ctx()
            .try_load_texture(url, TextureOptions::default(), size_hint_for(ui, rect));

        let state = match &poll {
            Ok(egui::load::TexturePoll::Ready { .. }) => ImageState::Ready,
            Ok(egui::load::TexturePoll::Pending { .. }) => ImageState::Loading,
            Err(_) => ImageState::Failed,
        };

        // The whole reason the pass is a parameter rather than the caller's
        // branch: the request above has already happened, which is all a warm
        // pass wanted.
        if pass == ImagePass::Warm {
            return state;
        }

        match poll {
            Ok(egui::load::TexturePoll::Ready { texture }) => {
                // `paint_at`, not a child `Ui` with an `Image` widget: the
                // widget form allocates and can reflow the layout around it,
                // and every caller here has already decided the rect.
                egui::Image::from_texture(texture)
                    .corner_radius(self.corner_radius)
                    .paint_at(ui, rect);
            }
            Ok(egui::load::TexturePoll::Pending { .. }) => self.paint_placeholder(ui, rect),
            Err(_) => self.paint_fallback(ui, rect),
        }
        state
    }

    fn paint_placeholder(&self, ui: &egui::Ui, rect: Rect) {
        match self.placeholder {
            Placeholder::Skeleton => {
                crate::skeleton::Skeleton::paint_block_at(
                    ui,
                    rect,
                    crate::skeleton::SkeletonReason::Loading,
                );
            }
            Placeholder::Fill(ink) => {
                let fill = ink.resolve(&ui.tokens());
                ui.painter().rect_filled(rect, self.corner_radius, fill);
            }
            Placeholder::None => {}
        }
    }

    fn paint_fallback(&self, ui: &egui::Ui, rect: Rect) {
        // A FAILED image is not a loading one: a pulsing skeleton over a
        // broken URL promises something that is never coming. It gets the
        // static backing and the glyph instead.
        match self.placeholder {
            Placeholder::None => {}
            Placeholder::Skeleton | Placeholder::Fill(_) => {
                let ink = match self.placeholder {
                    Placeholder::Fill(ink) => ink,
                    _ => Ink::Token(Token::BgHighlight),
                };
                let fill = ink.resolve(&ui.tokens());
                ui.painter().rect_filled(rect, self.corner_radius, fill);
            }
        }
        let Some(glyph) = self.fallback_glyph else {
            return;
        };
        let muted = Ink::Token(Token::TextMuted).resolve(&ui.tokens());
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            glyph,
            egui::FontId::proportional(ui.text_size(TextSize::Xl2)),
            muted,
        );
    }
}

/// How big this image actually needs to be decoded.
///
/// ⚠️ **Never `SizeHint::default()`** — see the module docs for what that
/// costs. In PHYSICAL pixels, so the hint carries the device's pixel ratio and
/// a HiDPI screen takes the next rung up rather than looking soft.
fn size_hint_for(ui: &egui::Ui, rect: Rect) -> SizeHint {
    SizeHint::Size {
        width: physical_px(rect.width(), ui.ctx().pixels_per_point()),
        height: physical_px(rect.height(), ui.ctx().pixels_per_point()),
        maintain_aspect_ratio: true,
    }
}

/// Points to physical pixels, rounded UP and floored at 1.
///
/// Up, because rounding down lands on the rung below and decodes an image
/// smaller than the space it fills, which is visible. Floored at 1 because a
/// zero-size hint is not a request for a zero-size image — surfaces do hand
/// out degenerate rects during a first layout pass, and a `0` would ask the
/// rung ladder for something it has no answer for.
fn physical_px(points: f32, pixels_per_point: f32) -> u32 {
    let px = (points * pixels_per_point).ceil();
    if px.is_finite() { (px as u32).max(1) } else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_loader::DecodeSize;

    /// The arithmetic that decides how much memory a grid costs. A 2× card
    /// landing one rung too high is the difference between 256KB and 640KB a
    /// thumbnail, times several hundred.
    #[test]
    fn a_card_at_2x_asks_for_physical_pixels_not_points() {
        assert_eq!(physical_px(128.0, 2.0), 256);
        assert_eq!(physical_px(128.0, 1.0), 128);
        // Fractional ratios are real (125% display scaling).
        assert_eq!(physical_px(100.0, 1.25), 125);
    }

    /// Rounding DOWN would decode below the rung the space needs.
    #[test]
    fn a_fractional_pixel_rounds_up_to_the_covering_rung() {
        assert_eq!(physical_px(127.5, 1.0), 128);
        assert_eq!(physical_px(128.1, 1.0), 129);
        // …and one pixel over a rung boundary must step up, or the image is
        // upscaled into its own card.
        assert_eq!(DecodeSize::for_hint(hint(129)), DecodeSize::Retina);
        assert_eq!(DecodeSize::for_hint(hint(128)), DecodeSize::Tile);
    }

    /// A degenerate rect during first layout must not ask for a zero image.
    #[test]
    fn a_zero_or_bogus_rect_still_asks_for_something() {
        assert_eq!(physical_px(0.0, 2.0), 1);
        assert_eq!(physical_px(f32::NAN, 2.0), 1);
        assert_eq!(physical_px(f32::INFINITY, 2.0), 1);
    }

    /// The regression this widget exists to make impossible: the default hint
    /// decodes at source resolution.
    #[test]
    fn the_default_size_hint_would_decode_natively() {
        assert_eq!(DecodeSize::for_hint(SizeHint::default()), DecodeSize::Native);
        // What a real card asks for instead.
        assert_eq!(DecodeSize::for_hint(hint(256)), DecodeSize::Retina);
    }

    /// The default has to be the skeleton: an image that is ON ITS WAY needs
    /// to say so. A flat rectangle is indistinguishable from a broken asset,
    /// which is exactly how a browse grid of forty loading thumbnails read
    /// after the spinner was removed.
    #[test]
    fn an_image_says_it_is_coming_unless_told_otherwise() {
        let image = SmartImage::new("https://example.test/a.png");
        assert_eq!(image.placeholder, Placeholder::Skeleton);
    }

    /// A failed load must not drive frames, or a broken URL spins forever.
    #[test]
    fn only_loading_asks_for_more_frames() {
        assert!(ImageState::Loading.wants_repaint());
        assert!(!ImageState::Failed.wants_repaint());
        assert!(!ImageState::Ready.wants_repaint());
        assert!(!ImageState::Absent.wants_repaint());
    }

    fn hint(px: u32) -> SizeHint {
        SizeHint::Size {
            width: px,
            height: px,
            maintain_aspect_ratio: true,
        }
    }
}
