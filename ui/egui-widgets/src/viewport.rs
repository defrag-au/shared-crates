//! `Breakpoint` — how wide the surface is, as three named sizes rather than a
//! number every call site re-compares.
//!
//! egui has no responsive layer. Every frontend in the estate therefore laid
//! out for a desktop window and shipped whatever happened at 390px, which was
//! usually a fixed-width panel wider than the screen and a hand-painted table
//! whose columns fell to 30px. The fix is not a pile of `available_width() <
//! 600.0` checks scattered through the pages — those disagree with each other
//! within a single app, and there is no way to see what a layout does at a
//! given width without running it.
//!
//! ## What it reads
//!
//! [`Context::content_rect`](egui::Context::content_rect), **not**
//! `screen_rect` — `content_rect` excludes the notch / dynamic island / status
//! bar, so a layout keyed off it does not start behind the hardware. Width is
//! in **points**, which is CSS pixels at zoom 1.0, so a 390px phone is 390.
//!
//! ## The thresholds
//!
//! - [`Breakpoint::Compact`] — under 700. A phone in portrait, or a narrow
//!   split window. One column, no persistent side panel, touch-sized targets.
//! - [`Breakpoint::Medium`] — 700 to 1200. A tablet, a small laptop, or a
//!   half-screen window. A side panel fits, but nothing extra does.
//! - [`Breakpoint::Wide`] — 1200 and up. The layout everything was designed
//!   for.
//!
//! 700 is the Compact ceiling because it is exactly a 320pt side panel plus a
//! 380pt content column — the width below which having both stops being
//! possible. That is the decision the number exists to make, so it is set by
//! that and not by any particular device;
//! `compact_ceiling_is_where_a_sidebar_stops_fitting` pins the two together so
//! moving one has to argue with the other.
//!
//! ## Touch targets
//!
//! [`apply_touch_sizing`] raises `spacing.interact_size` and `button_padding`
//! on Compact so that **every** button in the app clears the 44pt minimum
//! without a single call site changing. The alternative — auditing several
//! thousand `ui.button` calls — is not a thing anyone finishes, and a 22×18
//! icon button is the single most common reason a page is unusable on a phone.
//!
//! ## Ask for the decision, not the size
//!
//! There is deliberately **no `is_compact()`**. A predicate collapses three
//! variants to two at every call site, which hides where `Medium` was grouped
//! with `Wide` — and, worse, a fourth variant added later still compiles and
//! silently takes the wide branch everywhere.
//!
//! So the structural questions are answered by their own small enums
//! ([`PanelMode`], [`RecordLayout`], [`HeaderLayout`]), each mapped from
//! `Breakpoint` **once, here**, by an exhaustive `match`. The call site then
//! matches on a thing that says what the decision *is*, and adding a
//! breakpoint breaks the build in the one place that should have an opinion.
//!
//! ```ignore
//! match Breakpoint::from_ui(ui).record_layout() {
//!     RecordLayout::Cards => draw_cards(ui, rows),
//!     RecordLayout::Table => draw_table(ui, rows),
//! }
//! ```
//!
//! Scalars work the same way, as methods with an exhaustive `match` inside
//! ([`Breakpoint::gutter`], [`Breakpoint::min_touch`],
//! [`Breakpoint::columns`]) — so the call site carries no branch at all.
//! Anything app-specific that this crate should not have an opinion about
//! (type sizes, copy) is a `match` on `Breakpoint` in the app, spelling out
//! `Medium | Wide` rather than reaching for `_`.

use egui::{Context, Ui};

/// Compact ceiling, in points — see the module header for why 700.
const COMPACT_MAX: f32 = 700.0;
/// Medium ceiling, in points.
const MEDIUM_MAX: f32 = 1200.0;

/// How much room the layout has, as a named size.
///
/// Ordered narrow → wide, and `PartialOrd` is derived, so `bp >=
/// Breakpoint::Medium` reads the way it looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Breakpoint {
    /// Under 700pt — a phone in portrait, or a narrow split window.
    Compact,
    /// 700–1200pt — a tablet, small laptop, or half-screen window.
    Medium,
    /// 1200pt and up — the full desktop layout.
    Wide,
}

