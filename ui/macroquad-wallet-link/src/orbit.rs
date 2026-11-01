//! The system view: the reader's Discord identity as the sun, each chain a
//! planet orbiting it, and every wallet linked on that chain a moon orbiting
//! its planet. The tethers trace the link itself: moon to planet, planet to
//! sun, so a selected wallet lights its whole path home.
//!
//! A chain the reader can link but has not is a ghost planet, dashed and
//! dim, that a tap asks to link a wallet on.
//!
//! The view holds only motion: where each body is on its way, how far round
//! each orbit has turned, how far the sun has risen. The wallets are the
//! flow's; [`OrbitView::sync`] is told the list each frame and moves planets
//! and moons in, round and out to match it. Time is the `dt` handed to
//! [`OrbitView::update`], so a capture at `--at` is the same picture every run.

use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

use chains::ChainFamily;
use macroquad::prelude::*;
use macroquad_gamekit::wallet_modal::{chain_mark, chain_tint};
use macroquad_gamekit::{Animated, Ease, Particles};
use macroquad_widgets::{
    Align, Gradient, Painter, TextSize, fill_gradient, fill_rounded, fill_texture_rounded,
};
use wallet_link_flow::wallet_link::LinkedWallet;
use wallet_link_flow::{LINKABLE, family_of};

use crate::planet::{ATMOSPHERE, PlanetLook, bake_moon, bake_planet, mix};

/// The flattest the orbits look, their height over their width, on a wide
/// field. A tall field tilts them towards the reader (see [`Geo`]).
pub const TILT: f32 = 0.36;
/// The roundest they get, on a tall phone.
const MAX_TILT: f32 = 0.74;
/// Moons a ring round a planet holds before a second ring is used.
const MOONS_PER_RING: usize = 5;
/// A finger's worth round anything tappable.
const HIT_R: f32 = 22.0;
/// Seconds the planets take to go round the sun once.
const PLANET_TURN: f32 = 150.0;
/// Seconds a moon takes to go round its planet once, inner ring.
const MOON_TURN: f32 = 48.0;
/// Up to this many wallets, the near moons are named; past it only the one
/// looked at is, and the list view is the place to read them all.
const NAMED_UP_TO: usize = 4;

const SUN_RISE: f32 = 0.6;
const ORBIT_DRAW: f32 = 0.8;
const PLANET_EMERGE: f32 = 0.9;
const PLANET_STAGGER: f32 = 0.15;
const MOON_EMERGE: f32 = 0.8;
const MOON_STAGGER: f32 = 0.08;
const LAUNCH_SECS: f32 = 1.1;
const LEAVE_SECS: f32 = 0.6;
const RESPACE_SECS: f32 = 0.8;
const GHOST_SECS: f32 = 0.5;
const PAUSE_SECS: f32 = 0.6;
const RESUME_SECS: f32 = 0.9;

/// How a linked wallet is named on screen: its ADA handle when it has one,
/// else its address, shortened.
pub fn wallet_label(w: &LinkedWallet) -> String {
    match &w.handle {
        Some(h) if !h.is_empty() => {
            if h.starts_with('$') {
                h.clone()
            } else {
                format!("${h}")
            }
        }
        _ => short_tail(&w.address),
    }
}

/// An address cut to its first six and last four characters: short enough to
/// sit under a moon, and the ends a reader checks.
pub fn short_tail(address: &str) -> String {
    let chars: Vec<char> = address.chars().collect();
    if chars.len() <= 12 {
        return address.to_string();
    }
    let head: String = chars[..6].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}…{tail}")
}

/// The colour a chain's planet and moons are.
pub fn moon_tint(p: &Painter, family: Option<ChainFamily>) -> Color {
    family.map_or(p.theme.color.text_secondary, |f| chain_tint(&p.theme, f))
}

/// The chains drawn as planets for `wallets`, in the order a reader meets
/// them: every linkable chain (a ghost when it has no wallet), then any other
/// chain a link names.
pub fn planet_order(wallets: &[LinkedWallet]) -> Vec<ChainFamily> {
    let mut out: Vec<ChainFamily> = LINKABLE.to_vec();
    for w in wallets {
        if let Some(f) = family_of(w)
            && !out.contains(&f)
        {
            out.push(f);
        }
    }
    out.sort();
    out
}

/// Where moon `i` of `n` sits round its planet: its ring, and its angle.
pub fn moon_slot(i: usize, n: usize) -> (usize, f32) {
    let ring = i / MOONS_PER_RING;
    let on_ring = n
        .saturating_sub(ring * MOONS_PER_RING)
        .clamp(1, MOONS_PER_RING);
    let k = i % MOONS_PER_RING;
    let phase = -PI * 0.5 + ring as f32 * 0.6;
    (ring, phase + k as f32 * TAU / on_ring as f32)
}

