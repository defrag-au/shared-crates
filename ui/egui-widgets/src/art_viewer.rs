//! `ArtViewer` — one asset's art at size: the live piece where there is one, the cover still where there is not.
//!
//! Selecting an asset in a collection browser has never shown the art. The
//! largest thing any of these front ends paints is a grid thumbnail, so
//! "selected for viewing" opens a pricing panel — and a fully on-chain piece
//! needs more than a bigger `<img>` anyway: its art is a document the browser
//! runs (see [`crate::html_stage`]), which is why this widget exists at all
//! rather than a `full_size` flag on the card.
//!
//! ## It does not fetch
//!
//! The viewer takes the art's *state*, not a URL to resolve:
//! [`ArtViewerState::set_source`] carries [`ArtSource::Live`],
//! [`ArtSource::CoverOnly`], or a failure, and the host decides how to find out
//! which. That keeps this crate free of the app's HTTP layer, and it keeps the
//! one interesting question — "is this asset's art a document or a still?" —
//! where the metadata is, rather than guessing it here from a URL's shape.
//!
//! ## The disclosure is not decoration
//!
//! Where a piece has both, the still in the grid and the document in the viewer
//! can look nothing alike: the still is a frame captured at mint, and the piece
//! is animated, often with a different palette or composition. A reader who is
//! not told will read the thumbnail as the artwork and the viewer as something
//! else. So a cover being present alongside live art is stated on screen, in
//! words — see [`caption_text`].

use egui::{RichText, Ui};

use crate::icons::PhosphorIcon;
use crate::theme::{Radius, Space, SpaceExt, TextSize, ThemeExt};

/// What the viewer is showing.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum ArtSource {
    /// Nothing has asked yet. The host is expected to move this on as soon as
    /// it knows — the viewer draws the same "looking…" state as for
    /// [`ArtSource::Loading`], so a host that forgets leaves a spinner rather
    /// than an empty frame.
    #[default]
    Unknown,
    /// The question is in flight.
    Loading,
    /// The piece itself: a `data:` URI to hand to a stage, or a URL to one.
    Live(String),
    /// The asset has no on-chain art. Show the cover.
    CoverOnly,
    /// The lookup failed. The cover is still shown, with the reason beside it
    /// — a blank viewer reads as a broken asset rather than a broken request.
    Failed(String),
}

/// The asset being viewed. Plain data: the host has all of it already, and this
/// crate has no opinion about where it came from.
#[derive(Clone, Debug, Default)]
pub struct ArtViewerTarget {
    /// Stable identity — the asset's `policy:asset_name_hex`, or whatever else
    /// the host uses. **Not** the display name: two assets in a collection can
    /// share one, and this is what decides whether re-opening is the same asset
    /// or a different one, which in turn decides whether the mounted piece is
    /// kept or torn down.
    pub id: String,
    /// The asset's display name.
    pub name: String,
    /// One line under the name — collection, rarity rank, whatever the host
    /// considers the useful context. `None` leaves it out.
    pub subtitle: Option<String>,
    /// The still. Required in practice: it is what covers the loading, failed
    /// and cover-only states.
    pub cover_url: Option<String>,
    /// Trait rows, drawn below the art. Keep it short — the art is the subject.
    pub traits: Vec<(String, String)>,
}

/// What the host should do about the viewer this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArtViewerResponse {
    /// The viewer was dismissed this frame (Esc, a close button, or a click on
    /// the scrim). The host should drop the target and cancel anything it still
    /// has in flight.
    pub closed: bool,
}

/// See the [module docs](self).
#[derive(Default)]
pub struct ArtViewerState {
    target: Option<ArtViewerTarget>,
    source: ArtSource,
    /// Let the piece take pointer input. See [`crate::StageOptions`] for why
    /// this is off by default.
    interactive: bool,
    /// The mounted piece. Dropping it is the teardown — see
    /// [`crate::html_stage`].
    #[cfg(target_arch = "wasm32")]
    stage: Option<crate::html_stage::HtmlStage>,
    /// Why a mount failed, if one did.
    stage_error: Option<String>,
}

impl ArtViewerState {
    /// Show `target`, in whatever art state the host already knows.
    ///
    /// Re-opening with a *different* target resets the source to
    /// [`ArtSource::Unknown`], so a stale `Live` can never be handed to the next
    /// asset's stage. Identity is [`ArtViewerTarget::id`], not the display name.
    /// Re-opening the same one keeps what was already found — a host that calls
    /// this every frame must not re-ask every frame.
    pub fn open(&mut self, target: ArtViewerTarget) {
        let changed = self
            .target
            .as_ref()
            .is_none_or(|current| current.id != target.id);
        if changed {
            self.source = ArtSource::Unknown;
            self.teardown();
        }
        self.target = Some(target);
    }