impl Breakpoint {
    /// Every breakpoint, narrow → wide. For storybook pickers and tests that
    /// must cover the set rather than the two someone remembered.
    pub const ALL: [Self; 3] = [Self::Compact, Self::Medium, Self::Wide];

    /// Classify a width in points.
    pub fn from_width(points: f32) -> Self {
        if points < COMPACT_MAX {
            Self::Compact
        } else if points < MEDIUM_MAX {
            Self::Medium
        } else {
            Self::Wide
        }
    }

    /// Classify the context's safe content area.
    ///
    /// Uses `content_rect`, so the notch and status bar are already excluded.
    pub fn from_ctx(ctx: &Context) -> Self {
        Self::from_width(ctx.content_rect().width())
    }

    /// Classify the whole surface this `Ui` is on.
    ///
    /// Deliberately the **context**, not `ui.available_width()`: a widget
    /// inside a 300px column is not on a phone, and laying it out as though it
    /// were is how a desktop sidebar ends up with phone-sized buttons. Reach
    /// for `available_width` when the question is "does this fit here"; reach
    /// for a breakpoint when the question is "what kind of device is this".
    pub fn from_ui(ui: &Ui) -> Self {
        Self::from_ctx(ui.ctx())
    }

    /// Whether a secondary panel fits beside the content.
    ///
    /// Deliberately a named decision rather than a `bool`: the caller's
    /// question is "panel or drawer", not "is this a phone", and returning the
    /// answer means the Compact-vs-the-rest grouping is written **once, here**
    /// instead of re-decided at every call site.
    pub fn panel_mode(self) -> PanelMode {
        match self {
            Self::Compact => PanelMode::Drawer,
            Self::Medium | Self::Wide => PanelMode::Beside,
        }
    }

    /// Whether a set of records reads as a dense table or as stacked cards.
    pub fn record_layout(self) -> RecordLayout {
        match self {
            Self::Compact => RecordLayout::Cards,
            Self::Medium | Self::Wide => RecordLayout::Table,
        }
    }

    /// Whether a title and its actions share a line or stack.
    pub fn header_layout(self) -> HeaderLayout {
        match self {
            Self::Compact => HeaderLayout::Stacked,
            Self::Medium | Self::Wide => HeaderLayout::Inline,
        }
    }

    /// Minimum height for anything tappable, in points.
    ///
    /// 44 on Compact is the Apple HIG / Material minimum; both land in the
    /// same place because it is a measurement of a fingertip, not a fashion.
    pub fn min_touch(self) -> f32 {
        match self {
            Self::Compact => 44.0,
            Self::Medium | Self::Wide => 24.0,
        }
    }

    /// Padding inside a button, in points.
    ///
    /// Horizontal padding goes DOWN on compact while [`Self::min_touch`] goes
    /// up: a thumb needs 44pt of *height*, and the width is set by the label.
    /// 14pt of side padding across five nav entries is 140pt of a 366pt row —
    /// enough on its own to push a nav strip onto a second line.
    ///
    /// Public so a storybook rung can size itself from the same source the app
    /// ships. A story that hardcodes its own copy drifts, and then reports that
    /// a layout wraps when the real app fits it on one row.
    pub fn button_padding(self) -> egui::Vec2 {
        match self {
            Self::Compact => egui::vec2(9.0, 12.0),
            Self::Medium | Self::Wide => egui::vec2(12.0, 6.0),
        }
    }

    /// Page margin, in points. A 24px gutter on a 390px screen spends 12% of
    /// the width on nothing.
    pub fn gutter(self) -> f32 {
        match self {
            Self::Compact => 12.0,
            Self::Medium => 16.0,
            Self::Wide => 24.0,
        }
    }

    /// How many equal columns a grid of cards should use.
    pub fn columns(self) -> usize {
        match self {
            Self::Compact => 1,
            Self::Medium => 2,
            Self::Wide => 3,
        }
    }

    /// Lower-case name, for storybook labels and debug overlays.
    pub fn label(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Medium => "medium",
            Self::Wide => "wide",
        }
    }
}

