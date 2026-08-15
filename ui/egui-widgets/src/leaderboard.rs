//! `Leaderboard` — ranked standings: podium-tinted ranks, an optional prize
//! thumbnail, one headline metric, supporting stats, and a share bar.
//!
//! Domain-free by design. The host formats every string (ADA, counts, handles)
//! and hands over view-model rows; the widget owns ranking presentation only.
//! That keeps one renderer usable for buyer leaderboards, seller leaderboards,
//! holder rankings or anything else with an order and a number.
//!
//! Two layout rules worth knowing, both learned the hard way:
//!
//! - **The share bar is its own thin strip under the row, never a fill behind
//!   the text.** A translucent fill behind text washes it out at exactly the
//!   rows you most want to read (the leaders, whose bars are widest).
//! - **Columns are measured across all rows before anything is drawn**, so the
//!   value and stat columns line up vertically regardless of how wide each
//!   row's numbers happen to be.
//!
//! No emoji: the bundled fonts have no colour-emoji coverage, so a medal
//! renders as a tofu box. The podium is conveyed with colour instead (see
//! `tests/no_broken_glyphs.rs`, which enforces this).

use egui::{Align, Color32, CornerRadius, Layout, Rect, RichText, Sense, Stroke, Ui, Vec2};

/// A supporting stat shown after the headline value (e.g. `12` / `assets`).
#[derive(Clone, Debug, Default)]
pub struct LeaderboardStat {
    pub value: String,
    pub label: String,
}

impl LeaderboardStat {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }

    fn text(&self) -> String {
        format!("{} {}", self.value, self.label)
    }
}

/// The standout item behind a row's ranking — e.g. the priciest asset a buyer
/// took. Gives a leaderboard a face instead of a wall of numbers.
#[derive(Clone, Debug, Default)]
pub struct LeaderboardPrize {
    /// Thumbnail URL. `None` renders the placeholder frame.
    pub image_url: Option<String>,
    /// What it is (asset name) — hover only, to keep rows single-line.
    pub label: String,
    /// What it cost, pre-formatted — also hover only.
    pub value: String,
}

/// One ranked row.
#[derive(Clone, Debug, Default)]
pub struct LeaderboardRow {
    /// 1-based rank. Rendered verbatim rather than derived from position, so a
    /// host can show a slice ("your position: 47") without the numbers lying.
    pub rank: usize,
    /// Primary identity — a handle, name, or shortened address.
    pub name: String,
    /// Extra hover detail (e.g. the full stake address behind a handle).
    pub tooltip: Option<String>,
    /// Formatted headline metric.
    pub value: String,
    /// Share of the top row's value, `0.0..=1.0`. Drives the bar strip.
    pub share: f32,
    /// Supporting stats, rendered right of the value in fixed columns.
    pub stats: Vec<LeaderboardStat>,
    /// Standout item for this row, shown as a leading thumbnail.
    pub prize: Option<LeaderboardPrize>,
    /// Draw this row as the viewer's own.
    pub is_viewer: bool,
}

#[derive(Clone, Debug)]
pub struct LeaderboardConfig {
    /// Tint the top three ranks gold/silver/bronze.
    pub podium_tint: bool,
    /// Height of the text portion of a row (the bar strip sits below it).
    pub row_height: f32,
    /// Width reserved for the rank gutter.
    pub rank_width: f32,
    /// Edge length of the prize thumbnail. Rows without a prize still reserve
    /// the space when any row has one, so names stay aligned.
    pub prize_size: f32,
    /// Thickness of the share bar strip.
    pub bar_height: f32,
    pub bar_color: Color32,
    pub bar_color_viewer: Color32,
    /// Track behind the bar, so a short bar still reads as "out of a whole".
    pub bar_track_color: Color32,
    pub text_primary: Color32,
    pub text_muted: Color32,
    pub value_color: Color32,
    /// Podium colours for ranks 1, 2 and 3.
    pub podium_colors: [Color32; 3],
    /// Message when there are no rows.
    pub empty_text: String,
}

impl Default for LeaderboardConfig {
    fn default() -> Self {
        Self {
            podium_tint: true,
            row_height: 30.0,
            rank_width: 26.0,
            prize_size: 24.0,
            bar_height: 2.0,
            bar_color: Color32::from_rgb(122, 162, 247),
            bar_color_viewer: Color32::from_rgb(158, 206, 106),
            // NB unmultiplied: `from_rgba_premultiplied` expects RGB already
            // scaled by alpha, so passing full-brightness channels with a low
            // alpha renders near-opaque instead of as a tint.
            bar_track_color: Color32::from_rgba_unmultiplied(122, 162, 247, 26),
            text_primary: Color32::from_rgb(192, 202, 245),
            text_muted: Color32::from_rgb(86, 95, 137),
            value_color: Color32::from_rgb(125, 207, 255),
            podium_colors: [
                Color32::from_rgb(224, 175, 104), // gold
                Color32::from_rgb(169, 177, 214), // silver
                Color32::from_rgb(191, 130, 90),  // bronze
            ],
            empty_text: "No entries".to_string(),
        }
    }
}

