//! Tokyo Night Dark theme — the token set every defrag egui frontend reads.
//!
//! ## Theme is DATA, not constants
//!
//! This module used to be twenty `const Color32` values. Widgets referenced them
//! directly, which meant a frontend could not re-skin the widgets it embedded: an
//! `ActivityFeed` inside the aliens app still painted Tokyo Night `TEXT_MUTED`,
//! because the widget read a const. Nine frontends had forked `theme.rs` to work
//! around that, and none of the forks could reach the shared suite.
//!
//! So a theme is now a [`Theme`] value carried on the `egui::Context`, and a
//! widget reads it through [`ThemeExt::tokens`]:
//!
//! ```ignore
//! let t = ui.tokens();
//! ui.label(RichText::new("hello").color(t.color.text_muted));
//! ```
//!
//! Swapping one value re-skins everything. `macroquad-widgets` has carried its
//! palette on `Painter` this way for a while; this is the egui side catching up.
//!
//! ## The module consts are deprecated, deliberately loudly
//!
//! Every `pub const` below still exists and still holds its original value, so
//! nothing breaks. All are `#[deprecated]` so that `cargo` enumerates the call
//! sites for us — there are several hundred across this crate and its consumers,
//! and a grep would only find the ones we already guessed at. Expect a large
//! warning count until the migration finishes; that count IS the burn-down.
//!
//! Migrate **by axis, not by widget**: a widget drawing half-themed reads as
//! broken, an axis half-themed reads as a bug you can find.
//!
//! ## Axes not yet here
//!
//! [`Theme`] carries colour, typography, density, geometry and motion. Three
//! axes are deliberately absent because they need design decisions this pass did
//! not make, and inventing them now would force a second reshape:
//!
//! - **elevation** — how a surface separates from the page (fill / border /
//!   shadow), currently hardcoded per widget as a secondary fill plus a hairline.
//! - **series palettes** — the categorical ramp and the named semantic ramps
//!   (`rarity_rank_color` here, plus `flow_ring::ring_tint`,
//!   `channel_bands::assign_colors`, `exposure_bar::ltv_risk_color` and friends).
//!   These carry hand-validated separability tests that must travel with them.
//! - **presentation hints** — need the `SlotRole` vocabulary, which belongs with
//!   the layout work rather than here.
//!
//! See `cnft.dev-workers/docs/design/EGUI_THEMING_AND_LAYOUT.md`.

use egui::{Color32, CornerRadius, FontId, Margin, Stroke, TextStyle, Ui, Visuals};
use std::sync::Arc;

// ============================================================================
// Raw palette values
// ============================================================================

/// The literal Tokyo Night values.
///
/// Private and **not** deprecated, so the token constructors below can read them
/// without tripping the deprecations on the public consts. Without this split,
/// `ColorTokens::tokyo_night` would warn about the very API it replaces.
mod raw {
    use egui::Color32;

    pub const BG_PRIMARY: Color32 = Color32::from_rgb(26, 27, 38);
    pub const BG_SECONDARY: Color32 = Color32::from_rgb(36, 40, 59);
    pub const BG_HIGHLIGHT: Color32 = Color32::from_rgb(41, 46, 66);

    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(192, 202, 245);
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(169, 177, 214);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(139, 149, 196);

    pub const ACCENT_BLUE: Color32 = Color32::from_rgb(122, 162, 247);
    pub const ACCENT_CYAN: Color32 = Color32::from_rgb(125, 207, 255);
    pub const ACCENT_GREEN: Color32 = Color32::from_rgb(158, 206, 106);
    pub const ACCENT_YELLOW: Color32 = Color32::from_rgb(224, 175, 104);
    pub const ACCENT_ORANGE: Color32 = Color32::from_rgb(255, 158, 100);
    pub const ACCENT_RED: Color32 = Color32::from_rgb(247, 118, 142);
    pub const ACCENT_MAGENTA: Color32 = Color32::from_rgb(187, 154, 247);

    pub const BORDER: Color32 = Color32::from_rgb(65, 72, 104);

    // `GOLD` lived here, "for the top rarity band. Not part of the accent ramp."
    // It is gone because the rarity band is no longer a hand-picked hue — it is
    // the top of the theme's ordinal ramp (`SeriesPalette::ordinal`), which is
    // what made the band monotonic. A colour that belongs to exactly one ramp
    // belongs in that ramp.
}

// ============================================================================
// Deprecated module consts — the migration worklist
// ============================================================================

#[deprecated(note = "use `ui.tokens().color.bg_primary`")]
pub const BG_PRIMARY: Color32 = raw::BG_PRIMARY;
#[deprecated(note = "use `ui.tokens().color.bg_secondary`")]
pub const BG_SECONDARY: Color32 = raw::BG_SECONDARY;
#[deprecated(note = "use `ui.tokens().color.bg_highlight`")]
pub const BG_HIGHLIGHT: Color32 = raw::BG_HIGHLIGHT;

#[deprecated(note = "use `ui.tokens().color.text_primary`")]
pub const TEXT_PRIMARY: Color32 = raw::TEXT_PRIMARY;
/// Tokyo Night `fg_dark` — ~6.9:1 on the secondary background.
#[deprecated(note = "use `ui.tokens().color.text_secondary`")]
pub const TEXT_SECONDARY: Color32 = raw::TEXT_SECONDARY;
/// De-emphasis tier, but still AA at small sizes — ~5.0:1 on the secondary
/// background. (The previous `#565F89` sat at 2.2-2.8:1 and carried real copy.)
#[deprecated(note = "use `ui.tokens().color.text_muted`")]
pub const TEXT_MUTED: Color32 = raw::TEXT_MUTED;

#[deprecated(note = "use `ui.tokens().color.accent_blue`")]
pub const ACCENT_BLUE: Color32 = raw::ACCENT_BLUE;
#[deprecated(note = "use `ui.tokens().color.accent_cyan`")]
pub const ACCENT_CYAN: Color32 = raw::ACCENT_CYAN;
#[deprecated(note = "use `ui.tokens().color.accent_green`")]
pub const ACCENT_GREEN: Color32 = raw::ACCENT_GREEN;
#[deprecated(note = "use `ui.tokens().color.accent_yellow`")]
pub const ACCENT_YELLOW: Color32 = raw::ACCENT_YELLOW;
#[deprecated(note = "use `ui.tokens().color.accent_orange`")]
pub const ACCENT_ORANGE: Color32 = raw::ACCENT_ORANGE;
#[deprecated(note = "use `ui.tokens().color.accent_red`")]
pub const ACCENT_RED: Color32 = raw::ACCENT_RED;
#[deprecated(note = "use `ui.tokens().color.accent_magenta`")]
pub const ACCENT_MAGENTA: Color32 = raw::ACCENT_MAGENTA;

/// Primary call-to-action accent.
#[deprecated(note = "use `ui.tokens().color.accent`")]
pub const ACCENT: Color32 = raw::ACCENT_BLUE;
/// Positive / success status.
#[deprecated(note = "use `ui.tokens().color.success`")]
pub const SUCCESS: Color32 = raw::ACCENT_GREEN;
/// Warning / caution status.
#[deprecated(note = "use `ui.tokens().color.warning`")]
pub const WARNING: Color32 = raw::ACCENT_YELLOW;
/// Error / danger status.
#[deprecated(note = "use `ui.tokens().color.error`")]
pub const ERROR: Color32 = raw::ACCENT_RED;
/// Default border stroke colour. Deliberately its own value: when this aliased
/// the highlight background it sat at 1.24:1 against the primary background and
/// panel edges were effectively invisible.
#[deprecated(note = "use `ui.tokens().color.border`")]
pub const BORDER: Color32 = raw::BORDER;

// ============================================================================
// Colour tokens
// ============================================================================

/// The colour axis.
///
/// The semantic entries (`accent`, `success`, `warning`, `error`) are **fields,
/// not accessors that alias the ramp**. A theme must be able to say that success
/// is not green without redefining the ramp it borrows from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColorTokens {
    pub bg_primary: Color32,
    pub bg_secondary: Color32,
    pub bg_highlight: Color32,

    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,

    pub accent_blue: Color32,
    pub accent_cyan: Color32,
    pub accent_green: Color32,
    pub accent_yellow: Color32,
    pub accent_orange: Color32,
    pub accent_red: Color32,
    pub accent_magenta: Color32,

    pub accent: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub error: Color32,
    pub border: Color32,
}

impl ColorTokens {
    /// The Tokyo Night Dark palette — the values this crate has always shipped.
    pub const fn tokyo_night() -> Self {
        Self {
            bg_primary: raw::BG_PRIMARY,
            bg_secondary: raw::BG_SECONDARY,
            bg_highlight: raw::BG_HIGHLIGHT,
            text_primary: raw::TEXT_PRIMARY,
            text_secondary: raw::TEXT_SECONDARY,
            text_muted: raw::TEXT_MUTED,
            accent_blue: raw::ACCENT_BLUE,
            accent_cyan: raw::ACCENT_CYAN,
            accent_green: raw::ACCENT_GREEN,
            accent_yellow: raw::ACCENT_YELLOW,
            accent_orange: raw::ACCENT_ORANGE,
            accent_red: raw::ACCENT_RED,
            accent_magenta: raw::ACCENT_MAGENTA,
            accent: raw::ACCENT_BLUE,
            success: raw::ACCENT_GREEN,
            warning: raw::ACCENT_YELLOW,
            error: raw::ACCENT_RED,
            border: raw::BORDER,
        }
    }

    /// Every background a text colour can land on.
    ///
    /// The contrast floors have to be checked against all three, and listing
    /// them here means a new theme cannot forget one.
    pub const fn backgrounds(&self) -> [Color32; 3] {
        [self.bg_primary, self.bg_secondary, self.bg_highlight]
    }

    /// Every tier of the text ramp, for the same reason.
    pub const fn text_ramp(&self) -> [Color32; 3] {
        [self.text_primary, self.text_secondary, self.text_muted]
    }

