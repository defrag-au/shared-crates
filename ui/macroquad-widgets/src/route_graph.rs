//! `RouteGraph` — an amount splitting across several routes and joining again: a source node fanning out through pipes as thick as each share, one card per route, then a sink node; and `route_strip`, the same split as one labelled bar.
//!
//! Built for a swap split across venues (ADA into Splash and a CSWAP order,
//! out as one token), but domain-free: the host words every card and picks
//! every colour, the widget lays them out.
//!
//! ## The form
//!
//! - **Thickness is share.** A pipe is `2 + 14 × share` points across, and
//!   pipes stack where they meet a node, Sankey-style, so the node's edge reads
//!   as the whole amount and the eye compares parts without reading a number.
//! - **Horizontal tangents.** Each pipe leaves the source and enters its card
//!   flat, so pipes fan rather than cross however many there are.
//! - **A pipe's strength is where its leg is.** Planned or sent: solid.
//!   Waiting on someone else (a batcher): faint, still to fill. Done: strong.
//!   Closed without finishing, or failed: faintest (and grey, for a failure).
//!   The status dot where the pipe meets its card says the same in colour:
//!   hollow while it is out of the reader's hands, green when done.
//! - **A declined route is still drawn.** A [`RouteCard`] with `declined` set
//!   is faded, its pipe a dotted line, its share `0%`, and its line says why. A
//!   route weighed and refused is not the same as no route: hiding it leaves
//!   the reader asking why the split never happened.
//! - **The meter is the host's.** An optional bar on a card with words beside
//!   it: a swap shows the route's own price impact, ticks at the host's marks,
//!   the fill in the warning colour when the host flags it.
//! - **Still, and plain.** Nothing moves and nothing is shaded: thickness
//!   carries the split, strength carries the state, and a still picture is
//!   the same picture in every capture.
//!
//! ## Who owns what
//!
//! Stateless: a function of the VM. Text goes through the [`Painter`], so a
//! capture report sees it.

use macroquad::prelude::*;
use ui_theme::{Ink, TextSize, Token};

use crate::painter::{Painter, draw_rounded_rect, with_alpha};

/// Wide enough for a six-letter ticker (`ALIENS`) at the smallest size.
const NODE_R: f32 = 25.0;
/// Node centres, in from the area's left and right edges.
const NODE_X: f32 = 26.0;
/// Sides on a node's disc: macroquad's own circle is 20, which facets
/// visibly at this size on a phone's pixel density.
const NODE_SIDES: u8 = 64;
/// The gap between a node's disc and its figure.
const FIGURE_GAP: f32 = 7.0;
/// Cards' left and right edges, in from the area's.
const CARD_INSET: f32 = 80.0;
const CARD_GAP: f32 = 8.0;
const CARD_PAD: f32 = 8.0;
const CARD_R: f32 = 9.0;
/// A card with room for its meter, and one without, from three cards up: four
/// tall cards would push a phone sheet's Close off the screen.
const CARD_H_FULL: f32 = 58.0;
const CARD_H_COMPACT: f32 = 44.0;
const TOP: f32 = 6.0;
/// Room under the nodes for their figures.
const BOTTOM: f32 = 24.0;
const PIPE_MIN: f32 = 2.0;
const PIPE_SPAN: f32 = 14.0;
/// Between a card's lines.
const LINE_GAP: f32 = 4.0;
const METER_H: f32 = 4.0;
/// The meter's share of a card's inner width.
const METER_W: f32 = 0.4;
/// A declined card's opacity.
const DECLINED_ALPHA: f32 = 0.45;

/// The strip's height: its legend, then its bar.
pub const ROUTE_STRIP_H: f32 = 26.0;
const STRIP_BAR_H: f32 = 6.0;

/// Where one route is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LegState {
    /// Planned; nothing sent. A solid pipe, a solid dot.
    #[default]
    Idle,
    /// On its way. A solid pipe, a hollow dot: out of the reader's hands.
    Pending,
    /// Waiting on someone else to finish it (a batcher, a counterparty). A
    /// faint pipe, still to fill; a hollow dot.
    Waiting,
    /// Done as planned. A strong pipe, a green dot.
    Done,
    /// Ended, but not as planned. The faintest pipe, a dot in the warning
    /// colour.
    Warn,
    /// Failed. Every pipe faintest and grey, a red dot.
    Failed,
}

