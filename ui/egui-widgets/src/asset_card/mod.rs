//! Asset card rendering toolkit — 3D-projected card shapes with holographic effects.
//!
//! Provides geometry, mesh, projection, overlay, and effect primitives for
//! rendering NFT/asset cards in egui with perspective tilt, rarity borders,
//! spark animations, and swappable visual effects.

pub mod effects;
pub mod geometry;
pub mod mesh;
pub mod overlay;
pub mod projection;

// Effects
pub use effects::{
    AuroraCurtain, BrushedMetal, CardEffect, DiffractionGrating, EffectVertex, Glitter,
    PrismaticDispersion, StreakHolo, ThinFilmIridescence, EFFECT_NAMES,
};

// Projection
pub use projection::{project_3d, project_points, update_tilt, TiltState};

// Geometry
pub use geometry::{
    base_outline, cumulative_lengths, expand_outline, regular_polygon_vertices,
    rounded_rect_vertices, sample_path, unified_outline, with_badge, BADGE_H, BADGE_W_FRAC,
};

// Mesh drawing
pub use mesh::{
    draw_colored_fan, draw_colored_ring, draw_effect_fan, draw_effect_quad, draw_quad,
    draw_spark_streak, draw_textured_fan, draw_textured_fan_rect_uv, draw_textured_quad,
};

// Overlay
pub use overlay::{draw_tile_overlay, rarity_color, rarity_glow, CardMask, RARITIES};
