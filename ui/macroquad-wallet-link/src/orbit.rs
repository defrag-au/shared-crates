//! The orbit view: the reader's Discord account as a planet, every wallet
//! linked to it a moon on a tilted ring, tethered to it.
//!
//! The view holds only motion: where each moon is on its way to its slot, how
//! far round each ring has turned, how far the planet has risen. The wallets
//! themselves are the flow's; [`OrbitView::sync`] is told the list each frame
//! and moves moons in, round and out to match it. Time is the `dt` handed in,
//! so a capture at `--at` is the same picture every run.

use std::f32::consts::{PI, TAU};

use chains::ChainFamily;
use macroquad::prelude::*;
use macroquad_gamekit::wallet_modal::{chain_mark, chain_tint};
use macroquad_gamekit::{Animated, Ease, Particles};
use macroquad_widgets::{Align, Gradient, Painter, TextSize, fill_gradient, fill_rounded};
use wallet_link_flow::family_of;
use wallet_link_flow::wallet_link::LinkedWallet;

use crate::planet::{ATMOSPHERE, PlanetLook, bake_moon, bake_planet, mix};

/// The flattest the rings look: their height over their width, on a wide
/// field. A tall one tilts them further towards the reader (see [`Geo`]).
pub const TILT: f32 = 0.36;
/// The most rings drawn; a longer list crowds the outer one.
const MAX_RINGS: usize = 4;
/// A moon's radius, and the radius of the one selected.
const MOON_R: f32 = 18.0;
const MOON_R_SELECTED: f32 = 22.0;
/// A finger's worth round a moon.
const HIT_R: f32 = 22.0;
/// Seconds ring 0 takes to turn once; each ring further out takes this much
/// longer.
const TURN_SECS: f32 = 70.0;
const TURN_STEP: f32 = 25.0;

const PLANET_RISE: f32 = 0.7;
const RING_DRAW: f32 = 0.9;
const RING_STAGGER: f32 = 0.12;
const EMERGE_SECS: f32 = 0.8;
const EMERGE_STAGGER: f32 = 0.09;
const LAUNCH_SECS: f32 = 0.9;
const LEAVE_SECS: f32 = 0.6;
const RESPACE_SECS: f32 = 0.8;
const PAUSE_SECS: f32 = 0.6;
const RESUME_SECS: f32 = 0.9;

/// Moons a ring holds before the next ring out is used.
fn capacity(ring: usize) -> usize {
    3 + 2 * ring
}

/// Where each of `n` moons goes: its ring, and its angle on it. Inner rings
/// fill first; the last ring takes whatever is left over.
pub fn slots(n: usize) -> Vec<(usize, f32)> {
    let mut out = Vec::with_capacity(n);
    let mut left = n;
    let mut ring = 0;
    while left > 0 {
        let here = if ring + 1 == MAX_RINGS {
            left
        } else {
            left.min(capacity(ring))
        };
        // Each ring starts a little further round, so moons on neighbouring
        // rings do not line up in spokes.
        let phase = -PI * 0.5 + ring as f32 * 0.9;
        for i in 0..here {
            out.push((ring, phase + i as f32 * TAU / here as f32));
        }
        left -= here;
        ring += 1;
    }
    out
}

/// The rings `n` moons use, at least one.
pub fn rings_for(n: usize) -> usize {
    slots(n).last().map_or(1, |(r, _)| r + 1)
}

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

/// The colour a wallet's moon is: its chain's.
pub fn moon_tint(p: &Painter, family: Option<ChainFamily>) -> Color {
    family.map_or(p.theme.color.text_secondary, |f| chain_tint(&p.theme, f))
}

struct Moon {
    address: String,
    family: Option<ChainFamily>,
    label: String,
    ring: usize,
    /// The angle of its slot on its ring, before the ring's turn.
    slot: Animated<f32>,
    /// 0 at the planet's centre, 1 on its ring, past 1 leaving.
    reach: Animated<f32>,
    alpha: Animated<f32>,
    leaving: bool,
    /// Launched by a link just made: bursts on arrival.
    launched: bool,
}

