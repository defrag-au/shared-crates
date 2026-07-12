//! Tag list — a wrapping row of removable tags with an optional trailing
//! "clear all" button. The host owns the tag data; this just renders it and
//! reports which tag was removed / whether clear was clicked.
//!
//! For active filters, selected facets, applied options, etc. Wraps cleanly:
//! each tag is measured and placed with `allocate_exact_size` inside a
//! `horizontal_wrapped` layout, so it flows onto the next line when it won't fit
//! (a `Frame`-based chip instead lays out in the remaining width and can't wrap).

use egui::{Color32, CornerRadius, Sense, Stroke, Ui, Vec2};

use crate::chip::ChipVariant;

#[derive(Default, Debug, Clone, Copy)]
pub struct TagListResponse {
    /// Index of the tag whose × (or body) was clicked this frame, if any.
    pub removed: Option<usize>,
    /// The "clear all" button was clicked this frame.
    pub cleared: bool,
}

pub struct TagList<'a> {
    tags: &'a [String],
    variant: ChipVariant,
    clear_label: Option<&'a str>,
}

impl<'a> TagList<'a> {
    pub fn new(tags: &'a [String]) -> Self {
        Self {
            tags,
            variant: ChipVariant::Tag,
            clear_label: None,
        }
    }

    /// Tag palette (default [`ChipVariant::Tag`]).
    pub fn variant(mut self, variant: ChipVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Append a "clear all" button with this label after the tags.
    pub fn clearable(mut self, label: &'a str) -> Self {
        self.clear_label = Some(label);
        self
    }

    pub fn show(self, ui: &mut Ui) -> TagListResponse {
        const PAD: Vec2 = Vec2::new(8.0, 3.0);
        const ICON: f32 = 9.0;
        const GAP: f32 = 6.0; // space between label and ×

        let mut resp = TagListResponse::default();
        let (fg, bg, border) = self.variant.palette();
        let font = egui::FontId::proportional(12.0);

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
            let painter = ui.painter().clone();
            for (i, tag) in self.tags.iter().enumerate() {
                let galley = painter.layout_no_wrap(tag.clone(), font.clone(), fg);
                let size = Vec2::new(
                    PAD.x * 2.0 + galley.size().x + GAP + ICON,
                    PAD.y * 2.0 + galley.size().y.max(ICON),
                );
                // The wrap happens here: if the chip won't fit the remaining row,
                // the wrapped layout moves to the next line before placing it.
                let (rect, r) = ui.allocate_exact_size(size, Sense::click());
                let cr = CornerRadius::same(3);
                painter.rect_filled(rect, cr, bg);
                if let Some(b) = border {
                    painter.rect_stroke(rect, cr, Stroke::new(1.0_f32, b), egui::StrokeKind::Inside);
                }
                painter.galley(
                    egui::pos2(rect.left() + PAD.x, rect.center().y - galley.size().y / 2.0),
                    galley,
                    fg,
                );
                // × affordance (two strokes — no icon-font dependency).
                let cx = rect.right() - PAD.x - ICON / 2.0;
                let cy = rect.center().y;
                let h = ICON * 0.35;
                let stroke = Stroke::new(1.2_f32, x_color(fg, r.hovered()));
                painter.line_segment(
                    [egui::pos2(cx - h, cy - h), egui::pos2(cx + h, cy + h)],
                    stroke,
                );
                painter.line_segment(
                    [egui::pos2(cx - h, cy + h), egui::pos2(cx + h, cy - h)],
                    stroke,
                );
                if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    resp.removed = Some(i);
                }
            }
            if let Some(label) = self.clear_label {
                if !self.tags.is_empty() && ui.button(label).clicked() {
                    resp.cleared = true;
                }
            }
        });
        resp
    }
}

/// Brighten the × on hover so the remove affordance reads as interactive.
fn x_color(fg: Color32, hovered: bool) -> Color32 {
    if hovered {
        Color32::WHITE
    } else {
        fg
    }
}
