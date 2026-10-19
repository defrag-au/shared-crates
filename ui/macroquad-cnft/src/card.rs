//! `CardPainter` — an NFT drawn as a game card: clean, with stat pills, framed, or as a token.
//!
//! The concepts come from `egui-widgets`' asset card (tier-coloured edges, a
//! glow for the rarer tiers, foil, stat pills down one edge, a badge popping
//! out of the bottom), redrawn for a game loop: everything is a fixed-cost
//! draw per frame, text goes through the kit so captures report it, and time
//! is a parameter.
//!
//! | layout | for |
//! |---|---|
//! | [`CardLayout::Clean`] | browsing: the art, the name, the tier as an edge |
//! | [`CardLayout::Stats`] | picking: stat pills on the art, the tier as a badge |
//! | [`CardLayout::Framed`] | a hand or a detail view: a printed trading card, built the way the fight spike's faces are (`macroquad-gamekit/CARD-FACES.md`) |
//! | [`CardLayout::Token`] | a board piece or a portrait: round, a tier ring, one number |
//!
//! Rare and above glow; the card under the pointer catches a foil sheen. All
//! of it through the gamekit's `Lighting`, so a backend without shaders draws
//! the card without them.

use cnft_holdings::{AssetRarity, Stat, StatBlock};
use macroquad::prelude::*;
use macroquad_gamekit::{
    Align, Gradient, Icon, Lighting, MeasureAt, Pose, RichText, TextStyle, bevel, draw_icon, face,
    fill_gradient, fill_rounded, grain, inner_shadow, noise_texture, shrink_to_fit, stroke_rounded,
    tab_width,
};
use macroquad_widgets::Theme;
use ui_theme::{legible_on, mix};

use crate::art::ArtLoader;
use crate::tile::ArtTile;

/// How a card is laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CardLayout {
    #[default]
    Clean,
    Stats,
    Framed,
    Token,
}

impl CardLayout {
    pub const ALL: [CardLayout; 4] = [Self::Clean, Self::Stats, Self::Framed, Self::Token];

    pub fn name(self) -> &'static str {
        match self {
            Self::Clean => "Clean",
            Self::Stats => "Stats",
            Self::Framed => "Framed",
            Self::Token => "Token",
        }
    }

    /// Height over width.
    pub fn aspect(self) -> f32 {
        match self {
            Self::Framed => 1.4,
            _ => 1.0,
        }
    }

    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|l| *l == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
}

/// Tier index 0 (Common) to 4 (Legendary).
fn tier_index(t: &AssetRarity) -> usize {
    match t {
        AssetRarity::Common => 0,
        AssetRarity::Uncommon => 1,
        AssetRarity::Rare => 2,
        AssetRarity::Epic => 3,
        AssetRarity::Legendary => 4,
    }
}

pub fn tier_name(t: &AssetRarity) -> &'static str {
    ["Common", "Uncommon", "Rare", "Epic", "Legendary"][tier_index(t)]
}

/// The card's colours.
///
/// Tier hues follow the games convention collectors read without a legend
/// (grey, green, blue, purple, gold), mapped onto theme accents the same way
/// `egui-widgets`' asset card maps them, so the two renderers agree.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CardPalette {
    pub tiers: [Color; 5],
    /// A card without a known tier.
    pub untiered: Color,
    /// Plates, pills and the frame.
    pub panel: Color,
    pub text: Color,
    pub muted: Color,
    pub attack: Color,
    pub defence: Color,
    pub speed: Color,
    pub health: Color,
    pub other: Color,
    /// The printed card's materials: the dark rim, the dark plates (title,
    /// type line), the parchment rules box and its ink, the brass edge round
    /// the art. Materials, not UI surfaces, so not theme tokens: the same
    /// values as the fight spike's faces, so the two read as one game.
    pub rim: (Color, Color),
    pub plate: (Color, Color),
    pub parchment: (Color, Color),
    pub ink: Color,
    pub brass: Color,
    /// Text on a dark plate.
    pub plate_text: Color,
}