/// The planet's geometry in a field.
#[derive(Clone, Copy, Debug)]
pub struct Geo {
    pub centre: Vec2,
    pub planet_r: f32,
    /// The widest a ring may be.
    pub max_rx: f32,
    /// How flat the rings look, their height over their width: flatter on a
    /// wide field, rounder on a tall one, so a phone's height is used.
    pub tilt: f32,
}

impl Geo {
    pub fn of(field: Rect) -> Self {
        let planet_r = (field.w.min(field.h) * 0.17).clamp(46.0, 96.0);
        let centre = vec2(field.center().x, field.y + field.h * 0.47);
        let tilt = (field.h / field.w.max(1.0) * 0.38).clamp(TILT, 0.58);
        let max_rx = (field.w * 0.5 - 34.0)
            .min((field.h * 0.5 - 44.0) / tilt)
            .min(planet_r * 4.2)
            .max(planet_r * 2.1);
        Self {
            centre,
            planet_r,
            max_rx,
            tilt,
        }
    }

    /// Ring `k`'s width when `rings` are in use.
    pub fn ring_rx(&self, k: usize, rings: usize) -> f32 {
        let inner = self.planet_r * 1.9;
        if rings <= 1 {
            return (self.max_rx * 0.9).max(inner);
        }
        inner + (self.max_rx - inner) * k as f32 / (rings - 1) as f32
    }
}

/// What a tap on the orbit hit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrbitTap {
    Moon(String),
    /// Open sky: a tap that should let go of a selection.
    Sky,
}

/// See the [module docs](self).
pub struct OrbitView {
    moons: Vec<Moon>,
    spins: [f32; MAX_RINGS],
    speed: Animated<f32>,
    ring_rx: [Animated<f32>; MAX_RINGS],
    rings_sized: bool,
    planet_in: Animated<f32>,
    /// Seconds since the view first drew: the rings' and moons' entrance.
    age: f32,
    /// Seconds since the list first arrived.
    loaded_age: Option<f32>,
    planet: Option<(Texture2D, Color)>,
    moon: Option<Texture2D>,
    particles: Particles,
    paused: bool,
}

impl Default for OrbitView {
    fn default() -> Self {
        Self::new()
    }
}

impl OrbitView {
    pub fn new() -> Self {
        let mut planet_in = Animated::new(0.0);
        planet_in.to(1.0, PLANET_RISE, Ease::ExpoOut);
        Self {
            moons: Vec::new(),
            spins: [0.0; MAX_RINGS],
            speed: Animated::new(1.0),
            ring_rx: [Animated::new(0.0); MAX_RINGS],
            rings_sized: false,
            planet_in,
            age: 0.0,
            loaded_age: None,
            planet: None,
            moon: None,
            particles: Particles::new(11, 0.0),
            paused: false,
        }
    }

    /// Hold the rings still (a moon is being looked at), or let them turn.
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

