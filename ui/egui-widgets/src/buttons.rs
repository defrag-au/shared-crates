//! Button helpers that add consistent UX behavior (pointer cursor, etc.).

use egui::{Response, Ui, Widget};

/// Extension trait for [`egui::Ui`] that adds buttons with pointer cursor on hover.
pub trait UiButtonExt {
    /// Add a widget (typically a [`egui::Button`]) and show a pointing hand cursor on hover.
    fn add_clickable(&mut self, widget: impl Widget) -> Response;

    /// Like [`egui::Ui::add_sized`] but with pointing hand cursor on hover.
    fn add_clickable_sized(&mut self, size: [f32; 2], widget: impl Widget) -> Response;
}

impl UiButtonExt for Ui {
    fn add_clickable(&mut self, widget: impl Widget) -> Response {
        let response = self.add(widget);
        if response.hovered() {
            self.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        response
    }

    fn add_clickable_sized(&mut self, size: [f32; 2], widget: impl Widget) -> Response {
        let response = self.add_sized(size, widget);
        if response.hovered() {
            self.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        response
    }
}
