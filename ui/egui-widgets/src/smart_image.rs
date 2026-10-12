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
//!
//! # Fit
//!
//! A ready image is drawn into its rect per [`Fit`], defaulting to
//! [`Fit::Contain`] — the rect is a hole the caller has already decided, and
//! putting the art in it without lying about its shape is this widget's job.
//! [`Fit::Stretch`] survives only for a caller whose rect already matches
//! its art; [`Fit::Cover`] fills the rect by cropping the centre.

use crate::theme::{Ink, TextSize, ThemeExt, Token};
use egui::{CornerRadius, Pos2, Rect, TextureOptions, Vec2, load::SizeHint};

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
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImageState {
    /// Texture is up. Painted, unless this was a [`ImagePass::Warm`] pass.
    ///
    /// `size` is the texture's own size — the aspect a caller needs when it
    /// wants to lay out to the art rather than the other way round (a grid
    /// learning its thumb ratio from the images it draws).
    Ready { size: Vec2 },
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

/// How a ready image occupies the rect it was given.
///
/// The default is [`Fit::Contain`], and that is the point: stretching is the
/// one mode that can distort the art, and it is only ever the right answer
/// when the rect's aspect already equals the art's — in which case `Contain`
/// draws the same thing. [`Fit::Stretch`] stays for exactly that caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fit {
    /// Aspect preserved, the whole image visible, centred. The unused bands
    /// show [`SmartImage::backdrop`] when one was given, and whatever the
    /// caller painted behind the rect otherwise.
    #[default]
    Contain,
    /// Aspect preserved, the rect fully covered — the overflow is cropped
    /// from the centre.
    Cover,
    /// Stretched to the rect, aspect be damned.
    Stretch,
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
    fit: Fit,
    /// Painted behind a contained image's letterbox bands. `None` leaves
    /// whatever the caller painted behind the rect showing through.
    backdrop: Option<Ink>,
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
            fit: Fit::default(),
            backdrop: None,
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

    /// How the image occupies its rect. Defaults to [`Fit::Contain`].
    pub fn fit(mut self, fit: Fit) -> Self {
        self.fit = fit;
        self
    }

    /// A fill painted behind a [`Fit::Contain`] image's letterbox bands.
    ///
    /// Without one the bands show whatever the caller painted behind the
    /// rect — the card's own background, in a grid — which is often what a
    /// caller wants, and why this is not the default.
    pub fn backdrop(mut self, ink: Ink) -> Self {
        self.backdrop = Some(ink);
        self
    }

    pub fn show(self, ui: &mut egui::Ui, rect: Rect, pass: ImagePass) -> ImageState {
        let Some(url) = self.url else {
            if pass == ImagePass::Paint {
                self.paint_fallback(ui, rect);
            }
            return ImageState::Absent;
        };

        let poll =
            ui.ctx()
                .try_load_texture(url, TextureOptions::default(), size_hint_for(ui, rect));

        let state = match &poll {
            Ok(egui::load::TexturePoll::Ready { texture }) => {
                ImageState::Ready { size: texture.size }
            }
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
            Ok(egui::load::TexturePoll::Ready { texture }) => self.paint_ready(ui, rect, texture),
            Ok(egui::load::TexturePoll::Pending { .. }) => self.paint_placeholder(ui, rect),
            Err(_) => self.paint_fallback(ui, rect),
        }
        state
    }

    /// A ready texture, placed per [`Self::fit`].
    ///
    /// Still `paint_at`, not a child `Ui` with an `Image` widget: the widget
    /// form allocates and can reflow the layout around it, and every caller
    /// here has already decided the rect. `paint_at` honours the image's uv
    /// rect, so the cover crop needs no mesh of our own either.
    fn paint_ready(&self, ui: &egui::Ui, rect: Rect, texture: egui::load::SizedTexture) {
        let size = texture.size;
        let image = egui::Image::from_texture(texture).corner_radius(self.corner_radius);
        match self.fit {
            Fit::Stretch => {
                image.paint_at(ui, rect);
            }
            Fit::Contain => {
                if let Some(ink) = self.backdrop {
                    let fill = ink.resolve(&ui.tokens());
                    ui.painter().rect_filled(rect, self.corner_radius, fill);
                }
                image.paint_at(ui, contain_rect(size, rect));
            }
            Fit::Cover => {
                image.uv(cover_uv(size, rect)).paint_at(ui, rect);
            }
        }
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

/// The largest rect with `image_size`'s aspect that fits inside `rect`,
/// centred — what a [`Fit::Contain`] image is painted into.
fn contain_rect(image_size: Vec2, rect: Rect) -> Rect {
    let usable =
        image_size.x > 0.0 && image_size.y > 0.0 && rect.width() > 0.0 && rect.height() > 0.0;
    if !usable {
        return rect;
    }
    let scale = (rect.width() / image_size.x).min(rect.height() / image_size.y);
    Rect::from_center_size(rect.center(), image_size * scale)
}

/// The uv rect of `image_size` that covers `rect` with the aspect kept —
/// the centre crop a [`Fit::Cover`] image is sampled through.
fn cover_uv(image_size: Vec2, rect: Rect) -> Rect {
    let full = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    let usable =
        image_size.x > 0.0 && image_size.y > 0.0 && rect.width() > 0.0 && rect.height() > 0.0;
    if !usable {
        return full;
    }
    let image_aspect = image_size.x / image_size.y;
    let rect_aspect = rect.width() / rect.height();
    // Crop the dimension with slack: an image wider than the rect loses its
    // sides, a taller one loses its top and bottom. Both from the centre.
    let (span, crop_horizontally) = if image_aspect > rect_aspect {
        (rect_aspect / image_aspect, true)
    } else {
        (image_aspect / rect_aspect, false)
    };
    let offset = (1.0 - span) * 0.5;
    if crop_horizontally {
        Rect::from_min_max(Pos2::new(offset, 0.0), Pos2::new(offset + span, 1.0))
    } else {
        Rect::from_min_max(Pos2::new(0.0, offset), Pos2::new(1.0, offset + span))
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
    if px.is_finite() {
        (px as u32).max(1)
    } else {
        1
    }
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
        assert_eq!(
            DecodeSize::for_hint(SizeHint::default()),
            DecodeSize::Native
        );
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
        assert!(
            !ImageState::Ready {
                size: Vec2::splat(64.0)
            }
            .wants_repaint()
        );
        assert!(!ImageState::Absent.wants_repaint());
    }

    /// Stretching is the mode that lies about the art, so it must never be
    /// what a caller gets without asking.
    #[test]
    fn the_default_fit_contains() {
        assert_eq!(Fit::default(), Fit::Contain);
    }

    /// The bug this option exists to kill: a wide image in a square rect
    /// keeps its aspect and gains bands above and below.
    #[test]
    fn contain_letterboxes_a_wide_image_in_a_square() {
        let square = Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0));
        let out = contain_rect(Vec2::new(200.0, 100.0), square);
        assert_eq!(out.size(), Vec2::new(100.0, 50.0));
        assert_eq!(out.center(), square.center());
    }

    #[test]
    fn contain_pillarboxes_a_tall_image_in_a_square() {
        let square = Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0));
        let out = contain_rect(Vec2::new(100.0, 200.0), square);
        assert_eq!(out.size(), Vec2::new(50.0, 100.0));
    }

    /// A texture that already matches — or one whose size the loader has not
    /// reported — must land on the rect itself, never a NaN one.
    #[test]
    fn contain_leaves_matching_and_degenerate_inputs_alone() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 200.0));
        assert_eq!(contain_rect(Vec2::new(30.0, 20.0), rect), rect);
        assert_eq!(contain_rect(Vec2::ZERO, rect), rect);
        assert_eq!(contain_rect(Vec2::new(f32::NAN, 20.0), rect), rect);
    }

    /// Cover keeps the whole rect and takes the same bands contain would
    /// leave empty, off the CENTRE of the image instead.
    #[test]
    fn cover_crops_the_sides_of_a_wide_image() {
        let square = Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0));
        let uv = cover_uv(Vec2::new(200.0, 100.0), square);
        assert_eq!(
            uv,
            Rect::from_min_max(Pos2::new(0.25, 0.0), Pos2::new(0.75, 1.0))
        );
    }

    #[test]
    fn cover_crops_the_top_and_bottom_of_a_tall_image() {
        let square = Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0));
        let uv = cover_uv(Vec2::new(100.0, 200.0), square);
        assert_eq!(
            uv,
            Rect::from_min_max(Pos2::new(0.0, 0.25), Pos2::new(1.0, 0.75))
        );
    }

    #[test]
    fn cover_uses_the_whole_image_when_it_already_matches() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 200.0));
        let full = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        assert_eq!(cover_uv(Vec2::new(30.0, 20.0), rect), full);
        assert_eq!(cover_uv(Vec2::ZERO, rect), full);
    }

    fn hint(px: u32) -> SizeHint {
        SizeHint::Size {
            width: px,
            height: px,
            maintain_aspect_ratio: true,
        }
    }
}
