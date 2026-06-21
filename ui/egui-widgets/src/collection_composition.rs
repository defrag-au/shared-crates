//! Collection composition — a promotable "how this collection is generated" infographic.
//!
//! Renders the z-ordered layer stack (front on top, like a layers panel): each layer
//! with its presence rate (how often it appears), option count, and variant badges,
//! plus curved connectors in a left gutter for `variant_flow` relationships, under a
//! headline stats band. Read-only — fed from the config + diagnosis. It's both an
//! explainer and a clean graphic you could screenshot to promote the collection.

use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use crate::theme;

// ============================================================================
// Types
// ============================================================================

/// One z-ordered layer of the collection.
pub struct CompositionLayer {
    /// Display z-index, e.g. "02".
    pub z_label: String,
    /// Trait / slot name.
    pub name: String,
    /// How often this layer is present, `0.0..=100.0` (100 = always).
    pub present_pct: f32,
    /// Number of distinct values (assets) for this layer.
    pub option_count: usize,
    /// Variant tokens, e.g. `["a", "b"]` (empty if the layer has no variants).
    pub variants: Vec<String>,
}

/// A `variant_flow` connection from one layer to another (by `name`).
pub struct CompositionFlow {
    pub from: String,
    pub to: String,
}

/// A headline metric for the stats band (`value` big, `label` under it).
pub struct CompositionStat {
    pub value: String,
    pub label: String,
}

/// The whole composition: title, headline stats, the layer stack, and flow edges.
pub struct CollectionComposition {
    pub title: String,
    pub stats: Vec<CompositionStat>,
    /// Layers in any order — rendered front (highest z) on top.
    pub layers: Vec<CompositionLayer>,
    pub flow: Vec<CompositionFlow>,
}

/// Layout/styling knobs.
pub struct CompositionConfig {
    pub row_height: f32,
    pub gutter: f32,
    pub bar_width: f32,
    pub name_col: f32,
}

impl Default for CompositionConfig {
    fn default() -> Self {
        Self {
            row_height: 30.0,
            gutter: 30.0,
            bar_width: 150.0,
            name_col: 150.0,
        }
    }
}

// ============================================================================
// Widget
// ============================================================================

/// Render the composition infographic. Returns the overall `Response`.
pub fn show(ui: &mut Ui, comp: &CollectionComposition, cfg: &CompositionConfig) -> egui::Response {
    let resp = ui
        .scope(|ui| {
            // Title.
            ui.label(
                egui::RichText::new(&comp.title)
                    .color(theme::TEXT_PRIMARY)
                    .strong()
                    .size(18.0),
            );

            // Stats band.
            if !comp.stats.is_empty() {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    for (i, s) in comp.stats.iter().enumerate() {
                        if i > 0 {
                            ui.add_space(18.0);
                        }
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new(&s.value)
                                    .color(theme::ACCENT)
                                    .strong()
                                    .size(18.0),
                            );
                            ui.label(
                                egui::RichText::new(&s.label)
                                    .color(theme::TEXT_MUTED)
                                    .size(10.5),
                            );
                        });
                    }
                });
            }

            ui.add_space(10.0);
            ui.label(
                egui::RichText::new("Layer stack — how each token is assembled (front on top)")
                    .color(theme::TEXT_MUTED)
                    .size(10.5),
            );
            ui.add_space(4.0);

            layer_stack(ui, comp, cfg);
        })
        .response;
    resp
}