    /// A foreground **from this palette** that reads on `fill`.
    ///
    /// For solid semantic fills — a danger chip, a status pill — where the
    /// caller knows the background and needs text that survives it. Returns
    /// whichever end of the theme's own ramp contrasts more, so the answer moves
    /// with the theme instead of being a hardcoded `Color32::WHITE`.
    ///
    /// This is what lets `ChipVariant` carry semantics rather than literals: a
    /// chip says "this is a failure", the theme says what failure looks like, and
    /// the label stays legible on whatever that turns out to be. Picking by
    /// measured ratio rather than by a luminance threshold matters for the
    /// mid-tone fills (a 60%-luminance amber) where the two are close and a
    /// threshold guesses wrong.
    pub fn on(&self, fill: Color32) -> Color32 {
        if contrast_ratio(self.bg_primary, fill) >= contrast_ratio(self.text_primary, fill) {
            self.bg_primary
        } else {
            self.text_primary
        }
    }
}

/// `color` at `alpha` (0–255) — a scrim, a wash, a translucent band.
///
/// Exists because `Color32::from_rgba_premultiplied` is the wrong constructor
/// for this and reads like the right one: it requires each channel to be
/// **already** multiplied by alpha, so passing a palette colour straight in
/// produces an invalid colour that blends additively and comes out far lighter
/// than intended. That shipped once in this crate's selection wash. Taking a
/// token and an alpha, and doing the multiply internally, removes the choice.
pub fn with_alpha(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

/// WCAG relative luminance of an opaque colour.
fn relative_luminance(c: Color32) -> f32 {
    fn channel(v: u8) -> f32 {
        let v = v as f32 / 255.0;
        if v <= 0.039_28 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
}

/// WCAG contrast ratio between two opaque colours, in `1.0..=21.0`.
///
/// Public because the palette decisions this crate makes — [`ColorTokens::on`],
/// the contrast suite, a consumer picking a label colour over a chart series —
/// should all be measuring the same thing.
pub fn contrast_ratio(a: Color32, b: Color32) -> f32 {
    let (x, y) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if x > y { (x, y) } else { (y, x) };
    (hi + 0.05) / (lo + 0.05)
}

// ============================================================================
// Typography
// ============================================================================

/// What a piece of text *is*, rather than how big it is.
///
/// Pairs with [`TextSize`], which says how big. The suite used to carry ~294
/// inline `.size(11.0)` / `FontId::proportional(9.0)` literals and no way to
/// make the whole thing one step larger; both now resolve through
/// [`TextScale`], so a theme moves headings and tick labels together.
///
/// Prefer a role when the call site knows what its text *is* — that is the
/// information a step cannot carry, and it is what lets `monospace` re-rung the
/// roles without touching the ramp.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextRole {
    /// 8-9px. Axis ticks, dense table annotations. Use sparingly.
    Micro,
    /// 10-11px. Secondary labels, captions.
    Small,
    /// The default reading size.
    Body,
    /// A field or column label. Same size as body, distinct so a theme can
    /// letterspace or capitalise it without touching body copy.
    Label,
    /// Section and page headings.
    Heading,
    /// Money, counts, hashes. **Separate on purpose** — this estate renders a
    /// great deal of tabular value and wants monospace independent of body text.
    Numeric,
}

impl TextRole {
    pub const ALL: &'static [TextRole] = &[
        TextRole::Micro,
        TextRole::Small,
        TextRole::Body,
        TextRole::Label,
        TextRole::Heading,
        TextRole::Numeric,
    ];
}

/// A step on the type ramp.
///
/// # Steps AND roles, and why both
///
/// [`TextRole`] says what a piece of text *is*; `TextSize` says how big it is.
/// The suite needs both because it already had 294 `.size(11.0)`-style literals
/// that carry **no** semantics — and inventing one for each of them would be
/// 294 judgement calls, which is how a migration mispairs things at scale. The
/// same argument [`Radius`] makes.
///
/// So: a new call site that knows what its text is should take the role and let
/// the theme size it. A migrated site takes the nearest step. Roles resolve
/// *through* this ramp ([`TypeScale::size`]), so there is one set of numbers
/// rather than two that drift.
///
/// # Why these eight
///
/// They are what the estate uses. The 294 literals land on:
///
/// ```text
/// 11.0 × 89   10.0 × 77   9.0 × 43   12.0 × 41   13.0 × 11   14.0 × 11
///  8.0 × 6    18.0 × 5   15.0 × 4   16.0 × 4    8.5 × 4    20.0 × 3 …
/// ```
///
/// 270 of them sit exactly on `{9, 10, 11, 12, 14, 16, 20, 24}`, and nothing
/// moves by more than 2px. Note the body of the ramp is 1px apart: dense
/// dashboard chrome genuinely distinguishes 10 from 11, and a coarser ramp
/// would flatten a distinction the suite is already making 166 times.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextSize {
    /// 9px. Axis ticks, dense annotations.
    Xs,
    /// 10px.
    Sm,
    /// 11px. The commonest size in the suite.
    Base,
    /// 12px.
    Md,
    /// 14px. Comfortable reading.
    Lg,
    /// 16px.
    Xl,
    /// 20px. Section headings.
    Xl2,
    /// 24px. Page titles.
    Xl3,
}

impl TextSize {
    pub const ALL: &'static [TextSize] = &[
        TextSize::Xs,
        TextSize::Sm,
        TextSize::Base,
        TextSize::Md,
        TextSize::Lg,
        TextSize::Xl,
        TextSize::Xl2,
        TextSize::Xl3,
    ];

    /// The nearest step to a raw point size. **Ties round up**, matching
    /// [`Radius::nearest`] and [`Space::nearest`] — text that comes out a point
    /// large is legible, text that snaps down may not be.
    pub fn nearest(px: f32, scale: &TextScale) -> Self {
        let mut best = TextSize::Xs;
        let mut best_gap = f32::MAX;
        for step in Self::ALL {
            let gap = (scale.get(*step) - px).abs();
            if gap <= best_gap {
                best_gap = gap;
                best = *step;
            }
        }
        best
    }
}

/// What each [`TextSize`] step is worth — Tailwind's `theme.fontSize`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextScale {
    pub xs: f32,
    pub sm: f32,
    pub base: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
    pub xl2: f32,
    pub xl3: f32,
}

impl TextScale {
    /// The sizes the suite already used, in their observed proportions.
    pub const fn tokyo_night() -> Self {
        Self {
            xs: 9.0,
            sm: 10.0,
            base: 11.0,
            md: 12.0,
            lg: 14.0,
            xl: 16.0,
            xl2: 20.0,
            xl3: 24.0,
        }
    }

    /// Everything a size up, with the ramp opening out at the top rather than
    /// scaling uniformly — which is what [`TypeScale::scale`] already does, and
    /// is a different thing. Uniform scaling keeps a dense 9px tick 9/11ths of
    /// the body size forever; this closes that gap, so the smallest text gains
    /// proportionally more.
    pub const fn large() -> Self {
        Self {
            xs: 11.0,
            sm: 12.0,
            base: 13.0,
            md: 14.0,
            lg: 16.0,
            xl: 18.0,
            xl2: 22.0,
            xl3: 26.0,
        }
    }

    pub fn get(&self, s: TextSize) -> f32 {
        match s {
            TextSize::Xs => self.xs,
            TextSize::Sm => self.sm,
            TextSize::Base => self.base,
            TextSize::Md => self.md,
            TextSize::Lg => self.lg,
            TextSize::Xl => self.xl,
            TextSize::Xl2 => self.xl2,
            TextSize::Xl3 => self.xl3,
        }
    }
}

/// Which family a role renders in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Proportional,
    Monospace,
}

/// The typography axis: the [`TextScale`] ramp, which role sits on which step,
/// a global multiplier, and a family per role.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypeScale {
    /// The ramp. Roles resolve through it, so a theme that opens the ramp out
    /// moves headings and tick labels together instead of one at a time.
    pub steps: TextScale,
    /// Which step each role takes. Separate from the ramp because *what a
    /// heading is worth* and *which rung a heading stands on* are different
    /// decisions — `monospace` re-rungs the roles without touching the ramp.
    pub micro: TextSize,
    pub small: TextSize,
    pub body: TextSize,
    pub label: TextSize,
    pub heading: TextSize,
    pub numeric: TextSize,
    /// Multiplies every size. The accessibility and density knob.
    pub scale: f32,
    /// Family for prose roles (micro/small/body/label/heading).
    pub prose: Family,
}

impl TypeScale {
    /// Proportional prose — the `FontStrategy::proportional` sizes.
    pub const fn proportional() -> Self {
        Self {
            steps: TextScale::tokyo_night(),
            micro: TextSize::Xs,    // 9
            small: TextSize::Md,    // 12
            body: TextSize::Lg,     // 14
            label: TextSize::Lg,    // 14
            heading: TextSize::Xl2, // 20
            numeric: TextSize::Md,  // 12
            scale: 1.0,
            prose: Family::Proportional,
        }
    }

    /// Monospace throughout — the dashboard feel, matching
    /// `FontStrategy::monospace`.
    ///
    /// Monospace runs wide at the same point size, so the prose roles drop a
    /// rung rather than the ramp shrinking: the ramp is the product's, the
    /// rungs are this variant's.
    pub const fn monospace() -> Self {
        Self {
            micro: TextSize::Xs,   // 9
            small: TextSize::Base, // 11
            body: TextSize::Md,    // 12
            label: TextSize::Md,   // 12
            heading: TextSize::Xl, // 16
            numeric: TextSize::Md, // 12
            prose: Family::Monospace,
            ..Self::proportional()
        }
    }

    /// The step a role stands on.
    pub fn step(&self, role: TextRole) -> TextSize {
        match role {
            TextRole::Micro => self.micro,
            TextRole::Small => self.small,
            TextRole::Body => self.body,
            TextRole::Label => self.label,
            TextRole::Heading => self.heading,
            TextRole::Numeric => self.numeric,
        }
    }

