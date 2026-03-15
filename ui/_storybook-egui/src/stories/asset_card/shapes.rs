use egui::{Color32, Pos2, Rect, Vec2};

use super::effects::CardEffect;
use super::geometry::{expand_outline, rounded_rect_vertices, unified_outline};
use super::mesh::{
    draw_colored_fan, draw_colored_ring, draw_effect_fan, draw_effect_quad, draw_quad,
    draw_spark_streak, draw_textured_fan, draw_textured_fan_rect_uv, draw_textured_quad,
};
use super::overlay::{draw_tile_overlay, rarity_color, rarity_glow, CardMask};
use super::projection::{project_3d, project_points, update_tilt, TiltState};

#[allow(clippy::too_many_arguments)]
pub(super) fn demo_square(
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

    // Unified border (card + badge silhouette)
    let outline = unified_outline(center, half, CardMask::Square);
    let border_outer = expand_outline(&outline, 3.0);
    let proj_outline = project_points(&outline, center, ax, ay, perspective);
    let proj_border = project_points(&border_outer, center, ax, ay, perspective);

    // Rarity glow
    if let Some(glow) = rarity_glow(rarity) {
        let glow_outer = expand_outline(&outline, 6.0);
        let proj_glow = project_points(&glow_outer, center, ax, ay, perspective);
        draw_colored_ring(&painter, &proj_border, &proj_glow, glow);
    }

    // Rarity border ring
    draw_colored_ring(&painter, &proj_outline, &proj_border, rarity_color(rarity));

    // Art fill
    if let Some(tex) = art_tex {
        draw_textured_quad(&painter, proj4, tex, Color32::WHITE);
    } else {
        draw_quad(&painter, proj4, Color32::from_rgb(30, 30, 48));
    }

    // Effect overlay (Rare+, only while hovering)
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

    // Spark traversal
    if spark_enabled && rarity >= 2 {
        draw_spark_streak(&painter, &proj_outline, spark_phase, rarity_color(rarity));
    }

    // Overlay: title, stats, badge
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
pub(super) fn demo_hex(
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

    // Unified border
    let outline = unified_outline(center, radius, CardMask::Hex { radius });
    let border_outer = expand_outline(&outline, 3.0);
    let proj_outline = project_points(&outline, center, ax, ay, perspective);
    let proj_border = project_points(&border_outer, center, ax, ay, perspective);

    // Glow
    if let Some(glow) = rarity_glow(rarity) {
        let glow_outer = expand_outline(&outline, 6.0);
        let proj_glow = project_points(&glow_outer, center, ax, ay, perspective);
        draw_colored_ring(&painter, &proj_border, &proj_glow, glow);
    }

    // Rarity border ring
    draw_colored_ring(&painter, &proj_outline, &proj_border, rarity_color(rarity));

    // Art fill
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

    // Effect overlay (Rare+, only while hovering)
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

    // Spark traversal
    if spark_enabled && rarity >= 2 {
        draw_spark_streak(&painter, &proj_outline, spark_phase, rarity_color(rarity));
    }

    // Overlay: title, stats, badge
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
pub(super) fn demo_rounded_square(
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

    // Unified border (card + badge silhouette)
    let outline = unified_outline(center, half, CardMask::RoundedSquare { corner_radius });
    let border_outer = expand_outline(&outline, 3.0);
    let proj_outline = project_points(&outline, center, ax, ay, perspective);
    let proj_border = project_points(&border_outer, center, ax, ay, perspective);

    // Glow
    if let Some(glow) = rarity_glow(rarity) {
        let glow_outer = expand_outline(&outline, 6.0);
        let proj_glow = project_points(&glow_outer, center, ax, ay, perspective);
        draw_colored_ring(&painter, &proj_border, &proj_glow, glow);
    }

    // Rarity border ring
    draw_colored_ring(&painter, &proj_outline, &proj_border, rarity_color(rarity));

    // Art fill (uses plain rounded rect, not the badge-merged outline)
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

    // Effect overlay (Rare+, only while hovering)
    if rarity >= 2 {
        if let Some(hover_pos) = response.hover_pos() {
            let mu = ((hover_pos.x - bbox.left()) / bbox.width()).clamp(0.0, 1.0);
            let mv = ((hover_pos.y - bbox.top()) / bbox.height()).clamp(0.0, 1.0);
            draw_effect_fan(&painter, art_center, &art_proj, bbox, mu, mv, effect);
        }
    }

    // Spark traversal
    if spark_enabled && rarity >= 2 {
        draw_spark_streak(&painter, &proj_outline, spark_phase, rarity_color(rarity));
    }

    // Overlay: title, stats, badge
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