    /// Match the moons to `wallets`: new ones rise from the planet (launched
    /// when `launched` names them, emerging in turn on the first list), gone
    /// ones fly off, and the rest re-space round their rings.
    pub fn sync(&mut self, wallets: &[LinkedWallet], launched: Option<&str>) {
        let first = self.loaded_age.is_none();
        if first {
            self.loaded_age = Some(0.0);
        }
        for m in &mut self.moons {
            if !m.leaving && !wallets.iter().any(|w| w.address == m.address) {
                m.leaving = true;
                m.reach.to(1.8, LEAVE_SECS, Ease::CubicIn);
                m.alpha.to(0.0, LEAVE_SECS, Ease::CubicIn);
            }
        }
        for (i, w) in wallets.iter().enumerate() {
            if let Some(m) = self.moons.iter_mut().find(|m| m.address == w.address && !m.leaving) {
                // A handle resolved since: the label follows.
                m.label = wallet_label(w);
                continue;
            }
            let mut reach = Animated::new(0.0);
            let mut alpha = Animated::new(0.0);
            let is_launch = launched == Some(w.address.as_str());
            let delay = if first {
                // After the planet has risen and the rings have drawn.
                0.45 + i as f32 * EMERGE_STAGGER
            } else {
                0.0
            };
            let (secs, ease) = if is_launch {
                (LAUNCH_SECS, Ease::BackOut)
            } else {
                (EMERGE_SECS, Ease::BackOut)
            };
            reach.to_after(1.0, delay, secs, ease);
            alpha.to_after(1.0, delay, secs * 0.4, Ease::CubicOut);
            self.moons.push(Moon {
                address: w.address.clone(),
                family: family_of(w),
                label: wallet_label(w),
                ring: 0,
                slot: Animated::new(f32::NAN),
                reach,
                alpha,
                leaving: false,
                launched: is_launch,
            });
        }
        // Slots in the list's own order: the server's, which is the reader's.
        let order: Vec<usize> = wallets
            .iter()
            .filter_map(|w| {
                self.moons
                    .iter()
                    .position(|m| m.address == w.address && !m.leaving)
            })
            .collect();
        let slots = slots(order.len());
        let spins = self.spins;
        for (k, &i) in order.iter().enumerate() {
            let (ring, angle) = slots[k];
            let ring_spin = spins[ring];
            let m = &mut self.moons[i];
            let fresh = m.slot.get().is_nan();
            if fresh {
                // A new moon starts at its own slot.
                m.ring = ring;
                m.slot.set(angle);
                continue;
            }
            if m.ring != ring {
                // Moving rings: keep where it is on screen, in the new ring's
                // turn, then ease to the new slot.
                let at = m.slot.get() + spins[m.ring];
                m.slot.set(at - ring_spin);
                m.ring = ring;
            }
            let now = m.slot.target();
            let target = now + shortest(angle - now);
            if (target - now).abs() > 1e-4 {
                m.slot.to(target, RESPACE_SECS, Ease::CubicInOut);
            }
        }
    }

    /// Whether anything is still moving that a capture should wait out.
    pub fn settled(&self) -> bool {
        self.planet_in.settled()
            && self
                .moons
                .iter()
                .all(|m| m.reach.settled() && m.slot.settled())
    }

    /// Advance, draw into `field`, and report a tap. `selected` is drawn
    /// lit; `fade` is the whole view's opacity (a cross-fade to the list).
    /// `breathing` draws the rings pulsing, for a load still under way.
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
        let live: Vec<usize> = (0..self.moons.len())
            .filter(|i| !self.moons[*i].leaving)
            .collect();
        let rings = rings_for(live.len());
        for k in 0..MAX_RINGS {
            let target = if k < rings { geo.ring_rx(k, rings) } else { 0.0 };
            if !self.rings_sized {
                self.ring_rx[k].set(target);
            } else if (self.ring_rx[k].target() - target).abs() > 0.5 {
                self.ring_rx[k].to(target, RESPACE_SECS, Ease::CubicInOut);
            }
        }
        self.rings_sized = true;

        let c = p.theme.color;
        let accent = c.accent;
        let ring_ink = |k: usize, front: bool| {
            let base = if breathing {
                0.14 + 0.08 * (self.age * 2.4 - k as f32 * 0.6).sin()
            } else {
                0.16
            };
            let a = if front { base * 1.6 } else { base };
            Color::new(accent.r, accent.g, accent.b, a * fade)
        };
        let sweep = |k: usize| {
            let t = (self.age - 0.25 - k as f32 * RING_STAGGER) / RING_DRAW;
            Ease::CubicOut.apply(t)
        };
        let empty = live.is_empty() && self.loaded_age.is_some();
        let rise = self.planet_in.get();

        // A haze of the accent round the planet: the system's light.
        let haze = geo.planet_r * 3.4;
        fill_gradient(
            Rect::new(
                geo.centre.x - haze,
                geo.centre.y - haze,
                haze * 2.0,
                haze * 2.0,
            ),
            haze,
            Gradient::Radial {
                centre: with_alpha(accent, 0.09 * fade * rise),
                edge: with_alpha(accent, 0.0),
            },
        );

