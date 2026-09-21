//! `HtmlStage` story — a real on-chain art piece, running.
//!
//! The piece is not a fixture invented for the storybook: it is a real
//! BlockGen authority token's CIP-25 metadata, parsed by
//! `cardano_assets::AssetEnvelope::live_art` exactly as a front end would. So
//! this story exercises the whole path — chunked `files[].src` in, a live
//! document out — rather than only the mounting half of it.
//!
//! ## What to watch for
//!
//! - **The frame counter.** "iframes in the DOM" reads the live document
//!   count. Unmount and it must return to 0 — a stage left mounted is a
//!   `requestAnimationFrame` loop left running, which is the failure this
//!   widget exists to prevent. The number is here so that is checkable at a
//!   glance rather than in a profiler.
//! - **The piece animates on its own.** Nothing drives it; the document is
//!   the artwork.
//!
//! ## Why there is no cutout demo here
//!
//! [`egui_widgets::StageLayer::BelowCanvas`] is not shown, and cannot be, in
//! this storybook: the storybook paints its own central panel opaque
//! (`CHROME_BG_MAIN`), and an iframe *under* the canvas is hidden by it. The
//! cutout needs the host to render transparent and to stop filling every
//! frame that covers the stage — a property of the app, not of the story. The
//! reference implementation is the sentience hub-guardian prototype
//! (`augminted-bots/widgets/_prototype-iframe-punch`): transparent
//! `clear_color`, a `z-index: 1` canvas, and a `CentralPanel` with
//! `Frame::NONE`.

use egui_widgets::theme::ThemeExt as _;
use egui_widgets::{HtmlStage, PhosphorIcon, StageOptions};

use crate::accent;

/// A real on-chain piece: `files[0]` is a chunked `data:text/html;utf8,…`
/// document (10,040 bytes over 157 chunks) with an IPFS still as its cover.
/// Shared with `cardano-assets`' own corpus tests.
const PIECE_METADATA: &str =
    include_str!("../../../../cardano-assets/resources/test/blockgen-artist-charlesmachin.json");

pub struct HtmlStageState {
    stage: Option<HtmlStage>,
    /// The parsed piece — what the stage was mounted from.
    live: Option<cardano_assets::LiveArt>,
    /// Why parsing failed, if it did. Shown rather than swallowed: an empty
    /// story with no explanation is indistinguishable from a broken widget.
    parse_error: Option<String>,
    /// Edge length of the region the stage occupies, in points.
    size: f32,
    radius: f32,
    interactive: bool,
}