impl LegState {
    /// How opaque the pipe is: how complete its leg reads.
    fn fill(self) -> f32 {
        match self {
            LegState::Idle | LegState::Pending => 0.55,
            LegState::Waiting => 0.14,
            LegState::Done => 0.85,
            LegState::Warn | LegState::Failed => 0.1,
        }
    }

    /// Out of the reader's hands, and not finished.
    fn in_flight(self) -> bool {
        matches!(self, LegState::Pending | LegState::Waiting)
    }
}

/// A bar on a card: how far `fill` reaches, ticks at the host's marks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RouteMeter {
    /// 0..=1 of the bar.
    pub fill: f32,
    /// Marks along the bar, each 0..=1.
    pub ticks: Vec<f32>,
    /// Fill in the warning colour.
    pub warn: bool,
    /// Words at the bar's right: `impact 3.1%`.
    pub label: String,
}

/// One route's card.
#[derive(Clone, Debug, PartialEq)]
pub struct RouteCard {
    /// The route's name, drawn in `ink`: `Splash`.
    pub label: String,
    /// Of the whole, 0..=1: the pipe's thickness. A declined card's share is
    /// not drawn.
    pub share: f32,
    /// The share as printed: `91.3%`.
    pub share_text: String,
    /// The line under the name: what goes in and what comes out.
    pub detail: String,
    /// The route's own colour: its name, its pipe, its strip segment.
    pub ink: Token,
    pub state: LegState,
    pub meter: Option<RouteMeter>,
    /// Weighed and not taken: drawn faded with a dotted line for a pipe.
    /// Declined cards go after every taken one.
    pub declined: bool,
}

/// A source or sink node.
#[derive(Clone, Debug, PartialEq)]
pub struct RouteNode {
    /// Inside the disc; up to six letters fit.
    pub name: String,
    /// Under the disc: `1,000`, `4.75M`.
    pub figure: String,
    pub ink: Token,
}

pub struct RouteGraphVm<'a> {
    pub source: &'a RouteNode,
    pub sink: &'a RouteNode,
    pub cards: &'a [RouteCard],
}

fn card_h(cards: usize) -> f32 {
    if cards <= 2 {
        CARD_H_FULL
    } else {
        CARD_H_COMPACT
    }
}

/// How tall the graph is with `cards` cards, for a host making room.
pub fn route_graph_height(cards: usize) -> f32 {
    let n = cards.max(1);
    let stack = n as f32 * card_h(n) + (n - 1) as f32 * CARD_GAP;
    TOP + stack.max(NODE_R * 2.0) + BOTTOM
}

fn ink(p: &Painter, token: Token) -> Color {
    Ink::Token(token).resolve(&p.theme)
}

fn faded(c: Color, a: f32) -> Color {
    with_alpha(c, c.a * a)
}

/// `a` towards `b` by `t`, alpha included.
fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

const WHITE: Color = Color::new(1.0, 1.0, 1.0, 1.0);
const BLACK: Color = Color::new(0.0, 0.0, 0.0, 1.0);

/// `s` cut to `max_w` at `size`, with an ellipsis when cut.
fn fit(p: &Painter, s: &str, size: f32, max_w: f32) -> String {
    if p.measure(s, size).width <= max_w {
        return s.to_string();
    }
    let mut out = String::new();
    for ch in s.chars() {
        let candidate = format!("{out}{ch}…");
        if p.measure(&candidate, size).width > max_w {
            break;
        }
        out.push(ch);
    }
    out.push('…');
    out
}

/// A point on the cubic from `a` to `b` leaving and arriving horizontally.
fn curve(a: Vec2, b: Vec2, t: f32) -> Vec2 {
    let dx = (b.x - a.x) * 0.5;
    let (c1, c2) = (vec2(a.x + dx, a.y), vec2(b.x - dx, b.y));
    let u = 1.0 - t;
    a * (u * u * u) + c1 * (3.0 * u * u * t) + c2 * (3.0 * u * t * t) + b * (t * t * t)
}

/// The cubic's direction at `t`.
fn tangent(a: Vec2, b: Vec2, t: f32) -> Vec2 {
    let dx = (b.x - a.x) * 0.5;
    let (c1, c2) = (vec2(a.x + dx, a.y), vec2(b.x - dx, b.y));
    let u = 1.0 - t;
    ((c1 - a) * (3.0 * u * u) + (c2 - c1) * (6.0 * u * t) + (b - c2) * (3.0 * t * t))
        .normalize_or(Vec2::X)
}

