//! `ButtonGroup` — a row of related action buttons with shared layout.
//!
//! Generalises the action-bar pattern used on the collection card, the
//! wallet card, the orders viewer toolbar, etc. — anywhere a small set
//! of related buttons needs to live on one row with consistent spacing,
//! optional Phosphor icons, optional disabled state, optional click
//! tooltips, and a single drained "which one was clicked?" response.
//!
//! ## Why not just `ui.horizontal_wrapped` with raw buttons?
//!
//! Inline button bars routinely accumulate:
//! - inconsistent spacing across pages
//! - mixed icon/text patterns (some use Phosphor, some use raw Unicode
//!   that breaks per CLAUDE.md)
//! - one-off disabled/in-flight handling
//! - per-card glue code to dispatch which button was clicked
//!
//! `ButtonGroup` collapses this into one builder. Each button gets a
//! caller-supplied `id` (any `u64` — the host typically casts an enum
//! discriminant); on click, the response carries `Some(id)`.
//!
//! ## Layout
//!
//! - Default: `horizontal_wrapped` — a narrow surface spills onto the
//!   next row rather than overlapping siblings. Switch to `wrapped(false)`
//!   for fixed-width contexts.
//! - Item spacing default = 4px; override with `spacing(f32)`.
//! - Icons render via `phosphor_with_text` (LayoutJob mixing Phosphor +
//!   proportional fonts) so a button label stays crisp even when the
//!   icon family isn't pre-installed on the parent Ui — the widget
//!   installs the font itself.
//!
//! ## Example
//!
//! ```ignore
//! use egui_widgets::{ButtonGroup, ButtonGroupButton, PhosphorIcon};
//!
//! let resp = ButtonGroup::new()
//!     .add(ButtonGroupButton::new(1, "Test mint"))
//!     .add(ButtonGroupButton::new(2, "Activity"))
//!     .add(
//!         ButtonGroupButton::new(3, "Configure")
//!             .icon(PhosphorIcon::Gear)
//!             .hover_text("Edit phases, gates, and allowlist"),
//!     )
//!     .add(
//!         ButtonGroupButton::new(4, "Refuelling…")
//!             .enabled(false)
//!             .hover_text("Refuel tx in flight — wait for the ack"),
//!     )
//!     .show(ui);
//! match resp.clicked {
//!     Some(1) => dispatch_test_mint(),
//!     Some(2) => dispatch_activity(),
//!     Some(3) => dispatch_configure(),
//!     _ => {}
//! }
//! ```

use egui::{Color32, FontFamily, RichText, TextStyle, Ui, WidgetText};

use crate::icons::{install_phosphor_font, phosphor_family, PhosphorIcon};

/// Builder.
pub struct ButtonGroup<'a> {
    buttons: Vec<ButtonGroupButton<'a>>,
    wrap: bool,
    spacing: f32,
}

/// Outcome of one `ButtonGroup::show()` call.
#[derive(Default, Debug)]
pub struct ButtonGroupResponse {
    /// The `id` of the button the user clicked this frame, if any. The
    /// host typically matches on this against its own enum discriminants.
    pub clicked: Option<u64>,
}

/// One button in the group.
pub struct ButtonGroupButton<'a> {
    id: u64,
    label: &'a str,
    icon: Option<PhosphorIcon>,
    enabled: bool,
    hover_text: Option<&'a str>,
}

impl<'a> ButtonGroupButton<'a> {
    /// Construct a default-enabled, text-only button with the given
    /// caller-supplied click `id` and visible `label`.
    pub fn new(id: u64, label: &'a str) -> Self {
        Self {
            id,
            label,
            icon: None,
            enabled: true,
            hover_text: None,
        }
    }

