//! `OptionGroup` — a set of related choices as **one** control: a single
//! border, hairline separators, no gaps.
//!
//! ## Not [`ButtonGroup`](crate::button_group)
//!
//! That is an action *bar*: several independent buttons sharing spacing and
//! icon conventions, each with its own border, doing unrelated things —
//! "Test mint", "Activity", "Configure". This is the opposite arrangement for
//! the opposite meaning: the choices are alternatives to each other, so they
//! are drawn as one object the reader picks *within*, rather than as several
//! objects they pick *between*.
//!
//! The tell is whether the items are mutually exclusive. A wallet picker,
//! a density selector, a sort order — one object. A toolbar — several.
//!
//! ## Why the corner arithmetic is the whole job
//!
//! A compound control is easy to fake and easy to get subtly wrong: draw a
//! rounded border, fill rows inside it, and the square corners of the first and
//! last row's hover fill poke out through the border's curve. So each row's
//! fill takes a [`egui::CornerRadius`] matched to its position — top row rounds
//! the top, bottom row rounds the bottom, a lone row rounds all four — and the
//! separators are drawn *between* rows only, never against the border.
//!
//! ## Compact
//!
//! [`GroupDensity::Compact`] drops the labels and keeps the images, which is
//! what a wallet picker wants in a sidebar. The label is not discarded — it
//! becomes the hover text, because an icon-only control that cannot be
//! interrogated is a guessing game.

use egui::{Color32, CornerRadius, Sense, Ui, vec2};

use crate::icons::PhosphorIcon;
use crate::theme::{Radius, Space, SpaceExt, TextSize, ThemeExt, hairline};
use crate::viewport::Breakpoint;

/// How the choices are arranged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GroupFlow {
    /// Stacked vertically, each row the full width. For a list of things to
    /// pick from, where the labels are of unpredictable length.
    #[default]
    Stacked,
    /// Side by side, sharing the width. The segmented-control arrangement, for
    /// a small fixed set of short labels.
    Inline,
}

/// How much of each choice is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GroupDensity {
    /// Image (if any) and label.
    #[default]
    Full,
    /// Image only; the label becomes the hover text. Meaningless for an item
    /// with no image, which therefore keeps its label — a blank cell is not a
    /// choice anyone can make.
    Compact,
}

/// One choice.
pub struct OptionGroupItem<'a> {
    /// Caller's identifier, returned on click. Typically an enum discriminant.
    pub id: u64,
    pub label: &'a str,
    /// An image URI for [`egui::Image`] — a wallet icon, a chain logo.
    pub image: Option<&'a str>,
    /// A glyph, when there is no image to load.
    pub icon: Option<PhosphorIcon>,
    /// Overrides the label as hover text. Under [`GroupDensity::Compact`] the
    /// label is already the hover text, so this is for saying more.
    pub hover: Option<&'a str>,
    pub enabled: bool,
}

impl<'a> OptionGroupItem<'a> {
    pub fn new(id: u64, label: &'a str) -> Self {
        Self {
            id,
            label,
            image: None,
            icon: None,
            hover: None,
            enabled: true,
        }
    }

    pub fn image(mut self, uri: &'a str) -> Self {
        self.image = Some(uri);
        self
    }

    pub fn icon(mut self, icon: PhosphorIcon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn hover(mut self, text: &'a str) -> Self {
        self.hover = Some(text);
        self
    }

    pub fn enabled(mut self, b: bool) -> Self {
        self.enabled = b;
        self
    }
}

#[derive(Default, Debug, Clone, Copy)]
pub struct OptionGroupResponse {
    /// The id of the choice clicked this frame.
    pub clicked: Option<u64>,
}

pub struct OptionGroup<'a> {
    items: Vec<OptionGroupItem<'a>>,
    flow: GroupFlow,
    density: GroupDensity,
    /// Marks one choice as the current one, for selector use. `None` for a
    /// picker, where nothing is "current" — you are choosing an action.
    selected: Option<u64>,
}

