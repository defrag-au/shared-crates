//! [`Breakpoint`] — how much room a layout has, as a named size, for every
//! renderer.
//!
//! The thresholds are `egui-widgets`' (`viewport.rs`): under 700 points is
//! Compact, under 1200 Medium, and Wide beyond. 700 rather than a phone's 390
//! because a phone on its side is 844 wide and still wants the one-handed
//! layout's priorities, and because a narrow split window behaves like a phone.
//! The two renderers must agree on where a phone ends, so the number is
//! written here once; `egui-widgets` still carries its own copy, with its
//! egui-specific decisions around it, and is expected to move onto this one.
//!
//! The scalars a layout reads off it (the page gutter, the smallest thing a
//! finger can hit, how many columns of cards) are methods with an exhaustive
//! `match`, so a call site carries no branch of its own.

/// Compact ceiling, in points.
pub const COMPACT_MAX: f32 = 700.0;
/// Medium ceiling, in points.
pub const MEDIUM_MAX: f32 = 1200.0;

/// How much room the layout has. Ordered narrow to wide, so
/// `bp >= Breakpoint::Medium` reads the way it looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Breakpoint {
    /// Under 700 points: a phone, upright or on its side, or a narrow window.
    Compact,
    /// 700 to 1200 points: a tablet, a small laptop, half a screen.
    Medium,
    /// 1200 points and up: the whole desktop layout.
    Wide,
}

impl Breakpoint {
    /// Every breakpoint, narrow to wide.
    pub const ALL: [Self; 3] = [Self::Compact, Self::Medium, Self::Wide];

    /// Classify a width in points (the safe area's width, not the window's,
    /// where they differ).
    pub fn from_width(points: f32) -> Self {
        if points < COMPACT_MAX {
            Self::Compact
        } else if points < MEDIUM_MAX {
            Self::Medium
        } else {
            Self::Wide
        }
    }

    /// The breakpoint whose [`Breakpoint::label`] matches, ignoring case.
    pub fn by_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|b| b.label().eq_ignore_ascii_case(name))
    }

    /// Lower-case name, for labels, flags and debug overlays.
    pub fn label(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Medium => "medium",
            Self::Wide => "wide",
        }
    }

    /// The least height of anything tappable, in points: 44 under a finger
    /// (Apple's and Material's minimum alike), 24 under a mouse.
    pub fn min_touch(self) -> f32 {
        match self {
            Self::Compact => 44.0,
            Self::Medium | Self::Wide => 24.0,
        }
    }

    /// The page's side margin, in points. A wide gutter on a phone spends a
    /// tenth of the width on nothing.
    pub fn gutter(self) -> f32 {
        match self {
            Self::Compact => 12.0,
            Self::Medium => 16.0,
            Self::Wide => 24.0,
        }
    }

    /// How many equal columns a grid of record cards uses.
    pub fn columns(self) -> usize {
        match self {
            Self::Compact => 1,
            Self::Medium => 2,
            Self::Wide => 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_phone_either_way_up_is_compact() {
        assert_eq!(Breakpoint::from_width(390.0), Breakpoint::Compact);
        assert_eq!(Breakpoint::from_width(699.9), Breakpoint::Compact);
        assert_eq!(Breakpoint::from_width(700.0), Breakpoint::Medium);
        assert_eq!(Breakpoint::from_width(1199.0), Breakpoint::Medium);
        assert_eq!(Breakpoint::from_width(1200.0), Breakpoint::Wide);
    }

    #[test]
    fn names_round_trip() {
        for bp in Breakpoint::ALL {
            assert_eq!(Breakpoint::by_name(bp.label()), Some(bp));
        }
        assert_eq!(Breakpoint::by_name("WIDE"), Some(Breakpoint::Wide));
        assert_eq!(Breakpoint::by_name("phone"), None);
    }

    #[test]
    fn the_order_reads_narrow_to_wide() {
        assert!(Breakpoint::Compact < Breakpoint::Medium);
        assert!(Breakpoint::Wide >= Breakpoint::Medium);
    }
}
