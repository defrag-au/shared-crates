//! relationship_editor — edit a list of directed `source → target` edges over a
//! known option set. Backs variant_flow, dependencies, and slot-lock style
//! relationships in the config editor (and the wires in the node-graph view).
//!
//! Existing edges render as removable rows; an "add" row picks a source + target
//! from the options. The host owns the edge list; the widget reports add/remove.
//! The add-row's pending selection is kept in egui temp memory, so the host
//! doesn't have to manage it.
//!
//! ## The spine
//!
//! Every row — the existing edges and the add row alike — is laid out on the
//! same four columns:
//!
//! ```text
//! │ source        │ → │ target        │ ×
//! ```
//!
//! This is the whole of what was wrong with the first version. The edge rows
//! were bare `ui.label`s in a `ui.horizontal`, so each one's arrow and remove
//! button landed wherever the text before them happened to end; the add row was
//! two 160px [`Select`]s, so ITS arrow sat a couple of hundred pixels right of
//! the list's. The list and the way you extend it read as two unrelated widgets
//! stacked on top of each other, which is the opposite of what they are.
//!
//! The fix is alignment, not decoration — the same answer as
//! [`SliderGroup`](crate::slider_group). The columns are derived from the
//! [`Select`]'s own geometry (egui documents a frame's size as
//! `content_size + inner_margin + 2 × stroke.width`), so the text in an edge row
//! starts at exactly the x where a select's value text starts, and the arrow is
//! on one axis down the whole widget. Nothing here is a guessed pixel offset:
//! change the theme's `Space::Md` and both the select and the rows above it move
//! together.

use egui::{Align, Align2, Layout, Sense, Ui, vec2};

use crate::icons::PhosphorIcon;
use crate::select::{Select, SelectOption};
use crate::theme::{Space, SpaceExt, TextSize, ThemeExt};

/// The inner width handed to each [`Select`], and so the width of the source and
/// target columns for every row.
///
/// Deliberately one constant rather than the two separate `.width(160.0)` calls
/// this widget used to make: they were the same number by luck, and the edge
/// rows above them were aligned to neither.
const COLUMN_W: f32 = 160.0;

/// A [`theme::hairline`](crate::theme::hairline) is 1px, and egui counts a
/// frame's stroke as part of its total size. Named so the column arithmetic
/// below reads as geometry rather than as a fudge factor.
const HAIRLINE: f32 = 1.0;

/// Point size of the `→` between source and target.
const ARROW_PT: f32 = 12.0;

/// Point size of the `×` that removes a row.
const REMOVE_PT: f32 = 12.0;

#[derive(Default, Debug, Clone)]
pub struct RelationshipEditorResponse {
    /// A complete `(source, target)` edge chosen on the add row this frame.
    pub added: Option<(String, String)>,
    /// Index of the edge removed this frame.
    pub removed: Option<usize>,
}

pub struct RelationshipEditor<'a> {
    id_salt: &'a str,
    edges: &'a [(String, String)],
    options: &'a [String],
    add_label: &'a str,
    empty_text: &'a str,
}

/// The column geometry every row shares, measured once per frame.
struct Spine {
    /// Outer width of a source/target column — what a [`Select`] actually
    /// occupies, not the inner width it was asked for.
    column: f32,
    /// Distance from a column's left edge to where its text begins. Matches the
    /// inset of a select's value text, so a row of plain text sits on the same
    /// left edge as the control below it.
    inset: f32,
    /// Width of the arrow column.
    arrow: f32,
    /// Width of the trailing remove column.
    remove: f32,
    /// Height of an edge row. Shorter than a select — an edge is a line of text
    /// and reads as a list, while the add row is a form. They share x, not y.
    row_h: f32,
    /// The gap that separates the trailing action from the target column.
    pad: f32,
}

impl Spine {
    fn measure(ui: &Ui) -> Self {
        let pad = ui.space(Space::Md);
        Self {
            // egui: a frame is `content_size + inner_margin + 2 * stroke.width`.
            // `Select` uses `margin_xy(Space::Md, Space::Sm)` and a hairline
            // border, so this is exact rather than approximate.
            column: COLUMN_W + (pad + HAIRLINE) * 2.0,
            inset: pad + HAIRLINE,
            arrow: ARROW_PT + pad * 2.0,
            remove: REMOVE_PT + pad * 2.0,
            row_h: ui.text_size(TextSize::Base) + ui.space(Space::Sm) * 2.0,
            pad,
        }
    }
}

