//! `shell_layout` — the frame every screen of an app shares: the app bar at
//! the top, the navigation (a bar along the bottom on a phone, a rail down the
//! left on anything wider), and the body between them, all inside the safe
//! area.
//!
//! Pure arithmetic on the stage's size and the [`Insets`], so it is tested
//! without a window and a capture at any size is the layout a phone gets.
//!
//! - **The bars reach the screen's edges; their contents stay inside the safe
//!   area.** The app bar's band runs up under the notch so its colour fills
//!   it, and its row of controls sits below the notch. The bottom bar does the
//!   same over the home indicator.
//! - **The keyboard takes the bottom bar's place.** While it is open the bar
//!   is not laid out, as on a phone's own apps, and the body runs down to the
//!   keyboard's top, so the field being typed in is never under it.

use macroquad::prelude::{Rect, Vec2};
use ui_theme::Breakpoint;

use crate::Insets;

/// The app bar's row, in points.
pub const BAR_H: f32 = 56.0;
/// The bottom navigation bar's row, in points.
pub const NAV_H: f32 = 64.0;
/// The navigation rail's width, in points.
pub const RAIL_W: f32 = 88.0;

/// Where the navigation goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NavPlace {
    /// Along the bottom: `band` reaches the screen's edge, `row` is the part
    /// above the home indicator where the items go.
    Bottom { band: Rect, row: Rect },
    /// Down the left, beside the body.
    Rail(Rect),
}

/// See the [module docs](self).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShellLayout {
    pub breakpoint: Breakpoint,
    /// The whole stage.
    pub screen: Rect,
    /// The stage less every inset.
    pub safe: Rect,
    /// The app bar's band, from the screen's top edge: paint its background
    /// here.
    pub bar: Rect,
    /// The app bar's row of controls, inside the safe area.
    pub bar_row: Rect,
    /// Between the bars, inside the safe area. No gutter applied: a grid may
    /// run edge to edge, a list of text applies [`ShellLayout::gutter`].
    pub body: Rect,
    /// The navigation, when the app has some and the keyboard is not up.
    pub nav: Option<NavPlace>,
    /// The page's side margin at this breakpoint.
    pub gutter: f32,
}

/// The layout for a stage of `size` with `insets`; `nav` says whether the
/// screen shows the app's navigation (a detail screen often does not).
pub fn shell_layout(size: Vec2, insets: Insets, nav: bool) -> ShellLayout {
    let screen = Rect::new(0.0, 0.0, size.x, size.y);
    let safe = insets.safe(screen);
    let breakpoint = Breakpoint::from_width(safe.w);
    let bar = Rect::new(0.0, 0.0, size.x, safe.y + BAR_H);
    let bar_row = Rect::new(safe.x, safe.y, safe.w, BAR_H);
    let keyboard_up = insets.keyboard > 0.0;
    let top = bar.bottom();
    let mut body = Rect::new(safe.x, top, safe.w, (safe.bottom() - top).max(0.0));
    let nav = match (nav && !keyboard_up, breakpoint) {
        (false, _) => None,
        (true, Breakpoint::Compact) => {
            let row = Rect::new(safe.x, safe.bottom() - NAV_H, safe.w, NAV_H);
            let band = Rect::new(0.0, row.y, size.x, size.y - row.y);
            body.h = (row.y - top).max(0.0);
            Some(NavPlace::Bottom { band, row })
        }
        (true, Breakpoint::Medium | Breakpoint::Wide) => {
            let rail = Rect::new(safe.x, top, RAIL_W, body.h);
            body.x += RAIL_W;
            body.w = (body.w - RAIL_W).max(0.0);
            Some(NavPlace::Rail(rail))
        }
    };
    ShellLayout {
        breakpoint,
        screen,
        safe,
        bar,
        bar_row,
        body,
        nav,
        gutter: breakpoint.gutter(),
    }
}

#[cfg(test)]
mod tests {
    use macroquad::prelude::vec2;

    use super::*;

    const PHONE: Vec2 = Vec2::new(390.0, 844.0);

    fn notch() -> Insets {
        Insets {
            top: 47.0,
            bottom: 34.0,
            ..Insets::default()
        }
    }

    #[test]
    fn a_phone_has_its_bars_inside_the_safe_area_and_its_bands_at_the_edges() {
        let l = shell_layout(PHONE, notch(), true);
        assert_eq!(l.breakpoint, Breakpoint::Compact);
        assert_eq!(l.bar, Rect::new(0.0, 0.0, 390.0, 47.0 + BAR_H));
        assert_eq!(l.bar_row.y, 47.0);
        let Some(NavPlace::Bottom { band, row }) = l.nav else {
            panic!("a phone navigates along the bottom: {:?}", l.nav);
        };
        assert_eq!(row.bottom(), 844.0 - 34.0, "above the home indicator");
        assert_eq!(band.bottom(), 844.0, "its colour reaches the edge");
        assert_eq!(l.body.y, l.bar.bottom());
        assert_eq!(l.body.bottom(), row.y, "the body ends where the bar begins");
    }

    #[test]
    fn a_wider_screen_navigates_down_a_rail() {
        let l = shell_layout(vec2(1024.0, 768.0), Insets::default(), true);
        assert_eq!(l.breakpoint, Breakpoint::Medium);
        let Some(NavPlace::Rail(rail)) = l.nav else {
            panic!("expected a rail: {:?}", l.nav);
        };
        assert_eq!(rail.w, RAIL_W);
        assert_eq!(l.body.x, RAIL_W);
        assert_eq!(l.body.right(), 1024.0);
        assert_eq!(l.body.bottom(), 768.0);
    }

    #[test]
    fn a_phone_on_its_side_is_still_a_phone() {
        let side = Insets {
            left: 47.0,
            right: 47.0,
            bottom: 21.0,
            ..Insets::default()
        };
        let l = shell_layout(vec2(844.0, 390.0), side, true);
        // 844 less both notches is 750: past Compact, so the rail, beside the
        // notch rather than under it.
        assert_eq!(l.safe.x, 47.0);
        let Some(NavPlace::Rail(rail)) = l.nav else {
            panic!("expected a rail: {:?}", l.nav);
        };
        assert_eq!(rail.x, 47.0);
    }

    #[test]
    fn the_keyboard_takes_the_bottom_bars_place() {
        let typing = Insets {
            keyboard: 336.0,
            ..notch()
        };
        let l = shell_layout(PHONE, typing, true);
        assert_eq!(l.nav, None);
        assert_eq!(l.body.bottom(), 844.0 - 336.0);
    }

    #[test]
    fn a_detail_screen_without_navigation_has_the_body_to_the_bottom() {
        let l = shell_layout(PHONE, notch(), false);
        assert_eq!(l.nav, None);
        assert_eq!(l.body.bottom(), 844.0 - 34.0);
    }
}