    /// Dismiss the viewer. Also runs on `Drop`; call it directly when the
    /// teardown must be immediate — a page change, say, rather than whenever
    /// the state happens to be dropped.
    pub fn close(&mut self) {
        self.target = None;
        self.teardown();
    }

    /// Report what the host found out about this asset's art.
    pub fn set_source(&mut self, source: ArtSource) {
        // Leaving `Live` unmounts the piece: these documents animate
        // themselves, so one left mounted behind a cover is a core burnt for
        // nothing.
        if self.source != source {
            self.teardown();
        }
        self.source = source;
    }

    /// Is the viewer showing anything?
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.target.is_some()
    }

    /// The target being viewed, for a host that needs to re-read it.
    #[must_use]
    pub fn target(&self) -> Option<&ArtViewerTarget> {
        self.target.as_ref()
    }

    /// The art state currently held.
    #[must_use]
    pub fn source(&self) -> &ArtSource {
        &self.source
    }

    /// Draw the viewer. Does nothing when there is no target.
    pub fn show(&mut self, ui: &mut Ui) -> ArtViewerResponse {
        let Some(target) = self.target.clone() else {
            return ArtViewerResponse::default();
        };

        let mut response = ArtViewerResponse::default();
        let modal = egui::Modal::new(egui::Id::new("art_viewer")).show(ui.ctx(), |ui| {
            // Clamped to the viewport, not asserted: a flat minimum wider than
            // a phone makes the viewer itself the thing that overflows.
            let room = (ui.ctx().content_rect().width() - 32.0).max(240.0);
            ui.set_min_width(360.0_f32.min(room));
            ui.set_max_width(760.0_f32.min(room));

            header(ui, &target, &self.source, &mut response);
            ui.gap(Space::Md);

            let art_rect = art_region(ui);
            self.draw_art(ui, &target, art_rect);
            ui.gap(Space::Sm);
            caption(ui, &target, &self.source);

            if let Some(error) = &self.stage_error {
                ui.label(
                    RichText::new(error)
                        .color(ui.tokens().color.accent_red)
                        .size(ui.text_size(TextSize::Sm)),
                );
            }

            if !target.traits.is_empty() {
                ui.gap(Space::Md);
                trait_rows(ui, &target);
            }

            ui.gap(Space::Md);
            ui.separator();
            footer(ui, &mut self.interactive, &mut response);
        });

        // A click on the scrim is a dismissal — the standard escape hatch, which
        // egui reports separately from anything drawn here.
        if modal.should_close() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            response.closed = true;
        }
        if response.closed {
            self.close();
        }
        response
    }

    /// Remove the mounted piece, whatever state it was in.
    fn teardown(&mut self) {
        #[cfg(target_arch = "wasm32")]
        {
            self.stage = None;
        }
        self.stage_error = None;
    }

    fn draw_art(&mut self, ui: &mut Ui, target: &ArtViewerTarget, rect: egui::Rect) {
        // The stroke is drawn INSIDE `rect`, and the stage is inset by its
        // width so the frame stays visible — an `AboveCanvas` stage paints over
        // the canvas, and one at the full rect would hide its own border.
        let stage_rect = rect.shrink(1.0);

        // Taken, not borrowed: the `Live` arm mounts, which needs `&mut self`,
        // and a match on `&self.source` holds the immutable borrow across it.
        // Taken rather than CLONED because `Live` carries the piece's whole
        // document — 10–15 KB — and this runs every frame.
        let source = std::mem::take(&mut self.source);
        match &source {
            ArtSource::Live(src) => {
                frame(ui, rect);
                self.mount_and_present(ui, target, src, stage_rect);
            }
            ArtSource::CoverOnly | ArtSource::Unknown | ArtSource::Loading => {
                frame(ui, rect);
                draw_cover(ui, target, stage_rect);
                if matches!(source, ArtSource::Loading | ArtSource::Unknown) {
                    ui.put(
                        egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(28.0)),
                        egui::Spinner::new(),
                    );
                }
            }
            ArtSource::Failed(_) => {
                frame(ui, rect);
                draw_cover(ui, target, stage_rect);
                placeholder(ui, rect, "could not load the art");
            }
        }
        self.source = source;
    }

    #[cfg(target_arch = "wasm32")]
    fn mount_and_present(
        &mut self,
        ui: &Ui,
        target: &ArtViewerTarget,
        src: &str,
        stage_rect: egui::Rect,
    ) {
        if self.stage.is_none() {
            let options = crate::html_stage::StageOptions::default()
                .radius(ui.tokens().corner(Radius::Base).nw as f32)
                .title(target.name.clone())
                .interactive(self.interactive);
            match crate::html_stage::HtmlStage::mount(src, options) {
                Ok(stage) => self.stage = Some(stage),
                Err(e) => self.stage_error = Some(format!("could not mount the piece: {e:?}")),
            }
        }
        if let Some(stage) = &self.stage {
            // Read every frame, NOT just at mount: `interactive` is a live
            // control in the footer, and applying it only when the stage is
            // created is how the checkbox came to do nothing.
            stage.set_interactive(self.interactive);
            stage.present(stage_rect);
            // The piece animates on its own and egui repaints on demand, so
            // without this the stage would only be repositioned when something
            // else asked for a frame.
            ui.ctx().request_repaint();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn mount_and_present(
        &mut self,
        ui: &Ui,
        _target: &ArtViewerTarget,
        _src: &str,
        _stage_rect: egui::Rect,
    ) {
        // No DOM to mount in. Say so rather than showing nothing: a native
        // build is a real consumer of this widget (the storybook's native
        // check, any desktop harness), and a silently empty frame reads as a
        // broken asset.
        placeholder(ui, _stage_rect, "live art needs a browser");
    }
}

impl Drop for ArtViewerState {
    fn drop(&mut self) {
        self.teardown();
    }
}

/// The title row: name, subtitle, what kind of art this is, and the close
/// button.
///
/// The close button lives HERE rather than over the art on purpose. An
/// `AboveCanvas` stage covers the canvas, so anything egui draws inside the art
/// rect is invisible — a close button placed there would be a button nobody can
/// see or click.
fn header(
    ui: &mut Ui,
    target: &ArtViewerTarget,
    source: &ArtSource,
    response: &mut ArtViewerResponse,
) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new(&target.name)
                    .size(ui.text_size(TextSize::Xl))
                    .strong(),
            );
            if let Some(subtitle) = &target.subtitle {
                ui.label(
                    RichText::new(subtitle)
                        .size(ui.text_size(TextSize::Sm))
                        .color(ui.tokens().color.text_muted),
                );
            }
        });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            let close = crate::icons::phosphor_label(ui, PhosphorIcon::X, "Close");
            if ui.button(close).clicked() {
                response.closed = true;
            }
            if matches!(source, ArtSource::Live(_)) {
                ui.add_space(ui.space(Space::Sm));
                ui.label(
                    RichText::new("live on-chain piece")
                        .size(ui.text_size(TextSize::Sm))
                        .color(ui.tokens().color.accent_green),
                );
            }
        });
    });
}