    /// Point size for a ramp step, with [`Self::scale`] applied.
    pub fn at(&self, size: TextSize) -> f32 {
        self.steps.get(size) * self.scale
    }

    /// Point size for a role, with [`Self::scale`] applied.
    pub fn size(&self, role: TextRole) -> f32 {
        self.at(self.step(role))
    }

    /// The `FontId` for a role — family included, so a call site never picks one.
    pub fn font(&self, role: TextRole) -> FontId {
        let size = self.size(role);
        let family = match role {
            // Numeric is always monospace: tabular figures are the point.
            TextRole::Numeric => Family::Monospace,
            _ => self.prose,
        };
        match family {
            Family::Proportional => FontId::proportional(size),
            Family::Monospace => FontId::monospace(size),
        }
    }
}

// ============================================================================
// Density
// ============================================================================

/// How tightly the UI packs.
///
/// A named decision, not an `is_compact()` predicate and not a `bool` parameter:
/// "compact" currently appears ad hoc in two dozen modules with no shared meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Density {
    /// Maximum information per pixel. Dashboards, dense tables.
    Compact,
    /// The default.
    #[default]
    Comfortable,
    /// Touch targets and reading surfaces.
    Spacious,
}

impl Density {
    pub const ALL: &'static [Density] =
        &[Density::Compact, Density::Comfortable, Density::Spacious];

    /// Gap between successive widgets, **on the default spacing ramp**.
    ///
    /// Derived rather than declared, so there is one definition of "a gap" and
    /// not two that drift. Prefer [`Theme::item_spacing`], which uses the theme's
    /// own [`SpaceScale`]; this exists for a `Density` held on its own and is
    /// equivalent whenever that ramp is [`SpaceScale::tokyo_night`].
    pub fn item_spacing(self) -> egui::Vec2 {
        Self::item_spacing_on(&SpaceScale::tokyo_night(), self)
    }

    /// Padding inside a button, on the default spacing ramp. See
    /// [`Self::item_spacing`] for why this is derived.
    pub fn button_padding(self) -> egui::Vec2 {
        Self::button_padding_on(&SpaceScale::tokyo_night(), self)
    }

    fn item_spacing_on(scale: &SpaceScale, density: Self) -> egui::Vec2 {
        let m = density.multiplier();
        egui::vec2(scale.get(Space::Md) * m, scale.get(Space::Base) * m)
    }

    fn button_padding_on(scale: &SpaceScale, density: Self) -> egui::Vec2 {
        let m = density.multiplier();
        egui::vec2(scale.get(Space::Xl) * m, scale.get(Space::Base) * m)
    }

    /// Height of one row in a list or table.
    ///
    /// **Not** on the spacing ramp, deliberately. A row height is a hit target
    /// with a usability floor, not a gap: scaling 24px by `Compact`'s 0.75 gives
    /// 18px, which is below a comfortable click. The steps here are chosen, and a
    /// theme that wants denser rows changes the density rather than the ramp.
    pub fn row_height(self) -> f32 {
        match self {
            Self::Compact => 20.0,
            Self::Comfortable => 24.0,
            Self::Spacious => 32.0,
        }
    }

    /// What this density multiplies the [`SpaceScale`] by.
    ///
    /// This is the knob that makes density matter. Before the spacing ramp existed
    /// density reached exactly three egui `Style` fields — [`Self::item_spacing`],
    /// [`Self::button_padding`] and [`Self::row_height`] — while the ~270 explicit
    /// `add_space` calls that do most of the actual spacing ignored it entirely.
    /// Routed through [`Theme::space`], one density change now moves every gap in
    /// the suite.
    ///
    /// The ratios are [`Self::item_spacing`]'s own (6 / 8 / 12), which is the
    /// closest existing analogue: a gap between two things. `button_padding` and
    /// `row_height` sit nearer 0.67 / 1.33 because a touch target has a floor that
    /// a gap does not.
    pub fn multiplier(self) -> f32 {
        match self {
            Self::Compact => 0.75,
            Self::Comfortable => 1.0,
            Self::Spacious => 1.5,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Comfortable => "comfortable",
            Self::Spacious => "spacious",
        }
    }
}

// ============================================================================
// Spacing
// ============================================================================

/// A step on the spacing ramp — gaps, padding and margins.
///
/// # Why nine steps, when [`Radius`] has six
///
/// Because that is what the estate actually uses. The 271 literal `add_space`
/// calls in this crate land on:
///
/// ```text
/// 4.0 × 87   6.0 × 49   8.0 × 44   2.0 × 32   10.0 × 19   12.0 × 16
/// 14.0 × 6    3.0 × 5    5.0 × 3   20.0 × 3   16.0 × 3    18.0 × 2   7.0 × 1   1.0 × 1
/// ```
///
/// 247 of those 271 (91%) sit exactly on `{2, 4, 6, 8, 10, 12}` — a 2px ramp,
/// already, by convention rather than by design. Collapsing it to Tailwind's
/// coarser `{4, 8, 12, 16}` would move 98 sites; keeping the 2px granularity in
/// the body and adding two sparse tail steps moves **none by more than 2px**.
///
/// # Why not numeric steps
///
/// Tailwind names spacing numerically (`p-2`, `gap-4`) precisely because there
/// are many steps, and reserves t-shirt sizes for the short ramps. That is the
/// better model and it is not available here: Rust has no `Space::2`, and the
/// fractional steps this ramp needs (10px is 2.5 base units) render as
/// `Space::TwoAndAHalf`, which is worse than `Lg` at every call site. So the
/// t-shirt names continue up through `Xl2` / `Xl3` — the Rust spelling of
/// Tailwind's own `2xl` / `3xl`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    /// No gap. Always 0, whatever the theme or density.
    None,
    /// Hairline separation — 2px. Between a label and its value.
    Xs,
    /// 4px. The commonest gap in the suite by a wide margin.
    Sm,
    /// 6px. Between rows of a group.
    Base,
    /// 8px. Between groups.
    Md,
    /// 10px.
    Lg,
    /// 12px. Between sections.
    Xl,
    /// 16px.
    Xl2,
    /// 20px. Page gutters.
    Xl3,
}

impl Space {
    pub const ALL: &'static [Space] = &[
        Space::None,
        Space::Xs,
        Space::Sm,
        Space::Base,
        Space::Md,
        Space::Lg,
        Space::Xl,
        Space::Xl2,
        Space::Xl3,
    ];

    /// The nearest step to a raw pixel value.
    ///
    /// Same contract as [`Radius::nearest`], for the same reasons: **ties round
    /// up**, and only an exact zero returns [`Space::None`]. A caller that wanted
    /// no gap wrote `0`; anything above it asked for one, and swallowing it would
    /// read as a layout bug rather than a style choice.
    pub fn nearest(px: f32, scale: &SpaceScale) -> Self {
        if px <= 0.0 {
            return Space::None;
        }
        let mut best = Space::Xs;
        let mut best_gap = f32::MAX;
        for step in [
            Space::Xs,
            Space::Sm,
            Space::Base,
            Space::Md,
            Space::Lg,
            Space::Xl,
            Space::Xl2,
            Space::Xl3,
        ] {
            let gap = (scale.get(step) - px).abs();
            // `<=` so a later — therefore roomier — step wins a tie.
            if gap <= best_gap {
                best_gap = gap;
                best = step;
            }
        }
        best
    }
}

/// What each [`Space`] step is worth — Tailwind's `theme.spacing`.
///
/// Read through [`Theme::space`], never directly, so [`Density::multiplier`] gets
/// applied. [`Self::get`] is the unscaled value and exists for [`Space::nearest`]
/// and for tests.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpaceScale {
    pub xs: f32,
    pub sm: f32,
    pub base: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
    pub xl2: f32,
    pub xl3: f32,
}

impl SpaceScale {
    /// The values the suite already used, in their observed proportions — a 2px
    /// ramp to 12, then 16 and 20 for the sparse tail.
    pub const fn tokyo_night() -> Self {
        Self {
            xs: 2.0,
            sm: 4.0,
            base: 6.0,
            md: 8.0,
            lg: 10.0,
            xl: 12.0,
            xl2: 16.0,
            xl3: 20.0,
        }
    }

    /// A 3px ramp against the house 2px one — roomier, and the idiom of a
    /// marketplace that expects to be read rather than monitored.
    ///
    /// **1.5×, not 2×, and that is a correction rather than a preference.** The
    /// first pass doubled every step, which compounds with
    /// [`Density::multiplier`] — a `Spacious` reader on this theme got 3× the
    /// house gaps, and the order-list filter strip overflowed its row. A ramp and
    /// a density that each claim to be "the roomy one" multiply; the ramp is the
    /// design system's grid and density is the reader's knob, so the ramp moves
    /// by the smaller amount.
    pub const fn airy() -> Self {
        Self {
            xs: 3.0,
            sm: 6.0,
            base: 9.0,
            md: 12.0,
            lg: 15.0,
            xl: 18.0,
            xl2: 24.0,
            xl3: 30.0,
        }
    }

    /// The unscaled value of a step. Prefer [`Theme::space`], which applies
    /// density.
    pub fn get(&self, s: Space) -> f32 {
        match s {
            Space::None => 0.0,
            Space::Xs => self.xs,
            Space::Sm => self.sm,
            Space::Base => self.base,
            Space::Md => self.md,
            Space::Lg => self.lg,
            Space::Xl => self.xl,
            Space::Xl2 => self.xl2,
            Space::Xl3 => self.xl3,
        }
    }
}

// ============================================================================
// Geometry
// ============================================================================