/// What the user did. `None` when nothing was interacted with this frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LeaderboardAction {
    /// Index into the rows passed to [`show`] (not the rank).
    RowClicked(usize),
}

/// Rank colour: podium tint for the top three, muted otherwise.
fn rank_color(rank: usize, config: &LeaderboardConfig) -> Color32 {
    match (config.podium_tint, rank) {
        (true, 1..=3) => config.podium_colors[rank - 1],
        _ => config.text_muted,
    }
}

/// Render the leaderboard. Returns an action when a row was clicked.
///
/// Lays out at natural height — the host decides whether to wrap it in a
/// scroll area, so an embedded leaderboard in an already-scrolling page
/// doesn't grow a nested scrollbar.
pub fn show(
    ui: &mut Ui,
    rows: &[LeaderboardRow],
    config: &LeaderboardConfig,
) -> Option<LeaderboardAction> {
    crate::install_phosphor_font(ui.ctx());

    if rows.is_empty() {
        ui.label(
            RichText::new(&config.empty_text)
                .color(config.text_muted)
                .size(10.0),
        );
        return None;
    }

    let value_font = egui::FontId::monospace(12.0);
    let stat_font = egui::FontId::proportional(10.0);
    let name_font = egui::FontId::proportional(12.0);

    // Measure every column across every row FIRST. Right-aligning each row
    // independently makes the decimal points wander, which is what makes a
    // numeric column look broken.
    let measure = |text: String, font: egui::FontId| -> f32 {
        ui.painter()
            .layout_no_wrap(text, font, Color32::WHITE)
            .size()
            .x
    };
    let value_w = rows
        .iter()
        .map(|r| measure(r.value.clone(), value_font.clone()))
        .fold(0.0_f32, f32::max);
    let stat_count = rows.iter().map(|r| r.stats.len()).max().unwrap_or(0);
    let stat_ws: Vec<f32> = (0..stat_count)
        .map(|i| {
            rows.iter()
                .filter_map(|r| r.stats.get(i))
                .map(|s| measure(s.text(), stat_font.clone()))
                .fold(0.0_f32, f32::max)
        })
        .collect();

    const COL_GAP: f32 = 12.0;
    let stats_total: f32 = stat_ws.iter().map(|w| w + COL_GAP).sum();
    let has_prize = rows.iter().any(|r| r.prize.is_some());
    let prize_w = if has_prize {
        config.prize_size + 6.0
    } else {
        0.0
    };

    let mut action = None;
    let full_width = ui.available_width();
    let row_total_height = config.row_height + config.bar_height + 3.0;

    for (idx, row) in rows.iter().enumerate() {
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(full_width, row_total_height), Sense::click());
        let text_rect = Rect::from_min_size(rect.min, Vec2::new(rect.width(), config.row_height));

        if response.hovered() {
            // A whisper of white: the row should lift off the background, not
            // become a white block that inverts every colour on it.
            ui.painter()
                .rect_filled(rect, CornerRadius::same(3), Color32::from_white_alpha(6));
        }
        if row.is_viewer {
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(3),
                Stroke::new(1.0_f32, config.bar_color_viewer),
                egui::StrokeKind::Inside,
            );
        }

        // Rank gutter — colour carries the podium, no glyph needed.
        ui.painter().text(
            egui::pos2(text_rect.min.x + 6.0, text_rect.center().y),
            egui::Align2::LEFT_CENTER,
            row.rank.to_string(),
            egui::FontId::proportional(if row.rank <= 3 && config.podium_tint {
                13.0
            } else {
                11.0
            }),
            rank_color(row.rank, config),
        );

        // Prize thumbnail.
        let mut cursor_x = text_rect.min.x + config.rank_width;
        if has_prize {
            let thumb = Rect::from_min_size(
                egui::pos2(cursor_x, text_rect.center().y - config.prize_size / 2.0),
                Vec2::splat(config.prize_size),
            );
            if let Some(prize) = &row.prize {
                let loading = crate::card_browser::draw_thumbnail(
                    ui,
                    thumb,
                    prize.image_url.as_deref(),
                    &crate::CardBrowserConfig::default(),
                );
                if loading {
                    crate::image_loader::CachedSpinner::request_repaint(ui);
                }
            }
            cursor_x += prize_w;
        }

        // Name, clipped so a long handle can't run under the value column.
        let name_rect = Rect::from_min_max(
            egui::pos2(cursor_x, text_rect.min.y),
            egui::pos2(
                (text_rect.max.x - value_w - stats_total - COL_GAP).max(cursor_x),
                text_rect.max.y,
            ),
        );
        ui.painter().with_clip_rect(name_rect).text(
            name_rect.left_center(),
            egui::Align2::LEFT_CENTER,
            &row.name,
            name_font.clone(),
            config.text_primary,
        );

        // Stats right-to-left in fixed columns, then the value column.
        let mut x = text_rect.max.x - 6.0;
        for (i, width) in stat_ws.iter().enumerate().rev() {
            if let Some(stat) = row.stats.get(i) {
                ui.painter().text(
                    egui::pos2(x, text_rect.center().y),
                    egui::Align2::RIGHT_CENTER,
                    stat.text(),
                    stat_font.clone(),
                    config.text_muted,
                );
            }
            x -= width + COL_GAP;
        }
        ui.painter().text(
            egui::pos2(x, text_rect.center().y),
            egui::Align2::RIGHT_CENTER,
            &row.value,
            value_font.clone(),
            config.value_color,
        );

        // Share bar: a thin strip along the bottom, over a faint full-width
        // track. Never behind the text.
        let bar_y = text_rect.max.y + 1.0;
        let track = Rect::from_min_size(
            egui::pos2(text_rect.min.x + 6.0, bar_y),
            Vec2::new(text_rect.width() - 12.0, config.bar_height),
        );
        ui.painter()
            .rect_filled(track, CornerRadius::same(1), config.bar_track_color);
        let share = row.share.clamp(0.0, 1.0);
        if share > 0.0 {
            let fill = Rect::from_min_size(
                track.min,
                Vec2::new(track.width() * share, config.bar_height),
            );
            ui.painter().rect_filled(
                fill,
                CornerRadius::same(1),
                if row.is_viewer {
                    config.bar_color_viewer
                } else {
                    config.bar_color
                },
            );
        }

        // Hover detail: the row's own tooltip plus what the prize was.
        let mut hover = row.tooltip.clone().unwrap_or_default();
        if let Some(prize) = &row.prize {
            if !hover.is_empty() {
                hover.push('\n');
            }
            hover.push_str(&format!(
                "top buy: {} \u{00b7} {}",
                prize.label, prize.value
            ));
        }
        let response = if hover.is_empty() {
            response
        } else {
            response.on_hover_text(hover)
        };
        if response.clicked() {
            action = Some(LeaderboardAction::RowClicked(idx));
        }
    }

    action
}