/// Reserve the art's square.
///
/// Derived from the viewport rather than a fixed size: the viewer's job is to
/// show the piece as large as it can without pushing the caption, the traits
/// and the controls off screen, and a square is what these documents expect
/// (`html,body{height:100%}` in a piece fills whatever frame it is given).
fn art_region(ui: &mut Ui) -> egui::Rect {
    let room = ui.ctx().content_rect();
    let side = (room.width() * 0.62)
        .min(room.height() * 0.58)
        .clamp(160.0, 560.0);
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(side), egui::Sense::hover());
    rect
}

fn frame(ui: &Ui, rect: egui::Rect) {
    ui.painter().rect_stroke(
        rect,
        ui.tokens().corner(Radius::Base),
        egui::Stroke::new(1.0, ui.tokens().color.border),
        egui::StrokeKind::Inside,
    );
}

fn draw_cover(ui: &Ui, target: &ArtViewerTarget, rect: egui::Rect) {
    let Some(url) = &target.cover_url else {
        return;
    };
    egui::Image::new(url)
        .fit_to_exact_size(rect.size())
        .corner_radius(ui.tokens().corner(Radius::Base))
        .paint_at(ui, rect);
}

fn placeholder(ui: &Ui, rect: egui::Rect, text: &str) {
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(ui.text_size(TextSize::Md)),
        ui.tokens().color.text_muted,
    );
}

/// The line under the art, and whether it is a warning.
///
/// Pure, so the states can be tested without a `Ui` — the disclosure is the
/// part of this widget worth asserting on, and it is exactly the part a
/// rendering test could not see.
fn caption_text(source: &ArtSource, has_cover: bool) -> (&str, bool) {
    match source {
        ArtSource::Live(_) if has_cover => (
            "This is the piece itself, running. The thumbnail is its cover — \
             a still captured at mint.",
            false,
        ),
        ArtSource::Live(_) => ("This is the piece itself, running.", false),
        ArtSource::CoverOnly => ("Cover only — this asset carries no on-chain art.", false),
        ArtSource::Failed(reason) => (reason.as_str(), true),
        ArtSource::Loading | ArtSource::Unknown => ("Looking for on-chain art…", false),
    }
}