/// Dots along the curve: the route not taken.
fn dotted(a: Vec2, b: Vec2, color: Color) {
    const DOTS: usize = 14;
    for i in 0..=DOTS {
        let at = curve(a, b, i as f32 / DOTS as f32);
        draw_circle(at.x, at.y, 1.3, color);
    }
}

/// A pipe's centre line, measured by distance, so its bands keep an even
/// width round the bends.
struct Path {
    points: [Vec2; Path::SAMPLES + 1],
    tangents: [Vec2; Path::SAMPLES + 1],
    /// Distance from the start to each point.
    along: [f32; Path::SAMPLES + 1],
}

impl Path {
    const SAMPLES: usize = 48;

    fn new(a: Vec2, b: Vec2) -> Self {
        let mut points = [Vec2::ZERO; Self::SAMPLES + 1];
        let mut tangents = [Vec2::X; Self::SAMPLES + 1];
        let mut along = [0.0; Self::SAMPLES + 1];
        for i in 0..=Self::SAMPLES {
            let t = i as f32 / Self::SAMPLES as f32;
            points[i] = curve(a, b, t);
            tangents[i] = tangent(a, b, t);
            if i > 0 {
                along[i] = along[i - 1] + points[i].distance(points[i - 1]);
            }
        }
        Self {
            points,
            tangents,
            along,
        }
    }

    fn len(&self) -> f32 {
        self.along[Self::SAMPLES]
    }

    /// The point `d` along, and the unit normal there, both interpolated so a
    /// band's edges bend smoothly rather than kinking at each sample.
    fn at(&self, d: f32) -> (Vec2, Vec2) {
        let i = self
            .along
            .partition_point(|&x| x < d)
            .clamp(1, Self::SAMPLES);
        let span = (self.along[i] - self.along[i - 1]).max(1e-3);
        let t = ((d - self.along[i - 1]) / span).clamp(0.0, 1.0);
        let at = self.points[i - 1].lerp(self.points[i], t);
        let dir = self.tangents[i - 1]
            .lerp(self.tangents[i], t)
            .normalize_or(Vec2::X);
        (at, vec2(-dir.y, dir.x))
    }
}

/// A pipe from `a` to `b`: one solid line `thickness` across, in `color` at
/// `fill` opacity. One strip of quads, so a translucent pipe has no darker
/// seams where round caps would overlap.
fn pipe(a: Vec2, b: Vec2, thickness: f32, fill: f32, color: Color) {
    const STEP: f32 = 1.5;
    let path = Path::new(a, b);
    let (len, half, color) = (path.len(), thickness * 0.5, faded(color, fill));
    if len < 0.1 || color.a <= 0.0 {
        return;
    }
    let n = (len / STEP).ceil().max(1.0) as usize;
    let edge = |d: f32| {
        let (at, normal) = path.at(d);
        (at + normal * half, at - normal * half)
    };
    let mut prev = edge(0.0);
    for i in 1..=n {
        let next = edge(len * i as f32 / n as f32);
        draw_triangle(prev.0, prev.1, next.0, color);
        draw_triangle(prev.1, next.1, next.0, color);
        prev = next;
    }
}

/// A filled disc with a feathered edge: macroquad draws polygons without
/// anti-aliasing, so a hard edge stair-steps at this size.
fn soft_disc(at: Vec2, r: f32, color: Color) {
    draw_poly(at.x, at.y, NODE_SIDES, r - 0.5, 0.0, color);
    draw_poly_lines(at.x, at.y, NODE_SIDES, r, 0.0, 1.0, faded(color, 0.45));
}

/// A ring with feathered edges.
fn soft_ring(at: Vec2, r: f32, width: f32, color: Color) {
    draw_poly_lines(at.x, at.y, NODE_SIDES, r, 0.0, width, color);
    for side in [-1.0, 1.0] {
        draw_poly_lines(
            at.x,
            at.y,
            NODE_SIDES,
            r + side * (width * 0.5 + 0.4),
            0.0,
            0.8,
            faded(color, 0.3),
        );
    }
}