/// The angle `d` brought into −π..π: the short way round.
fn shortest(d: f32) -> f32 {
    (d + PI).rem_euclid(TAU) - PI
}

/// The system's geometry in a field.
#[derive(Clone, Copy, Debug)]
pub struct Geo {
    pub centre: Vec2,
    pub sun_r: f32,
    pub planet_r: f32,
    pub moon_r: f32,
    /// The radius of a planet's first ring of moons, face on.
    pub moon_orbit: f32,
    /// The planets' orbit round the sun, face on.
    pub orbit_rx: f32,
    pub tilt: f32,
}

impl Geo {
    pub fn of(field: Rect) -> Self {
        let short = field.w.min(field.h);
        let sun_r = (short * 0.1).clamp(28.0, 56.0);
        let planet_r = (short * 0.08).clamp(22.0, 44.0);
        let moon_r = (planet_r * 0.36).clamp(8.0, 15.0);
        let moon_orbit = planet_r * 1.6 + 6.0;
        let tilt = (field.h / field.w.max(1.0) * 0.48).clamp(TILT, MAX_TILT);
        let reach = moon_orbit + moon_r + 14.0;
        let orbit_rx = (field.w * 0.5 - reach)
            .min((field.h * 0.5 - reach * tilt - 30.0) / tilt)
            .min(sun_r * 7.0)
            .max(sun_r + reach);
        Self {
            centre: vec2(field.center().x, field.y + field.h * 0.48),
            sun_r,
            planet_r,
            moon_r,
            moon_orbit,
            orbit_rx,
            tilt,
        }
    }

    /// A point on an orbit of radius `r` round `at`, at angle `theta`.
    fn on(&self, at: Vec2, r: f32, theta: f32) -> Vec2 {
        at + vec2(theta.cos() * r, theta.sin() * r * self.tilt)
    }
}

/// What a tap on the system hit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrbitTap {
    Moon(String),
    /// A chain's planet; `empty` when it is a ghost with no wallet yet.
    Planet {
        family: ChainFamily,
        empty: bool,
    },
    /// The sun: the reader's own Discord identity.
    Sun,
    /// Open sky: a tap that should let go of a selection.
    Sky,
}

struct Moon {
    address: String,
    label: String,
    /// Its angle round its planet, before the planet's moons have turned.
    slot: Animated<f32>,
    ring: usize,
    /// 0 at the planet's centre, 1 on its ring, past 1 leaving.
    reach: Animated<f32>,
    alpha: Animated<f32>,
    leaving: bool,
    /// Bursts once it is where it is going: a link just made, or the wallet
    /// connected was this one already.
    burst: bool,
}

struct Body {
    family: ChainFamily,
    /// Its angle round the sun, before the orbit has turned.
    slot: Animated<f32>,
    /// 0 at the sun's centre, 1 on its orbit.
    reach: Animated<f32>,
    /// 1 a ghost (no wallet on it), 0 a world.
    ghost: Animated<f32>,
    /// How far its moons have turned.
    spin: f32,
    moons: Vec<Moon>,
}

impl Body {
    fn live_moons(&self) -> impl Iterator<Item = &Moon> {
        self.moons.iter().filter(|m| !m.leaving)
    }
}

/// Who the sun is.
#[derive(Clone, Default)]
pub struct Identity {
    /// The reader's Discord name, when known.
    pub name: Option<String>,
    /// Their avatar, once loaded.
    pub avatar: Option<Texture2D>,
}

/// One thing drawn, in depth order.
#[derive(Clone, Copy)]
enum Item {
    Sun,
    Planet(usize),
    Moon(usize, usize),
}

/// See the [module docs](self).
pub struct OrbitView {
    bodies: Vec<Body>,
    /// How far the planets have turned round the sun.
    spin: f32,
    speed: Animated<f32>,
    sun_in: Animated<f32>,
    /// Seconds since the view was made: the entrance.
    age: f32,
    loaded: bool,
    paused: bool,
    planets: HashMap<ChainFamily, Texture2D>,
    planet_accent: Option<Color>,
    moon: Option<Texture2D>,
    particles: Particles,
    identity: Identity,
}

impl Default for OrbitView {
    fn default() -> Self {
        Self::new()
    }
}

