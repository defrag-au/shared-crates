//! High-level reusable asset-card widget.
//!
//! Encapsulates the full surface-fx render sequence (perspective tilt → shape
//! outline → rarity glow → rarity border → art quad → holographic effect overlay
//! → spark streak) behind one builder, so any frontend can drop a 3D holo card
//! into a grid tile with a couple of lines instead of hand-assembling ~30 calls
//! to the underlying primitives.
//!
//! The widget owns texture loading and per-frame animation (tilt easing + spark
//! advance, including repaint requests). The caller owns only a small persistent
//! [`AssetCardState`] (tilt + spark phase) per card.
//!
//! ```ignore
//! AssetCard::new(CardImage::Url(&iiif_url))
//!     .rarity(item.rarity)
//!     .effect(Some(CardEffectKind::from_index(item.effect_index)))
//!     .paint(ui, tile_rect, &tile_response, &mut item.card_state);
//! ```

use egui::{Color32, Pos2, Rect, Sense, Vec2};

use super::effects::{
    AuroraCurtain, BrushedMetal, CardEffect, DiffractionGrating, EffectVertex, Glitter,
    PrismaticDispersion, StreakHolo, ThinFilmIridescence,
};
use super::geometry::{base_outline, expand_outline};
use super::mesh::{
    draw_colored_ring, draw_effect_quad, draw_quad, draw_spark_streak, draw_textured_quad,
};
use super::overlay::{rarity_color, rarity_glow, CardMask};
use super::projection::{project_points, update_tilt, TiltState};

/// Runtime-selectable holographic effect kind.
///
/// A `Copy` enum over the seven [`CardEffect`] implementations, so callers can
/// store/select an effect by value (e.g. from a dropdown or per-asset metadata)
/// without juggling `Box<dyn CardEffect>`. [`Self::build`] instantiates the
/// concrete effect with its tuned default parameters.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum CardEffectKind {
    /// Rainbow streak that follows the cursor (the default treatment).
    #[default]
    StreakHolo,
    /// Oil-film iridescence.
    ThinFilm,
    /// Spectral diffraction grating.
    Diffraction,
    /// Sparkling facet glitter.
    Glitter,
    /// Anisotropic brushed-metal sheen.
    BrushedMetal,
    /// Animated aurora curtains.
    Aurora,
    /// Prismatic chromatic dispersion.
    Prismatic,
}

impl CardEffectKind {
    /// All kinds, in the same order as [`super::EFFECT_NAMES`].
    pub const ALL: [CardEffectKind; 7] = [
        CardEffectKind::StreakHolo,
        CardEffectKind::ThinFilm,
        CardEffectKind::Diffraction,
        CardEffectKind::Glitter,
        CardEffectKind::BrushedMetal,
        CardEffectKind::Aurora,
        CardEffectKind::Prismatic,
    ];

    /// Map an index (e.g. an asset's effect slot) onto a kind, wrapping so any
    /// `usize` is valid. Index order matches [`super::EFFECT_NAMES`].
    pub fn from_index(index: usize) -> Self {
        Self::ALL[index % Self::ALL.len()]
    }

