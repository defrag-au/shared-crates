//! Printing timeline widget — shows a card's printings across sets over time.
//!
//! Renders a horizontal timeline with nodes for each printing, connected by a
//! line. Each node shows the set icon/code, rarity, and optionally the card art.
//! Scrollable horizontally for cards with many printings.
//!
//! Designed for MtG but applicable to any TCG with set-based reprints.

use egui::{Color32, Pos2, Rect, RichText, Stroke, Vec2};

use crate::theme::{Ink, Radius, TextSize, ThemeExt, Token};

// ── Config ───────────────────────────────────────────────────────

pub struct PrintingTimelineConfig {
    /// Height of the entire timeline widget.
    pub height: f32,
    /// Width of each printing node.
    pub node_width: f32,
    /// Horizontal spacing between nodes.
    pub node_spacing: f32,
    /// Radius of the timeline dot.
    pub dot_radius: f32,
    /// Timeline line color.
    pub line_color: Ink,
    /// Dot color (normal).
    pub dot_color: Ink,
    /// Dot color (selected).
    pub dot_selected: Ink,
    /// Text color for set code.
    pub text_color: Ink,
    /// Muted text color.
    pub text_muted: Ink,
    /// Whether to show thumbnail images above nodes.
    pub show_thumbnails: bool,
    /// Thumbnail height (if shown).
    pub thumb_height: f32,
}

impl Default for PrintingTimelineConfig {
    fn default() -> Self {
        Self {
            height: 80.0,
            node_width: 40.0,
            node_spacing: 4.0,
            dot_radius: 5.0,
            line_color: Ink::Token(Token::TextMuted),
            dot_color: Ink::Token(Token::TextMuted),
            dot_selected: Ink::Token(Token::Accent),
            text_color: Ink::Token(Token::TextPrimary),
            text_muted: Ink::Token(Token::TextMuted),
            show_thumbnails: false,
            thumb_height: 80.0,
        }
    }
}

// ── Data ─────────────────────────────────────────────────────────

/// A single printing on the timeline.
pub struct PrintingNode {
    /// Display label (e.g., set code "MH3").
    pub set_code: String,
    /// Set name for tooltip.
    pub set_name: String,
    /// Release date string.
    pub released_at: String,
    /// Rarity of this printing.
    pub rarity: String,
    /// Collector number.
    pub collector_number: String,
    /// Image URL for the thumbnail (optional).
    pub image_url: Option<String>,
    /// Whether this is the first printing (not a reprint).
    pub is_original: bool,
}

/// State for the timeline widget.
#[derive(Default)]
pub struct PrintingTimelineState {
    /// Currently selected node index.
    pub selected: Option<usize>,
}

/// Response from the timeline widget.
pub struct PrintingTimelineResponse {
    /// Node that was clicked this frame.
    pub clicked: Option<usize>,
    /// Node being hovered.
    pub hovered: Option<usize>,
}

// ── Widget ───────────────────────────────────────────────────────

