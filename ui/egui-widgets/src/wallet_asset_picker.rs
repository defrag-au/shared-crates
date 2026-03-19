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
    /// Decoded trait strings for search (e.g. `["Background:Red", "Eyes:Laser"]`).
    pub traits: Vec<String>,
    /// Available quantity in wallet (1 for NFTs, >1 for FTs).
    pub quantity: u64,
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
    /// If true, this group represents fungible tokens (shown in "Tokens" section).
    pub is_token_group: bool,
    /// Whether this policy has been verified by at least one source.
    pub is_verified: bool,
}

/// A selected asset with its chosen quantity.
#[derive(Clone, Debug)]
pub struct SelectedAsset {
    pub asset_id: AssetId,
    pub quantity: u64,
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
    /// Asset IDs already in the offer — shown as disabled/dimmed in the picker.
    pub already_offered: std::collections::HashSet<AssetId>,
    /// Whether to show unverified/unknown collections.
    pub show_unverified: bool,
}

impl WalletAssetPickerState {
    /// Check whether a specific asset is already in the offer.
    pub fn is_already_offered(&self, policy_id: &str, asset_name_hex: &str) -> bool {
        self.already_offered
            .iter()
            .any(|a| a.policy_id == policy_id && a.asset_name_hex == asset_name_hex)
    }

    /// Count how many assets from a given policy are already offered.
    fn offered_count_for_policy(&self, policy_id: &str) -> usize {
        self.already_offered
            .iter()
            .filter(|a| a.policy_id == policy_id)
            .count()
    }
}

/// Actions emitted by the widget.
pub enum WalletAssetPickerAction {
    /// User clicked an asset — add it to the offer immediately.
    Selected(SelectedAsset),
    /// User clicked an already-offered asset — remove it from the offer.
    Removed(AssetId),
    /// User closed the picker.
    Closed,
}

/// Response from a single frame.
pub struct WalletAssetPickerResponse {
    pub action: Option<WalletAssetPickerAction>,
}

// ============================================================================
// Main render
// ============================================================================

/// Render the wallet asset picker as a centered modal window.
/// Call every frame while the modal might be open — it only renders when
/// `state.open` is true.
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
        .max_height(config.max_height)
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
            ui.set_max_height(config.max_height);
            action = draw_picker_content(ui, state, groups, config);
        });

    if !still_open {
        state.open = false;
        state.search.clear();
        action = Some(WalletAssetPickerAction::Closed);
    }

    WalletAssetPickerResponse { action }
}

/// Render the picker content inline into any `Ui` region (e.g. a side panel).
/// Returns the action if the user confirms or closes. The caller is responsible
/// for layout (panel width, visibility toggling, etc.).
pub fn show_inline(
    ui: &mut egui::Ui,
    state: &mut WalletAssetPickerState,
    groups: &[PickerPolicyGroup],
    config: &WalletAssetPickerConfig,
) -> WalletAssetPickerResponse {
    crate::install_phosphor_font(ui.ctx());
    let action = draw_picker_content(ui, state, groups, config);
    WalletAssetPickerResponse { action }
}

/// Shared picker content — search, tokens section, collections accordion,
/// selection summary, confirm button.
/// Used by both the modal `show()` and the inline `show_inline()`.
fn draw_picker_content(
    ui: &mut egui::Ui,
    state: &mut WalletAssetPickerState,
    groups: &[PickerPolicyGroup],
    config: &WalletAssetPickerConfig,
) -> Option<WalletAssetPickerAction> {
    let mut action = None;

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

    // Filter out token groups — only show NFT collections
    let verified_groups: Vec<&PickerPolicyGroup> = groups
        .iter()
        .filter(|g| !g.is_token_group && g.is_verified)
        .collect();
    let unverified_groups: Vec<&PickerPolicyGroup> = groups
        .iter()
        .filter(|g| !g.is_token_group && !g.is_verified)
        .collect();

    // ── Scrollable area ──
    let scroll_height = (ui.available_height() - 8.0).max(100.0);
    egui::ScrollArea::vertical()
        .max_height(scroll_height)
        .show(ui, |ui| {
            let search_lower = state.search.to_lowercase();
            let is_searching = !search_lower.is_empty();

            // ── Verified collections ──
            draw_collection_section(
                ui,
                &verified_groups,
                &search_lower,
                is_searching,
                config,
                state,
                &mut action,
            );

            // ── Unverified collections (behind checkbox) ──
            if !unverified_groups.is_empty() {
                ui.add_space(8.0);
                ui.checkbox(
                    &mut state.show_unverified,
                    egui::RichText::new("Show unverified collections")
                        .color(theme::TEXT_MUTED)
                        .size(10.0),
                );

                if state.show_unverified {
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Unverified Collections")
                            .color(theme::TEXT_MUTED)
                            .size(10.0)
                            .strong(),
                    );
                    ui.add_space(4.0);
                    draw_collection_section(
                        ui,
                        &unverified_groups,
                        &search_lower,
                        is_searching,
                        config,
                        state,
                        &mut action,
                    );
                }
            }
        });

    action
}

// ============================================================================
// Collection section (shared by verified + unverified)
// ============================================================================