    /// This kind's index into [`Self::ALL`] / [`super::EFFECT_NAMES`].
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&k| k == self).unwrap_or(0)
    }

    /// Human-readable name (matches [`super::EFFECT_NAMES`]).
    pub fn name(self) -> &'static str {
        super::EFFECT_NAMES[self.index()]
    }

    /// Instantiate the concrete effect with its default parameters.
    ///
    /// These are tuned for *restraint* — a tasteful foil that reads as a sheen
    /// rather than a light show (informed by simeydotme's Pokémon-cards-css and
    /// Alan Zucconi's iridescence work). The "amount" knobs (shimmer/overlay/
    /// intensity/brightness) sit roughly half of a naive maximum; dial the whole
    /// treatment globally with [`AssetCard::strength`] rather than re-tuning here.
    pub fn build(self) -> Box<dyn CardEffect> {
        match self {
            CardEffectKind::StreakHolo => Box::new(StreakHolo {
                hue_range: 60.0,
                shimmer_width: 0.15,
                shimmer_intensity: 0.2,
                overlay_opacity: 0.1,
            }),
            CardEffectKind::ThinFilm => Box::new(ThinFilmIridescence {
                iri_min: 250.0,
                iri_range: 400.0,
                fresnel_power: 5.0,
                intensity: 0.3,
            }),
            CardEffectKind::Diffraction => Box::new(DiffractionGrating {
                grating_spacing: 1500.0,
                grating_angle: 0.0,
                max_orders: 4,
                intensity: 0.6,
            }),
            CardEffectKind::Glitter => Box::new(Glitter {
                grid_scale: 40.0,
                sparkle_sharpness: 150.0,
                sparkle_threshold: 0.5,
                z_depth: 0.6,
            }),
            CardEffectKind::BrushedMetal => Box::new(BrushedMetal {
                roughness_along: 0.05,
                roughness_perp: 0.8,
                brush_angle: 0.0,
                metal_r: 0.75,
                metal_g: 0.78,
                metal_b: 0.82,
            }),
            CardEffectKind::Aurora => Box::new(AuroraCurtain {
                freq1: 8.0,
                freq2: 5.0,
                curtain_sharpness: 4.0,
                vertical_falloff: 1.5,
                brightness: 0.5,
            }),
            CardEffectKind::Prismatic => Box::new(PrismaticDispersion {
                dispersion: 0.08,
                spread: 0.1,
                facet_scale: 8.0,
                intensity: 0.6,
            }),
        }
    }
}

/// Persistent per-card animation state. Store one per card across frames; the
/// widget mutates it (tilt easing toward the cursor, spark phase advance).
#[derive(Clone, Copy, Default, Debug)]
pub struct AssetCardState {
    /// Eased perspective-tilt accumulator.
    pub tilt: TiltState,
    /// Spark-streak animation phase in `[0, 1)`. Seed with a per-card offset
    /// (e.g. `(index as f32 * 0.137) % 1.0`) to desynchronise a grid.
    pub spark_phase: f32,
}

/// The art source for a card.
pub enum CardImage<'a> {
    /// Async-load this URL via the egui texture loader. While not-yet-ready the
    /// card shows the placeholder fill and the widget requests a repaint.
    Url(&'a str),
    /// An already-resolved texture (e.g. one the caller loaded itself).
    Texture(egui::TextureId),
    /// No art — render the placeholder fill only.
    None,
}

impl<'a> CardImage<'a> {
    /// Convenience: `Some(url) → Url`, `None → None`.
    pub fn from_url_opt(url: Option<&'a str>) -> Self {
        match url {
            Some(u) => CardImage::Url(u),
            None => CardImage::None,
        }
    }
}

/// A holographic asset card. Build with [`AssetCard::new`] + builder setters,
/// then render with [`AssetCard::paint`] (into a known tile rect/response — the
/// `CardBrowser` integration) or [`AssetCard::show`] (allocate + render).
pub struct AssetCard<'a> {
    image: CardImage<'a>,
    rarity: usize,
    shape: CardMask,
    effect: Option<CardEffectKind>,
    perspective: f32,
    tilt_ease: f32,
    max_tilt_deg: f32,
    border: f32,
    glow: f32,
    show_spark: bool,
    placeholder: Color32,
    strength: f32,
}

impl<'a> AssetCard<'a> {
    /// A new card with the default treatment (square, no rarity, no effect,
    /// 800px perspective, 15° tilt, 3px border, 6px glow, spark enabled).
    pub fn new(image: CardImage<'a>) -> Self {
        Self {
            image,
            rarity: 0,
            shape: CardMask::Square,
            effect: None,
            perspective: 800.0,
            tilt_ease: 0.1,
            max_tilt_deg: 15.0,
            border: 3.0,
            glow: 6.0,
            show_spark: true,
            placeholder: Color32::from_rgb(30, 30, 48),
            strength: 0.7,
        }
    }

