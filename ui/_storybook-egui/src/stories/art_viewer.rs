//! `ArtViewer` story — one asset's art, in each of the three states it has.
//!
//! Three stories rather than one with a mode picker, because each state is a
//! different claim about what the reader is looking at, and the point of the
//! widget is that it says which:
//!
//! - **Art Viewer** — the live piece, running. `EternalChaos000`, parsed from
//!   the real metadata exactly as a front end would.
//! - **Art Viewer (cover only)** — an asset with no on-chain art. The cover
//!   fills the frame and the caption says so.
//! - **Art Viewer (failed)** — the lookup failed. The cover is still shown,
//!   with the reason, because a blank frame reads as a broken asset.
//!
//! ## Why they open themselves
//!
//! A modal that needs a click to appear screenshots as an empty page, which
//! makes it unreviewable in the one workflow that matters here. Each story
//! opens its viewer on the first frame; dismissing it (Esc, the scrim, either
//! Close) is remembered, and a **Reopen** button brings it back — so the
//! dismissal paths are still exercisable.

use egui_widgets::theme::ThemeExt as _;
use egui_widgets::{ArtSource, ArtViewerState, ArtViewerTarget};

use crate::accent;
use crate::stories::onchain_fixture;

/// Which of the viewer's states this story demonstrates.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Live,
    CoverOnly,
    Failed,
}

#[derive(Default)]
pub struct ArtViewerStoryState {
    viewer: ArtViewerState,
    /// The reader dismissed it. Remembered so auto-open does not fight them —
    /// and so the close paths are testable at all.
    dismissed: bool,
}

impl ArtViewerStoryState {
    /// Reset to the open state — the Reopen button.
    pub fn reopen(&mut self) {
        self.dismissed = false;
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut ArtViewerStoryState, mode: Mode) {
    let Some(live_src) = live_piece() else {
        ui.label(
            egui::RichText::new("the fixture carries no live art")
                .color(ui.tokens().color.accent_red),
        );
        return;
    };

    ui.label(match mode {
        Mode::Live => {
            "The piece is a document, not a still: it runs. The caption names the cover, \
             because the thumbnail a reader clicked is a frame captured at mint and can \
             look nothing like what they are now seeing."
        }
        Mode::CoverOnly => {
            "An asset with no on-chain art. The cover fills the frame and the caption \
             says there is nothing else to look for — no spinner, no empty box."
        }
        Mode::Failed => {
            "The lookup failed. The cover is still shown, with the reason beside it: a \
             blank viewer reads as a broken asset rather than a broken request."
        }
    });

    ui.add_space(12.0);

    ui.horizontal(|ui| {
        if ui.button("Reopen the viewer").clicked() {
            state.reopen();
        }
        ui.label(
            egui::RichText::new(if state.dismissed {
                "dismissed — reopen, or reload"
            } else {
                "open"
            })
            .color(ui.tokens().color.text_muted),
        );
    });

    ui.add_space(12.0);
    ui.label(
        egui::RichText::new("The viewer's target")
            .color(accent(ui))
            .strong(),
    );
    let target = target_for(mode);
    ui.label(format!("name: {}", target.name));
    if let Some(subtitle) = &target.subtitle {
        ui.label(format!("subtitle: {subtitle}"));
    }
    ui.label(format!(
        "cover_url: {}",
        target.cover_url.as_deref().unwrap_or("(none)")
    ));
    ui.label(format!("traits: {}", target.traits.len()));

    ui.add_space(12.0);
    ui.label(
        egui::RichText::new(match mode {
            Mode::Live => format!("live art: {} bytes", live_src.len()),
            Mode::CoverOnly => "live art: none — the endpoint answers 204".to_string(),
            Mode::Failed => "live art: unknown — the request failed".to_string(),
        })
        .color(ui.tokens().color.text_muted),
    );

    // Auto-open on the first frame; a dismissal sticks until Reopen.
    if !state.dismissed && !state.viewer.is_open() {
        state.viewer.open(target.clone());
    }
    if state.viewer.is_open() {
        state.viewer.set_source(match mode {
            Mode::Live => ArtSource::Live(live_src),
            Mode::CoverOnly => ArtSource::CoverOnly,
            Mode::Failed => ArtSource::Failed("art lookup timed out".into()),
        });
    }

    let response = state.viewer.show(ui);
    if response.closed {
        state.dismissed = true;
    }
}

/// The piece's document, out of the real metadata.
fn live_piece() -> Option<String> {
    onchain_fixture::live_art().map(|art| art.src)
}

fn target_for(mode: Mode) -> ArtViewerTarget {
    let base = ArtViewerTarget {
        id: format!(
            "{}:{}",
            onchain_fixture::POLICY_ID,
            onchain_fixture::ASSET_NAME_HEX
        ),
        name: match mode {
            Mode::Live => onchain_fixture::DISPLAY_NAME.to_string(),
            Mode::CoverOnly => "Toolhead #2274".to_string(),
            Mode::Failed => "EternalChaos001".to_string(),
        },
        subtitle: Some(onchain_fixture::COLLECTION.to_string()),
        // The failed state deliberately still carries the cover: that is the
        // point of it.
        cover_url: Some(onchain_fixture::cover_url()),
        traits: onchain_fixture::traits(),
        // The real shareable page for this piece, so the story shows the copy
        // button the app shows.
        share_url: Some(format!(
            "https://ownership.cnft.dev/piece/{}:{}",
            onchain_fixture::POLICY_ID,
            onchain_fixture::ASSET_NAME_HEX
        )),
    };
    // Distinct identities per story, or moving between them would read as the
    // same asset and keep the previous state's source.
    match mode {
        Mode::Live => base,
        Mode::CoverOnly => ArtViewerTarget {
            id: "cover-only".into(),
            ..base
        },
        Mode::Failed => ArtViewerTarget {
            id: "failed".into(),
            ..base
        },
    }
}
