//! Storybook demo for the CardBrowser widget from egui-widgets.

use egui_widgets::card_browser::{self, CardBrowserConfig, CardBrowserState};

use crate::{ACCENT, TEXT_MUTED};

const IIIF_BASE: &str = "https://iiif.hodlcroft.com/iiif/3";
const POLICY_ID: &str = "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6";

// ============================================================================
// State
// ============================================================================

pub struct CardBrowserStoryState {
    pub browser: CardBrowserState,
    pub preset: usize,
    pub card_width: f32,
    pub text_lines: u8,
    pub detail_width: f32,
    pub spacing: f32,
    pub items: Vec<DemoItem>,
}

impl Default for CardBrowserStoryState {
    fn default() -> Self {
        Self {
            browser: CardBrowserState::default(),
            preset: 0,
            card_width: 140.0,
            text_lines: 3,
            detail_width: 360.0,
            spacing: 8.0,
            items: build_preset_items(0),
        }
    }
}

// ============================================================================
// Demo data
// ============================================================================

pub struct DemoItem {
    pub name: String,
    pub subtitle: String,
    pub badge: Option<String>,
    pub badge_color: egui::Color32,
    pub price: Option<f64>,
    pub detail_lines: Vec<(String, String)>,
    pub accent: egui::Color32,
    /// IIIF image URL for the thumbnail.
    pub image_url: Option<String>,
}

/// Build a IIIF thumbnail URL from a hex asset name.
fn iiif_url(asset_hex: &str, size: u32) -> String {
    format!("{IIIF_BASE}/{POLICY_ID}:{asset_hex}/full/{size},/0/default.jpg")
}

// Real Hodlcroft Pirates assets (50 hex asset names from asset_ids.csv)
const PIRATE_HEX: &[&str] = &[
    "5069726174653834",
    "506972617465323733",
    "50697261746531303430",
    "506972617465333830",
    "506972617465313432",
    "506972617465393336",
    "50697261746531323736",
    "506972617465373835",
    "50697261746531393133",
    "50697261746531363133",
    "506972617465333138",
    "506972617465353532",
    "50697261746531323237",
    "5069726174653734",
    "5069726174653533",
    "506972617465363031",
    "5069726174653230",
    "506972617465393239",
    "50697261746531303732",
    "506972617465353832",
    "506972617465353234",
    "50697261746531373239",
    "50697261746531393431",
    "506972617465313038",
    "50697261746531373531",
    "50697261746531373530",
    "506972617465313533",
    "506972617465363134",
    "5069726174653130",
    "50697261746531313537",
    "506972617465313937",
    "50697261746531383436",
    "50697261746531363532",
    "506972617465313935",
    "50697261746531343539",
    "50697261746531313737",
    "5069726174653733",
    "50697261746531363238",
    "506972617465343137",
    "506972617465363839",
    "506972617465343132",
    "50697261746531333432",
    "506972617465323130",
    "506972617465363634",
    "506972617465313838",
    "5069726174653237",
    "506972617465343433",
    "50697261746531383432",
    "506972617465353631",
    "506972617465333435",
];

/// Decode a hex-encoded asset name to a UTF-8 string.
fn decode_hex_name(hex: &str) -> String {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect();
    String::from_utf8(bytes).unwrap_or_else(|_| hex.to_string())
}

const PRESET_NAMES: [&str; 3] = ["NFT Portfolio", "Marketplace", "Minimal"];

