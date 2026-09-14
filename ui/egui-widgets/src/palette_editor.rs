//! palette_editor — **superseded by [`effect_editor`](crate::effect_editor)**.
//!
//! Edits `[processing.techniques.colorization]` palettes: a name, a base colour,
//! and a list of variants. The compositor replaced that config block with
//! `[[effect]]`; the shipping project's only remaining mention of it is a
//! comment saying so. Three things this shape cannot express, all of which the
//! real config uses:
//!
//! - **Global tint.** `base_color` is `Option` in the config and the `None` arm
//!   is a *mode* — multiply the whole layer, for a grayscale value-map. A
//!   mandatory `[u8; 3]` here makes it unreachable, and it is the mode the one
//!   real palette in the repo (skin/neck/hand) runs in.
//! - **The public label.** A variant's `name` is identity — it lands in
//!   `asset://…?variant=tan` and is parsed back out — while `label` is the trait
//!   value written to metadata. One field cannot be both.
//! - **Pipelines.** A variant may be a whole material (`gold = tint → specular →
//!   grain`). Shown as a flat swatch, it is indistinguishable from a colour.
//!
//! Kept and deprecated rather than deleted so cargo names the call sites when a
//! consumer bumps its pin, instead of anyone grepping for them.
//!
//! Mutates the palette list in place (composite editor with nested rows); returns
//! `true` when anything changed so the host can re-serialise / re-validate.
//!
//! ## The spine
//!
//! A palette and its variants are the same kind of thing — a name and a colour —
//! so they are laid out as rows of one grid rather than as a header followed by
//! some other rows:
//!
//! ```text
//! ┌──────────────────────────────────────────┐
//! │ ■  warm                             🗑   │   the palette's base colour
//! │      variant              weight         │   (only when it has any)
//! │    ■  golden                 1.0    ×    │
//! │    ■  bronze                 1.0    ×    │
//! │    +  variant                            │
//! └──────────────────────────────────────────┘
//! ```
//!
//! The variants indent by one step so the hierarchy reads down the left edge,
//! and their name column narrows by exactly that indent so the **right** edges —
//! weight, and the trailing action — still land on one axis. Indenting a row
//! without taking the indent back out of a column is what pushes a trailing
//! button out of line, which is how this widget used to look.
//!
//! What the rewrite removed, all of it the same mistake in different clothes:
//! a `140px` name field on the palette row against a `100px` one on the variant
//! rows, so no two text fields shared an edge; the swatch sitting after a `Base`
//! label on one row and at the margin on the others; a `w` label repeated once
//! per variant doing a column header's job N times; and `×` / `Remove palette`
//! as filled `egui::Button`s, the heaviest objects on rows whose actual content
//! is a colour and a word.

// The module IS the deprecated thing; its own internals referring to each other
// are not the call sites the deprecation is meant to surface.
#![allow(deprecated)]

use egui::{Align, Layout, Sense, Ui, vec2};

use crate::icons::PhosphorIcon;
use crate::theme::{Space, SpaceExt, TextSize, ThemeExt};

/// Width of the name column on a palette (top-level) row.
///
/// One number, used by both tiers: a variant's field is this minus the indent,
/// which is what keeps every column after it on a shared axis.
const NAME_W: f32 = 200.0;

/// Width of the weight column. Sized for the widest value the range can print
/// (`1000.0`) so a digit appearing does not shift the column — the same rule
/// [`SliderGroup`](crate::slider_group) sizes its readouts by.
const WEIGHT_W: f32 = 64.0;

/// Point size of the trailing row actions.
const ACTION_PT: f32 = 13.0;

#[deprecated(
    since = "0.1.0",
    note = "models the superseded `[processing.techniques.colorization]` block; use `effect_editor::EffectVariant`, which separates identity (`name`) from the public trait value (`label`) and can carry a pipeline"
)]
#[derive(Debug, Clone)]
pub struct PaletteVariant {
    pub name: String,
    pub color: [u8; 3],
    pub weight: f32,
}

impl Default for PaletteVariant {
    fn default() -> Self {
        Self {
            name: String::new(),
            color: [200, 160, 120],
            weight: 1.0,
        }
    }
}