fn caption(ui: &mut Ui, target: &ArtViewerTarget, source: &ArtSource) {
    let (text, warning) = caption_text(source, target.cover_url.is_some());
    let color = if warning {
        ui.tokens().color.accent_red
    } else {
        ui.tokens().color.text_muted
    };
    ui.label(
        RichText::new(text)
            .size(ui.text_size(TextSize::Sm))
            .color(color),
    );
}

fn trait_rows(ui: &mut Ui, target: &ArtViewerTarget) {
    egui::Grid::new("art_viewer_traits")
        .num_columns(2)
        .spacing([ui.space(Space::Xl2), ui.space(Space::Xs)])
        .show(ui, |ui| {
            for (key, value) in &target.traits {
                ui.label(
                    RichText::new(key)
                        .size(ui.text_size(TextSize::Sm))
                        .color(ui.tokens().color.text_muted),
                );
                ui.label(RichText::new(value).size(ui.text_size(TextSize::Sm)));
                ui.end_row();
            }
        });
}

fn footer(ui: &mut Ui, interactive: &mut bool, response: &mut ArtViewerResponse) {
    ui.horizontal(|ui| {
        ui.checkbox(interactive, "interactive").on_hover_text(
            "Let the piece take clicks. Off by default: an interactive stage \
             swallows scrolling and hover for its whole area.",
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Close").clicked() {
                response.closed = true;
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(id: &str) -> ArtViewerTarget {
        ArtViewerTarget {
            id: id.to_string(),
            name: format!("name of {id}"),
            cover_url: Some("https://example.test/cover.jpg".into()),
            ..Default::default()
        }
    }

    fn live() -> ArtSource {
        ArtSource::Live("data:text/html;utf8,<html></html>".into())
    }

    #[test]
    fn opening_a_different_target_drops_the_previous_art() {
        let mut state = ArtViewerState::default();
        state.open(target("a"));
        state.set_source(live());

        state.open(target("b"));
        assert_eq!(
            state.source(),
            &ArtSource::Unknown,
            "a stale Live must not be handed to the next asset's stage"
        );
    }

    #[test]
    fn identity_is_the_id_not_the_display_name() {
        // Two assets can share a display name. Keying on it would treat the
        // second as "the same one" and leave the first one's piece mounted.
        let mut state = ArtViewerState::default();
        state.open(target("first"));
        state.set_source(live());

        let mut same_name = target("second");
        same_name.name = state.target().expect("open").name.clone();
        state.open(same_name);

        assert_eq!(state.source(), &ArtSource::Unknown);
    }

    #[test]
    fn reopening_the_same_target_keeps_what_was_already_found() {
        // A host may call `open` every frame; resetting would re-ask every frame.
        let mut state = ArtViewerState::default();
        state.open(target("same"));
        state.set_source(ArtSource::CoverOnly);
        state.open(target("same"));
        assert_eq!(state.source(), &ArtSource::CoverOnly);
    }

    #[test]
    fn closing_clears_the_target() {
        let mut state = ArtViewerState::default();
        state.open(target("piece"));
        assert!(state.is_open());
        state.close();
        assert!(!state.is_open());
    }

    #[test]
    fn every_art_state_gets_a_caption() {
        // The disclosure is the point of the widget. A state that produced no
        // line would leave the reader guessing which of the two things they are
        // looking at.
        for source in [
            ArtSource::Unknown,
            ArtSource::Loading,
            live(),
            ArtSource::CoverOnly,
            ArtSource::Failed("timed out".into()),
        ] {
            let (text, _) = caption_text(&source, true);
            assert!(!text.is_empty(), "no caption for {source:?}");
        }
    }

    #[test]
    fn a_cover_beside_live_art_is_called_out() {
        // The one caption that has to be different: a reader who is not told
        // reads the thumbnail as the artwork.
        let piece = live();
        let (with_cover, _) = caption_text(&piece, true);
        let (without, _) = caption_text(&piece, false);
        assert!(with_cover.contains("cover"), "{with_cover}");
        assert_ne!(with_cover, without);
    }

    #[test]
    fn a_failure_is_a_warning_carrying_its_reason() {
        let failed = ArtSource::Failed("timed out".into());
        let (text, warning) = caption_text(&failed, true);
        assert_eq!(text, "timed out");
        assert!(warning, "a failure must be drawn as one");
    }

    #[test]
    fn a_missing_cover_is_never_claimed_as_one() {
        let (text, _) = caption_text(&ArtSource::CoverOnly, false);
        assert!(!text.contains("thumbnail"), "{text}");
    }
}
