//! Collection composition — a promotable "how this collection is generated" infographic.
//!
//! Renders the z-ordered layer stack (front on top, like a layers panel): each layer
//! with its presence rate, and either a presence bar or a per-value **distribution
//! strip** (swatches + %, so even vs. overridden distributions read at a glance). A
//! rounded **bracket** in the left gutter groups slots coupled by `variant_flow`, under
//! a headline stats band. Read-only — fed from the config + diagnosis. It's both an
//! explainer and a clean graphic you could screenshot to promote the collection.

use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};

use crate::theme;

// ============================================================================
// Types
// ============================================================================

/// One value's share within a layer, for the distribution strip. `swatch` is a
/// placeholder colour (in studio, the value's dominant colour or a thumbnail).
pub struct ValueShare {
    pub pct: f32,
    pub swatch: Color32,
}

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
    /// Per-value distribution (top values). Empty → show a plain presence bar; non-empty
    /// → show a swatch strip (reveals even vs. per-slot-override distributions).
    pub values: Vec<ValueShare>,
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
    /// Max value swatches shown per layer before a "+N" overflow chip.
    pub max_swatches: usize,
}

impl Default for CompositionConfig {
    fn default() -> Self {
        Self {
            row_height: 32.0,
            gutter: 34.0,
            bar_width: 150.0,
            name_col: 150.0,
            max_swatches: 7,
        }
    }
}

// ============================================================================
// Widget
// ============================================================================