fn hex(rgb: u32) -> Color {
    Color::from_rgba((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255)
}

impl CardPalette {
    pub fn from_theme(theme: &Theme) -> Self {
        let c = theme.color;
        Self {
            tiers: [
                c.text_muted,
                c.accent_green,
                c.accent_blue,
                c.accent_magenta,
                c.accent_orange,
            ],
            untiered: c.border,
            panel: c.bg_primary,
            text: c.text_primary,
            muted: c.text_muted,
            attack: c.accent_red,
            defence: c.accent_blue,
            speed: c.accent_yellow,
            health: c.accent_green,
            other: c.accent_cyan,
            rim: (hex(0x1d212b), hex(0x07080b)),
            plate: (hex(0x2f3441), hex(0x161922)),
            parchment: (hex(0xefe6d0), hex(0xd2c3a0)),
            ink: hex(0x1d1a14),
            brass: hex(0xd9c27a),
            plate_text: hex(0xf3efe4),
        }
    }

    pub fn tier(&self, tier: Option<&AssetRarity>) -> Color {
        tier.map_or(self.untiered, |t| self.tiers[tier_index(t)])
    }

    fn stat(&self, key: &str) -> Color {
        match key {
            "atk" => self.attack,
            "def" => self.defence,
            "spd" => self.speed,
            "hp" => self.health,
            _ => self.other,
        }
    }
}

/// What a card shows.
pub struct CardData<'a> {
    pub name: &'a str,
    /// The art's URL, as requested from the [`ArtLoader`].
    pub art: &'a str,
    pub tier: Option<AssetRarity>,
    pub rank: Option<u32>,
    pub stats: Option<&'a StatBlock>,
    /// What the asset is in the game: its role, from the collection's
    /// `RoleRule`. The framed card's type line.
    pub tagline: Option<&'a str>,
    /// The role's rules, from the collection's profile: the framed card's
    /// rules box. `None` leaves the box off.
    pub rules: Option<&'a str>,
}

/// Per-draw animation inputs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CardFx {
    /// The game's clock, seconds.
    pub now: f32,
    /// The pointer is over the card.
    pub hover: bool,
    /// Offsets the sheen, so a grid of cards does not flash in unison.
    pub phase: f32,
    /// Where text may go: each line is drawn only if it lies wholly inside.
    /// For a card partly under a header, a footer or the screen's edge: its
    /// visible lines draw, and the covered ones are left out rather than
    /// drawn hidden, where a capture's report would still count them.
    /// `None` draws all text; an empty rect draws none.
    pub text_within: Option<Rect>,
}

/// Draws NFT cards. Host-owned: create once the window exists, keep.
pub struct CardPainter {
    pub palette: CardPalette,
    tile: ArtTile,
    light: Lighting,
    font: Option<Font>,
    /// Names, type lines and numbers on a framed card; `font` otherwise.
    display: Option<Font>,
    /// The tooth of a printed surface, laid over the frame and rules box.
    grain: Texture2D,
}

impl CardPainter {
    pub fn new(font: Option<Font>, palette: CardPalette, tile: ArtTile) -> Self {
        Self {
            palette,
            tile,
            light: Lighting::new(),
            display: font.clone(),
            font,
            grain: noise_texture(64, 7, 0.16),
        }
    }

    /// A condensed display face for a framed card's names and numbers, as a
    /// printed card sets them.
    pub fn with_display(mut self, display: Option<Font>) -> Self {
        if display.is_some() {
            self.display = display;
        }
        self
    }

    /// The tile drawer, for art that is not a card.
    pub fn tile(&self) -> &ArtTile {
        &self.tile
    }