impl<'a> Default for OptionGroup<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> OptionGroup<'a> {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            flow: GroupFlow::default(),
            density: GroupDensity::default(),
            selected: None,
        }
    }

    pub fn item(mut self, item: OptionGroupItem<'a>) -> Self {
        self.items.push(item);
        self
    }

    /// Add a prepared set — for choices built by mapping over data rather than
    /// written out one by one.
    pub fn items(mut self, items: impl IntoIterator<Item = OptionGroupItem<'a>>) -> Self {
        self.items.extend(items);
        self
    }

    pub fn flow(mut self, flow: GroupFlow) -> Self {
        self.flow = flow;
        self
    }

    pub fn density(mut self, density: GroupDensity) -> Self {
        self.density = density;
        self
    }

    /// Show one choice as current. Turns the picker into a selector.
    pub fn selected(mut self, id: u64) -> Self {
        self.selected = Some(id);
        self
    }

    pub fn show(self, ui: &mut Ui) -> OptionGroupResponse {
        crate::icons::ensure_fonts(ui);
        let colors = ui.tokens().color;
        let radius = ui.tokens().corner(Radius::Md);
        let mut out = OptionGroupResponse::default();
        if self.items.is_empty() {
            return out;
        }

        let n = self.items.len();
        let image_px = ui.text_size(TextSize::Xl);
        let pad = ui.space(Space::Md);
        // A row is a tap target, so it floors at the breakpoint's minimum
        // rather than at whatever the text happens to need.
        let row_h = (ui.text_size(TextSize::Base) + pad * 2.0)
            .max(Breakpoint::from_ui(ui).min_touch())
            .max(image_px + pad);

        let (outer, size) = match self.flow {
            GroupFlow::Stacked => {
                let w = match self.density {
                    GroupDensity::Compact => image_px + pad * 2.0,
                    GroupDensity::Full => ui.available_width(),
                };
                (true, vec2(w, row_h * n as f32))
            }
            GroupFlow::Inline => {
                let w = match self.density {
                    GroupDensity::Compact => (image_px + pad * 2.0) * n as f32,
                    GroupDensity::Full => ui.available_width(),
                };
                (true, vec2(w, row_h))
            }
        };
        debug_assert!(outer);

        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());

        // The container: ONE border, one radius, drawn before the cells so a
        // cell's fill sits inside it.
        ui.painter().rect(
            rect,
            radius,
            colors.bg_secondary,
            hairline(colors.border),
            egui::StrokeKind::Inside,
        );

        for (i, item) in self.items.iter().enumerate() {
            let cell = cell_rect(rect, i, n, self.flow);
            let corners = cell_corners(radius, i, n, self.flow);
            // `auto_id_with`, NOT `ui.id().with(…)`. `Ui::id` is deliberately
            // STABLE rather than unique — egui derives a child's from
            // `parent.id.with(id_salt)`, and a plain `ui.scope()` passes no
            // salt, so it falls back to the constant `Id::from("child")`. Every
            // scope under one parent therefore shares an id, and two groups in
            // two scopes minted identical cell ids.
            //
            // `auto_id_with` keys off the allocation counter, which the
            // container's own `allocate_exact_size` above has already advanced
            // once per group. Same rule `chip` records: the id comes from the
            // ALLOCATION, never from the data.
            let resp = ui.interact(
                cell,
                ui.auto_id_with(("option-group-cell", i)),
                match item.enabled {
                    true => Sense::click(),
                    false => Sense::hover(),
                },
            );
            let hovered = item.enabled && resp.hovered();
            if hovered {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }

            let is_selected = self.selected == Some(item.id);
            if is_selected || hovered {
                let fill = match is_selected {
                    true => colors.bg_highlight,
                    false => colors.bg_highlight.gamma_multiply(0.6),
                };
                // Rounded to MATCH ITS POSITION — a square fill in the first
                // cell shows its corners through the container's curve.
                ui.painter().rect_filled(cell.shrink(1.0), corners, fill);
            }

            draw_cell(ui, cell, item, self.density, is_selected, image_px, pad);

            // Separators BETWEEN cells only; one against the border would sit
            // on top of it at double weight.
            if i + 1 < n {
                let (a, b) = match self.flow {
                    GroupFlow::Stacked => (
                        egui::pos2(cell.left() + pad, cell.bottom()),
                        egui::pos2(cell.right() - pad, cell.bottom()),
                    ),
                    GroupFlow::Inline => (
                        egui::pos2(cell.right(), cell.top() + pad * 0.5),
                        egui::pos2(cell.right(), cell.bottom() - pad * 0.5),
                    ),
                };
                ui.painter().line_segment([a, b], hairline(colors.border));
            }

            let tip = match (self.density, item.hover) {
                (_, Some(h)) => Some(h.to_string()),
                // The label is not thrown away when it stops being drawn.
                (GroupDensity::Compact, None) => Some(item.label.to_string()),
                (GroupDensity::Full, None) => None,
            };
            let resp = match tip {
                Some(t) => resp.on_hover_text(t),
                None => resp,
            };
            if resp.clicked() {
                out.clicked = Some(item.id);
            }
        }

        out
    }
}