/// An arc of a ring, `from` to `to` radians, clockwise from three o'clock.
fn arc(at: Vec2, r: f32, from: f32, to: f32, width: f32, color: Color) {
    const SEGS: usize = 16;
    let point = |i: usize| {
        let a = from + (to - from) * i as f32 / SEGS as f32;
        at + vec2(a.cos(), a.sin()) * r
    };
    for i in 0..SEGS {
        let (p0, p1) = (point(i), point(i + 1));
        draw_line(p0.x, p0.y, p1.x, p1.y, width, color);
    }
}

/// A node: a disc shaded like a sphere lit from the upper left, with a soft
/// shadow, a faint glow in its colour, its name inside and its figure under.
fn node(p: &Painter, at: Vec2, n: &RouteNode) {
    let c = &p.theme.color;
    let rim = ink(p, n.ink);

    for (grow, a) in [(3.5, 0.08), (2.0, 0.1), (0.5, 0.14)] {
        soft_disc(at + vec2(0.0, 2.5), NODE_R + grow, faded(BLACK, a));
    }
    for i in 1..=4 {
        soft_ring(
            at,
            NODE_R + 1.6 * i as f32,
            1.6,
            faded(rim, 0.08 / i as f32),
        );
    }
    // Discs stepping in towards a lighter centre that sits up and left.
    let edge = mix(c.bg_primary, rim, 0.12);
    let centre = mix(c.bg_highlight, WHITE, 0.07);
    const SHADES: usize = 10;
    for i in 0..SHADES {
        let t = i as f32 / SHADES as f32;
        let r = NODE_R * (1.0 - 0.8 * t);
        let off = vec2(-0.2, -0.26) * NODE_R * t;
        let tone = mix(edge, centre, t.powf(0.8));
        if i == 0 {
            soft_disc(at + off, r, tone);
        } else {
            draw_poly(at.x + off.x, at.y + off.y, NODE_SIDES, r, 0.0, tone);
        }
    }
    soft_ring(at, NODE_R, 1.6, rim);
    // A highlight catching the upper left of the rim.
    arc(
        at,
        NODE_R - 3.2,
        std::f32::consts::PI * 1.08,
        std::f32::consts::PI * 1.42,
        1.4,
        faded(WHITE, 0.22),
    );

    let size = p.size(TextSize::Xs);
    let name = fit(p, &n.name, size, NODE_R * 2.0 - 6.0);
    let w = p.measure(&name, size).width;
    p.text(
        &name,
        at.x - w * 0.5,
        p.centre_baseline(at.y - NODE_R, NODE_R * 2.0, size),
        size,
        rim,
    );
    let w = p.measure(&n.figure, size).width;
    p.text_top(
        &n.figure,
        at.x - w * 0.5,
        at.y + NODE_R + FIGURE_GAP,
        size,
        c.text_secondary,
    );
}

fn meter(p: &Painter, r: Rect, m: &RouteMeter, alpha: f32) {
    let c = &p.theme.color;
    draw_rounded_rect(r.x, r.y, r.w, r.h, r.h * 0.5, faded(c.bg_primary, alpha));
    let fill = m.fill.clamp(0.0, 1.0) * r.w;
    if fill > 0.5 {
        let tone = if m.warn { c.warning } else { c.text_secondary };
        draw_rounded_rect(
            r.x,
            r.y,
            fill.max(r.h),
            r.h,
            r.h * 0.5,
            faded(tone, 0.85 * alpha),
        );
    }
    for t in &m.ticks {
        let x = r.x + t.clamp(0.0, 1.0) * r.w;
        draw_line(
            x,
            r.y - 2.0,
            x,
            r.bottom() + 2.0,
            1.0,
            faded(c.text_muted, 0.8 * alpha),
        );
    }
}

/// The status dot where a pipe meets its card.
fn status_dot(p: &Painter, at: Vec2, card: &RouteCard) {
    let c = &p.theme.color;
    let tone = match card.state {
        LegState::Idle | LegState::Pending | LegState::Waiting => ink(p, card.ink),
        LegState::Done => c.success,
        LegState::Warn => c.warning,
        LegState::Failed => c.error,
    };
    if card.state.in_flight() {
        soft_disc(at, 4.5, c.bg_highlight);
        soft_ring(at, 3.6, 1.6, tone);
    } else {
        if card.state == LegState::Done {
            soft_disc(at, 7.0, faded(tone, 0.22));
        }
        soft_disc(at, 4.2, tone);
    }
}