/// Where a secondary panel goes — see [`Breakpoint::panel_mode`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PanelMode {
    /// Beside the content, as a persistent side panel.
    Beside,
    /// Over the content, as a [`crate::drawer::Drawer`] the reader opens.
    Drawer,
}

/// How a set of records reads — see [`Breakpoint::record_layout`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RecordLayout {
    /// Aligned columns, one row per record.
    Table,
    /// One stacked card per record, the whole card tappable.
    Cards,
}

/// How a title and its actions sit — see [`Breakpoint::header_layout`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HeaderLayout {
    /// Title left, actions right, on one line.
    Inline,
    /// Title on its own line, actions beneath.
    Stacked,
}

/// Raise interactive sizing on Compact so every control is tappable.
///
/// Call once per frame, before the app draws — the breakpoint can change
/// mid-session (rotation, a resized window), so this is not startup-only.
/// Writes the global style only when the value actually changes, because
/// `set_global_style` invalidates layout and doing it every frame costs a
/// repaint forever.
pub fn apply_touch_sizing(ctx: &Context, bp: Breakpoint) {
    // Sizing comes from the `Breakpoint` methods, not from constants inlined
    // here — the storybook's width ladder reads the same ones, so a rung
    // cannot drift from what the app ships and quietly report that a layout
    // wraps when it does not.
    let interact_h = bp.min_touch();
    let pad = bp.button_padding();
    let item_spacing_y = match bp {
        Breakpoint::Compact => 10.0_f32,
        Breakpoint::Medium | Breakpoint::Wide => 6.0_f32,
    };

    let current = ctx.global_style().spacing.interact_size.y;
    if (current - interact_h).abs() < f32::EPSILON {
        return;
    }

    let mut style = (*ctx.global_style()).clone();
    style.spacing.interact_size.y = interact_h;
    style.spacing.button_padding = pad;
    style.spacing.item_spacing.y = item_spacing_y;
    ctx.set_global_style(style);
}