/// A step on the corner-radius ramp.
///
/// # Steps, not roles
///
/// Tailwind's model, and deliberately not the one this started as. The first
/// shape was `Small` / `Medium` / `Large` meaning *chip* / *card* / *modal*,
/// which sounds tidier and is worse for two reasons:
///
/// - **Migration becomes a judgement call.** The suite carries 155 hardcoded
///   radii at 1, 2, 3, 4, 5, 6, 7, 8, 10 and 14. Mapping those onto three roles
///   means deciding what each of 155 sites *is*. Mapping them onto a ramp is
///   nearest-value, and only **six** sites shift at all.
/// - **Three steps cannot express the range.** 3 and 4 are both "card-ish" and
///   both common (22 and 44 sites); collapsing them loses a real distinction.
///
/// A component picks a step; the **theme decides what the step is worth** (see
/// [`RadiusScale`]). That is what lets one override make a whole product rounder
/// rather than re-deciding 155 call sites.
///
/// [`Radius::None`] and [`Radius::Full`] are absolutes rather than scale entries:
/// square is square, and a pill is as round as its height allows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Radius {
    /// Square. Always 0, whatever the theme.
    None,
    Xs,
    Sm,
    /// The default step.
    Base,
    Md,
    Lg,
    Xl,
    /// A pill — as round as the shape allows, whatever the theme.
    Full,
}

impl Radius {
    pub const ALL: &'static [Radius] = &[
        Radius::None,
        Radius::Xs,
        Radius::Sm,
        Radius::Base,
        Radius::Md,
        Radius::Lg,
        Radius::Xl,
        Radius::Full,
    ];

    /// The nearest step to a raw pixel value.
    ///
    /// Exists for the migration and for callers still holding a number. Snapping
    /// to the ramp **is** the intended operation — an arbitrary radius is the
    /// thing a scale is meant to eliminate.
    ///
    /// **Ties round up**, and only zero returns [`Radius::None`].
    ///
    /// Both rules exist because the default ramp has 1px gaps in places, so ties
    /// are common rather than exotic — 1 sits between `None` and `Xs`, 5 between
    /// `Base` and `Md`, 7 between `Md` and `Lg`. Without a stated rule the answer
    /// would be whichever step the loop happened to reach first, which is not a
    /// decision, it is an accident.
    ///
    /// Rounding up is the safer direction: a shape that comes out slightly too
    /// round reads as a style choice, whereas one that snaps down to square reads
    /// as a bug. The same reasoning makes `None` reachable only from an exact
    /// zero — a caller that wanted square wrote `0`; anything above it asked to
    /// be rounded.
    pub fn nearest(px: f32, scale: &RadiusScale) -> Self {
        if px <= 0.0 {
            return Radius::None;
        }
        let mut best = Radius::Xs;
        let mut best_gap = f32::MAX;
        for step in [
            Radius::Xs,
            Radius::Sm,
            Radius::Base,
            Radius::Md,
            Radius::Lg,
            Radius::Xl,
        ] {
            let gap = (scale.get(step) as f32 - px).abs();
            // `<=` so a later — therefore rounder — step wins a tie.
            if gap <= best_gap {
                best_gap = gap;
                best = step;
            }
        }
        best
    }
}

/// What each [`Radius`] step is worth — Tailwind's `theme.borderRadius`.
///
/// A theme overrides these rather than picking different steps, so "make the
/// whole product rounder" is one value change per step instead of an audit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RadiusScale {
    pub xs: u8,
    pub sm: u8,
    pub base: u8,
    pub md: u8,
    pub lg: u8,
    pub xl: u8,
}

impl RadiusScale {
    /// The values the suite already used, in their observed proportions.
    pub const fn tokyo_night() -> Self {
        Self {
            xs: 2,
            sm: 3,
            base: 4,
            md: 6,
            lg: 8,
            xl: 12,
        }
    }

    /// Every step square. Proves the axis is honoured, and a useful fixture.
    pub const fn square() -> Self {
        Self {
            xs: 0,
            sm: 0,
            base: 0,
            md: 0,
            lg: 0,
            xl: 0,
        }
    }

    /// Roughly double — the marketplace idiom, where cards are 12px and filter
    /// chips are pills.
    pub const fn round() -> Self {
        Self {
            xs: 4,
            sm: 6,
            base: 8,
            md: 12,
            lg: 16,
            xl: 24,
        }
    }

    pub fn get(&self, r: Radius) -> u8 {
        match r {
            Radius::None => 0,
            Radius::Xs => self.xs,
            Radius::Sm => self.sm,
            Radius::Base => self.base,
            Radius::Md => self.md,
            Radius::Lg => self.lg,
            Radius::Xl => self.xl,
            // `CornerRadius` is u8 per corner, so this pills anything up to
            // ~510pt tall — every chip, button and tag in the suite.
            Radius::Full => u8::MAX,
        }
    }
}

/// The geometry axis: the rounding ramp and the border weight.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    pub radius: RadiusScale,
    /// Border weight. One pixel throughout today — see [`hairline`].
    pub border_width: f32,
}

impl Geometry {
    /// Today's de-facto values, read off the existing call sites.
    pub const fn tokyo_night() -> Self {
        Self {
            radius: RadiusScale::tokyo_night(),
            border_width: 1.0,
        }
    }

    /// Fully square — proves the axis is honoured, and a useful test fixture.
    pub const fn square() -> Self {
        Self {
            radius: RadiusScale::square(),
            border_width: 1.0,
        }
    }

    pub fn corner(&self, r: Radius) -> CornerRadius {
        CornerRadius::same(self.radius.get(r))
    }

    /// A border stroke at this theme's weight.
    pub fn border(&self, color: Color32) -> Stroke {
        Stroke::new(self.border_width, color)
    }
}

// ============================================================================
// Motion
// ============================================================================

/// How much the UI is allowed to move.
///
/// `Reduced` and `None` are not decoration: `prefers-reduced-motion` is a real
/// accessibility requirement, and `None` is what makes storybook screenshots
/// deterministic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MotionMode {
    #[default]
    Full,
    /// Cross-fades survive; travel and overshoot do not.
    Reduced,
    /// Everything snaps. Screenshot mode.
    None,
}

impl MotionMode {
    pub const ALL: &'static [MotionMode] =
        &[MotionMode::Full, MotionMode::Reduced, MotionMode::None];
}

/// How long a transition takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speed {
    /// Hover, press — must feel instant.
    Fast,
    /// The default: a state change the reader should notice.
    Normal,
    /// A scene change, where deliberateness is the point.
    Slow,
}

impl Speed {
    pub const ALL: &'static [Speed] = &[Speed::Fast, Speed::Normal, Speed::Slow];
}

/// The motion axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionTokens {
    pub mode: MotionMode,
    pub fast: f32,
    pub normal: f32,
    pub slow: f32,
}

impl MotionTokens {
    pub const fn standard() -> Self {
        Self {
            mode: MotionMode::Full,
            fast: 0.08,
            normal: 0.2,
            slow: 0.4,
        }
    }

    /// Duration in seconds, with [`MotionMode`] applied — so a widget asks for a
    /// speed and never has to check the mode itself.
    pub fn duration(&self, speed: Speed) -> f32 {
        if self.mode == MotionMode::None {
            return 0.0;
        }
        let base = match speed {
            Speed::Fast => self.fast,
            Speed::Normal => self.normal,
            Speed::Slow => self.slow,
        };
        match self.mode {
            MotionMode::Full => base,
            // Halved rather than zeroed: a fade that still happens, quickly.
            MotionMode::Reduced => base * 0.5,
            MotionMode::None => 0.0,
        }
    }

    /// Whether positional travel is permitted. Reduced motion keeps fades and
    /// drops flight.
    pub fn travel_allowed(&self) -> bool {
        self.mode == MotionMode::Full
    }
}

// ============================================================================
// Series palette
// ============================================================================

// The encoding axis lives in its own module — it grew five kinds, each with a
// different invariant, and it is about *data* rather than about chrome.
// Re-exported here so `theme::SeriesPalette` keeps resolving: a theme still owns
// it, it is just no longer defined in the same file.
pub use crate::encoding::{Diverging, IdentityEnvelope, Sequential, SeriesPalette};

// ============================================================================
// Theme
// ============================================================================

/// Everything a widget needs to decide how to look.
///
/// Not `Copy` — the later axes (series palettes) will not be. Carried as an
/// `Arc<Theme>` on the context, so installing one is a pointer write and reading
/// it is a cheap clone.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub name: &'static str,
    pub color: ColorTokens,
    pub text: TypeScale,
    pub density: Density,
    /// Gaps, padding and margins. Read via [`Theme::space`], which applies
    /// [`Density::multiplier`].
    pub spacing: SpaceScale,
    pub geometry: Geometry,
    pub motion: MotionTokens,
    /// What charts encode with. Separate from [`Self::color`] on purpose — see
    /// [`SeriesPalette`].
    pub series: SeriesPalette,
}

impl Theme {
    /// The palette and metrics this crate has always shipped.
    pub const fn tokyo_night() -> Self {
        Self {
            name: "tokyo night",
            color: ColorTokens::tokyo_night(),
            text: TypeScale::proportional(),
            density: Density::Comfortable,
            spacing: SpaceScale::tokyo_night(),
            geometry: Geometry::tokyo_night(),
            motion: MotionTokens::standard(),
            series: SeriesPalette::tokyo_night(),
        }
    }

    /// Tokyo Night with monospace prose — the dashboard feel.
    pub const fn tokyo_night_mono() -> Self {
        Self {
            name: "tokyo night mono",
            text: TypeScale::monospace(),
            ..Self::tokyo_night()
        }
    }