        // The far halves of the rings, behind everything.
        for k in 0..MAX_RINGS {
            let rx = self.ring_rx[k].get();
            if rx < 1.0 {
                continue;
            }
            let s = sweep(k);
            ellipse(geo.centre, rx, rx * geo.tilt, PI, PI + PI * s, 1.0, ring_ink(k, false), empty);
        }

        // The moons, nearest last, each with where it is this frame.
        let placed: Vec<(usize, Vec2, f32, f32)> = (0..self.moons.len())
            .map(|i| {
                let m = &self.moons[i];
                let theta = m.slot.get() + self.spins[m.ring];
                let rx = self.ring_rx[m.ring].get().max(geo.planet_r * 1.9) * m.reach.get();
                let at = geo.centre + vec2(theta.cos() * rx, theta.sin() * rx * geo.tilt);
                let depth = (theta.sin() + 1.0) * 0.5;
                (i, at, depth, theta.sin())
            })
            .collect();
        let mut back: Vec<_> = placed.iter().filter(|q| q.3 < 0.0).copied().collect();
        let mut front: Vec<_> = placed.iter().filter(|q| q.3 >= 0.0).copied().collect();
        back.sort_by(|a, b| a.2.total_cmp(&b.2));
        front.sort_by(|a, b| a.2.total_cmp(&b.2));

        for &(i, at, depth, _) in &back {
            self.draw_tether(p, &geo, i, at, selected, fade, true);
            self.draw_moon(p, i, at, depth, selected, fade);
        }

        // The planet, risen.
        if let Some((tex, _)) = &self.planet {
            let size = geo.planet_r * 2.0 * ATMOSPHERE * (0.6 + 0.4 * rise);
            let dim = if breathing { 0.9 } else { 1.0 };
            draw_texture_ex(
                tex,
                geo.centre.x - size * 0.5,
                geo.centre.y - size * 0.5,
                Color::new(dim, dim, dim, rise * fade),
                DrawTextureParams {
                    dest_size: Some(vec2(size, size)),
                    ..Default::default()
                },
            );
        }

        for k in 0..MAX_RINGS {
            let rx = self.ring_rx[k].get();
            if rx < 1.0 {
                continue;
            }
            let s = sweep(k);
            ellipse(geo.centre, rx, rx * geo.tilt, 0.0, PI * s, 1.4, ring_ink(k, true), empty);
        }

        for &(i, at, depth, _) in &front {
            self.draw_tether(p, &geo, i, at, selected, fade, false);
            self.draw_moon(p, i, at, depth, selected, fade);
        }
        // Labels last, over every body: the near half's, and the selected one.
        for &(i, at, depth, sin) in back.iter().chain(front.iter()) {
            let m = &self.moons[i];
            let is_sel = selected == Some(m.address.as_str());
            if sin < -0.15 && !is_sel {
                continue;
            }
            let r = moon_r(depth, is_sel);
            let a = m.alpha.get() * fade * if sin < 0.0 { 0.7 } else { 1.0 };
            draw_label(p, &m.label, at + vec2(0.0, r + 13.0), a, is_sel);
        }

        // A launch landing bursts in its chain's colour.
        for &(i, at, _, _) in &placed {
            let m = &mut self.moons[i];
            if m.launched && m.reach.settled() {
                m.launched = false;
                let tint = moon_tint(p, m.family);
                self.particles.burst(at, 26, 150.0, 3.0, 0.9, tint);
            }
        }
        self.particles.draw();

        if empty {
            let size = p.size(TextSize::Md);
            let at = Rect::new(
                field.x,
                geo.centre.y + geo.ring_rx(0, 1) * geo.tilt + 18.0,
                field.w,
                24.0,
            );
            p.text_in(
                "No wallets linked yet",
                at,
                size,
                with_alpha(c.text_muted, fade * rise),
                Align::Center,
            );
        }