#[deprecated(
    since = "0.1.0",
    note = "models the superseded `[processing.techniques.colorization]` block; use `effect_editor::Effect`, which carries the slots, the pick/base modes and a `Recolor` that can express global tint"
)]
#[derive(Debug, Clone)]
pub struct Palette {
    pub name: String,
    pub base_color: [u8; 3],
    pub variants: Vec<PaletteVariant>,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            name: String::new(),
            base_color: [212, 165, 116],
            variants: Vec::new(),
        }
    }
}

#[deprecated(
    since = "0.1.0",
    note = "models the superseded `[processing.techniques.colorization]` block; use `effect_editor::EffectEditor`"
)]
pub struct PaletteEditor<'a> {
    palettes: &'a mut Vec<Palette>,
    add_label: &'a str,
}

/// The column geometry every row in a palette card shares.
struct Spine {
    /// How far a variant row is inset from its palette row.
    indent: f32,
    /// Width of the trailing action column.
    action: f32,
    /// Gap between columns.
    gap: f32,
    /// Row height, so a row with no text in it still occupies one.
    row_h: f32,
}

impl Spine {
    fn measure(ui: &Ui) -> Self {
        Self {
            indent: ui.space(Space::Xl),
            action: ACTION_PT + ui.space(Space::Md) * 2.0,
            gap: ui.space(Space::Md),
            row_h: ui.text_size(TextSize::Base) + ui.space(Space::Md),
        }
    }

    /// Total width of a palette card's content — fixed, so the card does not
    /// resize as names are typed into it.
    fn content_w(&self, swatch: f32) -> f32 {
        swatch + self.gap + NAME_W + self.gap + WEIGHT_W + self.gap + self.action
    }
}

impl<'a> PaletteEditor<'a> {
    pub fn new(palettes: &'a mut Vec<Palette>) -> Self {
        Self {
            palettes,
            add_label: "Add palette",
        }
    }

    pub fn add_label(mut self, label: &'a str) -> Self {
        self.add_label = label;
        self
    }

    pub fn show(self, ui: &mut Ui) -> bool {
        let Self {
            palettes,
            add_label,
        } = self;

        crate::icons::ensure_fonts(ui);
        let colors = ui.tokens().color;
        let spine = Spine::measure(ui);
        // egui sizes a colour button from `interact_size`, so ask it rather than
        // assuming — the swatch is the first column and everything is measured
        // from its right edge.
        let swatch_w = ui.spacing().interact_size.x;

        let mut changed = false;
        let mut remove_palette: Option<usize> = None;

        for (pi, palette) in palettes.iter_mut().enumerate() {
            ui.push_id(pi, |ui| {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_width(spine.content_w(swatch_w));

                    // ── The palette itself ───────────────────────────────────
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = spine.gap;
                        if ui.color_edit_button_srgb(&mut palette.base_color).changed() {
                            changed = true;
                        }
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut palette.name)
                                    .hint_text("palette name")
                                    .desired_width(NAME_W),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                        // A palette has no weight of its own; the cell is held
                        // open so the action column below it stays on one axis.
                        //
                        // An allocation, NOT `add_space`: egui puts
                        // `item_spacing` after a widget but not after a bare
                        // space, so a space here left this row one gap short and
                        // the trash icon 8px left of every `×` below it.
                        ui.allocate_exact_size(vec2(WEIGHT_W, spine.row_h), Sense::hover());
                        if action(ui, &spine, PhosphorIcon::Trash, "Remove palette", &colors) {
                            remove_palette = Some(pi);
                        }
                    });

                    // ── Its variants ─────────────────────────────────────────
                    if !palette.variants.is_empty() {
                        // Said once, not once per row. Three words replace what
                        // was a `w` glued to every weight field.
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = spine.gap;
                            // Leading space takes no trailing gap, so the gap
                            // that follows the swatch on a real row is added here.
                            ui.add_space(spine.indent + swatch_w + spine.gap);
                            column_header(ui, &spine, NAME_W - spine.indent, "variant", &colors);
                            column_header(ui, &spine, WEIGHT_W, "weight", &colors);
                        });
                    }

                    let mut remove_variant: Option<usize> = None;
                    for (vi, variant) in palette.variants.iter_mut().enumerate() {
                        ui.push_id(vi, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = spine.gap;
                                ui.add_space(spine.indent);
                                if ui.color_edit_button_srgb(&mut variant.color).changed() {
                                    changed = true;
                                }
                                if ui
                                    .add(
                                        egui::TextEdit::singleline(&mut variant.name)
                                            .hint_text("variant")
                                            // The indent comes back out here.
                                            .desired_width(NAME_W - spine.indent),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                                if ui
                                    .add_sized(
                                        vec2(WEIGHT_W, spine.row_h),
                                        egui::DragValue::new(&mut variant.weight)
                                            .speed(0.1)
                                            .range(0.0..=1000.0),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                                if action(ui, &spine, PhosphorIcon::X, "Remove variant", &colors) {
                                    remove_variant = Some(vi);
                                }
                            });
                        });
                    }
                    if let Some(i) = remove_variant {
                        palette.variants.remove(i);
                        changed = true;
                    }

                    // The add affordance sits on the variant column, so it reads
                    // as "another one of these" rather than as a palette-level
                    // button that happens to be last.
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = spine.gap;
                        ui.add_space(spine.indent);
                        if add_row(ui, &spine, swatch_w, "add variant", &colors) {
                            palette.variants.push(PaletteVariant::default());
                            changed = true;
                        }
                    });
                });
            });
        }

        if let Some(i) = remove_palette {
            palettes.remove(i);
            changed = true;
        }
        if ui.button(add_label).clicked() {
            palettes.push(Palette::default());
            changed = true;
        }

        changed
    }
}