fn draw_card(p: &Painter, r: Rect, card: &RouteCard) {
    let c = &p.theme.color;
    let a = if card.declined { DECLINED_ALPHA } else { 1.0 };
    let tone = ink(p, card.ink);
    draw_rounded_rect(r.x, r.y, r.w, r.h, CARD_R, faded(c.bg_highlight, 0.9 * a));
    draw_rounded_outline(r, CARD_R, faded(tone, 0.55 * a));

    let x = r.x + CARD_PAD + 4.0;
    let right = r.right() - CARD_PAD;
    let sm = p.size(TextSize::Sm);
    let xs = p.size(TextSize::Xs);

    // The block of lines, centred in the card: name and share, the detail,
    // and the meter when there is room for it.
    let meter_row = card.meter.as_ref().filter(|_| r.h >= CARD_H_FULL);
    let rows = [Some(sm * 1.2), Some(xs * 1.2), meter_row.map(|_| xs * 1.2)];
    let block: f32 =
        rows.iter().flatten().sum::<f32>() + LINE_GAP * (rows.iter().flatten().count() - 1) as f32;
    let mut row_top = r.y + (r.h - block) * 0.5;
    let mut next_row = |h: f32| {
        let top = row_top;
        row_top += h + LINE_GAP;
        top
    };

    let top = next_row(sm * 1.2);
    let base = p.centre_baseline(top, sm * 1.2, sm);
    let share_w = p.measure(&card.share_text, sm).width;
    let share_ink = if card.declined {
        c.text_muted
    } else {
        c.text_primary
    };
    p.text(
        &card.share_text,
        right - share_w,
        base,
        sm,
        faded(share_ink, a),
    );
    let label = fit(p, &card.label, sm, right - x - share_w - 8.0);
    p.text(&label, x, base, sm, faded(tone, a));

    let top = next_row(xs * 1.2);
    p.text(
        &fit(p, &card.detail, xs, right - x),
        x,
        p.centre_baseline(top, xs * 1.2, xs),
        xs,
        faded(c.text_secondary, a),
    );

    if let Some(m) = meter_row {
        let top = next_row(xs * 1.2);
        let inner = right - x;
        let words_w = p.measure(&m.label, xs).width;
        let bar_w = (inner * METER_W).min(inner - words_w - 8.0).max(16.0);
        let bar_y = top + (xs * 1.2 - METER_H) * 0.5;
        meter(p, Rect::new(x, bar_y, bar_w, METER_H), m, a);
        p.text(
            &m.label,
            right - words_w,
            p.centre_baseline(top, xs * 1.2, xs),
            xs,
            faded(c.text_muted, a),
        );
    }

    if !card.declined {
        status_dot(p, vec2(r.x, r.center().y), card);
    }
}

/// A rounded rectangle's outline, one point wide.
fn draw_rounded_outline(r: Rect, radius: f32, color: Color) {
    let rad = radius.min(r.w * 0.5).min(r.h * 0.5);
    draw_line(r.x + rad, r.y, r.right() - rad, r.y, 1.0, color);
    draw_line(
        r.x + rad,
        r.bottom(),
        r.right() - rad,
        r.bottom(),
        1.0,
        color,
    );
    draw_line(r.x, r.y + rad, r.x, r.bottom() - rad, 1.0, color);
    draw_line(
        r.right(),
        r.y + rad,
        r.right(),
        r.bottom() - rad,
        1.0,
        color,
    );
    let corners = [
        (vec2(r.x + rad, r.y + rad), std::f32::consts::PI),
        (vec2(r.right() - rad, r.y + rad), 1.5 * std::f32::consts::PI),
        (vec2(r.right() - rad, r.bottom() - rad), 0.0),
        (
            vec2(r.x + rad, r.bottom() - rad),
            0.5 * std::f32::consts::PI,
        ),
    ];
    const SEGS: usize = 6;
    for (centre, start) in corners {
        for i in 0..SEGS {
            let a0 = start + std::f32::consts::FRAC_PI_2 * i as f32 / SEGS as f32;
            let a1 = start + std::f32::consts::FRAC_PI_2 * (i + 1) as f32 / SEGS as f32;
            let p0 = centre + vec2(a0.cos(), a0.sin()) * rad;
            let p1 = centre + vec2(a1.cos(), a1.sin()) * rad;
            draw_line(p0.x, p0.y, p1.x, p1.y, 1.0, color);
        }
    }
}

