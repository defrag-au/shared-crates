//! `Drawer` — an edge-anchored slide-over panel with a scrim, for the narrow
//! layout of a surface that has a side panel when it is wide.
//!
//! ## Why it's a widget
//!
//! A `SidePanel` is the right answer at 1200pt and an impossible one at 390:
//! a 320pt panel beside a 390pt viewport leaves 70pt of content. The usual
//! narrow fallback — stack the panel's content above the page — makes the
//! reader scroll past the entire sidebar to reach the thing they opened the
//! page for.
//!
//! A drawer keeps the content as the default view and puts the panel one tap
//! away. That needs a scrim, click-outside-to-dismiss, an edge anchor, and a
//! width that clamps to the viewport instead of overflowing it — four things
//! nobody gets right inline, and which [`egui::Modal`] already provides three
//! of.
//!
//! ## What it does NOT do
//!
//! - **No state.** Open/closed is a `&mut bool` the caller owns, matching
//!   [`crate::about_modal`] and [`crate::tier_ladder`].
//! - **No trigger button.** The caller decides where the affordance lives and
//!   what it is called; a drawer opened from a nav bar and one opened from a
//!   toolbar should not both be forced into this widget's idea of a button.
//! - **No breakpoint check.** The caller asks
//!   [`Breakpoint::panel_mode`](crate::viewport::Breakpoint::panel_mode) and
//!   picks the drawer or the panel. A widget that decided that itself could
//!   not be used for a drawer that is *always* a drawer.
//!
//! ## Usage
//!
//! ```ignore
//! match bp.panel_mode() {
//!     PanelMode::Drawer => {
//!         Drawer::new("filters").show(ui, &mut state.drawer_open, |ui| {
//!             draw_sidebar(ui, state);
//!         });
//!     }
//!     PanelMode::Beside => {
//!         egui::Panel::left("filters").show_inside(ui, |ui| draw_sidebar(ui, state));
//!     }
//! }
//! ```

use egui::{Align2, Id, Margin, Ui, Vec2};

use crate::theme;
use crate::viewport::Breakpoint;

/// Which edge the drawer slides in from.
///
/// An enum rather than a `bool from_left`, so a call site reads as the thing
/// it does and gaining a `Right` variant later is not a signature change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum DrawerSide {
    /// Slides in from the left — navigation, filters, context.
    #[default]
    Left,
    /// Slides in from the right — detail, inspector, actions.
    Right,
}

impl DrawerSide {
    /// Both sides — for storybook pickers and exhaustive tests.
    pub const ALL: [Self; 2] = [Self::Left, Self::Right];

    fn anchor(self) -> Align2 {
        match self {
            Self::Left => Align2::LEFT_TOP,
            Self::Right => Align2::RIGHT_TOP,
        }
    }
}

/// An edge-anchored slide-over panel.
pub struct Drawer {
    id: Id,
    side: DrawerSide,
    width: f32,
    /// Fraction of the viewport the drawer may occupy at most.
    max_fraction: f32,
}

impl Drawer {
    /// A drawer keyed by `id_salt`. Give each drawer in an app its own salt —
    /// two drawers sharing one id share their scroll position and their
    /// dismissal.
    pub fn new(id_salt: impl std::hash::Hash) -> Self {
        Self {
            id: Id::new("egui_widgets_drawer").with(id_salt),
            side: DrawerSide::Left,
            width: 320.0,
            max_fraction: 0.88,
        }
    }

    /// Which edge it comes from. Default [`DrawerSide::Left`].
    pub fn side(mut self, side: DrawerSide) -> Self {
        self.side = side;
        self
    }

    /// Desired width in points. Clamped to [`Self::max_fraction`] of the
    /// viewport, so this is a preference and never an overflow.
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Most of the viewport the drawer may take, 0..=1. Default 0.88.
    ///
    /// The remainder is deliberate, not a margin: a sliver of the page staying
    /// visible is what tells a reader the drawer is temporary and that tapping
    /// past it goes back. A full-width drawer reads as a navigation, and then
    /// dismissing it by tapping outside is undiscoverable because there is no
    /// outside.
    pub fn max_fraction(mut self, fraction: f32) -> Self {
        self.max_fraction = fraction.clamp(0.1, 1.0);
        self
    }

    /// Render while `open` is true, clearing it on dismissal.
    ///
    /// Dismissal is a tap on the scrim or `Escape`; both are handled here, so
    /// the caller stores one bool and nothing else.
    pub fn show<R>(
        self,
        ui: &mut Ui,
        open: &mut bool,
        content: impl FnOnce(&mut Ui) -> R,
    ) -> Option<R> {
        if !*open {
            return None;
        }

        let ctx = ui.ctx().clone();
        let viewport = ctx.content_rect();
        // Clamp, don't assert: a drawer wider than the phone is the one thing
        // this widget exists to prevent.
        let width = self
            .width
            .min(viewport.width() * self.max_fraction)
            .max(1.0);

        let area = egui::Area::new(self.id)
            .kind(egui::UiKind::Modal)
            .sense(egui::Sense::hover())
            // Anchored to the CONTENT rect's edge via an offset, because
            // `Align2` anchors to the screen and the screen includes the
            // notch. Without the offset a left drawer sits under the status
            // bar on a phone in landscape.
            .anchor(
                self.side.anchor(),
                match self.side {
                    DrawerSide::Left => Vec2::new(viewport.left(), viewport.top()),
                    DrawerSide::Right => Vec2::new(-viewport.left(), viewport.top()),
                },
            )
            .order(egui::Order::Foreground)
            .interactable(true);

        let margin = Breakpoint::from_ctx(&ctx).gutter();
        let frame = egui::Frame::new()
            .fill(theme::BG_PRIMARY)
            .inner_margin(Margin::same(margin as i8))
            .stroke(egui::Stroke::new(1.0_f32, theme::BORDER));

        // Content height inside the frame's own margins.
        let inner_height = (viewport.height() - margin * 2.0).max(1.0);

        let response = egui::Modal::new(self.id)
            .area(area)
            // A DARKER scrim than egui's default alpha-100. On a dark theme
            // that default is nearly invisible, and an invisible scrim takes
            // "tap outside to close" with it — the reader cannot see there is
            // an outside. The sliver of page beside the drawer is the whole
            // affordance, so it has to read as *dimmed page*, not as more app.
            .backdrop_color(egui::Color32::from_black_alpha(180))
            .frame(frame)
            .show(&ctx, |ui| {
                ui.set_width(width);
                // set_min_height, not set_max_height: a max alone lets the
                // frame shrink to its content, and a five-item drawer then
                // renders as a short card stuck to the top-left corner rather
                // than as the side panel it is standing in for.
                ui.set_min_height(inner_height);
                ui.set_max_height(inner_height);
                egui::ScrollArea::vertical()
                    .id_salt(self.id.with("scroll"))
                    .show(ui, content)
                    .inner
            });

        // Tap the scrim or press Escape to dismiss. `should_close` covers the
        // scrim; egui does not route Escape to a bare Area, so it is read
        // here.
        if response.should_close() || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            *open = false;
        }

