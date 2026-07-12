//! Variant split — explains a `variant_flow` source slot's **derived** variant
//! distribution and *why* it isn't uniform.
//!
//! In a constrained collection a source variant's share is weighted by its **downstream
//! asset capacity**, so a variant backed by more matching assets dominates. This widget
//! makes that cardinality driver visible at a glance: the coloured bar is the derived
//! split, the dotted ticks mark where a naive *uniform* split would fall, and each
//! segment is annotated with its asset count — so "skin is 81% `a`, not 50/50, because
//! clothes has 26 `a` vs 7 `b`" reads instantly.

use egui::{Align2, Color32, CornerRadius, FontId, Rect, RichText, Sense, Stroke, Ui, Vec2};

use crate::theme;

// ============================================================================
// Types
// ============================================================================

/// One variant of a `variant_flow` source slot.
pub struct VariantSegment {
    /// Variant token (e.g. `"a"`, `"b"`).
    pub variant: String,
    /// Derived share of the collection (`0.0..=1.0`).
    pub share: f32,
    /// Number of matching downstream assets — the cardinality that drives the share.
    pub asset_count: usize,
    /// Segment colour (see [`variant_color`]).
    pub color: Color32,
}

/// Configuration for [`show`].
pub struct VariantSplitConfig {
    pub bar_height: f32,
    pub corner_radius: u8,
    /// Draw the uniform-split baseline ticks (the naive `1/N` boundaries) so the
    /// cardinality skew is visible as the gap between derived and uniform.
    pub show_uniform_baseline: bool,
    pub label_size: f32,
    pub caption_size: f32,
}

impl Default for VariantSplitConfig {
    fn default() -> Self {
        Self {
            bar_height: 28.0,
            corner_radius: 5,
            show_uniform_baseline: true,
            label_size: 11.0,
            caption_size: 10.5,
        }
    }
}

/// A colour for variant `index`, cycling the accent palette.
pub fn variant_color(index: usize) -> Color32 {
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

// ============================================================================
// Widget
// ============================================================================

/// Render the variant split for one source `slot`. Returns the bar's `Response`.
pub fn show(
    ui: &mut Ui,
    slot: &str,
    segments: &[VariantSegment],
    config: &VariantSplitConfig,
) -> egui::Response {
    // Header: slot name + role.
    ui.horizontal(|ui| {
        ui.label(RichText::new(slot).color(theme::TEXT_PRIMARY).strong());
        ui.label(
            RichText::new("variant source")
                .color(theme::TEXT_MUTED)
                .size(config.caption_size),
        );
    });
    ui.add_space(4.0);

    let n = segments.iter().filter(|s| s.share > 0.0).count().max(1);
    let available_width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(available_width, config.bar_height),
        Sense::hover(),
    );

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let rounding = CornerRadius::same(config.corner_radius);
        painter.rect_filled(rect, rounding, theme::BG_SECONDARY);

        // Derived segments, left to right.
        let mut x = rect.min.x;
        for (i, seg) in segments.iter().enumerate() {
            if seg.share <= 0.0 {
                continue;
            }
            let w = rect.width() * seg.share;
            let seg_rect =
                Rect::from_min_size(egui::pos2(x, rect.min.y), Vec2::new(w, rect.height()));
            let is_first = i == 0 || segments[..i].iter().all(|s| s.share <= 0.0);
            let is_last =
                i == segments.len() - 1 || segments[i + 1..].iter().all(|s| s.share <= 0.0);
            let cr = config.corner_radius;
            let seg_rounding = CornerRadius {
                nw: if is_first { cr } else { 0 },
                sw: if is_first { cr } else { 0 },
                ne: if is_last { cr } else { 0 },
                se: if is_last { cr } else { 0 },
            };
            painter.rect_filled(seg_rect, seg_rounding, seg.color);

            if w >= 38.0 {
                painter.text(
                    seg_rect.center(),
                    Align2::CENTER_CENTER,
                    format!("{} · {}%", seg.variant, (seg.share * 100.0).round() as u32),
                    FontId::proportional(config.label_size),
                    theme::BG_PRIMARY,
                );
            }
            x += w;
        }

        // Uniform baseline: dotted ticks at the naive 1/N boundaries.
        if config.show_uniform_baseline && n > 1 {
            for k in 1..n {
                let tx = rect.min.x + rect.width() * (k as f32 / n as f32);
                let top = rect.min.y - 3.0;
                let bot = rect.max.y + 3.0;
                let mut y = top;
                while y < bot {
                    painter.line_segment(
                        [egui::pos2(tx, y), egui::pos2(tx, (y + 3.0).min(bot))],
                        Stroke::new(1.0_f32, theme::TEXT_PRIMARY.gamma_multiply(0.5)),
                    );
                    y += 6.0;
                }
            }
        }
    }

    // Legend: variant · share% · asset count (the driver).
    ui.add_space(5.0);
    ui.horizontal_wrapped(|ui| {
        for seg in segments {
            if seg.share <= 0.0 {
                continue;
            }
            let (dot, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
            if ui.is_rect_visible(dot) {
                ui.painter().circle_filled(dot.center(), 4.0, seg.color);
            }
            ui.label(
                RichText::new(format!(
                    "{} {}%",
                    seg.variant,
                    (seg.share * 100.0).round() as u32
                ))
                .color(theme::TEXT_SECONDARY)
                .size(config.label_size),
            );
            ui.label(
                RichText::new(format!("· {} assets", seg.asset_count))
                    .color(theme::TEXT_MUTED)
                    .size(config.caption_size),
            );
            ui.add_space(10.0);
        }
    });

    // Caption: the "why", generated from the data.
    if let Some(cap) = why_caption(segments, config.show_uniform_baseline && n > 1) {
        ui.add_space(2.0);
        ui.label(
            RichText::new(cap)
                .color(theme::TEXT_MUTED)
                .size(config.caption_size)
                .italics(),
        );
    }

    response
}

/// A one-line "why" derived from the segments: name the leading variant and contrast
/// its share with the uniform baseline, attributing it to asset capacity.
fn why_caption(segments: &[VariantSegment], has_baseline: bool) -> Option<String> {
    let live: Vec<&VariantSegment> = segments.iter().filter(|s| s.share > 0.0).collect();
    if live.len() < 2 {
        return None;
    }
    let top = live
        .iter()
        .max_by(|a, b| a.share.partial_cmp(&b.share).unwrap())?;
    let uniform = 1.0 / live.len() as f32;
    let skew = top.share - uniform;
    if has_baseline && skew > 0.02 {
        Some(format!(
            "Weighted by downstream capacity, not uniform — `{}` runs +{}pts over the {}% baseline ({} assets).",
            top.variant,
            (skew * 100.0).round() as i32,
            (uniform * 100.0).round() as u32,
            top.asset_count,
        ))
    } else {
        Some(format!(
            "Weighted by downstream capacity — `{}` leads with {} assets.",
            top.variant, top.asset_count
        ))
    }
}
