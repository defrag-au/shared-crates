//! Storybook demo for the AssetCard widget from egui-widgets.

use egui::{Color32, Pos2, Rect, Vec2};
use egui_widgets::asset_card::{
    base_outline, draw_colored_fan, draw_colored_ring, draw_effect_fan, draw_effect_quad,
    draw_quad, draw_spark_streak, draw_textured_fan, draw_textured_fan_rect_uv, draw_textured_quad,
    draw_tile_overlay, expand_outline, project_3d, project_points, rarity_color, rarity_glow,
    rounded_rect_vertices, update_tilt, with_badge, AuroraCurtain, BrushedMetal, CardEffect,
    CardMask, DiffractionGrating, Glitter, PrismaticDispersion, StreakHolo, ThinFilmIridescence,
    TiltState, EFFECT_NAMES, RARITIES,
};

use crate::{ACCENT, TEXT_MUTED};

/// IIIF test image: Pirate758 NFT (1:1 aspect)
const IIIF_ART_URL: &str = "https://iiif.hodlcroft.com/iiif/3/b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6:506972617465373538/full/400,/0/default.jpg";

// ============================================================================
// State
// ============================================================================

pub struct AssetCardState {
    pub size: f32,
    pub rarity: usize,
    pub tilt_ease: f32,
    pub perspective_distance: f32,
    pub effect_index: usize,
    pub hue_range: f32,
    pub shimmer_width: f32,
    pub shimmer_intensity: f32,
    pub overlay_opacity: f32,
    pub iri_min: f32,
    pub iri_range: f32,
    pub fresnel_power: f32,
    pub iri_intensity: f32,
    pub grating_spacing: f32,
    pub grating_angle: f32,
    pub max_orders: u32,
    pub diffraction_intensity: f32,
    pub glitter_scale: f32,
    pub sparkle_sharpness: f32,
    pub sparkle_threshold: f32,
    pub glitter_z_depth: f32,
    pub roughness_along: f32,
    pub roughness_perp: f32,
    pub brush_angle: f32,
    pub metal_preset: usize,
    pub aurora_freq1: f32,
    pub aurora_freq2: f32,
    pub curtain_sharpness: f32,
    pub vertical_falloff: f32,
    pub aurora_brightness: f32,
    pub prism_dispersion: f32,
    pub prism_spread: f32,
    pub facet_scale: f32,
    pub prism_intensity: f32,
    pub spark_phase: f32,
    pub spark_speed: f32,
    pub spark_enabled: bool,
    tilts: [TiltState; 3],
}

impl Default for AssetCardState {
    fn default() -> Self {
        Self {
            size: 200.0,
            rarity: 2,
            tilt_ease: 0.1,
            perspective_distance: 800.0,
            effect_index: 0,
            hue_range: 60.0,
            shimmer_width: 0.15,
            shimmer_intensity: 0.4,
            overlay_opacity: 0.2,
            iri_min: 250.0,
            iri_range: 400.0,
            fresnel_power: 5.0,
            iri_intensity: 0.3,
            grating_spacing: 1500.0,
            grating_angle: 0.0,
            max_orders: 4,
            diffraction_intensity: 1.5,
            glitter_scale: 40.0,
            sparkle_sharpness: 150.0,
            sparkle_threshold: 0.3,
            glitter_z_depth: 0.6,
            roughness_along: 0.05,
            roughness_perp: 0.8,
            brush_angle: 0.0,
            metal_preset: 0,
            aurora_freq1: 8.0,
            aurora_freq2: 5.0,
            curtain_sharpness: 4.0,
            vertical_falloff: 1.5,
            aurora_brightness: 1.0,
            prism_dispersion: 0.08,
            prism_spread: 0.1,
            facet_scale: 8.0,
            prism_intensity: 1.5,
            spark_phase: 0.0,
            spark_speed: 0.3,
            spark_enabled: true,
            tilts: [TiltState::default(); 3],
        }
    }
}

// ============================================================================
// Texture loading
// ============================================================================

fn try_load_texture(ctx: &egui::Context, url: &str) -> Option<egui::TextureId> {
    ctx.try_load_texture(
        url,
        egui::TextureOptions::LINEAR,
        egui::load::SizeHint::default(),
    )
    .ok()
    .and_then(|poll| match poll {
        egui::load::TexturePoll::Ready { texture } => Some(texture.id),
        _ => None,
    })
}

