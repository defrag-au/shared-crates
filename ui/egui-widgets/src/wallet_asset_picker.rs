//! Wallet asset picker — modal widget for browsing and selecting NFTs from a
//! wallet, grouped by policy in an accordion layout.
//!
//! Supports multi-select with a summary bar showing all selected assets.
//! The widget is UI-only: it emits confirmed selections and the caller handles
//! business logic (adding to trade offers, etc).

use cardano_assets::AssetId;
use egui::{Color32, CornerRadius, Vec2};

use crate::card_browser;
use crate::icons::PhosphorIcon;
use crate::image_loader::{iiif_asset_url, AssetImageSize};
use crate::theme;

// ============================================================================
// Types
// ============================================================================

/// A single asset available for selection.
#[derive(Clone, Debug)]
pub struct PickerAsset {
    /// Asset name hex (for IIIF URL construction).
    pub asset_name_hex: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Optional rarity rank.
    pub rarity_rank: Option<u32>,
    /// Total ranked assets in collection.
    pub total_ranked: Option<u32>,
}

/// A policy group for display in the picker.
#[derive(Clone, Debug)]
pub struct PickerPolicyGroup {
    /// Policy ID hex.
    pub policy_id: String,
    /// Human-readable label (collection name or truncated policy ID).
    pub label: String,
    /// Assets under this policy.
    pub assets: Vec<PickerAsset>,
}

/// Configuration for the picker appearance.
pub struct WalletAssetPickerConfig {
    /// Modal title.
    pub title: &'static str,
    /// Card size (square).
    pub card_size: f32,
    /// Maximum modal width.
    pub max_width: f32,
    /// Maximum modal height.
    pub max_height: f32,
}

impl Default for WalletAssetPickerConfig {
    fn default() -> Self {
        Self {
            title: "Select Assets",
            card_size: 80.0,
            max_width: 480.0,
            max_height: 600.0,
        }
    }
}

/// Persistent widget state.
#[derive(Clone, Debug, Default)]
pub struct WalletAssetPickerState {
    /// Whether the modal is open.
    pub open: bool,
    /// Search filter text.
    pub search: String,
    /// Currently selected assets.
    pub selected: Vec<AssetId>,
}

impl WalletAssetPickerState {
    /// Check whether a specific asset is selected.
    pub fn is_selected(&self, policy_id: &str, asset_name_hex: &str) -> bool {
        self.selected
            .iter()
            .any(|id| id.policy_id == policy_id && id.asset_name_hex == asset_name_hex)
    }

    /// Toggle selection of an asset.
    fn toggle(&mut self, policy_id: &str, asset_name_hex: &str) {
        if let Some(idx) = self
            .selected
            .iter()
            .position(|id| id.policy_id == policy_id && id.asset_name_hex == asset_name_hex)
        {
            self.selected.remove(idx);
        } else {
            self.selected.push(AssetId::new_unchecked(
                policy_id.to_string(),
                asset_name_hex.to_string(),
            ));
        }
    }

    /// Count how many assets are selected from a given policy.
    fn selected_count_for_policy(&self, policy_id: &str) -> usize {
        self.selected
            .iter()
            .filter(|id| id.policy_id == policy_id)
            .count()
    }
}

/// Actions emitted by the widget.
pub enum WalletAssetPickerAction {
    /// User confirmed their selection (one or more assets).
    Confirmed(Vec<AssetId>),
    /// User closed the modal without confirming.
    Closed,
}

/// Response from a single frame.
pub struct WalletAssetPickerResponse {
    pub action: Option<WalletAssetPickerAction>,
}

// ============================================================================
// Main render
// ============================================================================