impl OrbitView {
    pub fn new() -> Self {
        let mut sun_in = Animated::new(0.0);
        sun_in.to(1.0, SUN_RISE, Ease::ExpoOut);
        Self {
            bodies: Vec::new(),
            spin: 0.0,
            speed: Animated::new(1.0),
            sun_in,
            age: 0.0,
            loaded: false,
            paused: false,
            planets: HashMap::new(),
            planet_accent: None,
            moon: None,
            particles: Particles::new(11, 0.0),
            identity: Identity::default(),
        }
    }

    /// Who the sun is: the reader's name and avatar.
    pub fn set_identity(&mut self, identity: Identity) {
        self.identity = identity;
    }

    pub fn identity(&self) -> &Identity {
        &self.identity
    }

    /// Hold the system still (a wallet is being looked at), or let it turn.
    pub fn pause(&mut self, paused: bool) {
        if paused == self.paused {
            return;
        }
        self.paused = paused;
        if paused {
            self.speed.to(0.0, PAUSE_SECS, Ease::ExpoOut);
        } else {
            self.speed.to(1.0, RESUME_SECS, Ease::CubicInOut);
        }
    }

    /// Match the system to `wallets`: a chain's planet emerges from the sun,
    /// a new wallet's moon from its planet (launched when `launched` names
    /// it), a gone one flies off, and the rest re-space round their orbits.
    pub fn sync(&mut self, wallets: &[LinkedWallet], launched: Option<&str>) {
        let first = !self.loaded;
        self.loaded = true;
        let order = planet_order(wallets);

        // Planets: one per chain in the order, each to its slot.
        for (i, family) in order.iter().enumerate() {
            let angle = -PI * 0.5 + 0.35 + i as f32 * TAU / order.len() as f32;
            let has = wallets.iter().any(|w| family_of(w) == Some(*family));
            let ghost = if has { 0.0 } else { 1.0 };
            match self.bodies.iter_mut().find(|b| b.family == *family) {
                Some(b) => {
                    let to = b.slot.target() + shortest(angle - b.slot.target());
                    if (to - b.slot.target()).abs() > 1e-4 {
                        b.slot.to(to, RESPACE_SECS, Ease::CubicInOut);
                    }
                    if b.ghost.target() != ghost {
                        b.ghost.to(ghost, GHOST_SECS, Ease::CubicOut);
                    }
                }
                None => {
                    let mut reach = Animated::new(0.0);
                    let delay = if first {
                        0.35 + i as f32 * PLANET_STAGGER
                    } else {
                        0.0
                    };
                    reach.to_after(1.0, delay, PLANET_EMERGE, Ease::BackOut);
                    self.bodies.push(Body {
                        family: *family,
                        slot: Animated::new(angle),
                        reach,
                        ghost: Animated::new(ghost),
                        spin: i as f32 * 1.3,
                        moons: Vec::new(),
                    });
                }
            }
        }
        self.bodies.sort_by_key(|b| b.family);

        // Moons: on their chain's planet, in the list's own order.
        for body in &mut self.bodies {
            let mine: Vec<&LinkedWallet> = wallets
                .iter()
                .filter(|w| family_of(w) == Some(body.family))
                .collect();
            for m in &mut body.moons {
                if !m.leaving && !mine.iter().any(|w| w.address == m.address) {
                    m.leaving = true;
                    m.reach.to(2.0, LEAVE_SECS, Ease::CubicIn);
                    m.alpha.to(0.0, LEAVE_SECS, Ease::CubicIn);
                }
            }
            let n = mine.len();
            for (i, w) in mine.iter().enumerate() {
                let (ring, angle) = moon_slot(i, n);
                if let Some(m) = body
                    .moons
                    .iter_mut()
                    .find(|m| m.address == w.address && !m.leaving)
                {
                    m.label = wallet_label(w);
                    m.ring = ring;
                    let to = m.slot.target() + shortest(angle - m.slot.target());
                    if (to - m.slot.target()).abs() > 1e-4 {
                        m.slot.to(to, RESPACE_SECS, Ease::CubicInOut);
                    }
                    continue;
                }
                let launch = launched == Some(w.address.as_str());
                let delay = if first {
                    0.8 + i as f32 * MOON_STAGGER
                } else {
                    0.0
                };
                let secs = if launch { LAUNCH_SECS } else { MOON_EMERGE };
                let mut reach = Animated::new(0.0);
                let mut alpha = Animated::new(0.0);
                reach.to_after(1.0, delay, secs, Ease::BackOut);
                alpha.to_after(1.0, delay, secs * 0.4, Ease::CubicOut);
                body.moons.push(Moon {
                    address: w.address.clone(),
                    label: wallet_label(w),
                    slot: Animated::new(angle),
                    ring,
                    reach,
                    alpha,
                    leaving: false,
                    burst: launch,
                });
            }
        }
    }