    /// A marketplace-native dark palette, in the idiom collectors arriving from
    /// other chains already know.
    ///
    /// **A whole palette, not an accent swap.** The first pass at presets varied
    /// only `color.accent`, which the widget suite reads 35 times out of ~800 —
    /// about 4% of what gets painted, so the switcher appeared to do nothing.
    /// A preset has to move the backgrounds and the text ramp to read as a
    /// different product.
    ///
    /// Rounder and roomier than the house theme too (radius 8/12/16 against
    /// 3/4/8, spacing on a 4px ramp against 2px): the look is as much shape and
    /// rhythm as it is colour.
    pub const fn opensea() -> Self {
        Self {
            name: "opensea",
            color: ColorTokens {
                // Near-black and neutral rather than Tokyo Night's indigo cast.
                bg_primary: Color32::from_rgb(12, 13, 16),
                bg_secondary: Color32::from_rgb(22, 24, 29),
                bg_highlight: Color32::from_rgb(34, 37, 44),

                text_primary: Color32::from_rgb(247, 248, 249),
                text_secondary: Color32::from_rgb(180, 186, 196),
                text_muted: Color32::from_rgb(142, 150, 163),

                accent_blue: Color32::from_rgb(59, 142, 240),
                accent_cyan: Color32::from_rgb(56, 189, 248),
                accent_green: Color32::from_rgb(52, 199, 123),
                accent_yellow: Color32::from_rgb(245, 181, 68),
                accent_orange: Color32::from_rgb(251, 146, 60),
                accent_red: Color32::from_rgb(244, 88, 110),
                accent_magenta: Color32::from_rgb(192, 132, 252),

                accent: Color32::from_rgb(59, 142, 240),
                success: Color32::from_rgb(52, 199, 123),
                warning: Color32::from_rgb(245, 181, 68),
                error: Color32::from_rgb(244, 88, 110),
                // Lighter than the reference UI's own hairlines, which sit near
                // 1.4:1 against the page — under this crate's visibility floor.
                // `border_is_visible` is the negotiation point, not the source.
                border: Color32::from_rgb(56, 61, 71),
            },
            // Bigger type as well as roomier spacing: this is a browsing
            // surface, not a monitoring one, and the two have to move together
            // or the result is small text floating in a lot of space.
            text: TypeScale {
                steps: TextScale::large(),
                ..TypeScale::proportional()
            },
            spacing: SpaceScale::airy(),
            geometry: Geometry {
                radius: RadiusScale::round(),
                border_width: 1.0,
            },
            series: SeriesPalette::opensea(),
            ..Self::tokyo_night()
        }
    }

    /// Square corners, compact density, no overshoot — the preset that proves
    /// the **non-colour** axes are real.
    ///
    /// Exists because a switcher whose presets only vary hue demonstrates
    /// nothing about geometry, density or motion, which is most of what a theme
    /// is. Same palette as the default on purpose: everything that differs here
    /// differs in shape and rhythm.
    pub const fn industrial() -> Self {
        Self {
            name: "industrial",
            geometry: Geometry::square(),
            density: Density::Compact,
            ..Self::tokyo_night()
        }
    }

    /// Every preset. Contrast floors are asserted across all of these, so a new
    /// theme cannot ship below AA — add yours here or it is not covered.
    /// Every preset. Contrast floors run across all of these, so a new theme
    /// cannot ship below AA — add yours here or it is not covered.
    ///
    /// Deliberately short. An earlier list carried `ember` / `iris` / `aqua` /
    /// `rose`, which varied `color.accent` and nothing else; since the suite
    /// reads that token 35 times out of ~800, they were indistinguishable from
    /// the default in use. Four presets that do nothing are worse than none —
    /// they teach a reader the switcher is broken. A preset earns its place by
    /// moving the backgrounds and the text ramp.
    pub const PRESETS: &'static [fn() -> Theme] = &[
        Theme::tokyo_night,
        Theme::tokyo_night_mono,
        Theme::opensea,
        Theme::industrial,
    ];

    /// The preset whose [`Theme::name`] matches, for `?theme=` URL params and
    /// switcher round-tripping. Names are the slug: lowercase, spaces intact.
    pub fn by_name(name: &str) -> Option<Theme> {
        Self::PRESETS
            .iter()
            .map(|p| p())
            .find(|t| t.name.eq_ignore_ascii_case(name))
    }

    /// Same theme, different density — the common per-surface override.
    pub fn with_density(mut self, density: Density) -> Self {
        self.density = density;
        self
    }

    /// Same theme, no motion. For screenshots and reduced-motion readers.
    pub fn with_motion(mut self, mode: MotionMode) -> Self {
        self.motion.mode = mode;
        self
    }

    /// Shorthand for the font of a text role.
    pub fn font(&self, role: TextRole) -> FontId {
        self.text.font(role)
    }

    /// Point size for a step of the type ramp.
    ///
    /// The migration target for `.size(11.0)`-style literals. A call site that
    /// knows what its text *is* should prefer [`Theme::font`] with a
    /// [`TextRole`]; this is for the ones that only ever knew a number.
    pub fn text_size(&self, size: TextSize) -> f32 {
        self.text.at(size)
    }

    /// Same theme, a different spacing ramp.
    pub fn with_spacing(mut self, spacing: SpaceScale) -> Self {
        self.spacing = spacing;
        self
    }

    /// Same theme, a different type ramp. The rungs each role stands on are
    /// unchanged, so this opens the whole product out at once.
    pub fn with_text_scale(mut self, steps: TextScale) -> Self {
        self.text.steps = steps;
        self
    }

    /// Shorthand for a corner radius.
    pub fn corner(&self, r: Radius) -> CornerRadius {
        self.geometry.corner(r)
    }

    /// Pixels for a spacing step, **with density applied**.
    ///
    /// The only correct way to read the spacing ramp. `theme.spacing.get(step)`
    /// skips [`Density::multiplier`] and so ignores the compact/spacious setting
    /// entirely.
    pub fn space(&self, s: Space) -> f32 {
        // `None` must stay exactly zero: multiplying it is a no-op today but
        // would stop being one if a density ever carried an offset.
        match s {
            Space::None => 0.0,
            _ => self.spacing.get(s) * self.density.multiplier(),
        }
    }

    /// A square margin at a spacing step.
    pub fn margin(&self, s: Space) -> Margin {
        Margin::same(self.space(s) as i8)
    }

    /// A margin with independent horizontal and vertical steps — the shape most
    /// of the suite's frames actually want (`Margin::symmetric(10, 7)` and
    /// friends).
    pub fn margin_xy(&self, x: Space, y: Space) -> Margin {
        Margin::symmetric(self.space(x) as i8, self.space(y) as i8)
    }

    /// Gap between successive widgets, on **this theme's** ramp.
    pub fn item_spacing(&self) -> egui::Vec2 {
        Density::item_spacing_on(&self.spacing, self.density)
    }

    /// Padding inside a button, on **this theme's** ramp.
    pub fn button_padding(&self) -> egui::Vec2 {
        Density::button_padding_on(&self.spacing, self.density)
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::tokyo_night()
    }
}

// ============================================================================
// Reading the theme
// ============================================================================

/// Where the active theme lives on the context.
fn theme_id() -> egui::Id {
    egui::Id::new("egui_widgets::theme")
}

/// Read the active theme.
///
/// Implemented for both `Ui` and `Context` so a widget reads it from whatever it
/// has. Returns an `Arc` clone rather than a reference because `Context::data`
/// hands out access inside a closure and nothing can escape it.
///
/// If no theme was installed, the default is inserted and returned — a widget
/// used without [`install_theme`] renders in Tokyo Night rather than panicking.
///
/// # Why `tokens()` and not `theme()`
///
/// `egui::Context` and `egui::Ui` already have an inherent `theme()` returning
/// `egui::Theme`, which is the light/dark preference — a different thing entirely.
/// Inherent methods take precedence over trait methods, so a trait method named
/// `theme` would be silently unreachable at every call site: `ui.theme()` would
/// keep compiling and keep meaning egui's. `tokens()` cannot be shadowed, and it
/// says what it returns. **Do not "tidy" this back to `theme()`.**
pub trait ThemeExt {
    fn tokens(&self) -> Arc<Theme>;

    /// Point size for a step of the type ramp — shorthand for
    /// `self.tokens().text_size(s)`, which is what a `.size(…)` call site wants.
    ///
    /// `text_size` and not `size`: egui 0.34's `Ui` has no method by either
    /// name today, but `size` is the kind of word a UI toolkit adds, and an
    /// inherent method would silently shadow this at every call site. Same
    /// reasoning as [`ThemeExt::tokens`] itself.
    fn text_size(&self, size: TextSize) -> f32 {
        self.tokens().text_size(size)
    }
}

impl ThemeExt for egui::Context {
    fn tokens(&self) -> Arc<Theme> {
        self.data_mut(|d| {
            d.get_temp_mut_or_insert_with(theme_id(), || Arc::new(Theme::default()))
                .clone()
        })
    }
}

impl ThemeExt for egui::Ui {
    fn tokens(&self) -> Arc<Theme> {
        ThemeExt::tokens(self.ctx())
    }
}

/// Spacing shorthands, so a gap is one call rather than three.
///
/// `ui.add_space(ui.tokens().space(Space::Sm))` is what this replaces, and it
/// appears ~270 times. The long form still works and means the same thing.
///
/// # Why `gap` and not `space`
///
/// Same hazard as [`ThemeExt::tokens`], checked the same way: a trait method
/// whose name collides with an inherent `Ui` method is silently unreachable
/// forever. egui 0.34's `Ui` has no `gap`, `space` or `margin` — only
/// `add_space` and `spacing`/`spacing_mut` — so either name is free today.
/// `gap` is used because it cannot be confused with `spacing_mut()`'s
/// `Spacing` struct, which is a different thing (egui's own global metrics).
pub trait SpaceExt {
    /// Insert a gap at this step of the ramp.
    fn gap(&mut self, s: Space);

    /// Set the **horizontal** gap egui puts between successive widgets, for the
    /// rest of this `Ui`.
    ///
    /// A setter rather than an assignment because
    /// `ui.spacing_mut().item_spacing.x = ui.space(..)` cannot compile: the place
    /// expression takes the mutable borrow before the right-hand side is
    /// evaluated, so every one of the ~48 call sites would otherwise need a
    /// temporary local.
    fn set_item_gap_x(&mut self, s: Space);

    /// Set the **vertical** gap between successive widgets. See
    /// [`Self::set_item_gap_x`].
    fn set_item_gap_y(&mut self, s: Space);

    /// Pixels for a step, density applied. Shorthand for
    /// `self.tokens().space(s)`, for the cases that need the number rather than
    /// the gap — a `Vec2`, a manual `Rect`, a grid pitch.
    fn space(&self, s: Space) -> f32;
}