    /// Draw `card` filling `rect` (whose shape should match
    /// [`CardLayout::aspect`]). `px` is device pixels per unit, for text.
    pub fn draw(
        &self,
        art: &mut ArtLoader,
        card: &CardData,
        layout: CardLayout,
        rect: Rect,
        px: f32,
        fx: CardFx,
    ) {
        let tier = card.tier.as_ref().map(tier_index);
        let edge = self.palette.tier(card.tier.as_ref());
        let radius = match layout {
            CardLayout::Token => rect.w.min(rect.h) / 2.0,
            CardLayout::Framed => rect.w * 0.07,
            _ => (rect.w * 0.06).max(6.0),
        };

        // A glow for Rare and above, under everything.
        // Faint and close: an edge light, not a halo. At a third of the card's
        // opacity it read as every rare card shouting at once.
        if let Some(t) = tier.filter(|t| *t >= 2) {
            let glow = Color::new(edge.r, edge.g, edge.b, 0.10 + 0.04 * (t - 2) as f32);
            self.light.soft(
                Pose::at(rect.center()),
                rect.size(),
                radius,
                (rect.w * 0.035).max(4.0),
                glow,
            );
        }

        let ink = CardInk {
            font: self.font.as_ref(),
            display: self.display.as_ref(),
            px,
            within: fx.text_within,
        };
        match layout {
            CardLayout::Clean => self.clean(art, card, rect, radius, edge, &ink),
            CardLayout::Stats => self.stats(art, card, rect, radius, edge, &ink),
            CardLayout::Framed => self.framed(art, card, rect, radius, edge, &ink),
            CardLayout::Token => self.token(art, card, rect, edge, &ink),
        }

        // Foil: only on the card under the pointer. On every Epic and
        // Legendary card it was noise: a screen of the rarest is all of them,
        // and a sheen everywhere singles nothing out (`CARD-FACES.md` says
        // the same of the fight's cards). Not until the art is here either:
        // foil over a loading tile reads as a second shimmer.
        if fx.hover && art.ready(card.art) {
            let phase = ((fx.now * 0.22 + fx.phase).fract()) * 1.6 - 0.3;
            self.light
                .sheen(Pose::at(rect.center()), rect.size(), radius, phase, 0.55);
        }
    }

    fn clean(
        &self,
        art: &mut ArtLoader,
        card: &CardData,
        r: Rect,
        rad: f32,
        edge: Color,
        ink: &CardInk,
    ) {
        let p = &self.palette;
        self.tile.draw(art, card.art, r, rad);
        // A name plate inside the bottom edge, rather than a scrim: a
        // gradient would show square at the rounded corners.
        let size = (r.w * 0.075).clamp(9.0, 16.0);
        let h = size * 2.0;
        let m = (r.w * 0.04).max(4.0);
        let plate = Rect::new(r.x + m, r.bottom() - m - h, r.w - 2.0 * m, h);
        fill_rounded(plate, h / 2.0, with_alpha(p.panel, 0.82));
        ink.fit(
            card.name,
            size,
            p.text,
            vec2(plate.x + h * 0.45, plate.center().y),
            LEFT_MID,
            plate.w - h * 0.9 - rank_w(card.rank, size, ink),
        );
        if let Some(rank) = card.rank {
            ink.text(
                &format!("#{rank}"),
                size * 0.85,
                edge,
                vec2(plate.right() - h * 0.45, plate.center().y),
                RIGHT_MID,
            );
        }
        stroke_rounded(r, rad, 2.0, edge);
    }