// ============================================================================
// Demo shape functions
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn demo_square(
    ui: &mut egui::Ui,
    size: f32,
    rarity: usize,
    art_tex: Option<egui::TextureId>,
    tilt: &mut TiltState,
    tilt_ease: f32,
    max_tilt: f32,
    perspective: f32,
    effect: &dyn CardEffect,
    spark_phase: f32,
    spark_enabled: bool,
) {
    let half = size / 2.0;
    let padding = 40.0;
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(size + padding, size + padding),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    let center = rect.center();

    let (ax, ay) = update_tilt(&response, center, half, tilt, tilt_ease, max_tilt);
    if tilt.current_x.abs() > 0.001 || tilt.current_y.abs() > 0.001 {
        ui.ctx().request_repaint();
    }

    let card_rect = Rect::from_center_size(center, Vec2::splat(size));
    let corners = [
        card_rect.left_top(),
        card_rect.right_top(),
        card_rect.right_bottom(),
        card_rect.left_bottom(),
    ];
    let projected: Vec<Pos2> = project_points(&corners, center, ax, ay, perspective);
    let proj4: [Pos2; 4] = [projected[0], projected[1], projected[2], projected[3]];

    // Composable outline: base shape, then badge union
    let base = base_outline(center, half, CardMask::Square);
    let outline = with_badge(&base, center, half);
    let border_outer = expand_outline(&outline, 3.0);
    let proj_outline = project_points(&outline, center, ax, ay, perspective);
    let proj_border = project_points(&border_outer, center, ax, ay, perspective);

    if let Some(glow) = rarity_glow(rarity) {
        let glow_outer = expand_outline(&outline, 6.0);
        let proj_glow = project_points(&glow_outer, center, ax, ay, perspective);
        draw_colored_ring(&painter, &proj_border, &proj_glow, glow);
    }

    draw_colored_ring(&painter, &proj_outline, &proj_border, rarity_color(rarity));

    if let Some(tex) = art_tex {
        draw_textured_quad(&painter, proj4, tex, Color32::WHITE);
    } else {
        draw_quad(&painter, proj4, Color32::from_rgb(30, 30, 48));
    }

    if rarity >= 2 {
        if let Some(hover_pos) = response.hover_pos() {
            let mu = ((hover_pos.x - card_rect.left()) / card_rect.width()).clamp(0.0, 1.0);
            let mv = ((hover_pos.y - card_rect.top()) / card_rect.height()).clamp(0.0, 1.0);
            draw_effect_quad(
                &painter,
                card_rect,
                center,
                ax,
                ay,
                perspective,
                mu,
                mv,
                effect,
            );
        }
    }

    if spark_enabled && rarity >= 2 {
        draw_spark_streak(&painter, &proj_outline, spark_phase, rarity_color(rarity));
    }

    draw_tile_overlay(
        ui,
        &painter,
        rarity,
        card_rect,
        CardMask::Square,
        center,
        ax,
        ay,
        perspective,
    );
}

#[allow(clippy::too_many_arguments)]
fn demo_hex(
    ui: &mut egui::Ui,
    size: f32,
    rarity: usize,
    art_tex: Option<egui::TextureId>,
    tilt: &mut TiltState,
    tilt_ease: f32,
    max_tilt: f32,
    perspective: f32,
    effect: &dyn CardEffect,
    spark_phase: f32,
    spark_enabled: bool,
) {
    let radius = size / 2.0;
    let padding = 50.0;
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(size + padding, size + padding),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    let center = rect.center();

    let (ax, ay) = update_tilt(&response, center, radius, tilt, tilt_ease, max_tilt);
    if tilt.current_x.abs() > 0.001 || tilt.current_y.abs() > 0.001 {
        ui.ctx().request_repaint();
    }

    // Hex: base shape only, no badge union
    let outline = base_outline(center, radius, CardMask::Hex { radius });
    let border_outer = expand_outline(&outline, 3.0);
    let proj_outline = project_points(&outline, center, ax, ay, perspective);
    let proj_border = project_points(&border_outer, center, ax, ay, perspective);

    if let Some(glow) = rarity_glow(rarity) {
        let glow_outer = expand_outline(&outline, 6.0);
        let proj_glow = project_points(&glow_outer, center, ax, ay, perspective);
        draw_colored_ring(&painter, &proj_border, &proj_glow, glow);
    }

    draw_colored_ring(&painter, &proj_outline, &proj_border, rarity_color(rarity));

    let art_center = project_3d(center, center, ax, ay, perspective);
    if let Some(tex) = art_tex {
        draw_textured_fan(
            &painter,
            art_center,
            &proj_outline,
            tex,
            Color32::WHITE,
            &outline,
            center,
        );
    } else {
        draw_colored_fan(
            &painter,
            art_center,
            &proj_outline,
            Color32::from_rgb(30, 30, 48),
        );
    }

    let hex_bbox = Rect::from_center_size(center, Vec2::splat(radius * 2.0));
    if rarity >= 2 {
        if let Some(hover_pos) = response.hover_pos() {
            let mu = ((hover_pos.x - hex_bbox.left()) / hex_bbox.width()).clamp(0.0, 1.0);
            let mv = ((hover_pos.y - hex_bbox.top()) / hex_bbox.height()).clamp(0.0, 1.0);
            draw_effect_fan(
                &painter,
                art_center,
                &proj_outline,
                hex_bbox,
                mu,
                mv,
                effect,
            );
        }
    }

    if spark_enabled && rarity >= 2 {
        draw_spark_streak(&painter, &proj_outline, spark_phase, rarity_color(rarity));
    }

    draw_tile_overlay(
        ui,
        &painter,
        rarity,
        hex_bbox,
        CardMask::Hex { radius },
        center,
        ax,
        ay,
        perspective,
    );
}