/// Render the composition infographic. Returns the overall `Response`.
pub fn show(ui: &mut Ui, comp: &CollectionComposition, cfg: &CompositionConfig) -> egui::Response {
    ui.scope(|ui| {
        ui.label(
            egui::RichText::new(&comp.title)
                .color(theme::TEXT_PRIMARY)
                .strong()
                .size(18.0),
        );

        if !comp.stats.is_empty() {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                for (i, s) in comp.stats.iter().enumerate() {
                    if i > 0 {
                        ui.add_space(18.0);
                    }
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(&s.value).color(theme::ACCENT).strong().size(18.0));
                        ui.label(egui::RichText::new(&s.label).color(theme::TEXT_MUTED).size(10.5));
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
    .response
}

fn layer_stack(ui: &mut Ui, comp: &CollectionComposition, cfg: &CompositionConfig) {
    // Front (highest z) on top.
    let mut order: Vec<usize> = (0..comp.layers.len()).collect();
    let zkey = |s: &str| s.split(':').next().unwrap_or("0").trim().parse::<u32>().unwrap_or(0);
    order.sort_by(|&a, &b| zkey(&comp.layers[b].z_label).cmp(&zkey(&comp.layers[a].z_label)));

    let avail_w = ui.available_width();
    let mut y_center: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
    let mut row_left = 0.0_f32;

    for &i in &order {
        let layer = &comp.layers[i];
        let (rect, _) = ui.allocate_exact_size(Vec2::new(avail_w, cfg.row_height), Sense::hover());
        y_center.insert(layer.name.clone(), rect.center().y);
        row_left = rect.left();

        if !ui.is_rect_visible(rect) {
            continue;
        }
        let p = ui.painter();
        if i % 2 == 0 {
            p.rect_filled(rect, CornerRadius::same(4), theme::BG_SECONDARY.gamma_multiply(0.5));
        }

        let cy = rect.center().y;
        let mut x = rect.left() + cfg.gutter;
        p.text(Pos2::new(x, cy), Align2::LEFT_CENTER, &layer.z_label, FontId::monospace(11.0), theme::TEXT_MUTED);
        x += 26.0;
        p.text(Pos2::new(x, cy), Align2::LEFT_CENTER, &layer.name, FontId::proportional(13.0), theme::TEXT_PRIMARY);

        // Variant badges.
        let mut bx = rect.left() + cfg.gutter + 26.0 + cfg.name_col;
        for (vi, v) in layer.variants.iter().enumerate() {
            let badge = Rect::from_min_size(Pos2::new(bx, cy - 8.0), Vec2::new(20.0, 16.0));
            p.rect_filled(badge, CornerRadius::same(4), variant_color(vi).gamma_multiply(0.30));
            p.text(badge.center(), Align2::CENTER_CENTER, v, FontId::proportional(10.0), variant_color(vi));
            bx += 24.0;
        }

        // Right side: present% label (far right) + distribution (swatch strip or bar).
        let present_x = rect.right() - 4.0;
        let present_col = if layer.present_pct >= 99.5 { theme::SUCCESS } else { theme::ACCENT };
        p.text(
            Pos2::new(present_x, cy),
            Align2::RIGHT_CENTER,
            format!("{:.0}%", layer.present_pct),
            FontId::proportional(11.0),
            present_col,
        );
        let dist_right = rect.right() - 52.0;

        if layer.values.is_empty() {
            // Plain presence bar.
            let bar = Rect::from_min_size(
                Pos2::new(dist_right - cfg.bar_width, cy - 6.0),
                Vec2::new(cfg.bar_width, 12.0),
            );
            p.rect_filled(bar, CornerRadius::same(3), theme::BG_SECONDARY);
            let frac = (layer.present_pct / 100.0).clamp(0.0, 1.0);
            if frac > 0.0 {
                let fill = Rect::from_min_size(bar.min, Vec2::new(bar.width() * frac, bar.height()));
                p.rect_filled(fill, CornerRadius::same(3), present_col.gamma_multiply(0.8));
            }
            p.text(
                Pos2::new(bar.left() - 6.0, cy),
                Align2::RIGHT_CENTER,
                format!("{} opts", layer.option_count),
                FontId::proportional(10.0),
                theme::TEXT_MUTED,
            );
        } else {
            // Distribution strip: top values as swatches (sorted desc), each with its %.
            let mut vals: Vec<&ValueShare> = layer.values.iter().collect();
            vals.sort_by(|a, b| b.pct.partial_cmp(&a.pct).unwrap_or(std::cmp::Ordering::Equal));
            let shown = vals.len().min(cfg.max_swatches);
            let overflow = layer.option_count.saturating_sub(shown);
            let sw = 18.0;
            let gap = 6.0;
            let chip_w = if overflow > 0 { 34.0 } else { 0.0 };
            let strip_w = shown as f32 * (sw + gap) + chip_w;
            let mut sx = dist_right - strip_w;
            for v in vals.iter().take(shown) {
                let r = Rect::from_min_size(Pos2::new(sx, cy - 11.0), Vec2::splat(sw));
                p.rect_filled(r, CornerRadius::same(4), v.swatch);
                p.rect_stroke(r, CornerRadius::same(4), Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);
                p.text(
                    Pos2::new(r.center().x, r.bottom() + 6.0),
                    Align2::CENTER_CENTER,
                    format!("{:.0}", v.pct.round()),
                    FontId::proportional(8.0),
                    theme::TEXT_MUTED,
                );
                sx += sw + gap;
            }
            if overflow > 0 {
                p.text(
                    Pos2::new(sx, cy),
                    Align2::LEFT_CENTER,
                    format!("+{overflow}"),
                    FontId::proportional(10.0),
                    theme::TEXT_MUTED,
                );
            }
        }
    }

    // Flow groups: a rounded bracket per connected component of the flow graph.
    let gutter_right = row_left + cfg.gutter - 8.0;
    let spine_x = row_left + 8.0;
    for group in flow_groups(&comp.flow) {
        let mut ys: Vec<f32> = group.iter().filter_map(|n| y_center.get(n).copied()).collect();
        if ys.len() < 2 {
            continue;
        }
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        draw_bracket(ui.painter(), spine_x, gutter_right, &ys, theme::ACCENT_CYAN.gamma_multiply(0.85));
    }
}

/// Connected components of the flow graph — each is a set of coupled slot names.
fn flow_groups(flow: &[CompositionFlow]) -> Vec<Vec<String>> {
    use std::collections::{HashMap, HashSet};
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for f in flow {
        adj.entry(&f.from).or_default().push(&f.to);
        adj.entry(&f.to).or_default().push(&f.from);
    }
    let mut seen: HashSet<&str> = HashSet::new();
    let mut groups = Vec::new();
    for &start in adj.keys() {
        if seen.contains(start) {
            continue;
        }
        let mut stack = vec![start];
        let mut comp = Vec::new();
        while let Some(n) = stack.pop() {
            if !seen.insert(n) {
                continue;
            }
            comp.push(n.to_string());
            if let Some(ns) = adj.get(n) {
                stack.extend(ns.iter().copied());
            }
        }
        groups.push(comp);
    }
    groups
}

/// A rounded bracket in the gutter: a vertical spine from the top to bottom member,
/// rounded outer corners, and a stub to each member row.
fn draw_bracket(painter: &egui::Painter, spine_x: f32, right: f32, ys: &[f32], color: Color32) {
    let top = ys[0];
    let bot = ys[ys.len() - 1];
    let r = 6.0_f32;
    let stroke = Stroke::new(1.6, color);

    // Outer path: top stub → rounded corner → spine → rounded corner → bottom stub.
    let mut path = vec![Pos2::new(right, top), Pos2::new(spine_x + r, top)];
    arc(&mut path, Pos2::new(spine_x + r, top + r), r, 270.0, 180.0);
    path.push(Pos2::new(spine_x, bot - r));
    arc(&mut path, Pos2::new(spine_x + r, bot - r), r, 180.0, 90.0);
    path.push(Pos2::new(right, bot));
    painter.add(Shape::line(path, stroke));

    // Interior stubs.
    for &y in &ys[1..ys.len().saturating_sub(1)] {
        painter.line_segment([Pos2::new(spine_x, y), Pos2::new(right, y)], stroke);
        painter.circle_filled(Pos2::new(right, y), 2.2, color);
    }
    painter.circle_filled(Pos2::new(right, top), 2.2, color);
    painter.circle_filled(Pos2::new(right, bot), 2.2, color);
}

/// Append a quarter-arc (degrees) around `c` of radius `r` to `path`.
fn arc(path: &mut Vec<Pos2>, c: Pos2, r: f32, from_deg: f32, to_deg: f32) {
    let steps = 6;
    for s in 0..=steps {
        let t = s as f32 / steps as f32;
        let a = (from_deg + (to_deg - from_deg) * t).to_radians();
        path.push(Pos2::new(c.x + r * a.cos(), c.y + r * a.sin()));
    }
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