    fn stats(
        &self,
        art: &mut ArtLoader,
        card: &CardData,
        r: Rect,
        rad: f32,
        edge: Color,
        ink: &CardInk,
    ) {
        let p = &self.palette;
        self.tile.draw(art, card.art, r, rad);
        stroke_rounded(r, rad, 2.0, edge);
        let size = (r.w * 0.07).clamp(8.5, 15.0);
        let m = (r.w * 0.045).max(4.0);

        // The name across the top.
        let title_h = size * 1.9;
        let title = Rect::new(r.x + m, r.y + m, r.w - 2.0 * m, title_h);
        fill_rounded(title, title_h / 2.0, with_alpha(p.panel, 0.78));
        ink.fit(
            card.name,
            size,
            p.text,
            title.center(),
            CENTRE,
            title.w - title_h,
        );

        // Stat pills down the right edge, centred on the art.
        if let Some(block) = card.stats {
            let ph = size * 1.7;
            let pw = size * 3.6;
            let gap = size * 0.3;
            let n = block.stats.len() as f32;
            let top = r.center().y - (n * ph + (n - 1.0) * gap) / 2.0 + title_h * 0.3;
            for (i, s) in block.stats.iter().enumerate() {
                let pill = Rect::new(r.right() - m - pw, top + i as f32 * (ph + gap), pw, ph);
                self.pill(pill, s, size, ink);
            }
        }

        // The tier as a badge straddling the bottom edge.
        if let Some(t) = &card.tier {
            let label = tier_name(t).to_uppercase();
            let bh = size * 1.8;
            let bw = (r.w * 0.56).max(bh * 3.0);
            let badge = Rect::new(r.center().x - bw / 2.0, r.bottom() - bh * 0.6, bw, bh);
            fill_rounded(badge, bh / 2.0, p.panel);
            stroke_rounded(badge, bh / 2.0, 1.5, edge);
            ink.fit(&label, size * 0.85, edge, badge.center(), CENTRE, bw - bh);
        }
    }

