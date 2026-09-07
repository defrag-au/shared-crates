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

use egui::{Color32, CornerRadius, RichText, Sense, Stroke, Ui};

use crate::icons::{PhosphorIcon, install_phosphor_font};
use crate::viewport::Breakpoint;

/// Horizontal padding inside the chip, each side.
///
/// Module-level so [`Chip::width`] — which decides where a wrapping row breaks
/// — and [`Chip::show`] — which paints — cannot drift apart. They did when each
/// held its own copy, and the symptom is a row that breaks one chip early or
/// one chip late, which reads as an unrelated spacing bug.
const MARGIN_X: f32 = 5.0;
/// Vertical padding inside the chip, top and bottom.
const MARGIN_Y: f32 = 1.0;
/// Point size of the `×` remove affordance.
const REMOVE_GLYPH: f32 = 10.0;

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
            // Darker fill than the old gray(140): white-on-gray(140) was
            // 3.4:1 — and Muted is the default variant, so the accidental
            // chip was the unreadable one.
            Self::Muted => (Color32::WHITE, Color32::from_gray(90), None),
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
    clickable: bool,
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
            clickable: false,
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

    /// Advertise that the body does something when clicked — currently a
    /// pointer cursor on hover.
    ///
    /// `ChipResponse::clicked` is reported either way; this only changes the
    /// AFFORDANCE, so a static chip does not grow a hand cursor it cannot
    /// honour. A chip that acts on a click without saying so is a hidden
    /// control, and one that promises a click it ignores is worse.
    pub fn clickable(mut self, b: bool) -> Self {
        self.clickable = b;
        self
    }

    /// How wide this chip will draw, computed before anything is painted.
    ///
    /// Mirrors the geometry `show` builds: the label galley plus the frame's
    /// horizontal `inner_margin`, plus the remove affordance when there is
    /// one. Only used for the wrap decision, so being a pixel out moves a
    /// break point rather than misdrawing anything.
    fn width(&self, ui: &Ui, label: &str) -> f32 {
        let text = ui
            .painter()
            .layout_no_wrap(
                label.to_owned(),
                egui::TextStyle::Small.resolve(ui.style()),
                egui::Color32::WHITE,
            )
            .size()
            .x;
        let remove = if self.removable {
            ui.spacing().item_spacing.x + REMOVE_GLYPH
        } else {
            0.0
        };
        text + remove + MARGIN_X * 2.0
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

        // BREAK THE ROW OURSELVES INSIDE A WRAPPING LAYOUT.
        //
        // A chip draws as a `Frame`, and `Frame::end` reserves its space with
        // `ui.allocate_rect`, which only advances the cursor — it never asks
        // the layout whether the item fits. So inside a `horizontal_wrapped` a
        // run of chips does NOT wrap: it walks straight off the right edge and
        // is clipped. On a phone that showed up as a card's four tags running
        // past the card and taking the amount column's width with them.
        //
        // Same trap, same fix as the asset pills in `activity_feed` — measure
        // first, then make the layout decision explicitly.
        if ui.layout().main_wrap && ui.layout().is_horizontal() {
            let avail = ui.available_size_before_wrap().x;
            // Only break when there is something to break AWAY from. At the
            // start of a row an over-wide chip has nowhere better to go, and
            // breaking would just leave a blank line above it.
            let mid_row = avail < ui.max_rect().width();
            if mid_row && self.width(ui, &label_text) > avail {
                ui.end_row();
            }
        }

        // ALLOCATED AND PAINTED, NOT FRAMED — and that is what makes a chip sit
        // on its row's centre line.
        //
        // This was an `egui::Frame` around a label, which is the obvious way to
        // draw a small filled tag and the reason every chip in the estate hung
        // low in an app bar. A `Frame` cannot participate in vertical
        // alignment: it does not know its height until its content is laid out,
        // so `Frame::begin` starts at the cursor and `Frame::end` reserves what
        // it used. The row never gets the chance to centre it. Measured on the
        // flow-explorer's top bar: wordmark and buttons centred at y≈20, chip at
        // y≈27, hanging out of the bottom of the row.
        //
        // `allocate_exact_size` goes through `Layout::next_frame`, which
        // stretches an item's frame to the row height and centres the widget
        // within it — so the rect comes back aligned the way the row asked.
        // Painting straight into it keeps it there, with no nested `Ui` whose
        // own alignment pass could disagree.
        //
        // It also settles a second bug for free. `spacing.interact_size` is a
        // floor on ALLOCATED SPACE, not a property of buttons, and the old
        // inner `ui.horizontal` took it as the chip's height — so under
        // `apply_touch_sizing` every read-only tag became a 44pt square with
        // 10pt text rattling inside it. Nothing here reads `interact_size` at
        // all now; the chip is sized by its text, and a CLICKABLE chip meets the
        // touch minimum in its interaction rect instead (see below).
        if self.removable {
            // Phosphor `X` for the remove affordance, per the crate's
            // no-raw-Unicode rule. Installed before measuring: an uninstalled
            // font lays the glyph out as a fallback of a different width.
            install_phosphor_font(ui.ctx());
        }
        let text = RichText::new(&label_text).small().color(fg);
        let galley = egui::WidgetText::from(text).into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            egui::TextStyle::Small,
        );
        let x_galley = self.removable.then(|| {
            egui::WidgetText::from(PhosphorIcon::X.rich_text(REMOVE_GLYPH, fg)).into_galley(
                ui,
                Some(egui::TextWrapMode::Extend),
                f32::INFINITY,
                egui::TextStyle::Small,
            )
        });
        let gap = ui.spacing().item_spacing.x;
        let tail = x_galley.as_ref().map_or(0.0, |g| gap + g.size().x);
        let size = egui::vec2(
            galley.size().x + tail + MARGIN_X * 2.0,
            galley.size().y + MARGIN_Y * 2.0,
        );
        // THE ID COMES FROM THE ALLOCATION, not from the label. egui derives a
        // fresh one per allocated widget, so two chips reading "transfer" in the
        // same list get different ids — an id built from the text does not, and
        // egui paints a red "First use of widget ID …" over the collision. Which
        // it did, on every repeated tag in the feed and on the tier ladder's
        // three identical route rows.
        let (rect, alloc) = ui.allocate_exact_size(size, Sense::hover());
        let id = alloc.id;

        ui.painter().rect(
            rect,
            CornerRadius::same(3),
            bg,
            border.map_or(Stroke::NONE, |b| Stroke::new(1.0_f32, b)),
            egui::StrokeKind::Inside,
        );
        let text_pos = egui::pos2(
            rect.left() + MARGIN_X,
            rect.center().y - galley.size().y * 0.5,
        );
        ui.painter().galley(text_pos, galley, fg);

        if let Some(x) = x_galley {
            let x_rect = egui::Rect::from_min_size(
                egui::pos2(
                    rect.right() - MARGIN_X - x.size().x,
                    rect.center().y - x.size().y * 0.5,
                ),
                x.size(),
            );
            ui.painter().galley(x_rect.min, x, fg);
            let hit = ui.interact(x_rect, id.with("remove"), Sense::click());
            if hit.on_hover_text("Remove").clicked() {
                response.removed = true;
            }
        }

        // THE TAP TARGET IS BIGGER THAN THE CHIP. A clickable chip is a real
        // control — the flow-explorer's tier chip opens the ladder — and at
        // ~18pt tall it is half of the 44pt minimum a fingertip needs. The
        // answer is to grow what is HIT rather than what is drawn: a chip
        // inflated to 44pt of painted box is not a chip any more, and that is
        // the shape `interact_size` was giving us. Expansion is vertical only,
        // because a row of chips sits shoulder to shoulder and a horizontal
        // one would have neighbours stealing each other's clicks.
        //
        // Non-clickable chips are left alone: nothing senses them, so an
        // enlarged rect would only take hits away from whatever is above.
        let hit = match self.clickable {
            true => {
                let grow = (Breakpoint::from_ui(ui).min_touch() - rect.height()).max(0.0) / 2.0;
                rect.expand2(egui::vec2(0.0, grow))
            }
            false => rect,
        };
        let body = ui.interact(hit, id, Sense::click());
        if let Some(hover) = self.hover_text {
            body.clone().on_hover_text(hover);
        }
        if self.clickable && body.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        // A click that removed the chip is not also a click ON it — without
        // this, hitting the `×` would fire the host's body handler too.
        if body.clicked() && !response.removed {
            response.clicked = true;
        }
        response
    }
}