pub fn show(
    ui: &mut egui::Ui,
    state: &mut PrintingTimelineState,
    nodes: &[PrintingNode],
    config: &PrintingTimelineConfig,
) -> PrintingTimelineResponse {
    let mut response = PrintingTimelineResponse {
        clicked: None,
        hovered: None,
    };

    // Resolved once: a `Default` config names its tokens, it cannot hold values.
    let t = ui.tokens();
    let line_color = config.line_color.resolve(&t);
    let dot_color = config.dot_color.resolve(&t);
    let dot_selected = config.dot_selected.resolve(&t);
    let text_color = config.text_color.resolve(&t);
    let text_muted = config.text_muted.resolve(&t);

    if nodes.is_empty() {
        ui.label(RichText::new("No printings").color(text_muted));
        return response;
    }

    let total_width =
        nodes.len() as f32 * (config.node_width + config.node_spacing) - config.node_spacing;

    egui::ScrollArea::horizontal()
        .id_salt("printing_timeline")
        .show(ui, |ui| {
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(total_width, config.height), egui::Sense::hover());

            let painter = ui.painter_at(rect);

            // Timeline layout:
            //   [thumb]     ← optional card art thumbnails
            //   ---o---o---o---  ← timeline line with dots
            //   [set]       ← set code label
            //   [date]      ← release date
            //   [rarity]    ← rarity badge

            let thumb_area_h = if config.show_thumbnails {
                config.thumb_height + 4.0
            } else {
                0.0
            };
            let timeline_y = rect.top() + thumb_area_h + config.dot_radius + 2.0;
            let text_top = timeline_y + config.dot_radius + 4.0;

            // Draw timeline line
            if nodes.len() > 1 {
                let first_x = rect.left() + config.node_width / 2.0;
                let last_x = rect.left()
                    + (nodes.len() - 1) as f32 * (config.node_width + config.node_spacing)
                    + config.node_width / 2.0;
                painter.line_segment(
                    [
                        Pos2::new(first_x, timeline_y),
                        Pos2::new(last_x, timeline_y),
                    ],
                    Stroke::new(2.0_f32, line_color),
                );
            }

            // Draw each node
            for (i, node) in nodes.iter().enumerate() {
                let node_x = rect.left() + i as f32 * (config.node_width + config.node_spacing);
                let center_x = node_x + config.node_width / 2.0;
                let is_selected = state.selected == Some(i);

                // Thumbnail above the dot
                if config.show_thumbnails
                    && let Some(url) = &node.image_url
                {
                    let thumb_rect = Rect::from_min_size(
                        Pos2::new(node_x, rect.top()),
                        Vec2::new(config.node_width, config.thumb_height),
                    );

                    // Use egui's built-in image widget
                    let image = egui::Image::new(url.as_str())
                        .fit_to_exact_size(Vec2::new(config.node_width, config.thumb_height))
                        .corner_radius(ui.tokens().corner(Radius::Base));
                    ui.put(thumb_rect, image);
                }

                // Node dot
                let dot_center = Pos2::new(center_x, timeline_y);
                let dot_color = if is_selected {
                    dot_selected
                } else {
                    rarity_dot_color(&node.rarity, &t.color, dot_color)
                };
                painter.circle_filled(dot_center, config.dot_radius, dot_color);

                // Original printing marker (double ring)
                if node.is_original {
                    painter.circle_stroke(
                        dot_center,
                        config.dot_radius + 3.0,
                        Stroke::new(1.5_f32, dot_color),
                    );
                }

                // Clickable area
                let click_rect = Rect::from_center_size(
                    dot_center,
                    Vec2::new(config.node_width, config.dot_radius * 4.0),
                );
                let click_resp = ui.interact(
                    click_rect,
                    egui::Id::new(("timeline_node", i)),
                    egui::Sense::click(),
                );

                if click_resp.clicked() {
                    state.selected = if is_selected { None } else { Some(i) };
                    response.clicked = Some(i);
                }

                if click_resp.hovered() {
                    response.hovered = Some(i);
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }

                click_resp.on_hover_text(format!(
                    "{} ({}) · {} #{} · {}",
                    node.set_name,
                    node.set_code,
                    node.rarity,
                    node.collector_number,
                    node.released_at,
                ));

                // Set code label (centered under dot)
                let code_color = if is_selected {
                    dot_selected
                } else {
                    text_color
                };
                let set_galley = painter.layout_no_wrap(
                    node.set_code.clone(),
                    egui::FontId::monospace(ui.text_size(TextSize::Base)),
                    code_color,
                );
                let set_w = set_galley.size().x;
                painter.galley(
                    Pos2::new(center_x - set_w / 2.0, text_top),
                    set_galley,
                    code_color,
                );

                // Year only (compact date)
                let year = if node.released_at.len() >= 4 {
                    &node.released_at[..4]
                } else {
                    &node.released_at
                };
                let date_galley = painter.layout_no_wrap(
                    year.to_string(),
                    egui::FontId::proportional(ui.text_size(TextSize::Xs)),
                    text_muted,
                );
                let date_w = date_galley.size().x;
                painter.galley(
                    Pos2::new(center_x - date_w / 2.0, text_top + 11.0),
                    date_galley,
                    text_muted,
                );
            }
        });

    response
}

// ── Helpers ──────────────────────────────────────────────────────

/// The rarity ramp, off the theme's accent hues rather than six literals — a
/// mythic printing should read as "the theme's red", not as one fixed pink that
/// clashes with every palette but the one it was picked against.
fn rarity_dot_color(rarity: &str, c: &crate::theme::ColorTokens, common: Color32) -> Color32 {
    match rarity {
        "mythic" => c.accent_red,
        "rare" => c.accent_orange,
        "uncommon" => c.text_secondary,
        "common" => common,
        "special" => c.accent_magenta,
        "bonus" => c.accent_cyan,
        _ => common,
    }
}