/// Compute `share` for a set of values against the largest — the common case
/// for building rows.
pub fn shares(values: &[u64]) -> Vec<f32> {
    let max = values.iter().copied().max().unwrap_or(0);
    if max == 0 {
        return vec![0.0; values.len()];
    }
    values.iter().map(|v| *v as f32 / max as f32).collect()
}

/// Header row matching the widget's column layout — optional, but keeps a
/// standalone leaderboard legible about what the number means.
pub fn header(ui: &mut Ui, name_label: &str, value_label: &str, config: &LeaderboardConfig) {
    ui.horizontal(|ui| {
        ui.spacing_mut().interact_size.y = 0.0;
        ui.add_space(config.rank_width);
        ui.label(RichText::new(name_label).color(config.text_muted).size(9.0));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(6.0);
            ui.label(
                RichText::new(value_label)
                    .color(config.text_muted)
                    .size(9.0),
            );
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shares_scale_against_the_leader() {
        assert_eq!(shares(&[100, 50, 0]), vec![1.0, 0.5, 0.0]);
        // All-zero must not divide by zero.
        assert_eq!(shares(&[0, 0]), vec![0.0, 0.0]);
        assert!(shares(&[]).is_empty());
    }

    #[test]
    fn podium_tint_applies_to_the_top_three_only() {
        let config = LeaderboardConfig::default();
        assert_eq!(rank_color(1, &config), config.podium_colors[0]);
        assert_eq!(rank_color(3, &config), config.podium_colors[2]);
        assert_eq!(rank_color(4, &config), config.text_muted);
        // Disabled: every rank reads as an ordinary row.
        let plain = LeaderboardConfig {
            podium_tint: false,
            ..Default::default()
        };
        assert_eq!(rank_color(1, &plain), plain.text_muted);
    }
}