impl SpaceExt for egui::Ui {
    fn gap(&mut self, s: Space) {
        let px = self.tokens().space(s);
        self.add_space(px);
    }

    fn set_item_gap_x(&mut self, s: Space) {
        let px = self.tokens().space(s);
        self.spacing_mut().item_spacing.x = px;
    }

    fn set_item_gap_y(&mut self, s: Space) {
        let px = self.tokens().space(s);
        self.spacing_mut().item_spacing.y = px;
    }

    fn space(&self, s: Space) -> f32 {
        self.tokens().space(s)
    }
}

// ============================================================================
// Installing a theme
// ============================================================================

/// Install a theme: store it for [`ThemeExt::tokens`] and apply it to egui's own
/// `Style`, so unstyled `ui.label()` and stock widgets agree with the suite.
///
/// Call once at startup, after [`crate::install_defaults`] (which registers the
/// icon family the style names). Calling it again swaps the theme live — that is
/// the point of theme-as-data, and what a theme switcher does.
pub fn install_theme(ctx: &egui::Context, theme: Theme) {
    let theme = Arc::new(theme);
    ctx.data_mut(|d| d.insert_temp(theme_id(), theme.clone()));
    apply_style(ctx, &theme);
}

/// Write a theme into egui's `Style`. Separate from [`install_theme`] only so
/// the contrast test can ask what a theme does to `Visuals` without a context of
/// its own.
pub fn apply_style(ctx: &egui::Context, theme: &Theme) {
    // These are dark-only palettes, but egui's web default follows
    // prefers-color-scheme and set_global_style only writes the ACTIVE theme's
    // style — pin dark first so a light-mode device gets the same app.
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.set_global_style(style_for(theme, (*ctx.global_style()).clone()));
}

/// `base` with this theme's decisions written over it.
///
/// Split out of [`apply_style`] so a theme can be applied to **one `Ui`** rather
/// than the whole context — which is what [`scoped`] needs, and therefore what
/// lets two themes render side by side in a single frame.
pub fn style_for(theme: &Theme, base: egui::Style) -> egui::Style {
    let mut style = base;
    let c = &theme.color;

    style
        .text_styles
        .insert(TextStyle::Body, theme.font(TextRole::Body));
    style
        .text_styles
        .insert(TextStyle::Small, theme.font(TextRole::Small));
    style
        .text_styles
        .insert(TextStyle::Heading, theme.font(TextRole::Heading));
    style
        .text_styles
        .insert(TextStyle::Button, theme.font(TextRole::Body));
    style
        .text_styles
        .insert(TextStyle::Monospace, theme.font(TextRole::Numeric));

    let mut visuals = Visuals::dark();
    visuals.panel_fill = c.bg_primary;
    visuals.window_fill = c.bg_secondary;
    visuals.extreme_bg_color = c.bg_primary;
    visuals.faint_bg_color = c.bg_secondary;

    let border = theme.geometry.border(c.border);
    let text = theme.geometry.border(c.text_primary);

    // Default (unstyled) text is PRIMARY — with the secondary tier here a plain
    // `ui.label()` rendered at 3.9:1 inside any secondary-background card.
    // Widgets opt IN to de-emphasis, not out of it.
    visuals.widgets.noninteractive.bg_fill = c.bg_secondary;
    visuals.widgets.noninteractive.fg_stroke = text;
    visuals.widgets.noninteractive.bg_stroke = border;

    visuals.widgets.inactive.bg_fill = c.bg_secondary;
    visuals.widgets.inactive.fg_stroke = text;
    visuals.widgets.inactive.bg_stroke = border;

    visuals.widgets.hovered.bg_fill = c.bg_highlight;
    visuals.widgets.hovered.fg_stroke = text;
    visuals.widgets.hovered.bg_stroke = theme.geometry.border(c.accent);

    visuals.widgets.active.bg_fill = c.bg_highlight;
    visuals.widgets.active.fg_stroke = text;

    // Selection. Two traps here, both of which shipped once:
    //
    // 1. `from_rgba_premultiplied` requires each channel to be ALREADY
    //    multiplied by alpha, so it must be <= alpha. Passing (122,162,247)
    //    with alpha 40 is not a valid premultiplied colour and blends
    //    additively — the wash came out far lighter than the 16% tint
    //    intended. `from_rgba_unmultiplied` is what this wanted.
    // 2. egui's `interact_selectable` sets `fg_stroke` from
    //    `selection.stroke`, so that colour is the SELECTED LABEL'S TEXT, not
    //    a border. Leaving it as the accent put accent text on an accent wash.
    //
    // Selected text therefore uses the primary ramp, which is what everything
    // else on a tinted background uses. `selection_contrast_clears_wcag_aa`
    // pins both.
    let a = c.accent;
    visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(a.r(), a.g(), a.b(), 40);
    visuals.selection.stroke = text;

    // Derived states: egui's defaults (weak 0.6, disabled 0.5) would drop
    // even the primary tier below AA. 0.7 still clears ~4.5:1 on cards while
    // reading as de-emphasised.
    visuals.weak_text_alpha = 0.7;
    visuals.disabled_alpha = 0.7;

    style.visuals = visuals;

    // From the theme's own ramp, not `theme.density`'s default one, so a preset
    // that ships a roomier `SpaceScale` moves egui's stock widgets too — which is
    // most of what a reader notices when they flip the switcher.
    style.spacing.item_spacing = theme.item_spacing();
    style.spacing.button_padding = theme.button_padding();

    style
}

/// Render `add` under `theme`, leaving the surrounding theme untouched.
///
/// # Why this needs to do two things
///
/// A theme reaches a widget by two separate routes, and a swap that only does
/// one of them is the kind of bug that looks like the theme "half works":
///
/// 1. **[`ThemeExt::tokens`] reads `ctx.data` live**, at the moment a widget
///    asks. Swapping that alone re-tints everything a widget paints explicitly.
/// 2. **egui's own `Style` is snapshotted onto each `Ui` from its parent**, not
///    read from the context per call. So `ctx.set_global_style` mid-frame does
///    *not* reach a `Ui` that already exists — a plain `ui.label()`, button
///    padding and item spacing would all keep the outer theme.
///
/// So this swaps the context data *and* sets the child `Ui`'s style. Both are
/// restored on the way out, including if `add` panics is **not** guaranteed —
/// this is a review affordance, not a transaction.
///
/// # What it is for
///
/// Rendering the same surface under two themes in one frame, which is how a
/// reviewer sees a palette regression rather than having to flip between two
/// screenshots. The storybook's A/B mode is this function twice.
///
/// It also means a real app can preview a theme inside its own settings screen,
/// which is why this lives here and not in the storybook.
pub fn scoped<R>(ui: &mut Ui, theme: &Theme, add: impl FnOnce(&mut Ui) -> R) -> R {
    let ctx = ui.ctx().clone();
    let previous = ctx.tokens();
    let scoped = Arc::new(theme.clone());
    ctx.data_mut(|d| d.insert_temp(theme_id(), scoped.clone()));

    let out = ui
        .scope(|ui| {
            // `Ui::style` hands back `&Arc<Style>`, so this needs two derefs to
            // clone the `Style` rather than bump the `Arc`.
            ui.set_style(style_for(&scoped, (**ui.style()).clone()));
            add(ui)
        })
        .inner;

    ctx.data_mut(|d| d.insert_temp(theme_id(), previous));
    out
}

// ============================================================================
// Strokes
// ============================================================================

/// A one-pixel stroke — the border weight used throughout this crate.
///
/// Exists for two reasons, one cosmetic and one that bites.
///
/// **The weight is a decision, not a literal.** Panel edges, card outlines and
/// separators are all the same hairline; scattering `1.0` across a hundred
/// call sites means there is nowhere to change it.
///
/// **`Stroke::new` takes `impl Into<f32>`, so a bare `1.0` is ambiguous.**
/// Rustc falls back to `f32` and emits a future-compatibility warning at every
/// site — 56 of them in the storybook alone. The annotation lives here once
/// instead of `1.0_f32` forever, everywhere.
///
/// Prefer `ui.tokens().geometry.border(color)` in new code: this function cannot
/// see the active theme's border weight. Not deprecated yet — the geometry axis
/// migrates as a unit, and a warning on every border in the suite right now
/// would drown the colour burn-down.
pub fn hairline(color: Color32) -> Stroke {
    Stroke::new(1.0_f32, color)
}

/// A stroke of an explicit weight, for the cases that are deliberately not a
/// hairline (a card's rarity edge, a chart's series line). Same inference
/// problem, same one-place fix.
pub fn stroke(width: f32, color: Color32) -> Stroke {
    Stroke::new(width, color)
}

// ============================================================================
// Rarity rank colouring
// ============================================================================

/// Colour a rarity rank by its percentile within a collection.
///
/// Returns a colour that communicates scarcity at a glance:
/// - Gold for top 1%, amber for top 5%, cyan for top 10%, green for top 25%,
///   muted for everything else.
///
/// Used by offer slots, browse views, pricing panels — anywhere a `#rank`
/// label is displayed.
///
/// # Tiered thresholds, ramp colours
///
/// The **boundaries** stay hand-picked, because "top 1%" is a threshold
/// collectors name and a smooth gradient erases the edge they actually care
/// about. The **colours** now come off [`Sequential`], because the hand-picked
/// ones were not monotonic and so encoded the ranking wrongly:
///
/// ```text
/// gold   top 1%   L 0.628
/// amber  top 5%   L 0.475   <- darker than the tier BELOW it
/// cyan   top 10%  L 0.562
/// green  top 25%  L 0.525
/// muted  rest     L 0.120
/// ```
///
/// Five defensible hues that, read as a ramp, told a scanning reader that
/// top-10% outranked top-5%. Sourcing them from the ordinal ramp makes the
/// order structural — `Sequential::at` is linear in Oklab lightness, so tiers
/// at increasing `t` cannot invert, whatever palette a theme supplies.
pub fn rarity_rank_color(rank: u32, total: u32, series: &SeriesPalette) -> Color32 {
    if total == 0 {
        return series.ordinal.low;
    }
    let pct = rank as f32 / total as f32;
    let t = if pct <= 0.01 {
        1.0
    } else if pct <= 0.05 {
        0.78
    } else if pct <= 0.10 {
        0.56
    } else if pct <= 0.25 {
        0.34
    } else {
        0.0
    };
    series.ordinal.at(t)
}