    fn framed(
        &self,
        art: &mut ArtLoader,
        card: &CardData,
        r: Rect,
        rad: f32,
        edge: Color,
        ink: &CardInk,
    ) {
        // A printed card, back to front, every measure a fraction of the
        // width `w`; the proportions are the fight spike's faces
        // (`macroquad-gamekit/CARD-FACES.md`), so the two read as one game.
        let p = &self.palette;
        let w = r.w;
        let at = |x: f32, y: f32| vec2(r.x + x * w, r.y + y * w);
        let rect =
            |x: f32, y: f32, rw: f32, rh: f32| Rect::new(r.x + x * w, r.y + y * w, rw * w, rh * w);
        let inset = 0.075;
        let inner = 1.0 - 2.0 * inset;

        // 1. The rim: near-black, a little light at the top.
        fill_gradient(
            r,
            rad,
            Gradient::Vertical {
                top: p.rim.0,
                bottom: p.rim.1,
            },
        );

        // 2. The frame: the tier as metal, with grain and a bevel. The tier
        // reads from any sliver of the card, the way a frame colour does.
        let rim = 0.022 * w;
        let frame = Rect::new(r.x + rim, r.y + rim, r.w - 2.0 * rim, r.h - 2.0 * rim);
        let fr = (rad - 0.025 * w).max(1.0);
        fill_gradient(
            frame,
            fr,
            Gradient::Vertical {
                top: mix(edge, BLACK, 0.12),
                bottom: mix(edge, BLACK, 0.72),
            },
        );
        grain(&self.grain, frame, Color::new(1.0, 1.0, 1.0, 0.9));
        bevel(
            frame,
            fr,
            1.2,
            with_alpha(WHITE, 0.35),
            Color::new(0.0, 0.0, 0.0, 0.45),
        );

        // 3. The name, set straight on the frame's metal in whichever ink
        // has the higher WCAG contrast with it: dark on the light metals,
        // light on the dark. The metal is read where the name sits, near the
        // gradient's top.
        let name_y = 0.115;
        let metal = mix(edge, BLACK, 0.12 + 0.6 * (name_y / 1.4));
        let name_ink = legible_on(metal, p.plate_text, p.ink);
        ink.shrink_to_fit(
            card.name,
            (0.1 * w, 0.07 * w),
            name_ink,
            at(inset + 0.01, name_y),
            LEFT_MID,
            (inner - 0.02) * w,
        );

        // The card is about what the asset is: its role, on the type line,
        // and the role's rules in the box below when the collection's profile
        // gives some. With none the box is left off and the art takes its
        // room, rather than filling it with other traits.
        let type_y = if card.rules.is_some() {
            0.875
        } else {
            1.4 - 0.065 - 0.115
        };

        // 4. The art window, from just under the name down to the type line,
        // set behind the frame by an inner shadow and a brass edge.
        let art_top = name_y + 0.07;
        let art_h = type_y - 0.025 - art_top;
        let window = rect(inset, art_top, inner, art_h);
        let wr = 0.025 * w;
        self.tile.draw(art, card.art, window, wr);
        inner_shadow(window, wr, 0.045 * w, Color::new(0.0, 0.0, 0.0, 0.65));
        stroke_rounded(window, wr, 1.0, with_alpha(p.brass, 0.85));

        // 5. Stat tabs, flat side hard against the art window's right
        // border, stacked up from its bottom: part of the frame rather than
        // pills floating over the portrait. One width for the column, so
        // their rounded ends line up.
        if let Some(block) = card.stats {
            let shown: Vec<&Stat> = block.stats.iter().take(4).collect();
            let h = 0.115 * w;
            let widest = shown
                .iter()
                .map(|s| s.value.to_string())
                .max_by_key(String::len)
                .unwrap_or_default();
            let tw = tab_width(h, &widest, ink.display, ink.px);
            let n = shown.len();
            let bottom = art_top + art_h - 0.03 - 0.115;
            for (i, s) in shown.into_iter().enumerate() {
                // A step of 0.145 w for a 0.115 w tab: a quarter of its
                // height between them.
                let y = bottom - (n - 1 - i) as f32 * 0.145;
                self.tab(vec2(window.right(), r.y + y * w), (tw, h), s, ink);
            }
        }

        // 6. The type line: the role. Not the rarity: the card plays as what
        // it is, not how scarce it is.
        if let Some(role) = card.tagline {
            let type_plate = rect(inset, type_y, inner, 0.115);
            self.plate(type_plate, 0.03 * w);
            ink.shrink_to_fit(
                &role.to_uppercase(),
                (0.072 * w, 0.06 * w),
                with_alpha(p.plate_text, 0.92),
                at(0.1, type_y + 0.115 / 2.0),
                LEFT_MID,
                type_plate.w - 0.05 * w,
            );
        }

        // 7. The rules box: parchment, the role's rules as its text.
        if let Some(text) = card.rules {
            let rules = rect(inset, 1.01, inner, 0.315);
            let rr = 0.03 * w;
            fill_gradient(
                rules,
                rr,
                Gradient::Vertical {
                    top: p.parchment.0,
                    bottom: p.parchment.1,
                },
            );
            grain(&self.grain, rules, Color::new(0.55, 0.5, 0.4, 0.8));
            inner_shadow(rules, rr, 0.02 * w, Color::new(0.25, 0.18, 0.08, 0.45));
            let pad = 0.05 * w;
            ink.rules_text(
                text,
                (0.066 * w, 0.05 * w),
                p.ink,
                vec2(rules.x + pad, rules.y + 0.035 * w),
                (rules.w - 2.0 * pad, rules.h - 0.06 * w),
            );
        }
    }

    /// A dark plate: a title or a type line. The gamekit's `face::plate`.
    fn plate(&self, r: Rect, radius: f32) {
        face::plate(r, radius, self.palette.plate);
    }

    /// A stat tab: the gamekit's `face::tab`, in the stat's colour with its
    /// icon, its flat side at `top_right`. Its number is drawn only where
    /// text may go.
    fn tab(&self, top_right: Vec2, (w, h): (f32, f32), s: &Stat, ink: &CardInk) {
        face::tab(
            top_right,
            (w, h),
            stat_icon(&s.key),
            self.palette.stat(&s.key),
            (&s.value.to_string(), ink.display),
            ink.allows(Rect::new(top_right.x - w, top_right.y, w, h)),
            ink.px,
        );
    }

