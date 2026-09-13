//! Storybook demo for `CornerAction` — an icon button pinned to a corner of a
//! thumbnail.

use egui::{Rect, Vec2};
use egui_widgets::PhosphorIcon;
use egui_widgets::corner_action::{Corner, CornerAction};

use crate::{accent, muted};

const THUMB: f32 = 120.0;

/// A stand-in for a `card_browser` thumbnail: the same placeholder fill the
/// browser paints before an image lands.
fn placeholder_thumb(ui: &mut egui::Ui, label: &str) -> Rect {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(THUMB), egui::Sense::hover());
    ui.painter().rect_filled(rect, 4.0, crate::highlight(ui));
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(10.0),
        muted(ui),
    );
    rect
}

fn owned_dot(ui: &egui::Ui, thumb: Rect) {
    let r = 5.0;
    let center = egui::pos2(thumb.max.x - r - 4.0, thumb.min.y + r + 4.0);
    ui.painter().circle_filled(
        center,
        r,
        crate::tok(ui, egui_widgets::theme::Token::AccentGreen),
    );
    ui.painter()
        .circle_stroke(center, r, egui::Stroke::new(1.0_f32, crate::bg(ui)));
}

pub fn show(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("Corner Action")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "An icon button pinned to a corner of something already drawn. Takes the \
             click so the card under it doesn't also select.",
        )
        .color(muted(ui))
        .small(),
    );
    ui.add_space(12.0);

    ui.label(
        egui::RichText::new("Four corners, default chip")
            .color(accent(ui))
            .strong(),
    );
    ui.add_space(4.0);
    let clicked_id = egui::Id::new("corner_action_clicked");
    ui.horizontal_wrapped(|ui| {
        for corner in Corner::ALL {
            let thumb = placeholder_thumb(ui, &format!("{corner:?}"));
            let resp = CornerAction::new(PhosphorIcon::ArrowsClockwise)
                .corner(corner)
                .tooltip("Refresh image from on-chain metadata")
                .show(ui, thumb, ("story", corner as u8));
            if resp.clicked() {
                ui.ctx()
                    .data_mut(|d| d.insert_temp(clicked_id, format!("{corner:?}")));
            }
        }
    });
    if let Some(which) = ui.ctx().data_mut(|d| d.get_temp::<String>(clicked_id)) {
        ui.label(
            egui::RichText::new(format!("→ {which} click registered"))
                .color(crate::tok(ui, egui_widgets::theme::Token::AccentGreen))
                .size(10.0),
        );
    }
    ui.add_space(16.0);

    ui.label(
        egui::RichText::new("Beside an existing badge, other accents, other sizes")
            .color(accent(ui))
            .strong(),
    );
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        // Owned dot already in the top-right → shift the action inward.
        let thumb = placeholder_thumb(ui, "owned + refresh");
        owned_dot(ui, thumb);
        CornerAction::new(PhosphorIcon::ArrowsClockwise)
            .shift(14.0)
            .tooltip("Refresh image")
            .show(ui, thumb, "shifted");

        let thumb = placeholder_thumb(ui, "remove");
        CornerAction::new(PhosphorIcon::X)
            .accent(crate::tok(ui, egui_widgets::theme::Token::AccentRed))
            .tooltip("Remove")
            .show(ui, thumb, "remove");

        let thumb = placeholder_thumb(ui, "favourite, 20pt");
        CornerAction::new(PhosphorIcon::Star)
            .corner(Corner::BottomRight)
            .size(20.0)
            .accent(crate::tok(ui, egui_widgets::theme::Token::AccentYellow))
            .tooltip("Favourite")
            .show(ui, thumb, "favourite");

        let thumb = placeholder_thumb(ui, "on an image");
        egui::Image::new(egui::include_image!(
            "../../assets/placeholders/section_hero_64.png"
        ))
        .fit_to_exact_size(thumb.size())
        .corner_radius(4)
        .paint_at(ui, thumb);
        CornerAction::new(PhosphorIcon::ArrowsClockwise)
            .tooltip("Refresh image")
            .show(ui, thumb, "image");
    });
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(
            "Rest: dark chip, accent glyph. Hover: accent chip, dark glyph. \
             The chip is 16pt by default, 4pt in from the edges.",
        )
        .color(muted(ui))
        .size(10.0),
    );
}
