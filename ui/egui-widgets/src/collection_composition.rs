//! Collection composition — a promotable "how this collection is generated" infographic.
//!
//! Renders the z-ordered layer stack (front on top, like a layers panel): each layer
//! with its presence rate and a left-aligned grid of its values — one rounded cell per
//! value, showing its within-slot % (uniform = even, varied = per-asset overrides). A
//! rounded **bracket** in the left gutter groups slots coupled by `variant_flow`, under
//! a headline stats band. Hovering a value cell previews that asset in the top-right.
//! Read-only — fed from the config + diagnosis; both an explainer and a clean graphic
//! you could screenshot to promote the collection.

use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};

use crate::theme;

// ============================================================================
// Types
// ============================================================================

/// One value's within-slot share and what to preview on hover.
pub struct ValueShare {
    /// Within-slot share, `0.0..=100.0` (sums to ~100 across the slot's values).
    pub pct: f32,
    /// Display name, shown in the hover preview caption.
    pub label: String,
    /// Opaque key (e.g. asset path), surfaced via [`CompositionResponse::hovered`] so the
    /// caller can render a preview / fetch a thumbnail for the hovered value.
    pub id: String,
}

/// One layer of the collection. The caller supplies layers already in render order
/// (front-most first); the widget does not re-sort them.
pub struct CompositionLayer {
    pub name: String,
    /// How often this layer is present, `0.0..=100.0` (100 = always).
    pub present_pct: f32,
    pub option_count: usize,
    /// Variant tokens, e.g. `["a", "b"]`.
    pub variants: Vec<String>,
    /// Per-value distribution — all values, rendered as a wrapping grid of % cells.
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

/// The whole composition.
pub struct CollectionComposition {
    pub title: String,
    pub stats: Vec<CompositionStat>,
    /// Layers in any order — rendered front (highest z) on top.
    pub layers: Vec<CompositionLayer>,
    pub flow: Vec<CompositionFlow>,
}

/// Layout/styling knobs.
pub struct CompositionConfig {
    pub row_min_height: f32,
    pub gutter: f32,
    /// Width of the left column (gutter + z + name + badges + presence %).
    pub left_col: f32,
    pub cell: Vec2,
    pub cell_pitch: Vec2,
}

impl Default for CompositionConfig {
    fn default() -> Self {
        Self {
            row_min_height: 30.0,
            gutter: 34.0,
            left_col: 300.0,
            cell: Vec2::new(22.0, 17.0),
            cell_pitch: Vec2::new(26.0, 21.0),
        }
    }
}

/// The value currently under the cursor, so the caller can render a preview for it.
pub struct Hovered {
    pub id: String,
    pub layer: String,
    pub label: String,
    pub pct: f32,
}

// ============================================================================
// Widget
// ============================================================================

/// The layer-stack's response plus the value currently hovered (if any), so the caller
/// can render a preview for it (e.g. via its own thumbnail cache).
pub struct CompositionResponse {
    pub response: egui::Response,
    pub hovered: Option<Hovered>,
}

/// Render the composition header — title + headline stats + the legend caption. Keep this
/// OUTSIDE the scroll area (with the preview) so it (and the hover preview) stay visible
/// while the layer stack scrolls.
pub fn show_header(ui: &mut Ui, comp: &CollectionComposition) {
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
        egui::RichText::new(
            "Layer stack, front on top — the % is how often the layer appears in a token; \
             cells are its values, with weights shown only where overridden.",
        )
        .color(theme::TEXT_MUTED)
        .size(10.5),
    );
}

/// Render the z-ordered layer stack (intended for inside a scroll area). Returns the
/// value under the cursor so the caller can render a preview for it.
pub fn show_stack(
    ui: &mut Ui,
    comp: &CollectionComposition,
    cfg: &CompositionConfig,
) -> CompositionResponse {
    let mut hovered = None;
    let response = ui
        .scope(|ui| {
            hovered = layer_stack(ui, comp, cfg);
        })
        .response;
    CompositionResponse { response, hovered }
}