    fn token(&self, art: &mut ArtLoader, card: &CardData, r: Rect, edge: Color, ink: &CardInk) {
        let p = &self.palette;
        let d = r.w.min(r.h);
        let c = r.center();
        let disc = Rect::new(c.x - d / 2.0, c.y - d / 2.0, d, d);
        self.tile.draw(art, card.art, disc, d / 2.0);
        let ring = (d * 0.045).max(2.0);
        draw_circle_lines(c.x, c.y, d / 2.0 - ring / 2.0, ring, edge);
        if let Some(block) = card.stats {
            // Sits on the ring at the lower right, a little inside it.
            let br = d * 0.12;
            let at = c + Vec2::splat((d / 2.0 - br * 0.55) * std::f32::consts::FRAC_1_SQRT_2);
            // Polygons rather than `draw_circle`, whose few sides show as
            // facets at this size.
            draw_poly(at.x, at.y, 48, br + 1.5, 0.0, p.panel);
            draw_poly(at.x, at.y, 48, br, 0.0, edge);
            ink.text(
                &block.power().to_string(),
                (br * 0.9).clamp(8.0, 18.0),
                p.panel,
                at,
                CENTRE,
            );
        }
    }

    fn pill(&self, r: Rect, s: &Stat, size: f32, ink: &CardInk) {
        let p = &self.palette;
        let color = p.stat(&s.key);
        fill_rounded(r, r.h / 2.0, with_alpha(p.panel, 0.82));
        let icon = Rect::new(r.x + r.h * 0.25, r.y + r.h * 0.2, r.h * 0.6, r.h * 0.6);
        draw_stat_icon(&s.key, icon, color);
        ink.text(
            &s.value.to_string(),
            size,
            color,
            vec2(r.right() - r.h * 0.35, r.center().y),
            RIGHT_MID,
        );
    }
}

fn rank_w(rank: Option<u32>, size: f32, ink: &CardInk) -> f32 {
    rank.map_or(0.0, |r| {
        ink.measure(&format!("#{r}"), size * 0.85) + size * 0.5
    })
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a)
}

/// The gamekit icon a stat key is drawn with: a sword for attack, a shield
/// for defence, a bolt for speed, a heart for health, a diamond otherwise.
pub fn stat_icon(key: &str) -> Icon {
    match key {
        "atk" => Icon::Sword,
        "def" => Icon::Shield,
        "spd" => Icon::Bolt,
        "hp" => Icon::Heart,
        _ => Icon::Diamond,
    }
}

/// A stat's icon in `r`.
pub fn draw_stat_icon(key: &str, r: Rect, color: Color) {
    draw_icon(stat_icon(key), r, color);
}

const CENTRE: Vec2 = Vec2::new(0.5, 0.5);
const LEFT_MID: Vec2 = Vec2::new(0.0, 0.5);
const RIGHT_MID: Vec2 = Vec2::new(1.0, 0.5);

/// Text through the kit, so a capture's report sees every line.
struct CardInk<'a> {
    font: Option<&'a Font>,
    /// Names, type lines and numbers on a framed card.
    display: Option<&'a Font>,
    px: f32,
    /// Where text may go ([`CardFx::text_within`]).
    within: Option<Rect>,
}

