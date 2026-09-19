//! The **typography** axis: what a piece of text is, how big it is, and the
//! ramp that connects the two.
//!
//! Renderer-free, like the rest of this crate. Nothing here builds a font
//! handle — `TypeScale` answers "what point size, in which family", and each
//! front end turns that into its own type (egui's `FontId`, a macroquad
//! `TextParams`). That split is the whole reason this can be shared: the
//! numbers and the rungs are a product decision, the font handle is a renderer
//! detail.
//!
//! # Why it moved here
//!
//! `egui-widgets` grew this ramp to kill ~294 inline `.size(11.0)` literals,
//! and it worked — but `macroquad-widgets` had no ramp at all, so every size on
//! that side was still a literal and *no* theme switch could reach them. The
//! options were to copy the ramp or to move it. Copying is what already
//! happened to the colour model, and the cost is written up in this crate's
//! header: two definitions, measured drift, and a contrast suite that was not
//! quite measuring what the palette computed.
//!
//! So it moved. `egui-widgets` re-exports these types, so its call sites are
//! unchanged.

/// What a piece of text *is*, rather than how big it is.
///
/// Pairs with [`TextSize`], which says how big. The suite used to carry ~294
/// inline `.size(11.0)` / `FontId::proportional(9.0)` literals and no way to
/// make the whole thing one step larger; both now resolve through
/// [`TextScale`], so a theme moves headings and tick labels together.
///
/// Prefer a role when the call site knows what its text *is* — that is the
/// information a step cannot carry, and it is what lets
/// [`TypeScale::monospace`] re-rung the roles without touching the ramp.
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
/// 294 judgement calls, which is how a migration mispairs things at scale.
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

    /// The nearest step to a raw point size. **Ties round up** — text that comes
    /// out a point large is legible, text that snaps down may not be.
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

    /// The canvas ramp — what the macroquad surfaces measurably already use.
    ///
    /// **Derived, not designed.** The obvious guess is that a game canvas or a
    /// phone mint flow wants a much larger ramp than a dashboard; the sources
    /// say otherwise. The 73 point sizes in `macroquad-widgets` land on exactly
    /// eight values:
    ///
    /// ```text
    /// 14.0 × 22   16.0 × 18   13.0 × 14   12.0 × 8
    /// 11.0 × 7    15.0 × 2    17.0 × 1    22.0 × 1
    /// ```
    ///
    /// so the ramp is those values. Every site but one lands on a step exactly,
    /// and that one moves by 1px (17 → 18, a tie rounding up). Choosing a ramp
    /// the sources did not already use would have turned a rename into a
    /// restyle of every macroquad surface, smuggled in under a refactor — which
    /// is not a thing a migration gets to decide on its own.
    ///
    /// It is a touch tighter than [`Self::tokyo_night`] at the top (24 → 22) and
    /// looser at the bottom (9 → 11): these surfaces have no 9px dense-table
    /// tier, and never had one.
    pub const fn canvas() -> Self {
        Self {
            xs: 11.0,
            sm: 12.0,
            base: 13.0,
            md: 14.0,
            lg: 15.0,
            xl: 16.0,
            xl2: 18.0,
            xl3: 22.0,
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
    /// decisions — [`Self::monospace`] re-rungs the roles without touching the
    /// ramp.
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

    /// The [`TextScale::canvas`] ramp on the proportional rungs — the macroquad
    /// surfaces' default.
    pub const fn canvas() -> Self {
        Self {
            steps: TextScale::canvas(),
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

    /// The family a role renders in — so a call site never picks one.
    ///
    /// The renderer-free half of what used to be `TypeScale::font`: this says
    /// *which* family, each front end builds its own font handle from it.
    pub fn family(&self, role: TextRole) -> Family {
        match role {
            // Numeric is always monospace: tabular figures are the point.
            TextRole::Numeric => Family::Monospace,
            _ => self.prose,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_resolve_through_the_ramp_not_beside_it() {
        let t = TypeScale::proportional();
        for role in TextRole::ALL {
            assert_eq!(t.size(*role), t.at(t.step(*role)));
        }
    }

    /// The scale knob is the accessibility one; it must reach every role.
    #[test]
    fn scale_multiplies_every_role() {
        let base = TypeScale::proportional();
        let big = TypeScale { scale: 2.0, ..base };
        for role in TextRole::ALL {
            assert_eq!(big.size(*role), base.size(*role) * 2.0);
        }
    }

    /// Ties round up: a size that snaps down may stop being legible.
    #[test]
    fn nearest_rounds_a_tie_upwards() {
        let s = TextScale::tokyo_night();
        // 13.0 sits exactly between md (12) and lg (14).
        assert_eq!(TextSize::nearest(13.0, &s), TextSize::Lg);
        assert_eq!(TextSize::nearest(11.0, &s), TextSize::Base);
        assert_eq!(TextSize::nearest(100.0, &s), TextSize::Xl3);
    }

    /// Numeric is monospace whatever the prose family is — tabular figures are
    /// the whole reason the role is separate.
    #[test]
    fn numeric_is_monospace_regardless_of_prose_family() {
        for t in [TypeScale::proportional(), TypeScale::monospace()] {
            assert_eq!(t.family(TextRole::Numeric), Family::Monospace);
        }
        assert_eq!(
            TypeScale::proportional().family(TextRole::Body),
            Family::Proportional
        );
    }

    /// The point of `canvas`: the macroquad migration is a RENAME, not a
    /// restyle. Every size those sources used must be reachable, and the one
    /// that is not exact must be off by no more than a pixel.
    ///
    /// The literals below are the eight measured in `macroquad-widgets`. If a
    /// ramp change ever moves one of them further, this says so — which is the
    /// difference between opening the ramp out on purpose and doing it by
    /// accident while editing something else.
    #[test]
    fn the_canvas_ramp_covers_what_the_macroquad_sources_actually_used() {
        let s = TextScale::canvas();
        for used in [11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0, 22.0] {
            let snapped = s.get(TextSize::nearest(used, &s));
            assert!(
                (snapped - used).abs() <= 1.0,
                "{used}px snaps to {snapped}px — more than a pixel of drift"
            );
        }
        // Seven of the eight are exact; 17 is the tie that rounds up to 18.
        assert_eq!(s.get(TextSize::nearest(17.0, &s)), 18.0);
        assert_eq!(s.get(TextSize::nearest(14.0, &s)), 14.0);
    }

    /// `canvas` re-rungs the ramp rather than leaning on `scale`, so the
    /// accessibility knob stays free on top of it.
    #[test]
    fn canvas_changes_the_ramp_not_the_scale() {
        let c = TypeScale::canvas();
        assert_eq!(c.scale, TypeScale::proportional().scale);
        assert_ne!(c.steps, TypeScale::proportional().steps);
    }
}