impl Default for HtmlStageState {
    fn default() -> Self {
        let (live, parse_error) =
            match serde_json::from_str::<cardano_assets::AssetEnvelope>(PIECE_METADATA) {
                Ok(envelope) => match envelope.live_art() {
                    Some(art) => (Some(art), None),
                    None => (None, Some("fixture parsed but carries no live art".into())),
                },
                Err(e) => (None, Some(format!("fixture did not parse: {e}"))),
            };

        Self {
            stage: None,
            live,
            parse_error,
            size: 320.0,
            radius: 12.0,
            interactive: false,
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut HtmlStageState) {
    ui.label(
        "A fully on-chain piece is an HTML document, not a still — the browser runs it. \
         This stage mounts the real document in a sandboxed iframe and keeps it positioned \
         over the rectangle egui reserved.",
    );
    ui.add_space(8.0);

    if let Some(error) = &state.parse_error {
        ui.label(
            egui::RichText::new(error)
                .color(ui.tokens().color.accent_red)
                .strong(),
        );
        return;
    }

    let Some(live) = state.live.clone() else {
        return;
    };

    // ── What was extracted from the metadata ────────────────────────────
    ui.label(
        egui::RichText::new("From the metadata")
            .color(accent(ui))
            .strong(),
    );
    ui.label(format!(
        "live art: {} bytes, {}…",
        live.src.len(),
        &live.src[..48.min(live.src.len())]
    ));
    match &live.cover {
        Some(cover) => {
            ui.label(format!("cover still: {cover}"));
            ui.label(
                egui::RichText::new(
                    "The cover is what a grid thumbnail shows. It is not the piece.",
                )
                .color(ui.tokens().color.text_muted),
            );
        }
        None => {
            ui.label(
                egui::RichText::new("no separate cover — `image` is the document itself")
                    .color(ui.tokens().color.text_muted),
            );
        }
    }

    ui.add_space(12.0);

    // ── Controls ────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        let mounted = state.stage.is_some();
        let (icon, label) = if mounted {
            (PhosphorIcon::X, "Unmount")
        } else {
            (PhosphorIcon::Play, "Mount")
        };
        if ui
            .button(egui_widgets::icons::phosphor_label(ui, icon, label))
            .clicked()
        {
            if mounted {
                // Dropping the stage removes the iframe — the teardown.
                state.stage = None;
            } else {
                match HtmlStage::mount(
                    &live.src,
                    StageOptions::default()
                        .interactive(state.interactive)
                        .radius(state.radius)
                        .title("On-chain piece"),
                ) {
                    Ok(stage) => state.stage = Some(stage),
                    Err(e) => log::error!("mount failed: {e:?}"),
                }
            }
        }

        ui.separator();
        ui.checkbox(&mut state.interactive, "interactive")
            .on_hover_text(
                "Off by default: an iframe that takes input swallows the scroll wheel and \
                 egui's hover for its whole rect.",
            );
    });

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label("size");
        ui.add(egui::Slider::new(&mut state.size, 120.0..=520.0).suffix("pt"));
        ui.separator();
        ui.label("radius");
        ui.add(egui::Slider::new(&mut state.radius, 0.0..=40.0).suffix("px"));
    });

    // The trap this widget exists to close, made visible.
    ui.horizontal(|ui| {
        let count = iframe_count();
        let (color, note) = if state.stage.is_some() {
            if count == 1 {
                (ui.tokens().color.accent_green, "one stage, one loop")
            } else {
                (
                    ui.tokens().color.accent_yellow,
                    "more than one stage is mounted",
                )
            }
        } else if count == 0 {
            (
                ui.tokens().color.text_muted,
                "nothing mounted — and nothing running",
            )
        } else {
            (ui.tokens().color.accent_red, "a stage was left behind")
        };
        ui.label(egui::RichText::new(format!("iframes in the DOM: {count}")).color(color));
        ui.label(egui::RichText::new(note).color(color));
    });

    ui.add_space(12.0);

    // ── The stage's region ──────────────────────────────────────────────
    ui.label(egui::RichText::new("The stage").color(accent(ui)).strong());
    ui.label(
        egui::RichText::new(
            "egui draws this frame; the iframe draws the art. `present()` runs every frame, \
             so the piece tracks the rect through scrolls and resizes.",
        )
        .color(ui.tokens().color.text_muted),
    );
    ui.add_space(6.0);

    let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(state.size), egui::Sense::hover());

    // A frame to show the stage is where egui thinks it is — and, with the
    // stage unmounted, that the space is reserved and empty.
    ui.painter().rect_stroke(
        rect,
        egui::CornerRadius::same(state.radius.min(255.0) as u8),
        egui::Stroke::new(1.0, accent(ui)),
        egui::StrokeKind::Inside,
    );

    if let Some(stage) = &state.stage {
        // Every frame, not once: the rect moves when the sidebar toggles,
        // the window resizes, or anything above it reflows.
        stage.present(rect);
        ui.ctx().request_repaint();
    } else {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "stage unmounted",
            egui::FontId::proportional(12.0),
            ui.tokens().color.text_muted,
        );
    }
}

/// How many iframes are in the document right now.
///
/// The story's own instrument for the teardown trap — see the module docs.
fn iframe_count() -> usize {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.query_selector_all("iframe").ok())
        .map(|list| list.length() as usize)
        .unwrap_or(0)
}