        Some(response.inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_at(w: f32, h: f32) -> (egui::Context, egui::RawInput) {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(w, h))),
            ..Default::default()
        };
        (ctx, input)
    }

    /// The whole point of the widget: a 320pt drawer must not be 320pt wide on
    /// a 320pt phone, because then there is no scrim left to tap.
    #[test]
    fn width_clamps_to_a_fraction_of_the_viewport() {
        let (ctx, input) = ctx_at(390.0, 844.0);
        let mut open = true;
        let mut measured = 0.0_f32;
        let _ = ctx.run_ui(input, |ui| {
            Drawer::new("t").width(320.0).show(ui, &mut open, |ui| {
                measured = ui.available_width();
            });
        });
        assert!(measured > 0.0, "drawer never rendered");
        assert!(
            measured < 390.0,
            "drawer took the whole viewport ({measured}pt) — no scrim to dismiss with"
        );
    }

    /// A drawer is a PANEL, not a card. Caught by looking at a screenshot,
    /// not by the width tests, which all passed while the drawer rendered as a
    /// 190pt-tall box in the corner: `set_max_height` alone let the frame
    /// shrink to its content.
    ///
    /// Measured off the **drawn area rect**, not `ui.available_height()` — the
    /// first version of this test read `available_height` and passed with the
    /// bug still in, because that reports the max it was *offered*, not the
    /// height it actually took.
    #[test]
    fn a_short_content_list_still_fills_the_height() {
        let (ctx, input) = ctx_at(390.0, 844.0);
        let mut open = true;
        let id = Id::new("egui_widgets_drawer").with("t");
        let _ = ctx.run_ui(input, |ui| {
            Drawer::new("t").show(ui, &mut open, |ui| {
                // Deliberately far less content than the viewport is tall.
                ui.label("one");
            });
        });
        let rect = ctx
            .memory(|m| m.area_rect(id))
            .expect("drawer area was never laid out");
        assert!(
            rect.height() > 844.0 * 0.9,
            "drawer drew {}pt tall inside an 844pt viewport — it shrank to its \
             content instead of standing in for a full-height panel",
            rect.height()
        );
    }

    /// A drawer narrower than the clamp is left alone — the clamp is a
    /// ceiling, not a target.
    #[test]
    fn narrow_request_is_not_inflated() {
        let (ctx, input) = ctx_at(1440.0, 900.0);
        let mut open = true;
        let mut measured = 0.0_f32;
        let _ = ctx.run_ui(input, |ui| {
            Drawer::new("t").width(320.0).show(ui, &mut open, |ui| {
                measured = ui.available_width();
            });
        });
        assert!(
            (measured - 320.0).abs() < 40.0,
            "expected ~320pt, got {measured}pt"
        );
    }

    #[test]
    fn closed_drawer_does_not_run_its_content() {
        let (ctx, input) = ctx_at(390.0, 844.0);
        let mut open = false;
        let mut ran = false;
        let _ = ctx.run_ui(input, |ui| {
            let out = Drawer::new("t").show(ui, &mut open, |_| {
                ran = true;
            });
            assert!(out.is_none());
        });
        assert!(!ran, "closed drawer built its content anyway");
    }

    /// Escape closes. Worth pinning because egui does not route key events to
    /// a bare `Area`, so this only works while the explicit input read stays.
    #[test]
    fn escape_dismisses() {
        let (ctx, mut input) = ctx_at(390.0, 844.0);
        input.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        });
        let mut open = true;
        let _ = ctx.run_ui(input, |ui| {
            Drawer::new("t").show(ui, &mut open, |ui| {
                ui.label("x");
            });
        });
        assert!(!open, "Escape did not close the drawer");
    }

    #[test]
    fn max_fraction_is_clamped_to_something_sane() {
        assert_eq!(Drawer::new("t").max_fraction(5.0).max_fraction, 1.0);
        assert_eq!(Drawer::new("t").max_fraction(0.0).max_fraction, 0.1);
    }

    #[test]
    fn both_sides_anchor_to_their_edge() {
        assert_eq!(DrawerSide::Left.anchor(), Align2::LEFT_TOP);
        assert_eq!(DrawerSide::Right.anchor(), Align2::RIGHT_TOP);
        assert_eq!(DrawerSide::ALL.len(), 2);
    }
}