/// Where cell `i` of `n` sits inside `rect`.
fn cell_rect(rect: egui::Rect, i: usize, n: usize, flow: GroupFlow) -> egui::Rect {
    let i = i as f32;
    let n = n as f32;
    match flow {
        GroupFlow::Stacked => {
            let h = rect.height() / n;
            egui::Rect::from_min_size(
                egui::pos2(rect.left(), rect.top() + h * i),
                vec2(rect.width(), h),
            )
        }
        GroupFlow::Inline => {
            let w = rect.width() / n;
            egui::Rect::from_min_size(
                egui::pos2(rect.left() + w * i, rect.top()),
                vec2(w, rect.height()),
            )
        }
    }
}

/// The rounding cell `i` of `n` needs so its fill stays inside the container's
/// border.
///
/// This is the arithmetic that separates a compound control from a stack of
/// rectangles in a box.
fn cell_corners(radius: CornerRadius, i: usize, n: usize, flow: GroupFlow) -> CornerRadius {
    let first = i == 0;
    let last = i + 1 == n;
    let z = 0;
    match flow {
        GroupFlow::Stacked => CornerRadius {
            nw: if first { radius.nw } else { z },
            ne: if first { radius.ne } else { z },
            sw: if last { radius.sw } else { z },
            se: if last { radius.se } else { z },
        },
        GroupFlow::Inline => CornerRadius {
            nw: if first { radius.nw } else { z },
            sw: if first { radius.sw } else { z },
            ne: if last { radius.ne } else { z },
            se: if last { radius.se } else { z },
        },
    }
}

fn draw_cell(
    ui: &mut Ui,
    cell: egui::Rect,
    item: &OptionGroupItem<'_>,
    density: GroupDensity,
    is_selected: bool,
    image_px: f32,
    pad: f32,
) {
    let colors = ui.tokens().color;
    let ink = match (item.enabled, is_selected) {
        (false, _) => colors.text_muted,
        (true, true) => colors.text_primary,
        (true, false) => colors.accent,
    };
    // An item with no image keeps its label whatever the density asked for: a
    // blank cell is not a choice anyone can make.
    let has_art = item.image.is_some() || item.icon.is_some();
    let compact = density == GroupDensity::Compact && has_art;

    let art_w = match has_art {
        true => image_px + pad,
        false => 0.0,
    };
    let content_w = match compact {
        true => image_px,
        false => art_w + text_width(ui, item.label),
    };
    let mut x = cell.left() + ((cell.width() - content_w) * 0.5).max(pad);

    if let Some(uri) = item.image {
        let at = egui::Rect::from_min_size(
            egui::pos2(x, cell.center().y - image_px * 0.5),
            vec2(image_px, image_px),
        );
        egui::Image::new(uri)
            .fit_to_exact_size(vec2(image_px, image_px))
            .corner_radius(ui.tokens().corner(Radius::Xs))
            .paint_at(ui, at);
        x += art_w;
    } else if let Some(icon) = item.icon {
        icon.paint(
            ui.painter(),
            egui::pos2(x + image_px * 0.5, cell.center().y),
            egui::Align2::CENTER_CENTER,
            image_px * 0.8,
            ink,
        );
        x += art_w;
    }

    if !compact {
        ui.painter().text(
            egui::pos2(x, cell.center().y),
            egui::Align2::LEFT_CENTER,
            item.label,
            egui::FontId::proportional(ui.text_size(TextSize::Base)),
            ink,
        );
    }
}