fn build_preset_items(preset: usize) -> Vec<DemoItem> {
    match preset {
        0 => PIRATE_HEX
            .iter()
            .enumerate()
            .map(|(i, hex)| {
                let name = decode_hex_name(hex);
                let rank = (i * 137 + 42) % 2000;
                let price = 30.0 + (i as f64 * 47.0) % 400.0;
                let badge = match i % 4 {
                    0 => Some("100% (3x)".into()),
                    1 => Some("75% (2/3)".into()),
                    _ => None,
                };
                let badge_color = match i % 4 {
                    0 => egui::Color32::from_rgb(158, 206, 106),
                    1 => egui::Color32::from_rgb(224, 175, 104),
                    _ => egui::Color32::TRANSPARENT,
                };
                DemoItem {
                    name,
                    subtitle: format!("Rank {rank} / 2000"),
                    badge,
                    badge_color,
                    price: Some(price),
                    detail_lines: vec![
                        ("Rarity".into(), format!("Top {:.1}%", rank as f64 / 20.0)),
                        ("Collection".into(), "Hodlcroft Pirates".into()),
                        ("Policy".into(), format!("{}...", &POLICY_ID[..16])),
                    ],
                    accent: if i % 3 == 0 {
                        egui::Color32::from_rgb(125, 207, 255)
                    } else {
                        egui::Color32::from_rgb(224, 175, 104)
                    },
                    image_url: Some(iiif_url(hex, 400)),
                }
            })
            .collect(),
        1 => PIRATE_HEX
            .iter()
            .enumerate()
            .map(|(i, hex)| {
                let name = decode_hex_name(hex);
                let has_gaps = i % 2 == 0;
                DemoItem {
                    name: format!("Listed: {name}"),
                    subtitle: if i % 2 == 0 {
                        "Wayup".into()
                    } else {
                        "JPG Store".into()
                    },
                    badge: if has_gaps {
                        Some(format!("Fills {} gaps", (i % 3) + 1))
                    } else {
                        None
                    },
                    badge_color: if has_gaps {
                        egui::Color32::from_rgb(158, 206, 106)
                    } else {
                        egui::Color32::TRANSPARENT
                    },
                    price: Some(30.0 + (i as f64 * 12.0)),
                    detail_lines: vec![
                        (
                            "Marketplace".into(),
                            if i % 2 == 0 { "Wayup" } else { "JPG Store" }.into(),
                        ),
                        ("Collection".into(), "Hodlcroft Pirates".into()),
                    ],
                    accent: if has_gaps {
                        egui::Color32::from_rgb(158, 206, 106)
                    } else {
                        egui::Color32::from_rgb(96, 104, 128)
                    },
                    image_url: Some(iiif_url(hex, 400)),
                }
            })
            .collect(),
        _ => vec![
            DemoItem {
                name: "Item A".into(),
                subtitle: "Simple card".into(),
                badge: None,
                badge_color: egui::Color32::TRANSPARENT,
                price: None,
                detail_lines: vec![
                    ("Type".into(), "Basic".into()),
                    ("Status".into(), "Active".into()),
                ],
                accent: egui::Color32::from_rgb(125, 207, 255),
                image_url: None,
            },
            DemoItem {
                name: "Item B".into(),
                subtitle: "With badge".into(),
                badge: Some("New".into()),
                badge_color: egui::Color32::from_rgb(125, 207, 255),
                price: None,
                detail_lines: vec![
                    ("Type".into(), "Featured".into()),
                    ("Status".into(), "Active".into()),
                ],
                accent: egui::Color32::from_rgb(125, 207, 255),
                image_url: None,
            },
            DemoItem {
                name: "Item C".into(),
                subtitle: "Another card".into(),
                badge: None,
                badge_color: egui::Color32::TRANSPARENT,
                price: Some(10.0),
                detail_lines: vec![("Type".into(), "Standard".into())],
                accent: egui::Color32::from_rgb(96, 104, 128),
                image_url: None,
            },
        ],
    }
}

// ============================================================================
// Show
// ============================================================================