/// Draw a list of collection groups as collapsible accordions with card grids.
fn draw_collection_section(
    ui: &mut egui::Ui,
    groups: &[&PickerPolicyGroup],
    search_lower: &str,
    is_searching: bool,
    config: &WalletAssetPickerConfig,
    state: &WalletAssetPickerState,
    action: &mut Option<WalletAssetPickerAction>,
) {
    for group in groups {
        let group_label_matches = is_searching && group.label.to_lowercase().contains(search_lower);

        let filtered: Vec<&PickerAsset> = if is_searching && !group_label_matches {
            group
                .assets
                .iter()
                .filter(|a| {
                    a.display_name.to_lowercase().contains(search_lower)
                        || a.traits
                            .iter()
                            .any(|t| t.to_lowercase().contains(search_lower))
                })
                .collect()
        } else {
            group.assets.iter().collect()
        };

        if filtered.is_empty() {
            continue;
        }

        let offered_count = state.offered_count_for_policy(&group.policy_id);
        let header_text = if offered_count > 0 {
            format!(
                "{} ({}) \u{2022} {offered_count} in offer",
                group.label,
                filtered.len()
            )
        } else {
            format!("{} ({})", group.label, filtered.len())
        };

        let header_color = if offered_count > 0 {
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
                action,
            );
        });
    }
}

// ============================================================================
// Token list (FT section)
// ============================================================================

/// Format a large quantity for display (e.g. 1_500_000 → "1.5M").
pub fn format_quantity(qty: u64) -> String {
    if qty >= 1_000_000_000 {
        let v = qty as f64 / 1_000_000_000.0;
        format!("{v:.1}B")
    } else if qty >= 1_000_000 {
        let v = qty as f64 / 1_000_000.0;
        format!("{v:.1}M")
    } else if qty >= 10_000 {
        let v = qty as f64 / 1_000.0;
        format!("{v:.1}K")
    } else {
        qty.to_string()
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
    state: &WalletAssetPickerState,
    action: &mut Option<WalletAssetPickerAction>,
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
                draw_picker_card(ui, asset, policy_id, card_size, state, action);
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
    state: &WalletAssetPickerState,
    action: &mut Option<WalletAssetPickerAction>,
) {
    let size = Vec2::splat(card_size);
    let already_offered = state.is_already_offered(policy_id, &asset.asset_name_hex);
    let (card_rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

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

    // Dim overlay for already-offered assets
    if already_offered {
        painter.rect_filled(
            card_rect,
            rounding,
            Color32::from_rgba_premultiplied(10, 10, 20, 160),
        );
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
        if already_offered {
            theme::TEXT_MUTED
        } else {
            theme::TEXT_PRIMARY
        },
    );

    // "In offer" checkmark badge (top-left) for already-offered assets
    if already_offered {
        let badge_center = egui::pos2(card_rect.min.x + 10.0, card_rect.min.y + 10.0);
        painter.circle_filled(badge_center, 8.0, theme::TEXT_MUTED);
        PhosphorIcon::Check.paint(
            &painter,
            badge_center,
            egui::Align2::CENTER_CENTER,
            10.0,
            theme::BG_PRIMARY,
        );
    }

    // Border — already offered (muted), rarity, or default
    let (border_color, border_width) = if already_offered {
        (theme::TEXT_MUTED, 1.0)
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
        ui.set_min_width(180.0);

        ui.label(
            egui::RichText::new(&asset.display_name)
                .color(theme::TEXT_PRIMARY)
                .size(11.0)
                .strong(),
        );
        if let Some(rank) = asset.rarity_rank {
            let total = asset.total_ranked.unwrap_or(0);
            let rank_color = theme::rarity_rank_color(rank, total);
            let rank_text = if total > 0 {
                format!("Rank #{rank} / {total}")
            } else {
                format!("Rank #{rank}")
            };
            ui.label(egui::RichText::new(rank_text).color(rank_color).size(10.0));
        }
        if !asset.traits.is_empty() {
            ui.add_space(4.0);
            egui::Grid::new("trait_tooltip")
                .num_columns(2)
                .spacing([8.0, 2.0])
                .show(ui, |ui| {
                    for trait_str in &asset.traits {
                        if let Some((key, value)) = trait_str.split_once(':') {
                            ui.label(egui::RichText::new(key).color(theme::TEXT_MUTED).size(10.0));
                            ui.label(
                                egui::RichText::new(value)
                                    .color(theme::TEXT_SECONDARY)
                                    .size(10.0),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new(trait_str)
                                    .color(theme::TEXT_SECONDARY)
                                    .size(10.0),
                            );
                            ui.label("");
                        }
                        ui.end_row();
                    }
                });
        }
        if already_offered {
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new("Already in offer")
                    .color(theme::TEXT_MUTED)
                    .size(9.0),
            );
        }
    });

    // Click to toggle: add if not offered, remove if already offered.
    if response.clicked() {
        let asset_id =
            AssetId::new_unchecked(policy_id.to_string(), asset.asset_name_hex.to_string());
        if already_offered {
            *action = Some(WalletAssetPickerAction::Removed(asset_id));
        } else {
            *action = Some(WalletAssetPickerAction::Selected(SelectedAsset {
                asset_id,
                quantity: 1,
            }));
        }
    }
}