/// Clamp a desired width to what the viewport actually has.
///
/// `Ui::set_max_width` **widens** a `Ui` when less space is available — it
/// assigns `max_rect.max.x` outright rather than taking a minimum — so
/// `ui.set_max_width(520.0)` on a 342pt phone lays the content out at 520 and
/// lets it overflow off both edges. That is not hypothetical: it shipped on
/// the `AccessGate` sign-in screen, where the tagline was clipped at both ends
/// on every phone.
///
/// ```ignore
/// ui.set_max_width(fit(ui, 520.0));
/// ```
pub fn fit(ui: &Ui, desired: f32) -> f32 {
    desired.min(ui.available_width()).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phone_widths_are_compact() {
        // iPhone SE, iPhone 15, Pixel 8, and a phone in landscape.
        for w in [320.0, 390.0, 393.0, 430.0, 667.0] {
            assert_eq!(
                Breakpoint::from_width(w),
                Breakpoint::Compact,
                "{w}pt should be compact"
            );
        }
    }

    #[test]
    fn tablet_and_small_laptop_are_medium() {
        for w in [700.0, 768.0, 1024.0, 1199.0] {
            assert_eq!(Breakpoint::from_width(w), Breakpoint::Medium, "{w}pt");
        }
    }

    #[test]
    fn desktop_is_wide() {
        for w in [1200.0, 1440.0, 2560.0] {
            assert_eq!(Breakpoint::from_width(w), Breakpoint::Wide, "{w}pt");
        }
    }

    /// The ordering is load-bearing — `bp >= Medium` is the idiom callers use
    /// to mean "has room for a side panel".
    #[test]
    fn ordering_runs_narrow_to_wide() {
        assert!(Breakpoint::Compact < Breakpoint::Medium);
        assert!(Breakpoint::Medium < Breakpoint::Wide);
        assert!(Breakpoint::from_width(390.0) < Breakpoint::from_width(1440.0));
    }

    /// A sidebar's whole reason for collapsing is that it stops leaving a
    /// usable content column. Pin that, so moving the threshold has to argue
    /// with the reason it was chosen.
    #[test]
    fn compact_ceiling_is_where_a_sidebar_stops_fitting() {
        const SIDEBAR: f32 = 320.0;
        const MIN_CONTENT: f32 = 380.0;
        // Just under the ceiling there is not room for both.
        assert!(COMPACT_MAX - 1.0 < SIDEBAR + MIN_CONTENT);
        // At the ceiling there is.
        assert!(COMPACT_MAX >= SIDEBAR + MIN_CONTENT);
    }

    /// Compact trades side padding for height. Pinned because the two move in
    /// opposite directions, which looks like a mistake and gets "fixed".
    #[test]
    fn compact_is_taller_but_narrower_than_desktop() {
        let c = Breakpoint::Compact.button_padding();
        let w = Breakpoint::Wide.button_padding();
        assert!(c.y > w.y, "compact buttons should be taller");
        assert!(
            c.x < w.x,
            "compact side padding should be TIGHTER — it is what pushes a \
             five-entry nav onto a second row"
        );
    }

    #[test]
    fn compact_targets_clear_the_44pt_minimum() {
        assert!(Breakpoint::Compact.min_touch() >= 44.0);
        // And the wide default does NOT inflate — a desktop dashboard packing
        // 30 rows on screen is a feature, not an oversight.
        assert!(Breakpoint::Wide.min_touch() < 44.0);
    }

    /// Each structural decision maps from EVERY breakpoint, and the mapping
    /// actually differs — a decision that answers the same way everywhere is
    /// not a decision, it is a constant someone forgot to inline.
    #[test]
    fn structural_decisions_are_derived_for_every_breakpoint() {
        assert_eq!(Breakpoint::Compact.panel_mode(), PanelMode::Drawer);
        assert_eq!(Breakpoint::Medium.panel_mode(), PanelMode::Beside);
        assert_eq!(Breakpoint::Wide.panel_mode(), PanelMode::Beside);

        assert_eq!(Breakpoint::Compact.record_layout(), RecordLayout::Cards);
        assert_eq!(Breakpoint::Medium.record_layout(), RecordLayout::Table);
        assert_eq!(Breakpoint::Wide.record_layout(), RecordLayout::Table);

        assert_eq!(Breakpoint::Compact.header_layout(), HeaderLayout::Stacked);
        assert_eq!(Breakpoint::Medium.header_layout(), HeaderLayout::Inline);
        assert_eq!(Breakpoint::Wide.header_layout(), HeaderLayout::Inline);
    }

    /// A phone must not be handed a layout that assumes room beside the
    /// content. Stated against the WIDTH rather than the variant, so it keeps
    /// holding if the thresholds move.
    #[test]
    fn phone_widths_never_get_a_side_panel() {
        for w in [320.0, 390.0, 430.0, 667.0] {
            assert_eq!(
                Breakpoint::from_width(w).panel_mode(),
                PanelMode::Drawer,
                "{w}pt was given a side panel"
            );
        }
    }

    #[test]
    fn all_covers_every_variant() {
        assert_eq!(Breakpoint::ALL.len(), 3);
        for bp in Breakpoint::ALL {
            // Every variant answers every question — a `_ =>` arm added later
            // that forgets one would still pass this, so assert the values are
            // distinct where they must be.
            assert!(bp.gutter() > 0.0);
            assert!(bp.columns() >= 1);
            assert!(!bp.label().is_empty());
        }
        assert!(Breakpoint::Compact.gutter() < Breakpoint::Wide.gutter());
        assert!(Breakpoint::Compact.columns() < Breakpoint::Wide.columns());
    }

    /// `fit` exists because `set_max_width` widens. If it ever stops clamping,
    /// the gate screen silently overflows again.
    #[test]
    fn fit_clamps_down_but_never_up() {
        let ctx = Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(390.0, 844.0),
            )),
            ..Default::default()
        };
        let _ = ctx.run_ui(input, |ui| {
            let avail = ui.available_width();
            // Wider than the phone → clamped to the phone.
            assert!(fit(ui, 520.0) <= avail);
            // Narrower than the phone → left alone.
            assert_eq!(fit(ui, 100.0), 100.0);
            // Never zero or negative, whatever the caller passes.
            assert!(fit(ui, -5.0) > 0.0);
        });
    }
}
