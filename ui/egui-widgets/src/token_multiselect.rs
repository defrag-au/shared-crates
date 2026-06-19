//! token_multiselect — pick a subset from a known set of options. Selected items
//! render as removable chips (same flowing, wrap-clean style as [`TagList`]); an
//! "add" menu lists the options not yet selected. The host owns the selection;
//! the widget reports what was added / removed / cleared this frame.
//!
//! Foundation for membership pickers: exclusive-group members, bundled/linked-set
//! slots, a trait's required slots, etc.

use egui::{Color32, CornerRadius, Sense, Stroke, Ui, Vec2};

use crate::chip::ChipVariant;

#[derive(Default, Debug, Clone)]
pub struct TokenMultiselectResponse {
    /// An option picked from the "add" menu this frame.
    pub added: Option<String>,
    /// Index into `selected` whose chip was removed this frame.
    pub removed: Option<usize>,
    /// The "clear" button was clicked this frame.
    pub cleared: bool,
}

pub struct TokenMultiselect<'a> {
    selected: &'a [String],
    options: &'a [String],
    variant: ChipVariant,
    add_label: &'a str,
    clear_label: Option<&'a str>,
}

impl<'a> TokenMultiselect<'a> {
    pub fn new(selected: &'a [String], options: &'a [String]) -> Self {
        Self {
            selected,
            options,
            variant: ChipVariant::Tag,
            add_label: "+ add",
            clear_label: None,
        }
    }

    /// Chip palette (default [`ChipVariant::Tag`]).
    pub fn variant(mut self, variant: ChipVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Label for the "add" menu button (default `"+ add"`).
    pub fn add_label(mut self, label: &'a str) -> Self {
        self.add_label = label;
        self
    }

    /// Append a "clear" button with this label.
    pub fn clearable(mut self, label: &'a str) -> Self {
        self.clear_label = Some(label);
        self
    }

    pub fn show(self, ui: &mut Ui) -> TokenMultiselectResponse {
        const PAD: Vec2 = Vec2::new(8.0, 3.0);
        const ICON: f32 = 9.0;
        const GAP: f32 = 6.0;

        let mut resp = TokenMultiselectResponse::default();
        let (fg, bg, border) = self.variant.palette();
        let font = egui::FontId::proportional(12.0);

        // Options not yet selected (preserve `options` order).
        let remaining: Vec<&String> = self
            .options
            .iter()
            .filter(|o| !self.selected.iter().any(|s| s == *o))
            .collect();

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
            let painter = ui.painter().clone();

            // Selected → removable chips (measure → allocate → paint, so it wraps).
            for (i, tag) in self.selected.iter().enumerate() {
                let galley = painter.layout_no_wrap(tag.clone(), font.clone(), fg);
                let size = Vec2::new(
                    PAD.x * 2.0 + galley.size().x + GAP + ICON,
                    PAD.y * 2.0 + galley.size().y.max(ICON),
                );
                let (rect, r) = ui.allocate_exact_size(size, Sense::click());
                let cr = CornerRadius::same(3);
                painter.rect_filled(rect, cr, bg);
                if let Some(b) = border {
                    painter.rect_stroke(rect, cr, Stroke::new(1.0, b), egui::StrokeKind::Inside);
                }
                painter.galley(
                    egui::pos2(rect.left() + PAD.x, rect.center().y - galley.size().y / 2.0),
                    galley,
                    fg,
                );
                let cx = rect.right() - PAD.x - ICON / 2.0;
                let cy = rect.center().y;
                let h = ICON * 0.35;
                let stroke = Stroke::new(1.2, x_color(fg, r.hovered()));
                painter
                    .line_segment([egui::pos2(cx - h, cy - h), egui::pos2(cx + h, cy + h)], stroke);
                painter
                    .line_segment([egui::pos2(cx - h, cy + h), egui::pos2(cx + h, cy - h)], stroke);
                if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    resp.removed = Some(i);
                }
            }

            // "add" menu of the remaining options.
            if !remaining.is_empty() {
                let menu = ui.menu_button(self.add_label, |ui| {
                    let mut chosen = None;
                    for opt in &remaining {
                        if ui.button(opt.as_str()).clicked() {
                            chosen = Some((*opt).clone());
                            ui.close_menu();
                        }
                    }
                    chosen
                });
                resp.added = menu.inner.flatten();
            }

            if let Some(label) = self.clear_label {
                if !self.selected.is_empty() && ui.button(label).clicked() {
                    resp.cleared = true;
                }
            }
        });

        resp
    }
}

fn x_color(fg: Color32, hovered: bool) -> Color32 {
    if hovered {
        Color32::WHITE
    } else {
        fg
    }
}