fn layer_stack(
    ui: &mut Ui,
    comp: &CollectionComposition,
    cfg: &CompositionConfig,
) -> Option<Hovered> {
    // Layers are rendered in the order given (caller sorts front-most first).
    let avail_w = ui.available_width();
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let value_x_off = cfg.left_col;
    let value_area_w = (avail_w - value_x_off - 8.0).max(cfg.cell_pitch.x);
    let per_line = ((value_area_w / cfg.cell_pitch.x).floor() as usize).max(1);
    let pad = 6.0;

    let mut y_name: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
    let mut row_left = 0.0_f32;
    let mut hovered: Option<Hovered> = None;

    for (i, layer) in comp.layers.iter().enumerate() {
        let n = layer.values.len();
        let lines = if n == 0 { 1 } else { n.div_ceil(per_line) };
        let row_h = (lines as f32 * cfg.cell_pitch.y + pad).max(cfg.row_min_height);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(avail_w, row_h), Sense::hover());
        row_left = rect.left();
        let name_y = rect.top() + pad + cfg.cell.y * 0.5;
        y_name.insert(layer.name.clone(), name_y);

        if !ui.is_rect_visible(rect) {
            continue;
        }
        let p = ui.painter();
        if i % 2 == 0 {
            p.rect_filled(
                rect,
                CornerRadius::same(4),
                theme::BG_SECONDARY.gamma_multiply(0.5),
            );
        }

        // Left column: name, variant badges, presence %.
        let x = rect.left() + cfg.gutter;
        p.text(
            Pos2::new(x, name_y),
            Align2::LEFT_CENTER,
            &layer.name,
            FontId::proportional(13.0),
            theme::TEXT_PRIMARY,
        );
        let mut bx = rect.left() + cfg.gutter + 144.0;
        for (vi, v) in layer.variants.iter().enumerate() {
            let badge = Rect::from_min_size(Pos2::new(bx, name_y - 8.0), Vec2::new(20.0, 16.0));
            p.rect_filled(
                badge,
                CornerRadius::same(4),
                variant_color(vi).gamma_multiply(0.30),
            );
            p.text(
                badge.center(),
                Align2::CENTER_CENTER,
                v,
                FontId::proportional(10.0),
                variant_color(vi),
            );
            bx += 24.0;
        }
        let present_col = if layer.present_pct >= 99.5 {
            theme::SUCCESS
        } else {
            theme::ACCENT
        };
        // Show 2 decimals for present-but-rare layers (e.g. 0.18%) so they don't read 0%.
        let present_txt = if layer.present_pct > 0.0 && layer.present_pct < 1.0 {
            format!("{:.2}%", layer.present_pct)
        } else {
            format!("{:.0}%", layer.present_pct)
        };
        p.text(
            Pos2::new(rect.left() + cfg.left_col - 12.0, name_y),
            Align2::RIGHT_CENTER,
            present_txt,
            FontId::proportional(11.0),
            present_col,
        );

        // Value grid — all values, sorted by share desc, left-aligned, wrapping.
        let mut vals: Vec<&ValueShare> = layer.values.iter().collect();
        vals.sort_by(|a, b| {
            b.pct
                .partial_cmp(&a.pct)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        // Even distribution → no per-cell %s (just the count of cells); only overrides
        // (a non-uniform spread) show their weights.
        let spread = vals
            .first()
            .zip(vals.last())
            .map(|(a, b)| a.pct - b.pct)
            .unwrap_or(0.0);
        let is_even = vals.len() <= 1 || spread <= 1.5;
        for (vi, v) in vals.iter().enumerate() {
            let col = vi % per_line;
            let line = vi / per_line;
            let cell = Rect::from_min_size(
                Pos2::new(
                    rect.left() + value_x_off + col as f32 * cfg.cell_pitch.x,
                    rect.top() + pad + line as f32 * cfg.cell_pitch.y,
                ),
                cfg.cell,
            );
            let hot = pointer.map(|pp| cell.contains(pp)).unwrap_or(false);
            p.rect_filled(cell, CornerRadius::same(4), theme::BG_HIGHLIGHT);
            p.rect_stroke(
                cell,
                CornerRadius::same(4),
                Stroke::new(
                    if hot { 1.5 } else { 1.0 },
                    if hot { theme::ACCENT } else { theme::BORDER },
                ),
                egui::StrokeKind::Inside,
            );
            if !is_even {
                let txt = if v.pct < 0.95 {
                    "<1".to_string()
                } else {
                    format!("{:.0}", v.pct)
                };
                p.text(
                    cell.center(),
                    Align2::CENTER_CENTER,
                    txt,
                    FontId::proportional(9.0),
                    theme::TEXT_SECONDARY,
                );
            }
            if hot {
                hovered = Some(Hovered {
                    id: v.id.clone(),
                    layer: layer.name.clone(),
                    label: v.label.clone(),
                    pct: v.pct,
                });
            }
        }
    }

    // Flow groups: a rounded bracket per connected component of the flow graph.
    let gutter_right = row_left + cfg.gutter - 8.0;
    let spine_x = row_left + 8.0;
    for group in flow_groups(&comp.flow) {
        let mut ys: Vec<f32> = group
            .iter()
            .filter_map(|n| y_name.get(n).copied())
            .collect();
        if ys.len() < 2 {
            continue;
        }
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        draw_bracket(
            ui.painter(),
            spine_x,
            gutter_right,
            &ys,
            theme::ACCENT_CYAN.gamma_multiply(0.85),
        );
    }

    hovered
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

/// A rounded bracket: a vertical spine from top to bottom member with rounded outer
/// corners and a stub to each member row.
fn draw_bracket(painter: &egui::Painter, spine_x: f32, right: f32, ys: &[f32], color: Color32) {
    let top = ys[0];
    let bot = ys[ys.len() - 1];
    let r = 6.0_f32;
    let stroke = Stroke::new(1.6, color);

    let mut path = vec![Pos2::new(right, top), Pos2::new(spine_x + r, top)];
    arc(&mut path, Pos2::new(spine_x + r, top + r), r, 270.0, 180.0);
    path.push(Pos2::new(spine_x, bot - r));
    arc(&mut path, Pos2::new(spine_x + r, bot - r), r, 180.0, 90.0);
    path.push(Pos2::new(right, bot));
    painter.add(Shape::line(path, stroke));

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