        // Taps: the nearest moon under the finger, else the sky.
        let tap = p.tap.filter(|t| field.contains(*t))?;
        let hit = front
            .iter()
            .rev()
            .chain(back.iter().rev())
            .find(|(i, at, depth, _)| {
                let m = &self.moons[*i];
                !m.leaving
                    && at.distance(tap)
                        <= HIT_R.max(moon_r(*depth, selected == Some(m.address.as_str())) * 1.4)
            })
            .map(|(i, ..)| self.moons[*i].address.clone());
        Some(hit.map_or(OrbitTap::Sky, OrbitTap::Moon))
    }

    /// Advance the motion by `dt`: the rings turning, moons on their way.
    /// Call every frame, drawn or not.
    pub fn update(&mut self, dt: f32) {
        self.age += dt;
        if let Some(a) = &mut self.loaded_age {
            *a += dt;
        }
        self.speed.update(dt);
        self.planet_in.update(dt);
        for r in &mut self.ring_rx {
            r.update(dt);
        }
        let speed = self.speed.get();
        for (k, spin) in self.spins.iter_mut().enumerate() {
            *spin = (*spin + dt * speed * TAU / (TURN_SECS + TURN_STEP * k as f32)) % TAU;
        }
        for m in &mut self.moons {
            m.slot.update(dt);
            m.reach.update(dt);
            m.alpha.update(dt);
        }
        self.moons.retain(|m| !(m.leaving && m.reach.settled()));
        self.particles.update(dt);
    }

    /// Bake the planet and the moon on the first frame, and the planet again
    /// if the theme's accent moved.
    fn bake(&mut self, p: &Painter) {
        let c = p.theme.color;
        if self.planet.as_ref().is_none_or(|(_, a)| *a != c.accent) {
            let look = PlanetLook {
                ground: mix(c.bg_primary, c.accent_green, 0.22),
                band: c.accent,
                band2: c.accent_cyan,
                seed: 7,
            };
            self.planet = Some((bake_planet(&look, 512), c.accent));
        }
        if self.moon.is_none() {
            self.moon = Some(bake_moon(3, 128));
        }
    }

    fn draw_moon(
        &self,
        p: &Painter,
        i: usize,
        at: Vec2,
        depth: f32,
        selected: Option<&str>,
        fade: f32,
    ) {
        let m = &self.moons[i];
        let is_sel = selected == Some(m.address.as_str());
        let r = moon_r(depth, is_sel);
        let far = 0.7 + 0.3 * depth;
        let a = m.alpha.get() * fade * far;
        let tint = moon_tint(p, m.family);
        if is_sel {
            let pulse = 0.5 + 0.5 * (self.age * 3.0).sin();
            draw_circle_lines(at.x, at.y, r + 6.0 + pulse * 2.0, 1.5, with_alpha(tint, 0.7 * a));
            draw_circle(at.x, at.y, r + 10.0, with_alpha(tint, 0.10 * a));
        }
        if let Some(tex) = &self.moon {
            let lit = mix(tint, WHITE, 0.18);
            draw_texture_ex(
                tex,
                at.x - r,
                at.y - r,
                Color::new(lit.r, lit.g, lit.b, a),
                DrawTextureParams {
                    dest_size: Some(vec2(r * 2.0, r * 2.0)),
                    ..Default::default()
                },
            );
        }
        if let Some(f) = m.family {
            chain_mark(f, at, r * 1.0, with_alpha(p.theme.color.bg_primary, 0.78 * a));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_tether(
        &self,
        p: &Painter,
        geo: &Geo,
        i: usize,
        at: Vec2,
        selected: Option<&str>,
        fade: f32,
        behind: bool,
    ) {
        let m = &self.moons[i];
        let is_sel = selected == Some(m.address.as_str());
        let to = at - geo.centre;
        let len = to.length();
        if len < geo.planet_r * 1.02 {
            return;
        }
        let dir = to / len;
        // From the planet's limb, not its centre: the tether is drawn over
        // the sky, never across the face.
        let from = if behind {
            geo.centre + dir * geo.planet_r * 0.2
        } else {
            geo.centre + dir * geo.planet_r * 1.02
        };
        let a = m.alpha.get() * fade * if is_sel { 0.45 } else { 0.10 };
        let tint = moon_tint(p, m.family);
        draw_line(from.x, from.y, at.x, at.y, 1.0, with_alpha(tint, a));
    }
}

/// A moon's radius at `depth` (0 far, 1 near).
fn moon_r(depth: f32, selected: bool) -> f32 {
    let base = if selected { MOON_R_SELECTED } else { MOON_R };
    base * (0.82 + 0.18 * depth)
}

/// The angle `d` brought into −π..π: the short way round.
fn shortest(d: f32) -> f32 {
    (d + PI).rem_euclid(TAU) - PI
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a.clamp(0.0, 1.0))
}