#[allow(clippy::too_many_arguments)]
fn demo_rounded_square(
    ui: &mut egui::Ui,
    size: f32,
    rarity: usize,
    art_tex: Option<egui::TextureId>,
    tilt: &mut TiltState,
    tilt_ease: f32,
    max_tilt: f32,
    perspective: f32,
    effect: &dyn CardEffect,
    spark_phase: f32,
    spark_enabled: bool,
) {
    let half = size / 2.0;
    let corner_radius = size * 0.15;
    let padding = 40.0;
    let segs = 8;
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(size + padding, size + padding),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    let center = rect.center();

    let (ax, ay) = update_tilt(&response, center, half, tilt, tilt_ease, max_tilt);
    if tilt.current_x.abs() > 0.001 || tilt.current_y.abs() > 0.001 {
        ui.ctx().request_repaint();
    }

    let bbox = Rect::from_center_size(center, Vec2::splat(size));

    // Composable outline: base shape, then badge union
    let base = base_outline(center, half, CardMask::RoundedSquare { corner_radius });
    let outline = with_badge(&base, center, half);
    let border_outer = expand_outline(&outline, 3.0);
    let proj_outline = project_points(&outline, center, ax, ay, perspective);
    let proj_border = project_points(&border_outer, center, ax, ay, perspective);

    if let Some(glow) = rarity_glow(rarity) {
        let glow_outer = expand_outline(&outline, 6.0);
        let proj_glow = project_points(&glow_outer, center, ax, ay, perspective);
        draw_colored_ring(&painter, &proj_border, &proj_glow, glow);
    }

    draw_colored_ring(&painter, &proj_outline, &proj_border, rarity_color(rarity));

    let art_verts = rounded_rect_vertices(center, half, half, corner_radius, segs);
    let art_proj = project_points(&art_verts, center, ax, ay, perspective);
    let art_center = project_3d(center, center, ax, ay, perspective);
    if let Some(tex) = art_tex {
        draw_textured_fan_rect_uv(
            &painter,
            art_center,
            &art_proj,
            bbox,
            tex,
            Color32::WHITE,
            &art_verts,
            center,
        );
    } else {
        draw_colored_fan(
            &painter,
            art_center,
            &art_proj,
            Color32::from_rgb(30, 30, 48),
        );
    }

    if rarity >= 2 {
        if let Some(hover_pos) = response.hover_pos() {
            let mu = ((hover_pos.x - bbox.left()) / bbox.width()).clamp(0.0, 1.0);
            let mv = ((hover_pos.y - bbox.top()) / bbox.height()).clamp(0.0, 1.0);
            draw_effect_fan(&painter, art_center, &art_proj, bbox, mu, mv, effect);
        }
    }

    if spark_enabled && rarity >= 2 {
        draw_spark_streak(&painter, &proj_outline, spark_phase, rarity_color(rarity));
    }

    draw_tile_overlay(
        ui,
        &painter,
        rarity,
        bbox,
        CardMask::RoundedSquare { corner_radius },
        center,
        ax,
        ay,
        perspective,
    );
}

// ============================================================================
// Main show function
// ============================================================================

