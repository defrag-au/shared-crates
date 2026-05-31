//! `Chip` — small filled-tag label with optional remove (`×`) affordance.
//!
//! Generalises the half-dozen one-off `*_chip` helpers scattered through
//! `wallet_list`, `collection_list`, and the portal frontend (status,
//! standard, network, role, gate, archived). Variants pick a palette so
//! the surrounding code says **what the chip means** rather than which
//! colour to use.
//!
//! ## Variants
//!
//! - [`ChipVariant::Success`] — green. Active phases, healthy state, OK.
//! - [`ChipVariant::Warning`] — yellow. Archived / paused / mind-the-gap.
//! - [`ChipVariant::Danger`]  — red. Failures, ineligible, removed.
//! - [`ChipVariant::Tag`]     — soft blue. Generic enumerated tag (e.g.
//!   gate types: `public`, `allowlist`, `token_held`).
//! - [`ChipVariant::Info`]    — soft teal. Informational secondary signal.
//! - [`ChipVariant::Muted`]   — neutral grey. Background / deferred.
//!
//! ## Removable chips
//!
//! Setting [`Chip::removable`] adds a small `×` to the right of the
//! label. Click on `×` returns [`ChipResponse::removed = true`] so the
//! host can drop the row. The chip body is still hoverable for the
//! tooltip; the `×` has its own hover hint.
//!
//! ## Example
//!
//! ```ignore
//! use egui_widgets::{Chip, ChipVariant};
//!
//! Chip::new("active").variant(ChipVariant::Success).show(ui);
//! let resp = Chip::new("public").variant(ChipVariant::Tag).removable(true).show(ui);
//! if resp.removed { dispatch(RemoveGate { id }); }
//! ```

use egui::{Color32, CornerRadius, Frame, Margin, RichText, Sense, Stroke, Ui};

use crate::icons::{install_phosphor_font, PhosphorIcon};

/// Semantic palette pick — `Chip::variant(…)` consumes one of these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChipVariant {
    /// Green. Use for active / healthy / OK states.
    Success,
    /// Yellow. Use for paused / archived / "needs attention".
    Warning,
    /// Red. Use for failures / ineligible / removed.
    Danger,
    /// Soft blue. Generic enumerated tag (gate types, role tags, etc.).
    Tag,
    /// Soft teal. Informational secondary signal.
    Info,
    /// Neutral grey. Background / deferred / placeholder.
    Muted,
}

impl ChipVariant {
    /// Return the (foreground, background, optional border) triple. The
    /// border is only set on `Tag` / `Info` variants — the filled
    /// success/warning/danger chips don't need extra structure.
    pub fn palette(self) -> (Color32, Color32, Option<Color32>) {
        match self {
            Self::Success => (Color32::from_rgb(18, 28, 18), Color32::LIGHT_GREEN, None),
            Self::Warning => (Color32::from_rgb(40, 30, 10), Color32::LIGHT_YELLOW, None),
            Self::Danger => (Color32::WHITE, Color32::from_rgb(180, 80, 80), None),
            Self::Tag => (
                Color32::from_gray(220),
                Color32::from_rgb(30, 40, 60),
                Some(Color32::from_rgb(60, 80, 110)),
            ),
            Self::Info => (
                Color32::from_gray(220),
                Color32::from_rgb(26, 44, 44),
                Some(Color32::from_rgb(60, 100, 100)),
            ),
            Self::Muted => (Color32::WHITE, Color32::from_gray(140), None),
        }
    }
}

/// Builder.
pub struct Chip<'a> {
    text: &'a str,
    variant: ChipVariant,
    removable: bool,
    hover_text: Option<&'a str>,
    upper: bool,
}

/// Outcome of one `Chip::show()` call.
#[derive(Default, Debug)]
pub struct ChipResponse {
    /// `true` when the user clicked the `×` (only emitted for chips built
    /// with [`Chip::removable(true)`]).
    pub removed: bool,
    /// `true` when the chip body itself was clicked. Hosts can use this
    /// for "click chip to filter" patterns; for static chips, ignore.
    pub clicked: bool,
}

impl<'a> Chip<'a> {
    /// Construct a `Chip` displaying `text`. Defaults: `ChipVariant::Muted`,
    /// not removable, no tooltip, label rendered verbatim (no upper-casing).
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            variant: ChipVariant::Muted,
            removable: false,
            hover_text: None,
            upper: false,
        }
    }

    /// Set the semantic variant. See [`ChipVariant`] for the palette
    /// guide.
    pub fn variant(mut self, v: ChipVariant) -> Self {
        self.variant = v;
        self
    }

    /// Add a `×` affordance to the right of the label. Click → returns
    /// `ChipResponse { removed: true, .. }`.
    pub fn removable(mut self, b: bool) -> Self {
        self.removable = b;
        self
    }

    /// Attach a tooltip shown on hover.
    pub fn on_hover_text(mut self, s: &'a str) -> Self {
        self.hover_text = Some(s);
        self
    }

    /// Upper-case the label at render time (matches the old `status_chip`
    /// behaviour). Off by default — passing pre-cased text is cleaner.
    pub fn upper_case(mut self, b: bool) -> Self {
        self.upper = b;
        self
    }

    /// Render the chip inline at the current `Ui` cursor. The chip
    /// allocates a small filled frame; the caller does spacing.
    pub fn show(self, ui: &mut Ui) -> ChipResponse {
        let (fg, bg, border) = self.variant.palette();
        let mut response = ChipResponse::default();
        let label_text = if self.upper {
            self.text.to_ascii_uppercase()
        } else {
            self.text.to_string()
        };

        let mut frame = Frame::new()
            .fill(bg)
            .corner_radius(CornerRadius::same(3))
            .inner_margin(Margin::symmetric(5, 1));
        if let Some(b) = border {
            frame = frame.stroke(Stroke::new(1.0, b));
        }
        let inner = frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                let body = ui.label(RichText::new(&label_text).small().color(fg));
                if let Some(hover) = self.hover_text {
                    body.clone().on_hover_text(hover);
                }
                if self.removable {
                    // Phosphor `X` glyph for the remove affordance per the
                    // crate's no-raw-Unicode rule (see CLAUDE.md). Rendered
                    // as a click-sensed `egui::Label` rather than a
                    // `small_button` because the button has too much padding
                    // for a chip's footprint.
                    install_phosphor_font(ui.ctx());
                    let x = ui.add(
                        egui::Label::new(PhosphorIcon::X.rich_text(10.0, fg)).sense(Sense::click()),
                    );
                    if x.on_hover_text("Remove").clicked() {
                        response.removed = true;
                    }
                }
            });
        });

        if inner.response.clicked() {
            response.clicked = true;
        }
        response
    }
}