/// An arc of an ellipse from angle `a0` to `a1`, dashed when `dashed`.
#[allow(clippy::too_many_arguments)]
fn ellipse(c: Vec2, rx: f32, ry: f32, a0: f32, a1: f32, w: f32, color: Color, dashed: bool) {
    if a1 <= a0 {
        return;
    }
    let n = ((a1 - a0) / TAU * 160.0).ceil().max(2.0) as usize;
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

/// A moon's name under it, on a dark plate so it reads over the rings.
fn draw_label(p: &Painter, label: &str, top_mid: Vec2, a: f32, selected: bool) {
    let c = p.theme.color;
    let size = p.size(TextSize::Sm);
    let w = p.measure(label, size).width + 14.0;
    let plate = Rect::new(top_mid.x - w * 0.5, top_mid.y - 9.0, w, 18.0);
    fill_rounded(plate, 9.0, with_alpha(c.bg_primary, 0.62 * a));
    let ink = if selected { c.text_primary } else { c.text_secondary };
    p.text_in(label, plate, size, with_alpha(ink, a), Align::Center);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inner_rings_fill_first_and_the_last_takes_the_rest() {
        let rings = |n| slots(n).iter().map(|s| s.0).collect::<Vec<_>>();
        assert_eq!(rings(3), [0, 0, 0]);
        assert_eq!(rings(4), [0, 0, 0, 1]);
        assert_eq!(rings_for(0), 1);
        assert_eq!(rings_for(8), 2);
        assert_eq!(rings_for(9), 3);
        // 3 + 5 + 7 fill three rings; the fourth takes every one after.
        assert_eq!(rings_for(40), MAX_RINGS);
        assert_eq!(slots(40).len(), 40);
    }

    #[test]
    fn moons_on_a_ring_are_spaced_evenly() {
        let s = slots(3);
        let gap = shortest(s[1].1 - s[0].1);
        assert!((gap - TAU / 3.0).abs() < 1e-5);
    }

    #[test]
    fn the_short_way_round_is_never_more_than_half_a_turn() {
        for d in [-7.0, -3.2, 0.0, 3.2, 7.0] {
            assert!(shortest(d).abs() <= PI + 1e-5, "{d}");
        }
        assert!((shortest(TAU - 0.1) + 0.1).abs() < 1e-5);
    }

    #[test]
    fn a_handle_is_the_label_and_an_address_the_fallback() {
        let w = |handle: Option<&str>| LinkedWallet {
            chain: "cardano:mainnet".into(),
            address: "stake1u8962x3wtddcq2syq258ka3d9mxxkx5md5xawzx67pac9tgc5rhq9".into(),
            handle: handle.map(Into::into),
        };
        assert_eq!(wallet_label(&w(Some("boef"))), "$boef");
        assert_eq!(wallet_label(&w(Some("$boef"))), "$boef");
        assert_ne!(wallet_label(&w(None)), w(None).address, "shortened");
    }

    #[test]
    fn rings_fit_the_field_on_a_phone() {
        let geo = Geo::of(Rect::new(0.0, 0.0, 390.0, 560.0));
        let outer = geo.ring_rx(2, 3);
        assert!(outer <= 390.0 * 0.5 - 34.0, "{outer}");
        assert!(geo.ring_rx(0, 3) > geo.planet_r * 1.5);
    }
}