impl<'a> RelationshipEditor<'a> {
    pub fn new(id_salt: &'a str, edges: &'a [(String, String)], options: &'a [String]) -> Self {
        Self {
            id_salt,
            edges,
            options,
            add_label: "Add",
            empty_text: "No relationships yet",
        }
    }

    pub fn add_label(mut self, label: &'a str) -> Self {
        self.add_label = label;
        self
    }

    /// What to say where the edge list would be when there are no edges.
    ///
    /// An editor that renders nothing above its add row leaves the reader
    /// unsure whether the list is empty or the widget is broken.
    pub fn empty_text(mut self, text: &'a str) -> Self {
        self.empty_text = text;
        self
    }

    pub fn show(self, ui: &mut Ui) -> RelationshipEditorResponse {
        // The → and × are Phosphor icons (the default font lacks U+2192).
        crate::icons::ensure_fonts(ui);
        let colors = ThemeExt::tokens(ui).color;
        let spine = Spine::measure(ui);
        let mut resp = RelationshipEditorResponse::default();

        if self.edges.is_empty() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add_space(spine.inset);
                ui.label(
                    egui::RichText::new(self.empty_text)
                        .color(colors.text_muted)
                        .small(),
                );
            });
        }

        for (i, (src, tgt)) in self.edges.iter().enumerate() {
            ui.horizontal(|ui| {
                // The columns are explicit, so egui's own inter-item gap would
                // be a second, competing source of horizontal spacing.
                ui.spacing_mut().item_spacing.x = 0.0;
                text_cell(ui, &spine, src, colors.text_primary);
                arrow_cell(ui, &spine, colors.text_muted);
                text_cell(ui, &spine, tgt, colors.text_primary);
                if remove_cell(ui, &spine, colors.text_muted, colors.error) {
                    resp.removed = Some(i);
                }
            });
        }

        ui.add_space(ui.space(Space::Base));

        // Add row — pending source/target kept in temp memory by id.
        let id = egui::Id::new(("relationship_editor", self.id_salt));
        let mut pending: (String, String) = ui.data(|d| d.get_temp(id)).unwrap_or_default();
        // id == label: an edge names options by their own text.
        let options: Vec<SelectOption> = self
            .options
            .iter()
            .map(|o| SelectOption::new(o.clone(), o.clone()))
            .collect();

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;

            let src = Select::new(id.with("src"), &options)
                .value_from_id(&pending.0, "no longer an option")
                .placeholder("source")
                .width(COLUMN_W)
                .show(ui);
            if let Some(chosen) = src.chosen {
                pending.0 = chosen;
            }
            if src.cleared {
                pending.0.clear();
            }

            // Sized and painted exactly as on the rows above, so the arrow is
            // one axis through the whole widget rather than three.
            arrow_cell(ui, &spine, colors.text_muted);

            let tgt = Select::new(id.with("tgt"), &options)
                .value_from_id(&pending.1, "no longer an option")
                .placeholder("target")
                .width(COLUMN_W)
                .show(ui);
            if let Some(chosen) = tgt.chosen {
                pending.1 = chosen;
            }
            if tgt.cleared {
                pending.1.clear();
            }

            // The trailing action starts where the `×` glyphs above it do.
            ui.add_space(spine.pad);
            let can_add = !pending.0.is_empty() && !pending.1.is_empty();
            if ui
                .add_enabled(can_add, egui::Button::new(self.add_label))
                .clicked()
            {
                resp.added = Some((pending.0.clone(), pending.1.clone()));
                pending = (String::new(), String::new());
            }
        });
        ui.data_mut(|d| d.insert_temp(id, pending));

        resp
    }
}