impl CardInk<'_> {
    /// May text occupying `r` be drawn?
    fn allows(&self, r: Rect) -> bool {
        self.within.is_none_or(|w| {
            r.x >= w.x && r.y >= w.y && r.right() <= w.right() && r.bottom() <= w.bottom()
        })
    }

    fn layout(
        &self,
        s: &str,
        size: f32,
        color: Color,
    ) -> (RichText, macroquad_gamekit::TextLayout) {
        let mut t = RichText::new();
        let id = t.style(TextStyle::new(self.font.cloned(), size, color));
        t.text(s, id);
        let l = t.layout(&MeasureAt(self.px), f32::INFINITY, Align::Start, 0.0);
        (t, l)
    }

    fn measure(&self, s: &str, size: f32) -> f32 {
        self.layout(s, size, WHITE).1.size.x
    }

    fn text(&self, s: &str, size: f32, color: Color, at: Vec2, anchor: Vec2) {
        let (t, l) = self.layout(s, size, color);
        let origin = at - l.size * anchor;
        if self.allows(Rect::new(origin.x, origin.y, l.size.x, l.size.y)) {
            l.draw(&t, origin, self.px, 1.0, |_, _| {});
        }
    }

    /// One line in a given style, its box placed by `anchor` at `at`.
    fn styled(&self, s: &str, style: TextStyle, at: Vec2, anchor: Vec2) {
        let mut t = RichText::new();
        let id = t.style(style);
        t.text(s, id);
        let l = t.layout(&MeasureAt(self.px), f32::INFINITY, Align::Start, 0.0);
        let origin = at - l.size * anchor;
        if self.allows(Rect::new(origin.x, origin.y, l.size.x, l.size.y)) {
            l.draw(&t, origin, self.px, 1.0, |_, _| {});
        }
    }

    /// A name in the display face: the largest size from `sizes.0` down to
    /// `sizes.1` at which it fits `max_w`, else cut with an ellipsis at the
    /// smallest. A long name shrinks before it is cut, as a printed card's
    /// title does.
    fn shrink_to_fit(
        &self,
        s: &str,
        sizes: (f32, f32),
        color: Color,
        at: Vec2,
        anchor: Vec2,
        max_w: f32,
    ) {
        let (line, size) = shrink_to_fit(self.display, s, sizes, max_w, self.px);
        // No outline: the ink is chosen to contrast with what it sits on. A
        // faint offset in the opposite tone stamps it into the surface, light
        // under dark ink and dark under light.
        let stamp = if legible_on(color, WHITE, BLACK) == BLACK {
            Color::new(0.0, 0.0, 0.0, 0.45)
        } else {
            Color::new(1.0, 1.0, 1.0, 0.25)
        };
        let style =
            TextStyle::new(self.display.cloned(), size, color).shadow(stamp, vec2(0.0, 0.8));
        self.styled(&line, style, at, anchor);
    }

    /// Rules text wrapped to a box: the largest size from `sizes.0` down to
    /// `sizes.1` at which it fits `(w, h)`. At the smallest it is drawn
    /// whole even if it runs long, rather than cut mid-sentence: the
    /// capture report then says so, and the profile's text is what to fix.
    fn rules_text(&self, s: &str, sizes: (f32, f32), color: Color, at: Vec2, (w, h): (f32, f32)) {
        if !self.allows(Rect::new(at.x, at.y, w, h)) {
            return;
        }
        let layout = |size: f32| {
            let mut t = RichText::new();
            let id = t.style(TextStyle::new(self.font.cloned(), size, color));
            t.text(s, id);
            let l = t.layout(&MeasureAt(self.px), w, Align::Start, 0.5);
            (t, l)
        };
        let mut size = sizes.0;
        let (mut t, mut l) = layout(size);
        while l.size.y > h && size > sizes.1 {
            size -= 0.25;
            (t, l) = layout(size);
        }
        l.draw(&t, at, self.px, 1.0, |_, _| {});
    }

    /// As [`Self::text`], shortened with an ellipsis to fit `max_w`.
    fn fit(&self, s: &str, size: f32, color: Color, at: Vec2, anchor: Vec2, max_w: f32) {
        if self.measure(s, size) <= max_w {
            self.text(s, size, color, at, anchor);
            return;
        }
        let chars: Vec<char> = s.chars().collect();
        for keep in (0..chars.len()).rev() {
            let line = format!("{}…", chars[..keep].iter().collect::<String>().trim_end());
            if keep == 0 || self.measure(&line, size) <= max_w {
                self.text(&line, size, color, at, anchor);
                return;
            }
        }
    }
}