pub fn show(ui: &mut egui::Ui, state: &mut AssetCardState) {
    if state.spark_enabled && state.rarity >= 2 {
        let dt = ui.input(|i| i.stable_dt).min(0.1);
        state.spark_phase = (state.spark_phase + dt * state.spark_speed) % 1.0;
        ui.ctx().request_repaint();
    }

    let art_texture = try_load_texture(ui.ctx(), IIIF_ART_URL);

    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(&mut state.size, 100.0..=300.0).text("Size"));
        ui.add(
            egui::Slider::new(&mut state.perspective_distance, 200.0..=2000.0).text("Perspective"),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Rarity:");
        for (i, (name, color)) in RARITIES.iter().enumerate() {
            let text = if state.rarity == i {
                egui::RichText::new(*name).color(*color).strong()
            } else {
                egui::RichText::new(*name).color(TEXT_MUTED)
            };
            if ui.selectable_label(state.rarity == i, text).clicked() {
                state.rarity = i;
            }
        }
    });

    if state.rarity >= 2 {
        ui.horizontal(|ui| {
            ui.label("Effect:");
            egui::ComboBox::from_id_salt("effect_selector")
                .selected_text(EFFECT_NAMES[state.effect_index])
                .show_ui(ui, |ui| {
                    for (i, name) in EFFECT_NAMES.iter().enumerate() {
                        ui.selectable_value(&mut state.effect_index, i, *name);
                    }
                });
        });

        match state.effect_index {
            0 => {
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut state.hue_range, 0.0..=180.0).text("Hue Range"));
                    ui.add(
                        egui::Slider::new(&mut state.shimmer_width, 0.05..=0.5).text("Shimmer W"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Slider::new(&mut state.shimmer_intensity, 0.0..=1.0)
                            .text("Intensity"),
                    );
                    ui.add(
                        egui::Slider::new(&mut state.overlay_opacity, 0.0..=0.5).text("Opacity"),
                    );
                });
            }
            1 => {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Slider::new(&mut state.iri_min, 100.0..=500.0).text("Film Min (nm)"),
                    );
                    ui.add(
                        egui::Slider::new(&mut state.iri_range, 100.0..=800.0)
                            .text("Film Range (nm)"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Slider::new(&mut state.fresnel_power, 1.0..=10.0)
                            .text("Fresnel Power"),
                    );
                    ui.add(
                        egui::Slider::new(&mut state.iri_intensity, 0.05..=1.0).text("Intensity"),
                    );
                });
            }
            2 => {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Slider::new(&mut state.grating_spacing, 800.0..=3000.0)
                            .text("Spacing"),
                    );
                    ui.add(
                        egui::Slider::new(&mut state.grating_angle, 0.0..=std::f32::consts::PI)
                            .text("Angle"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut state.max_orders, 1..=8).text("Orders"));
                    ui.add(
                        egui::Slider::new(&mut state.diffraction_intensity, 0.5..=3.0)
                            .text("Intensity"),
                    );
                });
            }
            3 => {
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut state.glitter_scale, 10.0..=80.0).text("Scale"));
                    ui.add(
                        egui::Slider::new(&mut state.sparkle_sharpness, 50.0..=500.0)
                            .text("Sharpness"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Slider::new(&mut state.sparkle_threshold, 0.0..=1.0)
                            .text("Threshold"),
                    );
                    ui.add(
                        egui::Slider::new(&mut state.glitter_z_depth, 0.1..=1.0).text("Z Depth"),
                    );
                });
            }
            4 => {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Slider::new(&mut state.roughness_along, 0.01..=0.5)
                            .text("Roughness Along"),
                    );
                    ui.add(
                        egui::Slider::new(&mut state.roughness_perp, 0.1..=2.0)
                            .text("Roughness Perp"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Slider::new(&mut state.brush_angle, 0.0..=std::f32::consts::PI)
                            .text("Brush Angle"),
                    );
                    ui.label("Metal:");
                    for (i, name) in ["Silver", "Gold", "Copper"].iter().enumerate() {
                        if ui
                            .selectable_label(state.metal_preset == i, *name)
                            .clicked()
                        {
                            state.metal_preset = i;
                        }
                    }
                });
            }
            5 => {
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut state.aurora_freq1, 3.0..=15.0).text("Freq 1"));
                    ui.add(egui::Slider::new(&mut state.aurora_freq2, 3.0..=15.0).text("Freq 2"));
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Slider::new(&mut state.curtain_sharpness, 2.0..=8.0)
                            .text("Sharpness"),
                    );
                    ui.add(
                        egui::Slider::new(&mut state.vertical_falloff, 0.5..=3.0).text("V Falloff"),
                    );
                    ui.add(
                        egui::Slider::new(&mut state.aurora_brightness, 0.3..=2.0)
                            .text("Brightness"),
                    );
                });
            }
            6 => {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Slider::new(&mut state.prism_dispersion, 0.01..=0.15)
                            .text("Dispersion"),
                    );
                    ui.add(egui::Slider::new(&mut state.prism_spread, 0.02..=0.2).text("Spread"));
                });
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut state.facet_scale, 4.0..=20.0).text("Facets"));
                    ui.add(
                        egui::Slider::new(&mut state.prism_intensity, 0.5..=3.0).text("Intensity"),
                    );
                });
            }
            _ => {}
        }

        ui.horizontal(|ui| {
            ui.checkbox(&mut state.spark_enabled, "Spark");
            if state.spark_enabled {
                ui.add(egui::Slider::new(&mut state.spark_speed, 0.1..=1.0).text("Speed"));
            }
        });
    }

    let metal_colors: [(f32, f32, f32); 3] =
        [(0.75, 0.78, 0.82), (1.00, 0.84, 0.00), (0.72, 0.45, 0.20)];
    let (mr, mg, mb) = metal_colors[state.metal_preset.min(2)];

    let effect: Box<dyn CardEffect> = match state.effect_index {
        1 => Box::new(ThinFilmIridescence {
            iri_min: state.iri_min,
            iri_range: state.iri_range,
            fresnel_power: state.fresnel_power,
            intensity: state.iri_intensity,
        }),
        2 => Box::new(DiffractionGrating {
            grating_spacing: state.grating_spacing,
            grating_angle: state.grating_angle,
            max_orders: state.max_orders,
            intensity: state.diffraction_intensity,
        }),
        3 => Box::new(Glitter {
            grid_scale: state.glitter_scale,
            sparkle_sharpness: state.sparkle_sharpness,
            sparkle_threshold: state.sparkle_threshold,
            z_depth: state.glitter_z_depth,
        }),
        4 => Box::new(BrushedMetal {
            roughness_along: state.roughness_along,
            roughness_perp: state.roughness_perp,
            brush_angle: state.brush_angle,
            metal_r: mr,
            metal_g: mg,
            metal_b: mb,
        }),
        5 => Box::new(AuroraCurtain {
            freq1: state.aurora_freq1,
            freq2: state.aurora_freq2,
            curtain_sharpness: state.curtain_sharpness,
            vertical_falloff: state.vertical_falloff,
            brightness: state.aurora_brightness,
        }),
        6 => Box::new(PrismaticDispersion {
            dispersion: state.prism_dispersion,
            spread: state.prism_spread,
            facet_scale: state.facet_scale,
            intensity: state.prism_intensity,
        }),
        _ => Box::new(StreakHolo {
            hue_range: state.hue_range,
            shimmer_width: state.shimmer_width,
            shimmer_intensity: state.shimmer_intensity,
            overlay_opacity: state.overlay_opacity,
        }),
    };

    let art_status = if art_texture.is_some() {
        "loaded"
    } else {
        "loading..."
    };
    ui.label(
        egui::RichText::new(format!("Art: {art_status}"))
            .color(TEXT_MUTED)
            .small(),
    );
    ui.add_space(8.0);

    ui.label(egui::RichText::new("1. Square Tile").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Natural 1:1 fit. Art-first with right-edge stats and holographic foil.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);
    demo_square(
        ui,
        state.size,
        state.rarity,
        art_texture,
        &mut state.tilts[0],
        state.tilt_ease,
        15.0,
        state.perspective_distance,
        &*effect,
        state.spark_phase,
        state.spark_enabled,
    );
    ui.add_space(16.0);

    ui.label(egui::RichText::new("2. Hex Tile").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Pointy-top hexagon. Tessellates for game boards. Holographic foil on Rare+.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);
    demo_hex(
        ui,
        state.size,
        state.rarity,
        art_texture,
        &mut state.tilts[1],
        state.tilt_ease,
        15.0,
        state.perspective_distance,
        &*effect,
        state.spark_phase,
        state.spark_enabled,
    );
    ui.add_space(16.0);

    ui.label(
        egui::RichText::new("3. Rounded Square")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Modern app-icon aesthetic. Generous corner radius with holographic foil.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);
    demo_rounded_square(
        ui,
        state.size,
        state.rarity,
        art_texture,
        &mut state.tilts[2],
        state.tilt_ease,
        15.0,
        state.perspective_distance,
        &*effect,
        state.spark_phase,
        state.spark_enabled,
    );

    ui.add_space(24.0);
    ui.separator();
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new("Key observations:")
            .color(ACCENT)
            .strong(),
    );
    ui.label("- Square: best art utilisation, familiar grid layout");
    ui.label("- Hex: strongest game-board identity, tessellation-ready");
    ui.label("- Rounded Square: modern feel, good compromise");
    ui.label("- Holographic: specular streak + iridescence + fresnel glow (Rare+)");
}