/// A muted column label occupying exactly the column it names.
fn column_header(
    ui: &mut Ui,
    spine: &Spine,
    width: f32,
    text: &str,
    colors: &crate::theme::ColorTokens,
) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, spine.row_h), Sense::hover());
    let mut cell = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    cell.label(
        egui::RichText::new(text)
            .color(colors.text_muted)
            .size(cell.text_size(TextSize::Sm)),
    );
}

/// A trailing row action, as a glyph rather than a filled button.
///
/// Returns true when clicked. The hit target is the whole column, so it stays
/// usable at the size a 13pt glyph implies.
fn action(
    ui: &mut Ui,
    spine: &Spine,
    icon: PhosphorIcon,
    hint: &str,
    colors: &crate::theme::ColorTokens,
) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(spine.action, spine.row_h), Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let color = if resp.hovered() {
        colors.error
    } else {
        colors.text_muted
    };
    icon.paint(
        ui.painter(),
        rect.center(),
        egui::Align2::CENTER_CENTER,
        ACTION_PT,
        color,
    );
    resp.on_hover_text(hint).clicked()
}

/// `+ variant`, sitting on the swatch and name columns so it lines up with the
/// rows it adds to.
fn add_row(
    ui: &mut Ui,
    spine: &Spine,
    swatch_w: f32,
    label: &str,
    colors: &crate::theme::ColorTokens,
) -> bool {
    let width = swatch_w + spine.gap + NAME_W - spine.indent;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, spine.row_h), Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let color = if resp.hovered() {
        colors.accent
    } else {
        colors.text_secondary
    };
    let mut cell = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    PhosphorIcon::Plus.paint(
        cell.painter(),
        egui::pos2(rect.left() + ACTION_PT * 0.5, rect.center().y),
        egui::Align2::CENTER_CENTER,
        ACTION_PT,
        color,
    );
    cell.add_space(ACTION_PT + spine.gap);
    cell.label(egui::RichText::new(label).color(color));
    resp.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_pass::TestPass as _;
    use egui::{Id, Pos2, Rect};

    /// Every text shape the editor painted, as `(text, x)`.
    fn painted(palettes: &mut Vec<Palette>) -> Vec<(String, f32)> {
        let ctx = egui::Context::default();
        let frame = |palettes: &mut Vec<Palette>| {
            ctx.begin_pass(egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 900.0))),
                ..Default::default()
            });
            egui::Area::new(Id::new("pe")).show(&ctx, |ui| {
                PaletteEditor::new(palettes).show(ui);
            });
            ctx.end_test_pass()
        };
        // First pass binds the fonts — see `icons::ensure_fonts`.
        let _ = frame(palettes);
        let out = frame(palettes);

        let mut found = Vec::new();
        fn walk(shape: &egui::Shape, out: &mut Vec<(String, f32)>) {
            match shape {
                egui::Shape::Text(t) => out.push((t.galley.text().to_string(), t.pos.x)),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        for cs in &out.shapes {
            walk(&cs.shape, &mut found);
        }
        found
    }

    fn x_of(found: &[(String, f32)], text: &str) -> Vec<f32> {
        found
            .iter()
            .filter(|(t, _)| t == text)
            .map(|(_, x)| *x)
            .collect()
    }

    fn warm() -> Palette {
        Palette {
            name: "warm".into(),
            base_color: [212, 165, 116],
            variants: vec![
                PaletteVariant {
                    name: "golden".into(),
                    color: [230, 200, 100],
                    weight: 1.0,
                },
                PaletteVariant {
                    name: "bronze".into(),
                    color: [200, 160, 120],
                    weight: 2.5,
                },
            ],
        }
    }

    #[test]
    fn the_trailing_actions_share_one_axis() {
        // The palette's remove and every variant's remove sit in the same
        // column, despite the variants being indented — the indent is taken
        // back out of the name column rather than pushing everything right.
        let mut p = vec![warm()];
        let found = painted(&mut p);
        let trash = x_of(&found, &PhosphorIcon::Trash.as_str());
        let crosses = x_of(&found, &PhosphorIcon::X.as_str());
        assert_eq!(trash.len(), 1, "one palette, one trash");
        assert_eq!(crosses.len(), 2, "two variants, two crosses");
        for x in trash.iter().chain(crosses.iter()) {
            assert!(
                (x - trash[0]).abs() < 1.0,
                "actions at trash={trash:?} crosses={crosses:?} — one axis"
            );
        }
    }

    #[test]
    fn the_variants_are_indented_from_their_palette() {
        // The hierarchy has to read down the LEFT edge; without the indent a
        // variant is indistinguishable from another palette.
        let mut p = vec![warm()];
        let found = painted(&mut p);
        let header = x_of(&found, "variant");
        let hint = x_of(&found, "palette name");
        assert_eq!(
            header.len(),
            1,
            "the column header is said once, not per row"
        );
        // The palette's own name field is at the outer margin; the variant
        // column header sits inside it.
        assert!(
            header[0] > hint.first().copied().unwrap_or(0.0),
            "variant column at {:?} should be inside the palette name at {hint:?}",
            header[0]
        );
    }

    #[test]
    fn a_column_header_sits_over_the_column_it_names() {
        // A header row is built from spaces while the rows beneath it are built
        // from widgets, and egui only puts `item_spacing` after the latter — so
        // the two drift by one gap unless the header adds it back. Caught this
        // way once already on the trailing-action column.
        let mut p = vec![warm()];
        let found = painted(&mut p);
        let header = x_of(&found, "variant");
        let name = x_of(&found, "golden");
        assert_eq!(header.len(), 1);
        assert_eq!(name.len(), 1);
        assert!(
            (header[0] - name[0]).abs() < 5.0,
            "header at {:?} over a variant name at {:?}",
            header[0],
            name[0]
        );
    }

    #[test]
    fn a_palette_with_no_variants_says_nothing_about_weight() {
        // The header belongs to the rows it labels. With no rows it is noise.
        let mut p = vec![Palette {
            name: "empty".into(),
            ..Default::default()
        }];
        let found = painted(&mut p);
        assert!(x_of(&found, "weight").is_empty(), "no rows to label");
        assert!(x_of(&found, "variant").is_empty(), "no rows to label");
        // But the way to add one is still there — and is named distinctly from
        // the column header, so neither the reader nor this test has to guess
        // which "variant" it is looking at.
        assert_eq!(x_of(&found, "add variant").len(), 1);
    }
}