pub fn show(ui: &mut egui::Ui, state: &mut CardBrowserStoryState) {
    // Controls
    let mut text_lines_f32 = state.text_lines as f32;
    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(&mut state.card_width, 100.0..=200.0).text("Card width"));
        ui.add(
            egui::Slider::new(&mut text_lines_f32, 1.0..=5.0)
                .step_by(1.0)
                .text("Text lines"),
        );
    });
    state.text_lines = text_lines_f32 as u8;
    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(&mut state.detail_width, 200.0..=600.0).text("Detail width"));
        ui.add(egui::Slider::new(&mut state.spacing, 2.0..=16.0).text("Spacing"));
    });

    ui.horizontal(|ui| {
        ui.label("Preset:");
        for (i, name) in PRESET_NAMES.iter().enumerate() {
            let text = if state.preset == i {
                egui::RichText::new(*name).color(ACCENT).strong()
            } else {
                egui::RichText::new(*name).color(TEXT_MUTED)
            };
            if ui.selectable_label(state.preset == i, text).clicked() {
                state.preset = i;
                state.items = build_preset_items(i);
                state.browser.selected = None;
            }
        }
    });

    // Summary
    ui.add_space(4.0);
    let selected_label = state
        .browser
        .selected
        .and_then(|idx| state.items.get(idx))
        .map(|item| item.name.as_str())
        .unwrap_or("none");
    ui.label(
        egui::RichText::new(format!(
            "{} items, selected: {selected_label}",
            state.items.len()
        ))
        .color(egui::Color32::from_rgb(160, 160, 180))
        .size(11.0),
    );
    ui.add_space(8.0);

    // Build config from sliders
    let config = CardBrowserConfig {
        card_width: state.card_width,
        text_lines: state.text_lines,
        detail_width: state.detail_width,
        spacing: state.spacing,
        ..Default::default()
    };

    // Show computed card height
    ui.label(
        egui::RichText::new(format!("Card height: {:.0}px (auto)", config.card_height()))
            .color(TEXT_MUTED)
            .size(10.0),
    );

    // Show the browser
    card_browser::show(
        ui,
        &mut state.browser,
        &mut state.items,
        &config,
        // Card renderer
        |ui, ctx, item| {
            // Real IIIF thumbnail or placeholder
            let loading = card_browser::draw_thumbnail(
                ui,
                ctx.thumb_rect,
                item.image_url.as_deref(),
                &config,
            );
            if loading {
                ui.ctx().request_repaint();
            }

            // Badge banner (bottom of thumbnail)
            if let Some(badge) = &item.badge {
                let banner_h = 18.0;
                let banner_rect = egui::Rect::from_min_size(
                    egui::pos2(ctx.thumb_rect.min.x, ctx.thumb_rect.max.y - banner_h),
                    egui::Vec2::new(ctx.thumb_rect.width(), banner_h),
                );
                ui.painter().rect_filled(
                    banner_rect,
                    egui::CornerRadius {
                        nw: 0,
                        ne: 0,
                        sw: 4,
                        se: 4,
                    },
                    item.badge_color.gamma_multiply(0.85),
                );
                ui.painter().text(
                    banner_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    badge,
                    egui::FontId::monospace(9.0),
                    egui::Color32::from_rgb(26, 27, 38),
                );
            }

            // Name
            let name_rect =
                egui::Rect::from_min_size(ctx.text_origin, egui::Vec2::new(ctx.text_width, 14.0));
            ui.painter().with_clip_rect(name_rect).text(
                name_rect.left_center(),
                egui::Align2::LEFT_CENTER,
                &item.name,
                egui::FontId::proportional(11.0),
                egui::Color32::from_rgb(220, 220, 235),
            );

            // Subtitle
            ui.painter().text(
                egui::pos2(ctx.text_origin.x, ctx.text_origin.y + 14.0),
                egui::Align2::LEFT_TOP,
                &item.subtitle,
                egui::FontId::proportional(9.0),
                TEXT_MUTED,
            );

            // Price (if any)
            if let Some(price) = item.price {
                ui.painter().text(
                    egui::pos2(ctx.text_origin.x, ctx.text_origin.y + 26.0),
                    egui::Align2::LEFT_TOP,
                    format!("{price:.0} ADA"),
                    egui::FontId::monospace(10.0),
                    egui::Color32::from_rgb(158, 206, 106),
                );
            }
        },
        // Detail renderer
        |ui, _idx, item| {
            ui.label(
                egui::RichText::new(&item.name)
                    .color(item.accent)
                    .size(15.0)
                    .strong(),
            );
            ui.add_space(4.0);

            if let Some(price) = item.price {
                ui.label(
                    egui::RichText::new(format!("{price:.0} ADA"))
                        .color(egui::Color32::from_rgb(158, 206, 106))
                        .size(13.0),
                );
                ui.add_space(4.0);
            }

            // Larger image in detail panel
            if let Some(url) = &item.image_url {
                ui.add(
                    egui::Image::new(url.as_str())
                        .max_width(ui.available_width())
                        .corner_radius(6),
                );
                ui.add_space(8.0);
            }

            ui.separator();
            ui.add_space(4.0);

            // Detail lines
            for (key, value) in &item.detail_lines {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("{key}:"))
                            .color(TEXT_MUTED)
                            .size(11.0),
                    );
                    ui.label(
                        egui::RichText::new(value)
                            .color(egui::Color32::from_rgb(220, 220, 235))
                            .size(11.0),
                    );
                });
            }

            if let Some(badge) = &item.badge {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(badge)
                        .color(item.badge_color)
                        .size(11.0)
                        .strong(),
                );
            }

            ui.add_space(12.0);
            ui.label(
                egui::RichText::new("This detail panel is fully caller-controlled.")
                    .color(TEXT_MUTED)
                    .size(9.0),
            );
        },
    );

    ui.add_space(12.0);
    ui.label(egui::RichText::new("Features:").color(ACCENT).strong());
    let features = [
        "Real IIIF thumbnails from Hodlcroft Pirates collection",
        "Auto card height: computed from card_width + text_lines",
        "Closure-based: caller provides render_card + render_detail callbacks",
        "Selection: click to open detail panel, click again to close",
        "Layout: responsive grid + side-by-side detail panel",
        "Card chrome: hover/selected backgrounds and borders",
        "Supports interactive cards (AssetCard tilt) via ctx.response.hover_pos()",
        "draw_thumbnail() helper for async image loading with spinner",
    ];
    for f in features {
        ui.label(
            egui::RichText::new(format!("  {f}"))
                .color(egui::Color32::from_rgb(220, 220, 235))
                .small(),
        );
    }
}
