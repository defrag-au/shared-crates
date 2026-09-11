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

use egui::{Color32, CornerRadius, FontId, Stroke, TextStyle, Visuals};
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

    /// Gold, for the top rarity band. Not part of the accent ramp.
    pub const GOLD: Color32 = Color32::from_rgb(255, 215, 0);
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
}

// ============================================================================
// Typography
// ============================================================================

/// What a piece of text *is*, rather than how big it is.
///
/// The suite currently carries ninety-odd inline `FontId::proportional(9.0)`-style
/// literals. Named roles are what make a scale step — or a density change, or an
/// accessibility zoom — possible at all; there is no way to make the whole suite
/// one step larger today.
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

/// Which family a role renders in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Proportional,
    Monospace,
}

/// The typography axis: a size per role, a global scale, and a family per role.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypeScale {
    pub micro: f32,
    pub small: f32,
    pub body: f32,
    pub label: f32,
    pub heading: f32,
    pub numeric: f32,
    /// Multiplies every size. The accessibility and density knob.
    pub scale: f32,
    /// Family for prose roles (micro/small/body/label/heading).
    pub prose: Family,
}

impl TypeScale {
    /// Proportional prose — the `FontStrategy::proportional` sizes.
    pub const fn proportional() -> Self {
        Self {
            micro: 9.0,
            small: 12.0,
            body: 14.0,
            label: 14.0,
            heading: 20.0,
            numeric: 13.0,
            scale: 1.0,
            prose: Family::Proportional,
        }
    }

    /// Monospace throughout — the dashboard feel, matching
    /// `FontStrategy::monospace`.
    pub const fn monospace() -> Self {
        Self {
            micro: 9.0,
            small: 11.0,
            body: 13.0,
            label: 13.0,
            heading: 16.0,
            numeric: 13.0,
            scale: 1.0,
            prose: Family::Monospace,
        }
    }

    /// Point size for a role, with [`Self::scale`] applied.
    pub fn size(&self, role: TextRole) -> f32 {
        let base = match role {
            TextRole::Micro => self.micro,
            TextRole::Small => self.small,
            TextRole::Body => self.body,
            TextRole::Label => self.label,
            TextRole::Heading => self.heading,
            TextRole::Numeric => self.numeric,
        };
        base * self.scale
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

    /// Gap between successive widgets.
    pub fn item_spacing(self) -> egui::Vec2 {
        match self {
            Self::Compact => egui::vec2(6.0, 4.0),
            Self::Comfortable => egui::vec2(8.0, 6.0),
            Self::Spacious => egui::vec2(12.0, 10.0),
        }
    }

    /// Padding inside a button.
    pub fn button_padding(self) -> egui::Vec2 {
        match self {
            Self::Compact => egui::vec2(8.0, 4.0),
            Self::Comfortable => egui::vec2(12.0, 6.0),
            Self::Spacious => egui::vec2(16.0, 10.0),
        }
    }

    /// Height of one row in a list or table.
    pub fn row_height(self) -> f32 {
        match self {
            Self::Compact => 20.0,
            Self::Comfortable => 24.0,
            Self::Spacious => 32.0,
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
// Geometry
// ============================================================================

/// Which corner rounding a surface takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Radius {
    /// Chips, pills, inline tags.
    Small,
    /// Cards, panels — the common case.
    Medium,
    /// Modals, drawers, hero surfaces.
    Large,
}

impl Radius {
    pub const ALL: &'static [Radius] = &[Radius::Small, Radius::Medium, Radius::Large];
}

/// The geometry axis: a rounding ramp and the border weight.
///
/// The suite carries sixty-odd inline `CornerRadius::same(3|4|6|8)` literals.
/// Collapsing them onto three named steps is what makes "sharp industrial" versus
/// "soft product" a one-value swap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    pub radius_small: u8,
    pub radius_medium: u8,
    pub radius_large: u8,
    /// Border weight. One pixel throughout today — see [`hairline`].
    pub border_width: f32,
}

impl Geometry {
    /// Today's de-facto values, read off the existing call sites.
    pub const fn tokyo_night() -> Self {
        Self {
            radius_small: 3,
            radius_medium: 4,
            radius_large: 8,
            border_width: 1.0,
        }
    }

    /// Fully square — proves the axis is honoured, and a useful test fixture.
    pub const fn square() -> Self {
        Self {
            radius_small: 0,
            radius_medium: 0,
            radius_large: 0,
            border_width: 1.0,
        }
    }

    pub fn corner(&self, r: Radius) -> CornerRadius {
        CornerRadius::same(match r {
            Radius::Small => self.radius_small,
            Radius::Medium => self.radius_medium,
            Radius::Large => self.radius_large,
        })
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
    pub geometry: Geometry,
    pub motion: MotionTokens,
}

impl Theme {
    /// The palette and metrics this crate has always shipped.
    pub const fn tokyo_night() -> Self {
        Self {
            name: "tokyo night",
            color: ColorTokens::tokyo_night(),
            text: TypeScale::proportional(),
            density: Density::Comfortable,
            geometry: Geometry::tokyo_night(),
            motion: MotionTokens::standard(),
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

    /// Every preset. Contrast floors are asserted across all of these, so a new
    /// theme cannot ship below AA — add yours here or it is not covered.
    pub const PRESETS: &'static [fn() -> Theme] = &[Theme::tokyo_night, Theme::tokyo_night_mono];

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

    /// Shorthand for a corner radius.
    pub fn corner(&self, r: Radius) -> CornerRadius {
        self.geometry.corner(r)
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

    let mut style = (*ctx.global_style()).clone();
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

    style.spacing.item_spacing = theme.density.item_spacing();
    style.spacing.button_padding = theme.density.button_padding();

    ctx.set_global_style(style);
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
/// **Reads the default palette, not the active theme.** This is one of the
/// named semantic ramps, and they move onto `Theme` together in the series-palette
/// pass so their separability tests travel with them. Taking a `&Theme` here
/// alone would change a dozen call sites for a third of the benefit.
pub fn rarity_rank_color(rank: u32, total: u32) -> Color32 {
    if total == 0 {
        return raw::TEXT_MUTED;
    }
    let pct = rank as f32 / total as f32;
    if pct <= 0.01 {
        raw::GOLD // top 1%
    } else if pct <= 0.05 {
        raw::ACCENT_YELLOW // amber — top 5%
    } else if pct <= 0.10 {
        raw::ACCENT_CYAN // top 10%
    } else if pct <= 0.25 {
        raw::ACCENT_GREEN // top 25%
    } else {
        raw::TEXT_MUTED
    }
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
    pub fn type_scale(&self) -> TypeScale {
        match *self {
            Self::Monospace {
                body,
                small,
                heading,
                ..
            } => TypeScale {
                small,
                body,
                label: body,
                heading,
                numeric: body,
                ..TypeScale::monospace()
            },
            Self::Proportional {
                body,
                small,
                heading,
                monospace,
                ..
            } => TypeScale {
                small,
                body,
                label: body,
                heading,
                numeric: monospace,
                ..TypeScale::proportional()
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

    #[test]
    fn every_preset_is_named() {
        // `PRESETS` feeds a theme switcher and the contrast floors; an unnamed
        // entry means a blank row in both.
        for preset in Theme::PRESETS {
            assert!(!preset().name.is_empty());
        }
    }
}