/// Draw the graph in `area`: [`route_graph_height`] tall for its cards.
pub fn route_graph(p: &Painter, vm: &RouteGraphVm, area: Rect) {
    let c = &p.theme.color;
    let n = vm.cards.len().max(1);
    let ch = card_h(n);
    let stack = n as f32 * ch + (n - 1) as f32 * CARD_GAP;
    let mid = area.y + TOP + stack.max(NODE_R * 2.0) * 0.5;
    let source = vec2(area.x + NODE_X, mid);
    let sink = vec2(area.right() - NODE_X, mid);
    let top = mid - stack * 0.5;
    let slot = |i: usize| {
        Rect::new(
            area.x + CARD_INSET,
            top + i as f32 * (ch + CARD_GAP),
            area.w - CARD_INSET * 2.0,
            ch,
        )
    };
    let failed = vm.cards.iter().any(|k| k.state == LegState::Failed);

    // Pipes under everything, stacked where they meet a node.
    let thickness = |k: &RouteCard| PIPE_MIN + PIPE_SPAN * k.share.clamp(0.0, 1.0);
    let total: f32 = vm.cards.iter().filter(|k| !k.declined).map(thickness).sum();
    let mut at = -total * 0.5;
    for (i, k) in vm.cards.iter().enumerate() {
        let r = slot(i);
        let (card_in, card_out) = (vec2(r.x, r.center().y), vec2(r.right(), r.center().y));
        if k.declined {
            let dots = faded(c.text_muted, 0.6);
            dotted(source, card_in, dots);
            dotted(card_out, sink, dots);
            continue;
        }
        let th = thickness(k);
        let offset = at + th * 0.5;
        at += th;
        let from = vec2(source.x + NODE_R * 0.6, source.y + offset);
        let to = vec2(sink.x - NODE_R * 0.6, sink.y + offset);
        let tone = if failed { c.text_muted } else { ink(p, k.ink) };
        pipe(from, card_in, th, k.state.fill(), tone);
        pipe(card_out, to, th, k.state.fill(), tone);
    }

    for (i, k) in vm.cards.iter().enumerate() {
        draw_card(p, slot(i), k);
    }

    // Nodes last: the pipes tuck in behind them.
    node(p, source, vm.source);
    node(p, sink, vm.sink);
}