/// Render the wallet asset picker modal. Call every frame while the modal
/// might be open — it only renders when `state.open` is true.
pub fn show(
    ctx: &egui::Context,
    state: &mut WalletAssetPickerState,
    groups: &[PickerPolicyGroup],
    config: &WalletAssetPickerConfig,
) -> WalletAssetPickerResponse {
    let mut action = None;

    if !state.open {
        return WalletAssetPickerResponse { action };
    }

    crate::install_phosphor_font(ctx);

    let mut still_open = true;

    egui::Window::new(config.title)
        .open(&mut still_open)
        .resizable(false)
        .collapsible(false)
        .default_width(config.max_width)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .frame(
            egui::Frame::window(&ctx.style())
                .fill(theme::BG_PRIMARY)
                .stroke(egui::Stroke::new(1.0, theme::BG_HIGHLIGHT))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(16.0),
        )
        .show(ctx, |ui| {
            ui.set_max_width(config.max_width);

            // ── Search bar ──
            ui.horizontal(|ui| {
                PhosphorIcon::MagnifyingGlass.show(ui, 14.0, theme::TEXT_MUTED);
                ui.add(
                    egui::TextEdit::singleline(&mut state.search)
                        .desired_width(ui.available_width())
                        .hint_text("Search...")
                        .font(egui::FontId::monospace(11.0)),
                );
            });

            ui.add_space(8.0);

            // ── Scrollable accordion area ──
            let has_selection = !state.selected.is_empty();
            // Reserve space for summary bar + confirm when selections exist
            let bottom_reserve = if has_selection { 160.0 } else { 120.0 };
            let scroll_height = config.max_height - bottom_reserve;
            egui::ScrollArea::vertical()
                .max_height(scroll_height)
                .show(ui, |ui| {
                    let search_lower = state.search.to_lowercase();
                    let is_searching = !search_lower.is_empty();

                    for group in groups {
                        // Filter assets by search
                        let filtered: Vec<&PickerAsset> = if is_searching {
                            group
                                .assets
                                .iter()
                                .filter(|a| a.display_name.to_lowercase().contains(&search_lower))
                                .collect()
                        } else {
                            group.assets.iter().collect()
                        };

                        if filtered.is_empty() {
                            continue;
                        }

                        // Build header text with selection badge
                        let sel_count = state.selected_count_for_policy(&group.policy_id);
                        let header_text = if sel_count > 0 {
                            format!(
                                "{} ({}) \u{2022} {sel_count} selected",
                                group.label,
                                filtered.len()
                            )
                        } else {
                            format!("{} ({})", group.label, filtered.len())
                        };

                        let header_color = if sel_count > 0 {
                            theme::ACCENT_CYAN
                        } else {
                            theme::TEXT_PRIMARY
                        };

                        let mut header = egui::CollapsingHeader::new(
                            egui::RichText::new(header_text)
                                .color(header_color)
                                .size(11.0)
                                .strong(),
                        )
                        .id_salt(&group.policy_id)
                        .icon(phosphor_caret_icon);

                        // Force open when search is active
                        if is_searching {
                            header = header.open(Some(true));
                        }

                        header.show(ui, |ui| {
                            draw_card_grid(
                                ui,
                                &filtered,
                                &group.policy_id,
                                config.card_size,
                                state,
                            );
                        });
                    }
                });

            ui.add_space(8.0);

            // ── Selection summary bar ──
            if has_selection {
                draw_selection_summary(ui, state, groups);
                ui.add_space(8.0);
            }

            // ── Confirm button ──
            let count = state.selected.len();
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                    let label = if count > 0 {
                        format!("Confirm ({count})")
                    } else {
                        "Confirm".into()
                    };
                    let btn_text = if has_selection {
                        egui::RichText::new(label).color(theme::BG_PRIMARY).strong()
                    } else {
                        egui::RichText::new(label).color(theme::TEXT_MUTED).strong()
                    };
                    let btn = egui::Button::new(btn_text)
                        .fill(if has_selection {
                            theme::ACCENT_GREEN
                        } else {
                            theme::BG_SECONDARY
                        })
                        .corner_radius(CornerRadius::same(4))
                        .min_size(Vec2::new(120.0, 28.0));

                    let resp = ui.add_enabled(has_selection, btn);
                    if resp.clicked() && !state.selected.is_empty() {
                        let confirmed = state.selected.clone();
                        action = Some(WalletAssetPickerAction::Confirmed(confirmed));
                        state.open = false;
                        state.selected.clear();
                        state.search.clear();
                    }
                });
            });
        });

    if !still_open {
        state.open = false;
        state.selected.clear();
        state.search.clear();
        action = Some(WalletAssetPickerAction::Closed);
    }

    WalletAssetPickerResponse { action }
}

// ============================================================================
// Selection summary bar
// ============================================================================

/// Visual strip of selected asset thumbnails — click to deselect.
fn draw_selection_summary(
    ui: &mut egui::Ui,
    state: &mut WalletAssetPickerState,
    groups: &[PickerPolicyGroup],
) {
    use crate::asset_strip;

    let strip_items: Vec<asset_strip::AssetStripItem> = state
        .selected
        .iter()
        .filter_map(|id| {
            let group = groups.iter().find(|g| g.policy_id == id.policy_id)?;
            let asset = group
                .assets
                .iter()
                .find(|a| a.asset_name_hex == id.asset_name_hex)?;
            Some(asset_strip::AssetStripItem {
                asset_id: id.clone(),
                display_name: asset.display_name.clone(),
            })
        })
        .collect();

    let config = asset_strip::AssetStripConfig {
        thumb_size: 56.0,
        min_visible: 18.0,
    };
    let resp = asset_strip::show(ui, &strip_items, &config);

    if let Some(idx) = resp.clicked {
        if idx < state.selected.len() {
            state.selected.remove(idx);
        }
    }
}

// ============================================================================
// Phosphor caret icon for CollapsingHeader
// ============================================================================