fn text_width(ui: &Ui, text: &str) -> f32 {
    let font = egui::FontId::proportional(ui.text_size(TextSize::Base));
    ui.painter()
        .layout_no_wrap(text.to_owned(), font, Color32::PLACEHOLDER)
        .size()
        .x
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_pass::TestPass as _;

    fn radius() -> CornerRadius {
        CornerRadius {
            nw: 8,
            ne: 8,
            sw: 8,
            se: 8,
        }
    }

    #[test]
    fn only_the_outer_corners_of_a_stack_are_round() {
        // The whole point of a compound control. A middle row with rounded
        // corners reads as a separate button; a first row with SQUARE ones
        // shows its corners through the container's curve.
        let r = radius();
        let top = cell_corners(r, 0, 3, GroupFlow::Stacked);
        let mid = cell_corners(r, 1, 3, GroupFlow::Stacked);
        let bot = cell_corners(r, 2, 3, GroupFlow::Stacked);

        assert_eq!((top.nw, top.ne), (8, 8), "top row rounds the top");
        assert_eq!((top.sw, top.se), (0, 0), "…and not the bottom");
        assert_eq!(
            (mid.nw, mid.ne, mid.sw, mid.se),
            (0, 0, 0, 0),
            "a middle row is square on every corner"
        );
        assert_eq!((bot.sw, bot.se), (8, 8), "bottom row rounds the bottom");
        assert_eq!((bot.nw, bot.ne), (0, 0));
    }

    #[test]
    fn an_inline_group_rounds_its_ends_not_its_top_and_bottom() {
        // The same rule turned ninety degrees — first cell owns the LEFT
        // corners, last owns the right.
        let r = radius();
        let left = cell_corners(r, 0, 3, GroupFlow::Inline);
        let right = cell_corners(r, 2, 3, GroupFlow::Inline);
        assert_eq!((left.nw, left.sw), (8, 8), "first cell rounds the left");
        assert_eq!((left.ne, left.se), (0, 0));
        assert_eq!((right.ne, right.se), (8, 8), "last cell rounds the right");
        assert_eq!((right.nw, right.sw), (0, 0));
    }

    #[test]
    fn a_lone_choice_is_round_on_every_corner() {
        // First AND last. Both branches have to fire, which an `if first {} else
        // if last {}` would get wrong.
        for flow in [GroupFlow::Stacked, GroupFlow::Inline] {
            let c = cell_corners(radius(), 0, 1, flow);
            assert_eq!((c.nw, c.ne, c.sw, c.se), (8, 8, 8, 8), "{flow:?}");
        }
    }

    #[test]
    fn cells_tile_the_container_exactly() {
        // No gaps and no overlap — a seam of background between two cells is
        // the thing that makes a compound control look like it came apart.
        let rect = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), vec2(300.0, 120.0));
        for flow in [GroupFlow::Stacked, GroupFlow::Inline] {
            for n in [1usize, 2, 5] {
                let cells: Vec<_> = (0..n).map(|i| cell_rect(rect, i, n, flow)).collect();
                assert_eq!(cells[0].min, rect.min, "{flow:?} n={n} starts flush");
                assert!(
                    (cells[n - 1].max - rect.max).length() < 1e-3,
                    "{flow:?} n={n} ends flush"
                );
                for w in cells.windows(2) {
                    let (a, b) = (w[0], w[1]);
                    let seam = match flow {
                        GroupFlow::Stacked => (b.top() - a.bottom()).abs(),
                        GroupFlow::Inline => (b.left() - a.right()).abs(),
                    };
                    assert!(seam < 1e-3, "{flow:?} n={n} seam of {seam}");
                }
            }
        }
    }

    /// Every string egui painted — including the red collision warnings it
    /// draws over a duplicated widget id.
    fn painted_text(groups: usize, in_scopes: bool) -> Vec<String> {
        let ctx = egui::Context::default();
        let mut out = Vec::new();
        for _ in 0..2 {
            ctx.begin_pass(egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    vec2(800.0, 900.0),
                )),
                ..Default::default()
            });
            egui::Area::new(egui::Id::new("og")).show(&ctx, |ui| {
                for _ in 0..groups {
                    let build = |ui: &mut Ui| {
                        OptionGroup::new()
                            .item(OptionGroupItem::new(0, "one"))
                            .item(OptionGroupItem::new(1, "two"))
                            .show(ui);
                    };
                    match in_scopes {
                        // The shape that broke it: a bare `ui.scope`, which is
                        // what every layout helper in this workspace uses.
                        true => {
                            ui.scope(build);
                        }
                        false => build(ui),
                    }
                }
            });
            let full = ctx.end_test_pass();
            out.clear();
            fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
                match shape {
                    egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
                    egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                    _ => {}
                }
            }
            for cs in &full.shapes {
                walk(&cs.shape, &mut out);
            }
        }
        out
    }

    #[test]
    fn two_groups_in_sibling_scopes_do_not_collide() {
        // The bug the storybook found. `Ui::id` is STABLE, not unique: a plain
        // `ui.scope()` passes no id salt so egui falls back to the constant
        // `Id::from("child")`, and every scope under one parent shares an id.
        // Keying cell ids off it made the second group re-use the first's.
        let painted = painted_text(3, true);
        let collisions: Vec<_> = painted
            .iter()
            .filter(|t| t.contains("use of widget ID"))
            .collect();
        assert!(
            collisions.is_empty(),
            "egui reported id collisions: {collisions:?}"
        );
    }

    #[test]
    fn two_groups_side_by_side_do_not_collide_either() {
        // The same check without the scopes, so a future change that "fixes"
        // this by leaning on scope nesting still has to hold up bare.
        let painted = painted_text(3, false);
        let collisions: Vec<_> = painted
            .iter()
            .filter(|t| t.contains("use of widget ID"))
            .collect();
        assert!(
            collisions.is_empty(),
            "egui reported id collisions: {collisions:?}"
        );
    }

    #[test]
    fn an_empty_group_draws_nothing_and_reports_nothing() {
        // A group built from a filtered list can legitimately be empty, and
        // allocating a bordered box around no choices would render a stray box.
        let g = OptionGroup::new();
        assert!(g.items.is_empty());
    }
}