/// The layer rows + flow connectors.
fn layer_stack(ui: &mut Ui, comp: &CollectionComposition, cfg: &CompositionConfig) {
    // Front (highest z) on top: sort by z_label descending (numeric-ish).
    let mut order: Vec<usize> = (0..comp.layers.len()).collect();
    let zkey = |s: &str| s.split(':').next().unwrap_or("0").trim().parse::<u32>().unwrap_or(0);
    order.sort_by(|&a, &b| zkey(&comp.layers[b].z_label).cmp(&zkey(&comp.layers[a].z_label)));

    let avail_w = ui.available_width();
    let mut y_center: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
    let mut gutter_x = 0.0_f32;

    for &i in &order {
        let layer = &comp.layers[i];
        let (rect, _) = ui.allocate_exact_size(Vec2::new(avail_w, cfg.row_height), Sense::hover());
        y_center.insert(layer.name.clone(), rect.center().y);
        gutter_x = rect.left() + cfg.gutter * 0.5;

        if !ui.is_rect_visible(rect) {
            continue;
        }
        let p = ui.painter();
        // Alternating row tint for readability.
        if i % 2 == 0 {
            p.rect_filled(rect, CornerRadius::same(4), theme::BG_SECONDARY.gamma_multiply(0.5));
        }

        let mut x = rect.left() + cfg.gutter;
        // z chip.
        p.text(
            Pos2::new(x, rect.center().y),
            Align2::LEFT_CENTER,
            &layer.z_label,
            FontId::monospace(11.0),
            theme::TEXT_MUTED,
        );
        x += 26.0;
        // name.
        p.text(
            Pos2::new(x, rect.center().y),
            Align2::LEFT_CENTER,
            &layer.name,
            FontId::proportional(13.0),
            theme::TEXT_PRIMARY,
        );
        // variant badges (right after the name column).
        let mut bx = rect.left() + cfg.gutter + 26.0 + cfg.name_col;
        for (vi, v) in layer.variants.iter().enumerate() {
            let badge = Rect::from_min_size(Pos2::new(bx, rect.center().y - 8.0), Vec2::new(20.0, 16.0));
            p.rect_filled(badge, CornerRadius::same(4), variant_color(vi).gamma_multiply(0.30));
            p.text(badge.center(), Align2::CENTER_CENTER, v, FontId::proportional(10.0), variant_color(vi));
            bx += 24.0;
        }

        // Presence bar.
        let bar_x = (rect.right() - cfg.bar_width - 70.0).max(bx + 8.0);
        let bar = Rect::from_min_size(
            Pos2::new(bar_x, rect.center().y - 6.0),
            Vec2::new(cfg.bar_width, 12.0),
        );
        p.rect_filled(bar, CornerRadius::same(3), theme::BG_SECONDARY);
        let frac = (layer.present_pct / 100.0).clamp(0.0, 1.0);
        if frac > 0.0 {
            let fill = Rect::from_min_size(bar.min, Vec2::new(bar.width() * frac, bar.height()));
            // Full presence reads calm/green; partial reads as accent.
            let col = if layer.present_pct >= 99.5 { theme::SUCCESS } else { theme::ACCENT };
            p.rect_filled(fill, CornerRadius::same(3), col.gamma_multiply(0.8));
        }
        // "NN% · M opts" to the right of the bar.
        p.text(
            Pos2::new(bar.right() + 8.0, rect.center().y),
            Align2::LEFT_CENTER,
            format!("{:.0}% · {} opts", layer.present_pct, layer.option_count),
            FontId::proportional(10.5),
            theme::TEXT_SECONDARY,
        );
    }

    // Flow connectors in the gutter (curved), drawn after rows so all y's are known.
    let painter = ui.painter();
    for f in &comp.flow {
        if let (Some(&y0), Some(&y1)) = (y_center.get(&f.from), y_center.get(&f.to)) {
            draw_flow(painter, gutter_x, y0, y1, theme::ACCENT_CYAN.gamma_multiply(0.8));
        }
    }
}

/// Draw a curved connector in the gutter from `y0` to `y1` at column `x`, bulging left.
fn draw_flow(painter: &egui::Painter, x: f32, y0: f32, y1: f32, color: Color32) {
    let bulge = 14.0;
    let p0 = Pos2::new(x, y0);
    let p3 = Pos2::new(x, y1);
    let p1 = Pos2::new(x - bulge, y0);
    let p2 = Pos2::new(x - bulge, y1);
    // Sample the cubic bezier into segments.
    let mut prev = p0;
    let steps = 18;
    for s in 1..=steps {
        let t = s as f32 / steps as f32;
        let mt = 1.0 - t;
        let pt = Pos2::new(
            mt * mt * mt * p0.x + 3.0 * mt * mt * t * p1.x + 3.0 * mt * t * t * p2.x + t * t * t * p3.x,
            mt * mt * mt * p0.y + 3.0 * mt * mt * t * p1.y + 3.0 * mt * t * t * p2.y + t * t * t * p3.y,
        );
        painter.line_segment([prev, pt], Stroke::new(1.6, color));
        prev = pt;
    }
    // Small dot at the target end.
    painter.circle_filled(p3, 2.5, color);
}

/// A colour for variant `index`, cycling the accent palette (matches `variant_split`).
fn variant_color(index: usize) -> Color32 {
    const PALETTE: &[Color32] = &[
        theme::ACCENT_BLUE,
        theme::ACCENT_GREEN,
        theme::ACCENT_MAGENTA,
        theme::ACCENT_ORANGE,
        theme::ACCENT_CYAN,
        theme::ACCENT_YELLOW,
    ];
    PALETTE[index % PALETTE.len()]
}