    /// Burst at `address`'s moon, as a launch landing does: the wallet the
    /// reader just connected was this one already.
    pub fn flare(&mut self, address: &str) {
        for b in &mut self.bodies {
            if let Some(m) = b
                .moons
                .iter_mut()
                .find(|m| m.address == address && !m.leaving)
            {
                m.burst = true;
            }
        }
    }

    /// Whether anything is still moving that a capture should wait out.
    pub fn settled(&self) -> bool {
        self.sun_in.settled()
            && self.bodies.iter().all(|b| {
                b.reach.settled()
                    && b.slot.settled()
                    && b.moons
                        .iter()
                        .all(|m| m.reach.settled() && m.slot.settled())
            })
    }

    /// Advance the motion by `dt`: the orbits turning, bodies on their way.
    /// Call every frame, drawn or not.
    pub fn update(&mut self, dt: f32) {
        self.age += dt;
        self.speed.update(dt);
        self.sun_in.update(dt);
        let speed = self.speed.get();
        self.spin = (self.spin + dt * speed * TAU / PLANET_TURN) % TAU;
        for b in &mut self.bodies {
            b.slot.update(dt);
            b.reach.update(dt);
            b.ghost.update(dt);
            b.spin = (b.spin + dt * speed * TAU / MOON_TURN) % TAU;
            for m in &mut b.moons {
                m.slot.update(dt);
                m.reach.update(dt);
                m.alpha.update(dt);
            }
            b.moons.retain(|m| !(m.leaving && m.reach.settled()));
        }
        self.particles.update(dt);
    }

    /// Where body `i`'s planet is, and how near (0 far, 1 near).
    fn planet_at(&self, geo: &Geo, i: usize) -> (Vec2, f32) {
        let b = &self.bodies[i];
        let theta = b.slot.get() + self.spin;
        let at = geo.on(geo.centre, geo.orbit_rx * b.reach.get(), theta);
        (at, (theta.sin() + 1.0) * 0.5)
    }

    /// Where moon `j` of body `i` is, and how near.
    fn moon_at(&self, geo: &Geo, i: usize, j: usize, planet: Vec2, near: f32) -> (Vec2, f32) {
        let b = &self.bodies[i];
        let m = &b.moons[j];
        let phi = m.slot.get() + b.spin * (1.0 - 0.25 * m.ring as f32);
        let r = geo.moon_orbit * (1.0 + 0.45 * m.ring as f32) * depth_scale(near);
        let at = geo.on(planet, r * m.reach.get(), phi);
        (at, near + phi.sin() * 0.08)
    }