    /// Rarity tier (0=Common … 4=Legendary). Drives border colour, the glow
    /// ring (Rare+), and whether the spark streak renders.
    pub fn rarity(mut self, rarity: usize) -> Self {
        self.rarity = rarity;
        self
    }

    /// Border/glow outline shape. Note: the art quad and effect overlay are
    /// rectangular regardless; `shape` controls the rarity border/glow ring.
    pub fn shape(mut self, shape: CardMask) -> Self {
        self.shape = shape;
        self
    }

    /// Holographic overlay shown on hover. `None` disables the overlay.
    pub fn effect(mut self, effect: Option<CardEffectKind>) -> Self {
        self.effect = effect;
        self
    }

    /// Perspective focal distance (larger = flatter). Default 800.
    pub fn perspective(mut self, perspective: f32) -> Self {
        self.perspective = perspective;
        self
    }

    /// Tilt easing factor (0..1, higher = snappier). Default 0.1.
    pub fn tilt_ease(mut self, ease: f32) -> Self {
        self.tilt_ease = ease;
        self
    }

    /// Maximum tilt angle in degrees. Default 15.
    pub fn max_tilt_deg(mut self, deg: f32) -> Self {
        self.max_tilt_deg = deg;
        self
    }

    /// Placeholder fill drawn while art loads / when there is no art.
    pub fn placeholder(mut self, color: Color32) -> Self {
        self.placeholder = color;
        self
    }

    /// Global holographic-overlay strength, `0.0` (off) … `1.0` (full effect at
    /// the most glancing tilt). Default `0.7`. This is the single dial for
    /// toning the foil up/down across a whole surface — combined with the
    /// tilt-based fade, a flat card stays subtle and the holo grows as it tilts.
    /// Clamped to `[0, 1]`.
    pub fn strength(mut self, strength: f32) -> Self {
        self.strength = strength.clamp(0.0, 1.0);
        self
    }

    /// Disable the travelling spark streak (otherwise shown on Rare+).
    pub fn without_spark(mut self) -> Self {
        self.show_spark = false;
        self
    }