/// The same split as one bar, for a line in a panel: each taken route's name
/// in its colour with its share, then `trailer`, over a bar split by share.
/// [`ROUTE_STRIP_H`] tall. Declined cards are left out: a strip is the plan,
/// not the reasoning.
pub fn route_strip(p: &Painter, cards: &[RouteCard], trailer: &str, r: Rect) {
    let c = &p.theme.color;
    let xs = p.size(TextSize::Xs);
    let taken: Vec<&RouteCard> = cards.iter().filter(|k| !k.declined).collect();

    // The legend gives way in steps, keeping each name in its segment's
    // colour for as long as it can: the trailer goes first, then the
    // separators shrink, then the shares; only then is anything cut.
    let legend = |trail: bool, sep: &str, shares: bool| -> Vec<(String, Color)> {
        let mut parts = Vec::new();
        for (i, k) in taken.iter().enumerate() {
            if i > 0 {
                parts.push((sep.to_string(), c.text_muted));
            }
            parts.push((k.label.clone(), ink(p, k.ink)));
            if shares {
                parts.push((format!(" {}", k.share_text), c.text_secondary));
            }
        }
        if trail && !trailer.is_empty() {
            parts.push((format!(" · {trailer}"), c.text_secondary));
        }
        parts
    };
    let width = |parts: &[(String, Color)]| -> f32 {
        parts.iter().map(|(s, _)| p.measure(s, xs).width).sum()
    };
    let fitted = [
        legend(true, " · ", true),
        legend(false, " · ", true),
        legend(false, "  ", true),
        legend(false, "  ", false),
    ]
    .into_iter()
    .find(|parts| width(parts) <= r.w);
    match fitted {
        Some(parts) => {
            let mut x = r.x;
            for (s, tone) in &parts {
                p.text_top(s, x, r.y, xs, *tone);
                x += p.measure(s, xs).width;
            }
        }
        None => {
            let whole: String = legend(false, "  ", false)
                .iter()
                .map(|(s, _)| s.as_str())
                .collect();
            p.text_top(&fit(p, &whole, xs, r.w), r.x, r.y, xs, c.text_secondary);
        }
    }

    let bar = Rect::new(r.x, r.bottom() - STRIP_BAR_H, r.w, STRIP_BAR_H);
    draw_rounded_rect(
        bar.x,
        bar.y,
        bar.w,
        bar.h,
        bar.h * 0.5,
        faded(c.bg_primary, 0.8),
    );
    let gap = 2.0;
    let mut x = bar.x;
    for (i, k) in taken.iter().enumerate() {
        let last = i + 1 == taken.len();
        let w = if last {
            bar.right() - x
        } else {
            (bar.w * k.share).round()
        };
        let (tone, a) = match k.state {
            LegState::Idle | LegState::Pending => (ink(p, k.ink), 1.0),
            LegState::Waiting => (ink(p, k.ink), 0.4),
            LegState::Done => (c.success, 1.0),
            LegState::Warn => (c.warning, 1.0),
            LegState::Failed => (c.error, 1.0),
        };
        let seg = (w - if last { 0.0 } else { gap }).max(bar.h);
        draw_rounded_rect(x, bar.y, seg, bar.h, bar.h * 0.5, faded(tone, a));
        x += w;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_cards_get_room_for_their_meters_and_four_do_not() {
        let two = route_graph_height(2);
        assert_eq!(two, TOP + 2.0 * CARD_H_FULL + CARD_GAP + BOTTOM);
        let four = route_graph_height(4);
        assert_eq!(four, TOP + 4.0 * CARD_H_COMPACT + 3.0 * CARD_GAP + BOTTOM);
        assert!(
            four < 4.0 * CARD_H_FULL,
            "compact cards keep four on a phone"
        );
    }

    #[test]
    fn a_lone_card_is_never_shorter_than_its_nodes() {
        assert!(route_graph_height(1) >= TOP + NODE_R * 2.0 + BOTTOM);
        assert_eq!(route_graph_height(0), route_graph_height(1));
    }

    #[test]
    fn pipes_leave_and_arrive_flat() {
        let (a, b) = (vec2(0.0, 0.0), vec2(100.0, 40.0));
        assert_eq!(curve(a, b, 0.0), a);
        assert_eq!(curve(a, b, 1.0), b);
        let near_start = curve(a, b, 0.02);
        assert!(
            near_start.y.abs() < 0.1,
            "flat out of the source: {near_start}"
        );
        assert!((tangent(a, b, 0.0) - Vec2::X).length() < 1e-4);
        assert!((tangent(a, b, 1.0) - Vec2::X).length() < 1e-4);
    }

    /// How full a pipe reads orders the states: waiting reads emptier than
    /// planned, done reads fullest, and an ended leg empties.
    #[test]
    fn a_pipe_fills_as_its_leg_completes() {
        use LegState::*;
        assert!(Waiting.fill() < Idle.fill());
        assert_eq!(Pending.fill(), Idle.fill());
        assert!(Done.fill() > Idle.fill());
        assert!(Warn.fill() < Waiting.fill());
        assert!(Failed.fill() < Waiting.fill());
        assert!(Pending.in_flight() && Waiting.in_flight());
        assert!(!Idle.in_flight() && !Done.in_flight());
    }

    /// A pipe keeps an even width round a bend: the path is measured in
    /// points, and halfway along is the middle.
    #[test]
    fn the_path_is_measured_in_points() {
        let straight = Path::new(vec2(0.0, 0.0), vec2(100.0, 0.0));
        assert!((straight.len() - 100.0).abs() < 0.01);
        let (at, normal) = straight.at(25.0);
        assert!((at.x - 25.0).abs() < 0.01 && at.y.abs() < 0.01);
        assert!((normal - vec2(0.0, 1.0)).length() < 1e-4);

        let bent = Path::new(vec2(0.0, 0.0), vec2(60.0, 80.0));
        assert!(bent.len() > 100.0, "a bend is longer than its chord");
        let (mid, _) = bent.at(bent.len() * 0.5);
        assert!(
            (mid - vec2(30.0, 40.0)).length() < 1.0,
            "halfway is the middle: {mid}"
        );
    }
}