    /// Draw into `field` and report a tap. `selected` is the wallet drawn lit,
    /// with its path home; `fade` is the whole view's opacity (a cross-fade to
    /// the list); `breathing` pulses the orbit, for a load still under way.
    pub fn show(
        &mut self,
        p: &Painter,
        field: Rect,
        selected: Option<&str>,
        fade: f32,
        breathing: bool,
    ) -> Option<OrbitTap> {
        self.bake(p);
        let geo = Geo::of(field);
        let c = p.theme.color;
        let rise = self.sun_in.get();
        let total: usize = self.bodies.iter().map(|b| b.live_moons().count()).sum();

        // The haze of the sun's light.
        let haze = geo.sun_r * 5.0;
        fill_gradient(
            Rect::new(
                geo.centre.x - haze,
                geo.centre.y - haze,
                haze * 2.0,
                haze * 2.0,
            ),
            haze,
            Gradient::Radial {
                centre: with_alpha(c.accent, 0.10 * fade * rise),
                edge: with_alpha(c.accent, 0.0),
            },
        );

        // The planets' orbit: the far half behind everything, the near half
        // over the sun.
        let sweep = Ease::CubicOut.apply((self.age - 0.2) / ORBIT_DRAW);
        let orbit_a = if breathing {
            0.14 + 0.08 * (self.age * 2.4).sin()
        } else {
            0.16
        } * fade;
        let ry = geo.orbit_rx * geo.tilt;
        ellipse(
            geo.centre,
            geo.orbit_rx,
            ry,
            PI,
            PI + PI * sweep,
            1.0,
            with_alpha(c.accent, orbit_a),
            false,
        );

        // Every body, with where it is this frame and how near.
        let mut items: Vec<(Item, Vec2, f32)> = vec![(Item::Sun, geo.centre, 0.5)];
        let mut planet_pos = Vec::with_capacity(self.bodies.len());
        for i in 0..self.bodies.len() {
            let (at, near) = self.planet_at(&geo, i);
            planet_pos.push((at, near));
            items.push((Item::Planet(i), at, near));
            for j in 0..self.bodies[i].moons.len() {
                let (m_at, m_near) = self.moon_at(&geo, i, j, at, near);
                items.push((Item::Moon(i, j), m_at, m_near));
            }
        }
        items.sort_by(|a, b| a.2.total_cmp(&b.2));

        // The path home of the wallet looked at: its planet, then the sun.
        let path: Option<(usize, usize)> = selected.and_then(|s| {
            self.bodies.iter().enumerate().find_map(|(i, b)| {
                b.moons
                    .iter()
                    .position(|m| m.address == s && !m.leaving)
                    .map(|j| (i, j))
            })
        });

        let mut front_orbit_drawn = false;
        for &(item, at, near) in &items {
            if !front_orbit_drawn && near >= 0.5 {
                front_orbit_drawn = true;
                ellipse(
                    geo.centre,
                    geo.orbit_rx,
                    ry,
                    0.0,
                    PI * sweep,
                    1.4,
                    with_alpha(c.accent, orbit_a * 1.6),
                    false,
                );
            }
            match item {
                Item::Sun => self.draw_sun(p, &geo, fade, rise),
                Item::Planet(i) => {
                    let lit = path.is_some_and(|(pi, _)| pi == i);
                    self.draw_planet(p, &geo, i, at, near, fade, lit);
                }
                Item::Moon(i, j) => {
                    let (pl, _) = planet_pos[i];
                    let lit = path == Some((i, j));
                    self.draw_moon(p, &geo, i, j, at, pl, near, fade, lit);
                }
            }
        }
        if !front_orbit_drawn {
            ellipse(
                geo.centre,
                geo.orbit_rx,
                ry,
                0.0,
                PI * sweep,
                1.4,
                with_alpha(c.accent, orbit_a * 1.6),
                false,
            );
        }

        // Names last, over every body, in order of importance: one that would
        // land on a name already placed is left out this frame.
        let mut labels = Labels::new(p, field);
        let me = self.identity.name.clone().unwrap_or_else(|| "You".into());
        labels.place(
            &me,
            geo.centre + vec2(0.0, geo.sun_r + 14.0),
            fade * rise,
            true,
        );
        if let Some((i, j)) = path {
            let (pl, near) = planet_pos[i];
            let (at, m_near) = self.moon_at(&geo, i, j, pl, near);
            let m = &self.bodies[i].moons[j];
            let r = geo.moon_r * depth_scale(m_near) * 1.25;
            labels.place(
                &m.label,
                at + vec2(0.0, r + 11.0),
                m.alpha.get() * fade,
                true,
            );
        }
        for (i, &(at, near)) in planet_pos.iter().enumerate() {
            let b = &self.bodies[i];
            let count = b.live_moons().count();
            let ghost = b.ghost.get();
            let name = b.family.display_name();
            let text = if count == 0 {
                format!("Link {name}")
            } else {
                name.to_string()
            };
            let r = geo.planet_r * depth_scale(near);
            let a = fade * b.reach.get().clamp(0.0, 1.0) * (1.0 - 0.35 * ghost);
            labels.place(&text, at + vec2(0.0, r + 13.0), a, false);
        }
        if total <= NAMED_UP_TO {
            for (i, b) in self.bodies.iter().enumerate() {
                for (j, m) in b.moons.iter().enumerate() {
                    if m.leaving || path == Some((i, j)) {
                        continue;
                    }
                    let (pl, near) = planet_pos[i];
                    let (at, m_near) = self.moon_at(&geo, i, j, pl, near);
                    if m_near < near {
                        continue;
                    }
                    let r = geo.moon_r * depth_scale(m_near);
                    labels.place(
                        &m.label,
                        at + vec2(0.0, r + 11.0),
                        m.alpha.get() * fade,
                        false,
                    );
                }
            }
        }

        // A launch landing bursts in its chain's colour.
        let mut bursts = Vec::new();
        for (i, b) in self.bodies.iter_mut().enumerate() {
            for (j, m) in b.moons.iter_mut().enumerate() {
                if m.burst && m.reach.settled() {
                    m.burst = false;
                    bursts.push((i, j, b.family));
                }
            }
        }
        for (i, j, family) in bursts {
            let (pl, near) = planet_pos[i];
            let (at, _) = self.moon_at(&geo, i, j, pl, near);
            let tint = chain_tint(&p.theme, family);
            self.particles.burst(at, 24, 120.0, 2.6, 0.9, tint);
        }
        self.particles.draw();

        // Taps: the nearest body under the finger, else the sky.
        let tap = p.tap.filter(|t| field.contains(*t))?;
        for &(item, at, near) in items.iter().rev() {
            let r = match item {
                Item::Sun => geo.sun_r,
                Item::Planet(_) => geo.planet_r * depth_scale(near),
                Item::Moon(..) => geo.moon_r * depth_scale(near),
            };
            if at.distance(tap) > HIT_R.max(r * 1.3) {
                continue;
            }
            return Some(match item {
                Item::Sun => OrbitTap::Sun,
                Item::Planet(i) => {
                    let b = &self.bodies[i];
                    OrbitTap::Planet {
                        family: b.family,
                        empty: b.live_moons().next().is_none(),
                    }
                }
                Item::Moon(i, j) => {
                    let m = &self.bodies[i].moons[j];
                    if m.leaving {
                        continue;
                    }
                    OrbitTap::Moon(m.address.clone())
                }
            });
        }
        Some(OrbitTap::Sky)
    }

