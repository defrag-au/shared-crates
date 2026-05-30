//! `PropertyList` — compact label/value grid for read-only key data.
//!
//! Renders one row per `(label, value)` pair with the labels in a small
//! muted colour and the values in the default text colour. Labels are
//! right-aligned (or left, configurable) and given a stable width so the
//! values column stays visually aligned even with mixed label lengths.
//!
//! Used wherever a small descriptive list of "field: value" would
//! otherwise be hand-built with `egui::Grid` (the wallet card's
//! fuel/total/health rows, the phase card's price/window/per-wallet
//! rows, an offer's amount/expires/policy rows, etc.).
//!
//! ## Example
//!
//! ```ignore
//! PropertyList::new()
//!     .add("Price", "FREE")
//!     .add("Window", "unbounded → unbounded")
//!     .add("Per wallet", "unlimited")
//!     .show(ui);
//! ```

use egui::{Color32, Grid, RichText, Ui};

/// Builder.
pub struct PropertyList<'a> {
    items: Vec<(&'a str, String)>,
    label_color: Color32,
    label_align: PropertyLabelAlign,
    id: &'static str,
}

/// Horizontal alignment of the label column. Default = `Left`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropertyLabelAlign {
    Left,
    Right,
}

impl<'a> Default for PropertyList<'a> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            label_color: Color32::from_gray(150),
            label_align: PropertyLabelAlign::Left,
            id: "property_list",
        }
    }
}

impl<'a> PropertyList<'a> {
    /// Construct an empty `PropertyList`. Default label colour is a
    /// neutral grey suited to dark backgrounds.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append one row.
    pub fn add(mut self, label: &'a str, value: impl Into<String>) -> Self {
        self.items.push((label, value.into()));
        self
    }

    /// Append a row conditionally — useful for optional fields without
    /// having to compose a Vec at the call site.
    pub fn add_optional<S: Into<String>>(self, label: &'a str, value: Option<S>) -> Self {
        match value {
            Some(v) => self.add(label, v),
            None => self,
        }
    }

    /// Set the label colour (default: neutral grey).
    pub fn label_color(mut self, c: Color32) -> Self {
        self.label_color = c;
        self
    }

    /// Right-align the labels. Default = left-aligned.
    pub fn label_align(mut self, a: PropertyLabelAlign) -> Self {
        self.label_align = a;
        self
    }

    /// Override the egui Id salt used by the underlying `Grid`. Set when
    /// multiple `PropertyList`s render in the same `Ui` to avoid grid
    /// column-width collisions.
    ///
    /// For per-row uniqueness (e.g. one `PropertyList` per item in a
    /// loop), prefer wrapping the call in `ui.push_id(unique_key, |ui|
    /// PropertyList::new()…show(ui))` over passing a static string here
    /// — egui's Id system is hierarchical and `push_id` salts every
    /// descendant. `id()` is for distinguishing one-off `PropertyList`s
    /// that happen to live in the same scope.
    pub fn id(mut self, id: &'static str) -> Self {
        self.id = id;
        self
    }

    /// Render.
    pub fn show(self, ui: &mut Ui) {
        if self.items.is_empty() {
            return;
        }
        Grid::new(self.id)
            .num_columns(2)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                for (label, value) in &self.items {
                    match self.label_align {
                        PropertyLabelAlign::Left => {
                            ui.label(RichText::new(*label).small().color(self.label_color));
                        }
                        PropertyLabelAlign::Right => {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(RichText::new(*label).small().color(self.label_color));
                                },
                            );
                        }
                    }
                    ui.label(value);
                    ui.end_row();
                }
            });
    }
}