/// One source/target value as plain text, occupying a full column.
///
/// Uses the same `allocate_ui_with_layout` + truncating `Label` as [`Select`]'s
/// own value column: a label too long for its column elides rather than pushing
/// the arrow out of line, which is the failure the spine exists to prevent.
fn text_cell(ui: &mut Ui, spine: &Spine, text: &str, color: egui::Color32) {
    // `allocate_exact_size` and NOT `allocate_ui_with_layout`: the latter takes
    // its argument as a maximum and then shrinks the allocation to whatever the
    // content used, which is precisely the content-sized column this widget is
    // trying to stop having. The cell has to claim its width whether or not the
    // label needs it.
    let (rect, _) = ui.allocate_exact_size(vec2(spine.column, spine.row_h), Sense::hover());
    let mut cell = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    cell.add_space(spine.inset);
    cell.add(egui::Label::new(egui::RichText::new(text).color(color)).truncate());
}

fn arrow_cell(ui: &mut Ui, spine: &Spine, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(spine.arrow, spine.row_h), Sense::hover());
    PhosphorIcon::ArrowRight.paint(
        ui.painter(),
        rect.center(),
        Align2::CENTER_CENTER,
        ARROW_PT,
        color,
    );
}

/// The trailing `×`. Returns true when clicked.
///
/// A glyph rather than an `egui::Button`: at the default button style the
/// remove affordance was a filled box roughly twice the visual weight of the
/// edge it removes, which made a list of two edges read as a list of two
/// buttons. The hit target is the whole cell, so it stays clickable at the size
/// a 12pt glyph implies.
fn remove_cell(ui: &mut Ui, spine: &Spine, rest: egui::Color32, active: egui::Color32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(spine.remove, spine.row_h), Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let color = if resp.hovered() { active } else { rest };
    PhosphorIcon::X.paint(
        ui.painter(),
        egui::pos2(rect.left() + spine.pad + REMOVE_PT * 0.5, rect.center().y),
        Align2::CENTER_CENTER,
        REMOVE_PT,
        color,
    );
    resp.on_hover_text("Remove relationship").clicked()
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Id, Pos2, Rect};

    /// Every piece of text the widget painted, as `(text, x, y)`.
    ///
    /// The spine is a claim about where things land on screen, so the tests
    /// read the painted output rather than the builder — a refactor that
    /// silently stopped aligning the columns would still satisfy any assertion
    /// made against the struct.
    fn painted(
        edges: &[(String, String)],
        passes: Vec<Vec<egui::Event>>,
    ) -> (Vec<(String, f32, f32)>, RelationshipEditorResponse) {
        let options: Vec<String> = ["skin", "clothes", "neck", "hand"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let ctx = egui::Context::default();
        let mut resp = RelationshipEditorResponse::default();

        // TWO PASSES, and the first one is not incidental. `ctx.set_fonts` only
        // takes effect at the START of a pass, so the fonts a widget asks for
        // are not bound until the pass after it asked. `ensure_fonts` keeps that
        // pass from panicking by falling back to the proportional family — but a
        // fallback glyph is a different WIDTH, and these tests measure x
        // positions. So the first pass is discarded and the events go on the
        // measured one, which is also what a running app looks like.
        let frame = |events: Vec<egui::Event>, resp: &mut RelationshipEditorResponse| {
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 800.0))),
                events,
                ..Default::default()
            };
            ctx.begin_pass(raw);
            egui::Area::new(Id::new("re")).show(&ctx, |ui| {
                *resp = RelationshipEditor::new("t", edges, &options).show(ui);
            });
            ctx.end_pass()
        };
        crate::install_phosphor_font(&ctx);
        let mut out = frame(Vec::new(), &mut resp);
        // Interaction needs at least two more: egui resolves a click at the
        // start of a pass against the geometry the PREVIOUS pass registered, so
        // a press and its release have to land on different passes.
        let mut removed = None;
        for events in passes {
            out = frame(events, &mut resp);
            removed = removed.or(resp.removed);
        }
        resp.removed = removed.or(resp.removed);

        let mut found = Vec::new();
        fn walk(shape: &egui::Shape, out: &mut Vec<(String, f32, f32)>) {
            match shape {
                egui::Shape::Text(t) => {
                    out.push((t.galley.text().to_string(), t.pos.x, t.pos.y));
                }
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        for cs in &out.shapes {
            walk(&cs.shape, &mut found);
        }
        (found, resp)
    }

    fn xs_of(found: &[(String, f32, f32)], text: &str) -> Vec<f32> {
        found
            .iter()
            .filter(|(t, _, _)| t == text)
            .map(|(_, x, _)| *x)
            .collect()
    }

    fn edges(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn every_row_puts_its_arrow_on_one_axis() {
        // The bug this widget was rewritten for: with the rows in a bare
        // `ui.horizontal`, each arrow landed wherever the label before it
        // ended, and the add row's landed a couple of hundred px further right
        // again. Source labels of three different widths, plus the add row.
        let e = edges(&[("skin", "clothes"), ("hand", "neck"), ("clothes", "hand")]);
        let (found, _) = painted(&e, vec![vec![]]);
        let arrows = xs_of(&found, &PhosphorIcon::ArrowRight.as_str());
        assert_eq!(arrows.len(), 4, "three edge rows and the add row");
        for x in &arrows {
            assert!(
                (x - arrows[0]).abs() < 0.01,
                "arrows at {arrows:?} — they must share one axis"
            );
        }
    }

    #[test]
    fn an_edge_row_starts_its_text_where_a_select_starts_its_value() {
        // The other half of the claim: the list and the add row are one grid,
        // so a source name sits on the same left edge as the source picker's
        // placeholder. This is what `Spine::inset` is computed from.
        let (found, _) = painted(&edges(&[("skin", "clothes")]), vec![vec![]]);
        let row = xs_of(&found, "skin");
        let placeholder = xs_of(&found, "source");
        assert_eq!(row.len(), 1);
        assert_eq!(placeholder.len(), 1);
        assert!(
            (row[0] - placeholder[0]).abs() < 0.01,
            "edge text at {}, select placeholder at {} — same column, same edge",
            row[0],
            placeholder[0]
        );
    }

    #[test]
    fn a_label_too_wide_for_its_column_elides_instead_of_shifting_the_arrow() {
        // A column that grows to fit its content is a column that moves
        // everything after it. Truncation keeps the spine straight.
        let e = edges(&[
            ("skin", "clothes"),
            ("a_trait_slot_name_far_wider_than_the_column_allows", "neck"),
        ]);
        let (found, _) = painted(&e, vec![vec![]]);
        let arrows = xs_of(&found, &PhosphorIcon::ArrowRight.as_str());
        assert_eq!(arrows.len(), 3);
        for x in &arrows {
            assert!((x - arrows[0]).abs() < 0.01, "arrows at {arrows:?}");
        }
    }

    #[test]
    fn the_empty_list_says_so() {
        // Rendering nothing above the add row leaves the reader unable to tell
        // an empty list from a broken widget.
        let (found, _) = painted(&[], vec![vec![]]);
        assert!(
            found.iter().any(|(t, _, _)| t == "No relationships yet"),
            "expected the empty-state line, painted: {:?}",
            found.iter().map(|(t, _, _)| t).collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_remove_glyph_is_still_a_real_click_target() {
        // The `×` stopped being an `egui::Button` and became a painted glyph
        // with a hand-allocated hit rect. That is exactly the change that
        // silently turns a control into decoration, so click it.
        let e = edges(&[("skin", "clothes"), ("hand", "neck")]);
        let (found, _) = painted(&e, vec![vec![]]);
        let x_glyphs: Vec<_> = found
            .iter()
            .filter(|(t, _, _)| *t == PhosphorIcon::X.as_str())
            .cloned()
            .collect();
        assert_eq!(x_glyphs.len(), 2, "one remove affordance per edge");

        // Click the SECOND one — an index the widget has to carry through
        // correctly, unlike row 0 which a broken impl returns by default.
        let target = Pos2::new(x_glyphs[1].1 + 2.0, x_glyphs[1].2 + 2.0);
        let button = |pressed| egui::Event::PointerButton {
            pos: target,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let (_, resp) = painted(
            &e,
            vec![
                vec![egui::Event::PointerMoved(target)],
                vec![button(true)],
                vec![button(false)],
            ],
        );
        assert_eq!(resp.removed, Some(1), "clicked the second row's ×");
    }
}