    /// Allocate a `size`×`size` square, render the card into it, and return the
    /// interaction [`egui::Response`] (clickable + hover-sensed).
    pub fn show(self, ui: &mut egui::Ui, size: f32, state: &mut AssetCardState) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
        self.paint(ui, rect, &response, state);
        response
    }

    /// Render the card into `rect`, tilting/effecting based on `response`
    /// (typically a grid tile's hover/click response from `CardBrowser`).
    ///
    /// `rect` is the art square; the border/glow draw just outside it, so this
    /// expects a few px of slack around `rect` in the parent layout.
    pub fn paint(
        self,
        ui: &mut egui::Ui,
        rect: Rect,
        response: &egui::Response,
        state: &mut AssetCardState,
    ) {
        let center = rect.center();
        let half = rect.width() / 2.0;
        let pad = (self.border + self.glow).max(6.0);
        let painter = ui.painter_at(rect.expand(pad));

        // 1. Tilt toward the cursor (eased); repaint while it settles.
        let (ax, ay) = update_tilt(
            response,
            center,
            half,
            &mut state.tilt,
            self.tilt_ease,
            self.max_tilt_deg,
        );
        if state.tilt.current_x.abs() > 0.001 || state.tilt.current_y.abs() > 0.001 {
            ui.ctx().request_repaint();
        }

        // 2. Shape outline + projected border ring vertices.
        let outline = base_outline(center, half, self.shape);
        let border_outer = expand_outline(&outline, self.border);
        let proj_outline = project_points(&outline, center, ax, ay, self.perspective);
        let proj_border = project_points(&border_outer, center, ax, ay, self.perspective);

        // 3. Rarity glow ring (Rare+).
        if let Some(glow) = rarity_glow(self.rarity) {
            let glow_outer = expand_outline(&outline, self.border + self.glow);
            let proj_glow = project_points(&glow_outer, center, ax, ay, self.perspective);
            draw_colored_ring(&painter, &proj_border, &proj_glow, glow);
        }

        // 4. Rarity border ring.
        draw_colored_ring(
            &painter,
            &proj_outline,
            &proj_border,
            rarity_color(self.rarity),
        );

        // 5. Art quad (textured when ready, else placeholder fill).
        let corners = [
            rect.left_top(),
            rect.right_top(),
            rect.right_bottom(),
            rect.left_bottom(),
        ];
        let p = project_points(&corners, center, ax, ay, self.perspective);
        let quad: [Pos2; 4] = [p[0], p[1], p[2], p[3]];
        let texture = match self.image {
            CardImage::Texture(id) => Some(id),
            CardImage::Url(url) => {
                let tex = try_load_texture(ui.ctx(), url);
                if tex.is_none() {
                    ui.ctx().request_repaint(); // poll until the loader resolves
                }
                tex
            }
            CardImage::None => None,
        };
        match texture {
            Some(id) => draw_textured_quad(&painter, quad, id, Color32::WHITE),
            None => draw_quad(&painter, quad, self.placeholder),
        }

        // 6. Holographic overlay (on hover, when an effect is set). Faded by the
        //    global `strength` knob AND by tilt magnitude — so a near-flat card
        //    reads as a subtle sheen and the holo "pops" only at glancing angles
        //    (a Fresnel-like gate, the trick that makes real foil feel tasteful).
        if let Some(kind) = self.effect {
            if let Some(hover) = response.hover_pos() {
                let mu = ((hover.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                let mv = ((hover.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
                let max_rad = self.max_tilt_deg.to_radians().max(1e-3);
                let tilt_norm = ((ax * ax + ay * ay).sqrt() / max_rad).clamp(0.0, 1.0);
                let alpha = self.strength * (0.25 + 0.75 * tilt_norm);
                let effect = kind.build();
                let scaled = ScaledEffect {
                    inner: &*effect,
                    alpha,
                };
                draw_effect_quad(
                    &painter,
                    rect,
                    center,
                    ax,
                    ay,
                    self.perspective,
                    mu,
                    mv,
                    &scaled,
                );
            }
        }

        // 7. Travelling spark streak (Rare+), self-animating.
        if self.show_spark && rarity_glow(self.rarity).is_some() {
            let dt = ui.input(|i| i.stable_dt).min(0.1);
            state.spark_phase = (state.spark_phase + dt * 0.3) % 1.0;
            draw_spark_streak(
                &painter,
                &proj_outline,
                state.spark_phase,
                rarity_color(self.rarity),
            );
            ui.ctx().request_repaint();
        }
    }
}

/// Wraps a [`CardEffect`], fading every overlay colour toward transparent by
/// `alpha` (`0.0` = off … `1.0` = unchanged). Effect colours are premultiplied,
/// so scaling all four channels uniformly is a correct linear fade — used to
/// apply the card's global `strength` and tilt gate without touching the
/// per-effect maths or the mesh primitives.
struct ScaledEffect<'a> {
    inner: &'a dyn CardEffect,
    alpha: f32,
}

impl CardEffect for ScaledEffect<'_> {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn compute_colors(
        &self,
        vertices: &[EffectVertex],
        mouse_u: f32,
        mouse_v: f32,
    ) -> Vec<Color32> {
        let k = self.alpha.clamp(0.0, 1.0);
        self.inner
            .compute_colors(vertices, mouse_u, mouse_v)
            .into_iter()
            .map(|c| {
                Color32::from_rgba_premultiplied(
                    (c.r() as f32 * k) as u8,
                    (c.g() as f32 * k) as u8,
                    (c.b() as f32 * k) as u8,
                    (c.a() as f32 * k) as u8,
                )
            })
            .collect()
    }
}

/// Try to resolve an image URL to a texture id via the egui loader, returning
/// `None` while it is still loading (or on error).
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