// ============================================================================
// Legacy style entry point
// ============================================================================

/// Controls whether the app uses monospace or proportional fonts.
///
/// Superseded by [`TypeScale`], which has named roles rather than four sizes and
/// can be scaled as a unit. Kept because thirteen frontends call
/// [`configure_style`] with it.
pub enum FontStrategy {
    /// All text styles use monospace (dashboard feel).
    Monospace {
        body: f32,
        small: f32,
        heading: f32,
        button: f32,
    },
    /// Body/heading use proportional, monospace for code.
    Proportional {
        body: f32,
        small: f32,
        heading: f32,
        button: f32,
        monospace: f32,
    },
}

impl FontStrategy {
    /// Monospace preset matching collection-ownership defaults.
    pub fn monospace() -> Self {
        Self::Monospace {
            body: 13.0,
            small: 11.0,
            heading: 16.0,
            button: 13.0,
        }
    }

    /// Proportional preset matching rewards defaults.
    pub fn proportional() -> Self {
        Self::Proportional {
            body: 14.0,
            small: 12.0,
            heading: 20.0,
            button: 14.0,
            monospace: 13.0,
        }
    }

    /// The equivalent [`TypeScale`], so the legacy entry point and the theme
    /// agree rather than drifting.
    ///
    /// The caller's point sizes are **snapped onto the theme's ramp** rather
    /// than carried through raw. That is the whole reason the ramp exists: a
    /// strategy asking for 13.5pt body text would otherwise pin one surface off
    /// the grid forever, invisible to any theme. Snapping costs at most a
    /// couple of points and keeps one set of numbers in the product.
    pub fn type_scale(&self) -> TypeScale {
        let base = match self {
            Self::Monospace { .. } => TypeScale::monospace(),
            Self::Proportional { .. } => TypeScale::proportional(),
        };
        let snap = |px: f32| TextSize::nearest(px, &base.steps);
        match *self {
            Self::Monospace {
                body,
                small,
                heading,
                ..
            } => TypeScale {
                small: snap(small),
                body: snap(body),
                label: snap(body),
                heading: snap(heading),
                numeric: snap(body),
                ..base
            },
            Self::Proportional {
                body,
                small,
                heading,
                monospace,
                ..
            } => TypeScale {
                small: snap(small),
                body: snap(body),
                label: snap(body),
                heading: snap(heading),
                numeric: snap(monospace),
                ..base
            },
        }
    }
}