    /// Add a leading Phosphor glyph rendered before the label.
    pub fn icon(mut self, icon: PhosphorIcon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Disable the button. Renders greyed-out and unclickable; the
    /// `hover_text` (if any) becomes a `on_disabled_hover_text`.
    pub fn enabled(mut self, b: bool) -> Self {
        self.enabled = b;
        self
    }

    /// Tooltip shown on hover. On disabled buttons this becomes the
    /// disabled-hover hint instead.
    pub fn hover_text(mut self, s: &'a str) -> Self {
        self.hover_text = Some(s);
        self
    }
}

impl<'a> Default for ButtonGroup<'a> {
    fn default() -> Self {
        Self {
            buttons: Vec::new(),
            wrap: true,
            spacing: 4.0,
        }
    }
}

impl<'a> ButtonGroup<'a> {
    /// New empty group. Default layout: horizontal-wrapped, 4px spacing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append one button. Chain `.add(…).add(…)` to build the row.
    #[allow(clippy::should_implement_trait)] // builder verb, not arithmetic
    pub fn add(mut self, button: ButtonGroupButton<'a>) -> Self {
        self.buttons.push(button);
        self
    }

    /// Switch to a single horizontal row that doesn't wrap (the row may
    /// overflow on narrow surfaces). Default `true` (wrapped).
    pub fn wrap(mut self, b: bool) -> Self {
        self.wrap = b;
        self
    }

    /// Set item spacing in pixels. Default `4.0`.
    pub fn spacing(mut self, px: f32) -> Self {
        self.spacing = px;
        self
    }

    /// Render the group.
    pub fn show(self, ui: &mut Ui) -> ButtonGroupResponse {
        // Any button might carry a Phosphor icon. Idempotent.
        if self.buttons.iter().any(|b| b.icon.is_some()) {
            install_phosphor_font(ui.ctx());
        }
        let mut response = ButtonGroupResponse::default();
        let render_row = |ui: &mut Ui| {
            ui.spacing_mut().item_spacing.x = self.spacing;
            for button in &self.buttons {
                if let Some(id) = render_one(ui, button) {
                    response.clicked = Some(id);
                }
            }
        };
        if self.wrap {
            ui.horizontal_wrapped(render_row);
        } else {
            ui.horizontal(render_row);
        }
        response
    }
}

/// Render one button. Returns `Some(id)` if it was clicked this frame.
fn render_one(ui: &mut Ui, button: &ButtonGroupButton) -> Option<u64> {
    let widget_text: WidgetText = match button.icon {
        None => RichText::new(button.label).small().into(),
        Some(icon) => phosphor_text(ui, icon, button.label),
    };
    let resp = ui.add_enabled(button.enabled, egui::Button::new(widget_text).small());
    let resp = match (button.hover_text, button.enabled) {
        (Some(t), true) => resp.on_hover_text(t),
        (Some(t), false) => resp.on_disabled_hover_text(t),
        _ => resp,
    };
    if resp.clicked() {
        Some(button.id)
    } else {
        None
    }
}

/// Build a `LayoutJob`-backed `WidgetText` that mixes a Phosphor glyph
/// with proportional label text.
///
/// **Both runs use the size of egui's `TextStyle::Small`** so an
/// icon+label button visually matches a text-only button in the same
/// group. (Earlier shape hard-coded 12pt while text-only paths used
/// `.small()` which resolves to ~10.5pt — leaving icon buttons visibly
/// larger than their siblings inside a group. Resolving from
/// `ui.style()` keeps the icon size in lock-step with whatever the
/// active theme calls "small".)
///
/// Colour stays `PLACEHOLDER` so the button's enabled / hovered visual
/// state carries through automatically.
fn phosphor_text(ui: &Ui, icon: PhosphorIcon, label: &str) -> WidgetText {
    use egui::text::LayoutJob;
    use egui::{FontId, TextFormat};
    let small = TextStyle::Small.resolve(ui.style());
    let icon_font = FontId::new(small.size, phosphor_family());
    let text_font = FontId::new(small.size, FontFamily::Proportional);
    let mut job = LayoutJob::default();
    job.append(
        &icon.as_str(),
        0.0,
        TextFormat {
            font_id: icon_font,
            color: Color32::PLACEHOLDER,
            ..Default::default()
        },
    );
    job.append(
        &format!(" {label}"),
        0.0,
        TextFormat {
            font_id: text_font,
            color: Color32::PLACEHOLDER,
            ..Default::default()
        },
    );
    job.into()
}