    /// Bake the moon once, and a planet per chain, again if the theme's accent
    /// moved.
    fn bake(&mut self, p: &Painter) {
        let c = p.theme.color;
        if self.planet_accent != Some(c.accent) {
            self.planets.clear();
            self.planet_accent = Some(c.accent);
        }
        for b in &self.bodies {
            self.planets.entry(b.family).or_insert_with(|| {
                let tint = chain_tint(&p.theme, b.family);
                let look = PlanetLook {
                    ground: mix(c.bg_primary, tint, 0.28),
                    band: tint,
                    band2: mix(tint, c.text_primary, 0.4),
                    seed: 7 + b.family as u32 * 13,
                };
                bake_planet(&look, 256)
            });
        }
        if self.moon.is_none() {
            self.moon = Some(bake_moon(3, 96));
        }
    }

    fn draw_sun(&self, p: &Painter, geo: &Geo, fade: f32, rise: f32) {
        let c = p.theme.color;
        let r = geo.sun_r * (0.6 + 0.4 * rise);
        let a = fade * rise;
        let at = geo.centre;
        // The corona: the identity's light, breathing slowly.
        let pulse = 0.5 + 0.5 * (self.age * 1.3).sin();
        let corona = r * (1.9 + 0.1 * pulse);
        fill_gradient(
            Rect::new(at.x - corona, at.y - corona, corona * 2.0, corona * 2.0),
            corona,
            Gradient::Radial {
                centre: with_alpha(c.accent, 0.42 * a),
                edge: with_alpha(c.accent, 0.0),
            },
        );
        let face = Rect::new(at.x - r, at.y - r, r * 2.0, r * 2.0);
        match &self.identity.avatar {
            Some(tex) => fill_texture_rounded(tex, face, r, Color::new(1.0, 1.0, 1.0, a)),
            None => {
                fill_gradient(
                    face,
                    r,
                    Gradient::Radial {
                        centre: with_alpha(mix(c.accent, c.text_primary, 0.35), a),
                        edge: with_alpha(mix(c.accent, c.bg_primary, 0.35), a),
                    },
                );
                let initial: String = self
                    .identity
                    .name
                    .as_deref()
                    .and_then(|n| n.chars().find(|ch| ch.is_alphanumeric()))
                    .map_or("?".to_string(), |ch| ch.to_uppercase().collect());
                p.text_centred(
                    &initial,
                    face,
                    p.size(TextSize::Xl2),
                    with_alpha(c.bg_primary, 0.85 * a),
                );
            }
        }
        draw_circle_lines(at.x, at.y, r + 1.5, 2.0, with_alpha(c.accent, 0.9 * a));
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_planet(
        &self,
        p: &Painter,
        geo: &Geo,
        i: usize,
        at: Vec2,
        near: f32,
        fade: f32,
        lit: bool,
    ) {
        let c = p.theme.color;
        let b = &self.bodies[i];
        let reach = b.reach.get();
        if reach <= 0.01 {
            return;
        }
        let tint = chain_tint(&p.theme, b.family);
        let r = geo.planet_r * depth_scale(near) * reach.clamp(0.0, 1.0).sqrt();
        let ghost = b.ghost.get();
        let far = 0.75 + 0.25 * near;
        let a = fade * far * reach.clamp(0.0, 1.0);

        // Its tether to the sun: the chain's link to the reader.
        let to = at - geo.centre;
        if to.length() > geo.sun_r + r {
            let dir = to.normalize();
            let from = geo.centre + dir * geo.sun_r;
            let end = at - dir * r;
            let k = if lit { 0.5 } else { 0.12 } * (1.0 - 0.6 * ghost);
            draw_line(from.x, from.y, end.x, end.y, 1.0, with_alpha(tint, k * a));
        }

        // Its moons' rings, faint.
        let rings = b.live_moons().map(|m| m.ring + 1).max().unwrap_or(0);
        for k in 0..rings {
            let mr = geo.moon_orbit * (1.0 + 0.45 * k as f32) * depth_scale(near);
            ellipse(
                at,
                mr,
                mr * geo.tilt,
                0.0,
                TAU,
                1.0,
                with_alpha(tint, 0.14 * a),
                false,
            );
        }

        // The world, fading in from its ghost.
        if let Some(tex) = self.planets.get(&b.family)
            && ghost < 0.999
        {
            let size = r * 2.0 * ATMOSPHERE;
            draw_texture_ex(
                tex,
                at.x - size * 0.5,
                at.y - size * 0.5,
                Color::new(1.0, 1.0, 1.0, a * (1.0 - ghost)),
                DrawTextureParams {
                    dest_size: Some(vec2(size, size)),
                    ..Default::default()
                },
            );
        }
        if ghost > 0.001 {
            // A ghost: a dashed outline round the chain's mark, asking to be
            // linked.
            let ga = a * ghost;
            draw_circle(at.x, at.y, r, with_alpha(c.bg_primary, 0.6 * ga));
            ellipse(at, r, r, 0.0, TAU, 1.4, with_alpha(tint, 0.7 * ga), true);
            chain_mark(b.family, at, r * 0.9, with_alpha(tint, 0.8 * ga));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_moon(
        &self,
        p: &Painter,
        geo: &Geo,
        i: usize,
        j: usize,
        at: Vec2,
        planet: Vec2,
        near: f32,
        fade: f32,
        lit: bool,
    ) {
        let b = &self.bodies[i];
        let m = &b.moons[j];
        let tint = chain_tint(&p.theme, b.family);
        let r = geo.moon_r * depth_scale(near) * if lit { 1.25 } else { 1.0 };
        let a = m.alpha.get() * fade * (0.75 + 0.25 * near.clamp(0.0, 1.0));

        // Its tether to its planet.
        let to = at - planet;
        let pr = geo.planet_r * depth_scale(near);
        if to.length() > pr + r {
            let dir = to.normalize();
            let from = planet + dir * pr;
            let end = at - dir * r;
            let k = if lit { 0.6 } else { 0.14 };
            draw_line(from.x, from.y, end.x, end.y, 1.0, with_alpha(tint, k * a));
        }
        if lit {
            let pulse = 0.5 + 0.5 * (self.age * 3.0).sin();
            draw_circle_lines(
                at.x,
                at.y,
                r + 5.0 + pulse * 2.0,
                1.5,
                with_alpha(tint, 0.75 * a),
            );
            draw_circle(at.x, at.y, r + 9.0, with_alpha(tint, 0.10 * a));
        }
        if let Some(tex) = &self.moon {
            let shade = mix(tint, WHITE, 0.35);
            draw_texture_ex(
                tex,
                at.x - r,
                at.y - r,
                Color::new(shade.r, shade.g, shade.b, a),
                DrawTextureParams {
                    dest_size: Some(vec2(r * 2.0, r * 2.0)),
                    ..Default::default()
                },
            );
        }
    }
}

/// How large a body looks at `near` (0 far, 1 near).
fn depth_scale(near: f32) -> f32 {
    0.8 + 0.2 * near.clamp(0.0, 1.0)
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a.clamp(0.0, 1.0))
}

/// An arc of an ellipse from angle `a0` to `a1`, dashed when `dashed`.
#[allow(clippy::too_many_arguments)]
fn ellipse(c: Vec2, rx: f32, ry: f32, a0: f32, a1: f32, w: f32, color: Color, dashed: bool) {
    if a1 <= a0 || rx < 0.5 {
        return;
    }
    let n = ((a1 - a0) / TAU * (rx * 0.9).clamp(24.0, 160.0))
        .ceil()
        .max(2.0) as usize;
    let at = |t: f32| c + vec2(t.cos() * rx, t.sin() * ry);
    for i in 0..n {
        if dashed && i % 2 == 1 {
            continue;
        }
        let t0 = a0 + (a1 - a0) * i as f32 / n as f32;
        let t1 = a0 + (a1 - a0) * (i + 1) as f32 / n as f32;
        let (p0, p1) = (at(t0), at(t1));
        draw_line(p0.x, p0.y, p1.x, p1.y, w, color);
    }
}

/// Names under bodies, each on a dark plate so it reads over the orbits, kept
/// inside the field near its edge, placed in the order asked: a name whose
/// plate would land on one already placed is left out.
struct Labels<'a, 'p> {
    p: &'a Painter<'p>,
    field: Rect,
    placed: Vec<Rect>,
}

impl<'a, 'p> Labels<'a, 'p> {
    fn new(p: &'a Painter<'p>, field: Rect) -> Self {
        Self {
            p,
            field,
            placed: Vec::new(),
        }
    }

    /// The plate `label` takes, centred under `top_mid`.
    fn plate(&self, label: &str, top_mid: Vec2) -> Rect {
        let size = self.p.size(TextSize::Sm);
        let w = self.p.measure(label, size).width + 14.0;
        let f = self.field;
        let x = (top_mid.x - w * 0.5).clamp(f.x + 6.0, (f.right() - w - 6.0).max(f.x));
        Rect::new(x, top_mid.y - 9.0, w, 18.0)
    }

    /// Place `label` under `top_mid`, unless it would land on one placed.
    fn place(&mut self, label: &str, top_mid: Vec2, a: f32, strong: bool) {
        if a <= 0.01 {
            return;
        }
        let plate = self.plate(label, top_mid);
        let grown = Rect::new(plate.x - 2.0, plate.y - 2.0, plate.w + 4.0, plate.h + 4.0);
        if self.placed.iter().any(|r| r.overlaps(&grown)) {
            return;
        }
        self.placed.push(plate);
        let p = self.p;
        let c = p.theme.color;
        fill_rounded(plate, 9.0, with_alpha(c.bg_primary, 0.62 * a));
        let ink = if strong {
            c.text_primary
        } else {
            c.text_secondary
        };
        p.text_in(
            label,
            plate,
            p.size(TextSize::Sm),
            with_alpha(ink, a),
            Align::Center,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wallet(chain: &str, address: &str) -> LinkedWallet {
        LinkedWallet {
            chain: chain.into(),
            address: address.into(),
            handle: None,
        }
    }

    /// Every linkable chain is a planet, linked on or not; a chain only a
    /// link names joins them. `planet_order` sorts, so the order is
    /// `ChainFamily`'s own — not the order the links arrived in.
    #[test]
    fn every_linkable_chain_is_a_planet_and_others_join_them() {
        assert_eq!(
            planet_order(&[]),
            [
                ChainFamily::Cardano,
                ChainFamily::Evm,
                ChainFamily::Midnight
            ]
        );
        let ws = [wallet("solana", "So1")];
        assert_eq!(
            planet_order(&ws),
            [
                ChainFamily::Cardano,
                ChainFamily::Evm,
                ChainFamily::Solana,
                ChainFamily::Midnight
            ]
        );
    }

    #[test]
    fn moons_fill_a_ring_then_the_next() {
        assert_eq!(moon_slot(0, 3).0, 0);
        assert_eq!(moon_slot(4, 7).0, 0);
        assert_eq!(moon_slot(5, 7).0, 1);
        let gap = shortest(moon_slot(1, 3).1 - moon_slot(0, 3).1);
        assert!((gap - TAU / 3.0).abs() < 1e-5);
        // The second ring spaces only the moons on it: two, half a turn
        // apart (either way round).
        let gap2 = shortest(moon_slot(6, 7).1 - moon_slot(5, 7).1);
        assert!((gap2.abs() - PI).abs() < 1e-5, "{gap2}");
    }

    #[test]
    fn the_short_way_round_is_never_more_than_half_a_turn() {
        for d in [-7.0, -3.2, 0.0, 3.2, 7.0] {
            assert!(shortest(d).abs() <= PI + 1e-5, "{d}");
        }
    }

    #[test]
    fn a_handle_is_the_label_and_an_address_the_fallback() {
        let mut w = wallet(
            "cardano:mainnet",
            "stake1u8962x3wtddcq2syq258ka3d9mxxkx5md5xawzx67pac9tgc5rhq9",
        );
        assert_eq!(wallet_label(&w), "stake1…rhq9");
        w.handle = Some("boef".into());
        assert_eq!(wallet_label(&w), "$boef");
    }

    /// On a phone, a planet at the side of its orbit keeps its moons on the
    /// screen.
    #[test]
    fn the_system_fits_a_phone() {
        let field = Rect::new(0.0, 0.0, 390.0, 600.0);
        let geo = Geo::of(field);
        let outer = geo.centre.x + geo.orbit_rx + geo.moon_orbit + geo.moon_r;
        assert!(outer <= field.right(), "{outer}");
        assert!(geo.orbit_rx > geo.sun_r + geo.moon_orbit);
    }
}