/// Apply the Tokyo Night Dark theme to an egui context.
///
/// Kept for the frontends that already call it. New code should build a
/// [`Theme`] and call [`install_theme`], which is the same work plus the ability
/// to swap it later.
pub fn configure_style(ctx: &egui::Context, fonts: FontStrategy) {
    install_theme(
        ctx,
        Theme {
            text: fonts.type_scale(),
            ..Theme::tokyo_night()
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_theme_matches_the_palette_this_crate_shipped() {
        // The migration must be invisible: every deprecated const has to equal
        // its replacement field, or widgets change appearance as they migrate.
        let t = Theme::tokyo_night();
        assert_eq!(t.color.bg_primary, raw::BG_PRIMARY);
        assert_eq!(t.color.text_muted, raw::TEXT_MUTED);
        assert_eq!(t.color.accent, raw::ACCENT_BLUE);
        assert_eq!(t.color.success, raw::ACCENT_GREEN);
        assert_eq!(t.color.warning, raw::ACCENT_YELLOW);
        assert_eq!(t.color.error, raw::ACCENT_RED);
        assert_eq!(t.color.border, raw::BORDER);
    }

    #[test]
    fn reading_the_theme_without_installing_one_yields_the_default() {
        let ctx = egui::Context::default();
        assert_eq!(*ctx.tokens(), Theme::tokyo_night());
    }

    #[test]
    fn installing_a_theme_replaces_what_widgets_read() {
        let ctx = egui::Context::default();
        install_theme(&ctx, Theme::tokyo_night().with_density(Density::Compact));
        assert_eq!(ctx.tokens().density, Density::Compact);
        // And it reached egui's own style, not just our slot.
        assert_eq!(
            ctx.global_style().spacing.item_spacing,
            Density::Compact.item_spacing()
        );
    }

    #[test]
    fn numeric_is_monospace_even_when_prose_is_not() {
        // Tabular figures are the point; a proportional theme must not take
        // them with it.
        let t = Theme::tokyo_night();
        assert_eq!(t.text.prose, Family::Proportional);
        assert_eq!(
            t.font(TextRole::Numeric).family,
            egui::FontFamily::Monospace
        );
        assert_eq!(
            t.font(TextRole::Body).family,
            egui::FontFamily::Proportional
        );
    }

    #[test]
    fn the_scale_factor_moves_every_role_together() {
        let mut t = Theme::tokyo_night();
        let before: Vec<f32> = TextRole::ALL.iter().map(|r| t.text.size(*r)).collect();
        t.text.scale = 1.5;
        for (role, was) in TextRole::ALL.iter().zip(before) {
            assert!((t.text.size(*role) - was * 1.5).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn motion_none_zeroes_every_duration() {
        // Screenshot determinism depends on this being total, not partial.
        let m = MotionTokens {
            mode: MotionMode::None,
            ..MotionTokens::standard()
        };
        for speed in Speed::ALL {
            assert_eq!(m.duration(*speed), 0.0);
        }
        assert!(!m.travel_allowed());
    }

    #[test]
    fn the_legacy_entry_point_and_the_theme_agree_on_sizes() {
        // `configure_style(FontStrategy::proportional())` must produce the same
        // text as `Theme::tokyo_night()`, or the two paths drift.
        let scale = FontStrategy::proportional().type_scale();
        let t = TypeScale::proportional();
        assert_eq!(scale.body, t.body);
        assert_eq!(scale.small, t.small);
        assert_eq!(scale.heading, t.heading);
        assert_eq!(scale.prose, t.prose);
    }

    /// The `ctx.data` half of [`scoped`]: what a widget reads through
    /// `ui.tokens()` must be the scoped theme inside, and the outer one after.
    #[test]
    fn a_scoped_theme_is_visible_inside_and_restored_after() {
        let ember = Theme {
            name: "ember",
            color: ColorTokens {
                accent: Color32::from_rgb(246, 158, 76),
                ..ColorTokens::tokyo_night()
            },
            ..Theme::tokyo_night()
        };

        egui::__run_test_ui(|ui| {
            install_theme(ui.ctx(), Theme::tokyo_night());
            let outer = ui.tokens().color.accent;

            let inner = scoped(ui, &ember, |ui| ui.tokens().color.accent);

            assert_eq!(inner, ember.color.accent, "scope did not take effect");
            assert_ne!(
                inner, outer,
                "the two themes must differ for this to prove anything"
            );
            assert_eq!(
                ui.tokens().color.accent,
                outer,
                "the surrounding theme was not restored"
            );
        });
    }

    /// The `Style` half. This is the one that silently does not happen: egui
    /// snapshots `Style` onto each `Ui` from its parent, so swapping only
    /// `ctx.data` leaves plain labels, spacing and padding on the OUTER theme.
    #[test]
    fn a_scoped_theme_also_reaches_eguis_own_style() {
        let spacious = Theme::tokyo_night().with_density(Density::Spacious);

        egui::__run_test_ui(|ui| {
            install_theme(
                ui.ctx(),
                Theme::tokyo_night().with_density(Density::Compact),
            );
            let outer = ui.style().spacing.item_spacing;

            let inner = scoped(ui, &spacious, |ui| ui.style().spacing.item_spacing);

            assert_eq!(inner, Density::Spacious.item_spacing());
            assert_ne!(inner, outer);
            assert_eq!(
                ui.style().spacing.item_spacing,
                outer,
                "the child Ui's style must not leak back out"
            );
        });
    }

    /// A switcher is only a demonstration if the presets actually differ. This
    /// catches the failure mode where every preset is a recolour of one theme —
    /// or worse, identical.
    #[test]
    fn the_presets_differ_from_each_other() {
        let all: Vec<Theme> = Theme::PRESETS.iter().map(|p| p()).collect();
        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                assert_ne!(a, b, "`{}` and `{}` are the same theme", a.name, b.name);
            }
        }
    }

    /// At least one preset must vary something that is NOT colour, or the whole
    /// "theming beyond colour" claim is untested.
    #[test]
    fn at_least_one_preset_varies_a_non_colour_axis() {
        let base = Theme::tokyo_night();
        let varies = Theme::PRESETS.iter().map(|p| p()).any(|t| {
            t.geometry != base.geometry
                || t.density != base.density
                || t.text != base.text
                || t.motion != base.motion
        });
        assert!(
            varies,
            "every preset differs only by colour — geometry/density/type/motion are unproven"
        );
    }

    #[test]
    fn every_preset_round_trips_through_its_name() {
        // `?theme=` in the storybook URL depends on this being total.
        for preset in Theme::PRESETS {
            let t = preset();
            assert_eq!(
                Theme::by_name(t.name).as_ref().map(|r| r.name),
                Some(t.name),
                "`{}` does not resolve by name",
                t.name
            );
        }
        assert!(Theme::by_name("no such theme").is_none());
    }

    /// The ramp has to cover what the suite actually used, or the migration is a
    /// judgement call rather than a snap.
    ///
    /// These are the real observed radii and their counts across 155 sites. Only
    /// the four marked shift at all — everything else lands exactly.
    #[test]
    fn the_radius_ramp_covers_the_values_the_suite_used() {
        let s = RadiusScale::tokyo_night();
        let exact = [
            (2.0, Radius::Xs),   // 8 sites
            (3.0, Radius::Sm),   // 22 sites
            (4.0, Radius::Base), // 44 sites
            (6.0, Radius::Md),   // 42 sites
            (8.0, Radius::Lg),   // 14 sites
            (12.0, Radius::Xl),
        ];
        for (px, want) in exact {
            assert_eq!(Radius::nearest(px, &s), want, "{px} should land exactly");
            assert_eq!(s.get(want) as f32, px);
        }

        // Zero is the only thing that means square.
        assert_eq!(Radius::nearest(0.0, &s), Radius::None);

        // The outliers, and what they snap to — 6 sites in total. Every one of
        // these is a tie against the default ramp, so they are all decided by the
        // round-up rule rather than by distance.
        assert_eq!(Radius::nearest(1.0, &s), Radius::Xs); // 1 -> 2
        assert_eq!(Radius::nearest(5.0, &s), Radius::Md); // 5 -> 6
        assert_eq!(Radius::nearest(7.0, &s), Radius::Lg); // 7 -> 8
        assert_eq!(Radius::nearest(10.0, &s), Radius::Xl); // 10 -> 12
        assert_eq!(Radius::nearest(14.0, &s), Radius::Xl); // 14 -> 12
    }

    /// The point of a scale: a theme moves every step at once, so a component
    /// that picked `Base` gets rounder without being touched.
    #[test]
    fn overriding_the_scale_moves_every_step() {
        let house = Theme::tokyo_night();
        let round = Theme::opensea();
        for step in Radius::ALL {
            let (a, b) = (
                house.geometry.radius.get(*step),
                round.geometry.radius.get(*step),
            );
            assert!(
                b >= a,
                "`{step:?}` did not get rounder: {a} -> {b}",
                step = step
            );
        }
        // And at least one step must actually differ, or the override is inert.
        assert_ne!(house.geometry.radius, round.geometry.radius);
    }

    /// `None` and `Full` are absolutes, not scale entries — square is square, and
    /// a pill is a pill, whatever the theme says.
    #[test]
    fn none_and_full_ignore_the_theme() {
        for preset in Theme::PRESETS {
            let g = preset().geometry;
            assert_eq!(g.corner(Radius::None), CornerRadius::same(0));
            assert_eq!(g.corner(Radius::Full), CornerRadius::same(u8::MAX));
        }
    }

    #[test]
    fn every_preset_is_named() {
        // `PRESETS` feeds a theme switcher and the contrast floors; an unnamed
        // entry means a blank row in both.
        for preset in Theme::PRESETS {
            assert!(!preset().name.is_empty());
        }
    }

    /// Same contract as the other two ramps: these are the real observed sizes
    /// across 294 `.size(…)` / `FontId::*(…)` literals, and 270 land exactly.
    #[test]
    fn the_text_ramp_covers_the_sizes_the_suite_used() {
        let s = TextScale::tokyo_night();
        let exact = [
            (9.0, TextSize::Xs),    // 43 sites
            (10.0, TextSize::Sm),   // 77 sites
            (11.0, TextSize::Base), // 89 sites
            (12.0, TextSize::Md),   // 41 sites
            (14.0, TextSize::Lg),   // 11 sites
            (16.0, TextSize::Xl),   // 4 sites
            (20.0, TextSize::Xl2),  // 3 sites
            (24.0, TextSize::Xl3),  // 2 sites
        ];
        for (px, want) in exact {
            assert_eq!(TextSize::nearest(px, &s), want, "{px} should land exactly");
            assert_eq!(s.get(want), px);
        }

        // The tail, and what it snaps to — nothing moves by more than 2pt.
        assert_eq!(TextSize::nearest(8.0, &s), TextSize::Xs); // 8 -> 9
        assert_eq!(TextSize::nearest(8.5, &s), TextSize::Xs); // 8.5 -> 9
        assert_eq!(TextSize::nearest(10.5, &s), TextSize::Base); // tie, rounds up
        assert_eq!(TextSize::nearest(13.0, &s), TextSize::Lg); // tie, rounds up
        assert_eq!(TextSize::nearest(15.0, &s), TextSize::Xl); // tie, rounds up
        assert_eq!(TextSize::nearest(18.0, &s), TextSize::Xl2); // tie, rounds up
        assert_eq!(TextSize::nearest(22.0, &s), TextSize::Xl3); // tie, rounds up
    }

    /// A ramp is only a ramp if it ascends. Checked because a theme may supply
    /// its own, and a non-monotonic type ramp means `Small` can render larger
    /// than `Body` — the same class of bug the ordinal colour ramp had.
    #[test]
    fn every_preset_text_ramp_ascends() {
        for preset in Theme::PRESETS {
            let t = preset();
            let mut prev = 0.0_f32;
            for step in TextSize::ALL {
                let px = t.text_size(*step);
                assert!(
                    px > prev,
                    "`{}` type ramp does not ascend at {step:?}: {prev} -> {px}",
                    t.name
                );
                prev = px;
            }
        }
    }

    /// Roles resolve **through** the ramp, so there is one set of numbers. If a
    /// role could hold its own point size the two would drift, which is exactly
    /// what happened before: the role scale said 12 for `Small` while 77 call
    /// sites said 10.
    #[test]
    fn roles_take_their_size_from_the_ramp() {
        let t = Theme::tokyo_night();
        for role in TextRole::ALL {
            let step = t.text.step(*role);
            assert_eq!(t.text.size(*role), t.text_size(step));
        }
        // And moving the ramp moves the roles with it.
        let big = Theme::tokyo_night().with_text_scale(TextScale::large());
        assert!(big.text.size(TextRole::Body) > t.text.size(TextRole::Body));
        assert!(big.text.size(TextRole::Micro) > t.text.size(TextRole::Micro));
    }

    /// Same contract as the radius ramp, and the reason the spacing ramp keeps
    /// 2px granularity instead of adopting Tailwind's 4px one: these are the real
    /// observed `add_space` values across 271 sites, and 247 of them land exactly.
    #[test]
    fn the_space_ramp_covers_the_values_the_suite_used() {
        let s = SpaceScale::tokyo_night();
        let exact = [
            (2.0, Space::Xs),   // 32 sites
            (4.0, Space::Sm),   // 87 sites
            (6.0, Space::Base), // 49 sites
            (8.0, Space::Md),   // 44 sites
            (10.0, Space::Lg),  // 19 sites
            (12.0, Space::Xl),  // 16 sites
            (16.0, Space::Xl2), // 3 sites
            (20.0, Space::Xl3), // 3 sites
        ];
        for (px, want) in exact {
            assert_eq!(Space::nearest(px, &s), want, "{px} should land exactly");
            assert_eq!(s.get(want), px);
        }

        // Zero is the only thing that means "no gap".
        assert_eq!(Space::nearest(0.0, &s), Space::None);

        // The tail, and what it snaps to — 18 sites. Every one moves by at most
        // 2px, which is the whole argument for keeping the ramp this fine.
        assert_eq!(Space::nearest(1.0, &s), Space::Xs); // 1 -> 2
        assert_eq!(Space::nearest(3.0, &s), Space::Sm); // 3 -> 4  (tie, rounds up)
        assert_eq!(Space::nearest(5.0, &s), Space::Base); // 5 -> 6  (tie)
        assert_eq!(Space::nearest(7.0, &s), Space::Md); // 7 -> 8  (tie)
        assert_eq!(Space::nearest(14.0, &s), Space::Xl2); // 14 -> 16 (tie)
        assert_eq!(Space::nearest(18.0, &s), Space::Xl3); // 18 -> 20 (tie)
    }

    /// Density is no longer three hardcoded `Vec2`s that reach three `Style`
    /// fields — it scales the whole ramp, which is what makes it visible.
    #[test]
    fn density_scales_every_spacing_step() {
        let compact = Theme::tokyo_night().with_density(Density::Compact);
        let spacious = Theme::tokyo_night().with_density(Density::Spacious);

        for step in Space::ALL {
            let (c, s) = (compact.space(*step), spacious.space(*step));
            match step {
                // The absolutes stay absolute.
                Space::None => assert_eq!((c, s), (0.0, 0.0)),
                _ => assert!(c < s, "`{step:?}` did not widen: {c} -> {s}"),
            }
        }
    }

    /// The other half of the scale's point: a theme can move the ramp itself,
    /// independently of density.
    #[test]
    fn a_theme_can_override_the_spacing_ramp() {
        let house = Theme::tokyo_night();
        let airy = Theme::opensea();
        assert_ne!(house.spacing, airy.spacing);

        for step in Space::ALL {
            let (a, b) = (house.space(*step), airy.space(*step));
            assert!(b >= a, "`{step:?}` did not get roomier: {a} -> {b}");
        }

        // And it must reach egui's own metrics, or stock widgets ignore the theme
        // — the exact failure that made the first switcher look broken.
        assert_ne!(house.item_spacing(), airy.item_spacing());
    }

    /// `Density::item_spacing()` is the default-ramp shorthand; on a default-ramp
    /// theme the two must agree, or there are two definitions of "a gap".
    #[test]
    fn the_density_shorthand_matches_the_default_ramp() {
        for density in Density::ALL {
            let t = Theme::tokyo_night().with_density(*density);
            assert_eq!(t.item_spacing(), density.item_spacing());
            assert_eq!(t.button_padding(), density.button_padding());
        }
    }

    /// `gap` has to read the *scoped* theme, not the installed one, or a spacing
    /// override stops at the first `add_space` — the same half-working failure
    /// `a_scoped_theme_also_reaches_eguis_own_style` pins for `Style`.
    #[test]
    fn gap_follows_a_scoped_theme() {
        let airy = Theme::opensea();
        egui::__run_test_ui(|ui| {
            install_theme(ui.ctx(), Theme::tokyo_night());
            let outer = ui.space(Space::Md);
            let inner = scoped(ui, &airy, |ui| ui.space(Space::Md));
            assert_eq!(outer, Theme::tokyo_night().space(Space::Md));
            assert_eq!(inner, airy.space(Space::Md));
            assert_ne!(inner, outer);
        });
    }
}