/// Custom icon callback for `CollapsingHeader` that uses Phosphor CaretRight/CaretDown
/// instead of the default triangle. `openness` animates from 0.0 (closed) to 1.0 (open).
fn phosphor_caret_icon(ui: &mut egui::Ui, openness: f32, response: &egui::Response) {
    let icon = if openness > 0.5 {
        PhosphorIcon::CaretDown
    } else {
        PhosphorIcon::CaretRight
    };
    let color = if response.hovered() {
        theme::TEXT_PRIMARY
    } else {
        theme::TEXT_SECONDARY
    };
    let center = response.rect.center();
    icon.paint(
        ui.painter(),
        center,
        egui::Align2::CENTER_CENTER,
        12.0,
        color,
    );
}

// ============================================================================
// Card grid
// ============================================================================

fn draw_card_grid(
    ui: &mut egui::Ui,
    assets: &[&PickerAsset],
    policy_id: &str,
    card_size: f32,
    state: &mut WalletAssetPickerState,
) {
    let available_width = ui.available_width();
    let spacing = 6.0;
    let cols = ((available_width + spacing) / (card_size + spacing))
        .floor()
        .max(1.0) as usize;

    for row_assets in assets.chunks(cols) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = spacing;
            for asset in row_assets {
                draw_picker_card(ui, asset, policy_id, card_size, state);
            }
        });
        ui.add_space(spacing);
    }
}

fn draw_picker_card(
    ui: &mut egui::Ui,
    asset: &PickerAsset,
    policy_id: &str,
    card_size: f32,
    state: &mut WalletAssetPickerState,
) {
    let size = Vec2::splat(card_size);
    let (card_rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

    let is_selected = state.is_selected(policy_id, &asset.asset_name_hex);
    let hovered = response.hovered();
    let painter = ui.painter_at(card_rect);
    let rounding = CornerRadius::same(4);

    // Background
    painter.rect_filled(card_rect, rounding, theme::BG_SECONDARY);

    // IIIF thumbnail
    let image_url = iiif_asset_url(policy_id, &asset.asset_name_hex, AssetImageSize::Thumbnail);
    let browser_config = crate::CardBrowserConfig {
        rounding: 4.0,
        bg_card_hover: Color32::from_rgb(40, 40, 55),
        ..Default::default()
    };
    let loading = card_browser::draw_thumbnail(ui, card_rect, Some(&image_url), &browser_config);
    if loading {
        crate::image_loader::CachedSpinner::request_repaint(ui);
    }

    // Name overlay banner
    let banner_h = 16.0;
    let banner_rect = egui::Rect::from_min_size(
        egui::pos2(card_rect.min.x, card_rect.max.y - banner_h),
        Vec2::new(card_size, banner_h),
    );
    painter.rect_filled(
        banner_rect,
        CornerRadius {
            nw: 0,
            ne: 0,
            sw: 4,
            se: 4,
        },
        Color32::from_rgba_premultiplied(15, 15, 25, 200),
    );
    let name_rect = egui::Rect::from_min_size(
        egui::pos2(banner_rect.min.x + 3.0, banner_rect.min.y),
        Vec2::new(card_size - 6.0, banner_h),
    );
    painter.with_clip_rect(name_rect).text(
        name_rect.left_center(),
        egui::Align2::LEFT_CENTER,
        &asset.display_name,
        egui::FontId::monospace(8.0),
        theme::TEXT_PRIMARY,
    );

    // Selected checkmark badge (top-left)
    if is_selected {
        let badge_center = egui::pos2(card_rect.min.x + 10.0, card_rect.min.y + 10.0);
        painter.circle_filled(badge_center, 8.0, theme::ACCENT_CYAN);
        PhosphorIcon::Check.paint(
            &painter,
            badge_center,
            egui::Align2::CENTER_CENTER,
            10.0,
            theme::BG_PRIMARY,
        );
    }

    // Border — selection highlight, rarity, or default
    let (border_color, border_width) = if is_selected {
        (theme::ACCENT_CYAN, 2.0)
    } else if let Some(rank) = asset.rarity_rank {
        let total = asset.total_ranked.unwrap_or(10000);
        let color = theme::rarity_rank_color(rank, total);
        let width = if hovered { 2.0 } else { 1.5 };
        (color, width)
    } else {
        let color = if hovered {
            theme::TEXT_MUTED
        } else {
            theme::BG_HIGHLIGHT
        };
        (color, 1.0)
    };
    painter.rect_stroke(
        card_rect,
        rounding,
        egui::Stroke::new(border_width, border_color),
        egui::StrokeKind::Inside,
    );

    // Tooltip
    response.clone().on_hover_ui(|ui| {
        ui.label(
            egui::RichText::new(&asset.display_name)
                .color(theme::TEXT_PRIMARY)
                .size(11.0)
                .strong(),
        );
        if let Some(rank) = asset.rarity_rank {
            let total = asset.total_ranked.unwrap_or(0);
            let rank_color = theme::rarity_rank_color(rank, total);
            ui.label(
                egui::RichText::new(format!("Rank #{rank} / {total}"))
                    .color(rank_color)
                    .size(10.0),
            );
        }
    });

    // Click to toggle selection
    if response.clicked() {
        state.toggle(policy_id, &asset.asset_name_hex);
    }
}
