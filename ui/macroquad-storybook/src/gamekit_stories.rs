//! Stories for `macroquad-gamekit`: easing curves, a fanned card hand you can
//! hover, drag and long-press, rich text with inline icons, hit effects,
//! rounded art, and the preloader's cover lifting off a screen.
//!
//! Every story is driven by the storybook's clock (`dt` and `t`), so P pauses,
//! `.` steps a frame and S slows time: the way to judge a 150 ms settle. With
//! `--shot` the clock runs at a fixed 1/60 s, and `--at <seconds>` captures at
//! an exact moment, so a capture is repeatable.
//!
//! Card names and text here are sample strings for judging layout, not game
//! content.

use macroquad::prelude::*;
use macroquad_gamekit::{
    Align, Animated, Baked, Ease, HoverGrace, MacroquadMeasure, Particles, Pointer, PointerConfig,
    PointerEvent, Popups, Pose, Preloader, RichText, RoundedArt, Sequencer, Shake, Sprite,
    TextStyle, draw_order, fill_rounded, stroke_rounded, topmost,
};
use macroquad_widgets::{Gesture, Painter};
use ui_theme::TextSize;

/// Which kit story this is.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum KitView {
    Easing,
    Hand,
    Text,
    Fx,
    Art,
    Preload,
}

/// Card size in points. // theme-exempt: geometry, not text
const CARD: Vec2 = Vec2::new(120.0, 168.0);
/// The drag threshold is this fraction of the card's height.
const DRAG_FRACTION: f32 = 0.05;
const LONG_PRESS: f32 = 0.45;
const SETTLE: f32 = 0.15;
const HOVER_GRACE: f32 = 0.14;
const ICON_ENERGY: u32 = 1;
const ICON_GUARD: u32 = 2;

/// Sample cards: a name, a cost, and body text as (text, emphasised) runs with
/// an optional icon after the run.
struct Sample {
    name: &'static str,
    cost: u32,
    body: &'static [(&'static str, bool, Option<u32>)],
}

const SAMPLES: [Sample; 6] = [
    Sample {
        name: "Strike",
        cost: 1,
        body: &[
            ("Deal ", false, None),
            ("6", true, None),
            (" damage.", false, None),
        ],
    },
    Sample {
        name: "Brace",
        cost: 1,
        body: &[
            ("Gain ", false, None),
            ("5", true, Some(ICON_GUARD)),
            (" until your next turn.", false, None),
        ],
    },
    Sample {
        name: "Overclock",
        cost: 0,
        body: &[
            ("Gain ", false, None),
            ("2", true, Some(ICON_ENERGY)),
            (". Draw 1 card.", false, None),
        ],
    },
    Sample {
        name: "Heavy Swing With A Long Name",
        cost: 2,
        body: &[
            ("Deal ", false, None),
            ("12", true, None),
            (
                " damage. If the enemy is guarding, deal it twice.",
                false,
                None,
            ),
        ],
    },
    Sample {
        name: "Patch Up",
        cost: 1,
        body: &[
            ("Heal ", false, None),
            ("4", true, None),
            (".", false, None),
        ],
    },
    Sample {
        name: "Salvage",
        cost: 1,
        body: &[
            ("Draw ", false, None),
            ("2", true, None),
            (". Strip 1 from your hand.", false, None),
        ],
    },
];

struct Card {
    sample: usize,
    sprite: Sprite,
}

struct Drag {
    card: usize,
    grab: Vec2,
}

enum Cue {
    Deal,
}

struct Hand {
    faces: Vec<Baked>,
    big_faces: Vec<Baked>,
    cards: Vec<Card>,
    flying: Vec<Card>,
    pointer: Pointer,
    hover: HoverGrace<usize>,
    drag: Option<Drag>,
    detail: Option<usize>,
    seq: Sequencer<Cue>,
    next_sample: usize,
}

struct FxStage {
    shake: Shake,
    popups: Popups,
    particles: Particles,
    pulse: Animated<f32>,
    last_fire: f64,
}

/// A preloader over a stand-in screen, restarted every few seconds.
struct PreloadStage {
    preloader: Preloader,
    started: f64,
}

/// Seconds the stand-in screen takes to be ready, and between restarts.
const PRELOAD_READY: f64 = 1.6;
const PRELOAD_CYCLE: f64 = 4.0;

pub struct KitStory {
    pub view: KitView,
    art: RoundedArt,
    textures: Vec<Texture2D>,
    hand: Option<Hand>,
    fx: FxStage,
    preload: PreloadStage,
}

impl KitStory {
    pub fn new(view: KitView, textures: Vec<Texture2D>) -> Self {
        Self {
            view,
            art: RoundedArt::new(),
            textures,
            hand: None,
            fx: FxStage {
                shake: Shake::new(18.0, 1.6),
                popups: Popups::new(0.9, 46.0),
                particles: Particles::new(11, 520.0),
                pulse: Animated::new(1.0),
                last_fire: -10.0,
            },
            preload: PreloadStage {
                preloader: Preloader::new(),
                started: 0.0,
            },
        }
    }

    /// Draw the story in `area`. `dt` and `t` come from the storybook clock.
    pub fn draw(&mut self, p: &Painter, area: Rect, g: &Gesture, dt: f32, t: f64) {
        match self.view {
            KitView::Easing => easing(p, area, t),
            KitView::Hand => self.hand(p, area, g, dt),
            KitView::Text => text_story(p, area),
            KitView::Fx => self.fx(p, area, g, dt, t),
            KitView::Art => self.art_story(p, area),
            KitView::Preload => self.preload_story(p, area, g, dt, t),
        }
    }

    // ── preload ─────────────────────────────────────────────────────────

    /// The cover and spinner while a stand-in screen "loads", then the lift.
    fn preload_story(&mut self, p: &Painter, area: Rect, g: &Gesture, dt: f32, t: f64) {
        let stage = &mut self.preload;
        if g.tap.is_some_and(|at| area.contains(at)) || t - stage.started >= PRELOAD_CYCLE {
            stage.preloader = Preloader::new();
            stage.started = t;
        }
        let ready = t - stage.started >= PRELOAD_READY;
        stage.preloader.update(dt, ready);

        // The screen underneath, drawn whole.
        fill_rounded(area, 12.0, p.theme.color.bg_secondary);
        p.text(
            "the screen, drawn whole (tap to load again)",
            area.x + 16.0,
            area.y + 24.0,
            p.size(TextSize::Sm),
            p.theme.color.text_muted,
        );
        for (i, width) in [0.72, 0.55, 0.64].into_iter().enumerate() {
            let bar = Rect::new(
                area.x + 16.0,
                area.y + 52.0 + i as f32 * 34.0,
                (area.w - 32.0) * width,
                20.0,
            );
            fill_rounded(bar, 6.0, p.theme.color.bg_highlight);
        }
        stage.preloader.draw(
            area,
            p.theme.color.bg_secondary,
            p.theme.color.bg_primary,
            p.theme.color.accent,
        );
    }

    // ── hand ────────────────────────────────────────────────────────────

    fn hand(&mut self, p: &Painter, area: Rect, g: &Gesture, dt: f32) {
        if self.hand.is_none() {
            let dpi = screen_dpi_scale();
            let faces = (0..SAMPLES.len())
                .map(|i| bake_card(p, &self.art, &self.textures, i, dpi * 1.25))
                .collect();
            let big_faces = (0..SAMPLES.len())
                .map(|i| bake_card(p, &self.art, &self.textures, i, dpi * 2.0))
                .collect();
            let mut hand = Hand {
                faces,
                big_faces,
                cards: Vec::new(),
                flying: Vec::new(),
                pointer: Pointer::new(PointerConfig {
                    drag_threshold: CARD.y * DRAG_FRACTION,
                    long_press: LONG_PRESS,
                }),
                hover: HoverGrace::new(HOVER_GRACE),
                drag: None,
                detail: None,
                seq: Sequencer::new(),
                next_sample: 0,
            };
            for _ in 0..5 {
                hand.seq.push(Cue::Deal);
            }
            self.hand = Some(hand);
        }
        let Some(h) = &mut self.hand else { return };

        let deck_at = vec2(area.x + 50.0, area.y + area.h - 70.0);
        let play_line = area.y + area.h - CARD.y * 1.35;
        let mouse = Vec2::from(mouse_position());

        // Input.
        for ev in h.pointer.update(g, dt) {
            match ev {
                PointerEvent::DragStarted { from, .. } if h.detail.is_none() => {
                    if let Some(i) = topmost(h.cards.iter().map(|c| &c.sprite), from) {
                        let pos = h.cards[i].sprite.pose.get().pos;
                        h.drag = Some(Drag {
                            card: i,
                            grab: from - pos,
                        });
                        h.hover.clear();
                    }
                }
                PointerEvent::Dragged { at } => {
                    if let Some(d) = &h.drag
                        && let Some(c) = h.cards.get_mut(d.card)
                    {
                        c.sprite.pose.set(Pose::at(at - d.grab).scaled(1.1));
                        c.sprite.z = 1000;
                    }
                }
                PointerEvent::Released { at, dragged: true } => {
                    if let Some(d) = h.drag.take()
                        && d.card < h.cards.len()
                    {
                        if at.y < play_line {
                            let mut c = h.cards.remove(d.card);
                            let target = vec2(area.x + area.w / 2.0, area.y + 60.0);
                            c.sprite.go(
                                Pose::at(target).scaled(0.6).faded(0.0),
                                0.24,
                                Ease::CubicIn,
                            );
                            h.flying.push(c);
                            h.seq.push(Cue::Deal);
                        } else {
                            h.cards[d.card].sprite.z = 0;
                        }
                    }
                }
                PointerEvent::LongPress { at } if h.detail.is_none() => {
                    h.detail =
                        topmost(h.cards.iter().map(|c| &c.sprite), at).map(|i| h.cards[i].sample);
                }
                PointerEvent::Tap { .. } if h.detail.is_some() => h.detail = None,
                _ => {}
            }
        }
        let hit = if h.drag.is_none() && h.detail.is_none() {
            topmost(h.cards.iter().map(|c| &c.sprite), mouse)
        } else {
            None
        };
        let hovered = h.hover.update(hit, dt);

        // Cues: deal from the deck, one at a time. Dealing does not wait for
        // the hand to stop moving: the next card's arrival is the movement.
        if let Some(Cue::Deal) = h.seq.poll(dt, false) {
            let sample = h.next_sample % SAMPLES.len();
            h.next_sample += 1;
            h.cards.push(Card {
                sample,
                sprite: Sprite::new(CARD, Pose::at(deck_at).scaled(0.4).rotated(-0.6).faded(0.0)),
            });
            h.seq.hold(0.09);
        }

        // Layout: every card not in hand-drag glides to its slot.
        let slots = fan(h.cards.len(), hovered, area);
        for (i, c) in h.cards.iter_mut().enumerate() {
            if h.drag.as_ref().is_some_and(|d| d.card == i) {
                continue;
            }
            let (pose, z) = slots[i];
            c.sprite.z = z;
            if c.sprite.pose.target() != pose {
                c.sprite.go(pose, SETTLE, Ease::ExpoOut);
            }
        }
        for c in h.cards.iter_mut().chain(h.flying.iter_mut()) {
            c.sprite.update(dt);
        }
        h.flying.retain(|c| !c.sprite.settled());

        // Draw.
        fill_rounded(area, 12.0, p.theme.color.bg_secondary);
        draw_line(
            area.x + 12.0,
            play_line,
            area.x + area.w - 12.0,
            play_line,
            1.0,
            Color {
                a: 0.35,
                ..p.theme.color.accent
            },
        );
        let hint = p.size(TextSize::Sm);
        p.text(
            "release above this line to play",
            area.x + 16.0,
            play_line - 8.0,
            hint,
            p.theme.color.text_muted,
        );
        p.text(
            "hover · drag up to play · hold to inspect",
            area.x + 16.0,
            area.y + 24.0,
            hint,
            p.theme.color.text_muted,
        );
        let deck = Rect::new(deck_at.x - 26.0, deck_at.y - 36.0, 52.0, 72.0);
        fill_rounded(deck, 6.0, p.theme.color.bg_highlight);
        stroke_rounded(deck, 6.0, 2.0, p.theme.color.border);

        let sprites: Vec<Sprite> = h.cards.iter().map(|c| c.sprite).collect();
        for i in draw_order(&sprites) {
            let c = &h.cards[i];
            h.faces[c.sample].draw(c.sprite.pose.get(), WHITE);
        }
        for c in &h.flying {
            h.faces[c.sample].draw(c.sprite.pose.get(), WHITE);
        }

        if let Some(s) = h.detail {
            draw_rectangle(
                area.x,
                area.y,
                area.w,
                area.h,
                Color::new(0.0, 0.0, 0.0, 0.6),
            );
            let centre = vec2(area.x + area.w / 2.0, area.y + area.h / 2.0);
            h.big_faces[s].draw(Pose::at(centre).scaled(1.8), WHITE);
            p.text(
                "tap to close",
                centre.x - 40.0,
                area.y + area.h - 16.0,
                hint,
                p.theme.color.text_muted,
            );
        }
    }

    // ── fx ──────────────────────────────────────────────────────────────

    fn fx(&mut self, p: &Painter, area: Rect, g: &Gesture, dt: f32, t: f64) {
        let f = &mut self.fx;
        let centre = vec2(area.x + area.w / 2.0, area.y + area.h / 2.0);
        // Fire on a tap, and on its own every 1.6 s so a capture shows it.
        let auto = t - f.last_fire >= 1.6;
        if g.tap.is_some_and(|at| area.contains(at)) || auto {
            f.last_fire = t;
            f.shake.add(0.55);
            let style = TextStyle::new(p.font.cloned(), p.size(TextSize::Xl3), p.theme.color.error)
                .outline(BLACK, 2.0);
            f.popups.spawn("-7", style, centre + vec2(30.0, -60.0));
            f.particles
                .burst(centre, 24, 260.0, 6.0, 0.7, p.theme.color.accent_orange);
            f.pulse.set(1.18);
            f.pulse.to(1.0, 0.25, Ease::BackOut);
        }
        f.shake.update(dt);
        f.popups.update(dt);
        f.particles.update(dt);
        f.pulse.update(dt);

        fill_rounded(area, 12.0, p.theme.color.bg_secondary);
        p.text(
            "tap to hit (also fires every 1.6 s)",
            area.x + 16.0,
            area.y + 24.0,
            p.size(TextSize::Sm),
            p.theme.color.text_muted,
        );
        let off = f.shake.offset();
        let s = 110.0 * f.pulse.get();
        let target = Rect::new(centre.x - s / 2.0 + off.x, centre.y - s / 2.0 + off.y, s, s);
        fill_rounded(target, 14.0, p.theme.color.bg_highlight);
        stroke_rounded(target, 14.0, 3.0, p.theme.color.accent_red);
        f.particles.draw();
        f.popups.draw(screen_dpi_scale());
    }

    // ── art ─────────────────────────────────────────────────────────────

    fn art_story(&mut self, p: &Painter, area: Rect) {
        fill_rounded(area, 12.0, p.theme.color.bg_secondary);
        let note = if self.art.rounds() {
            "cover-cropped into each frame, corners clipped by shader"
        } else {
            "shader unavailable on this backend: corners are square"
        };
        p.text(
            note,
            area.x + 16.0,
            area.y + 24.0,
            p.size(TextSize::Sm),
            p.theme.color.text_muted,
        );
        let frames = [
            Rect::new(area.x + 24.0, area.y + 48.0, 180.0, 110.0),
            Rect::new(area.x + 224.0, area.y + 48.0, 100.0, 160.0),
            Rect::new(area.x + 344.0, area.y + 48.0, 130.0, 130.0),
        ];
        for (i, r) in frames.iter().enumerate() {
            if let Some(tex) = self.textures.get(i) {
                self.art.draw(tex, *r, 16.0, WHITE);
            }
            stroke_rounded(*r, 16.0, 2.0, p.theme.color.border);
        }
    }
}

/// Slots for a fanned hand of `n`, with `hovered` lifted and its neighbours
/// pushed aside. Returns each card's pose and draw order.
fn fan(n: usize, hovered: Option<usize>, area: Rect) -> Vec<(Pose, i32)> {
    if n == 0 {
        return Vec::new();
    }
    let cx = area.x + area.w / 2.0;
    let base_y = area.y + area.h - CARD.y * 0.62;
    let spacing = (CARD.x * 0.7).min((area.w - CARD.x - 40.0) / (n.max(2) - 1) as f32);
    let mid = (n as f32 - 1.0) / 2.0;
    (0..n)
        .map(|i| {
            let k = i as f32 - mid;
            let mut pos = vec2(cx + k * spacing, base_y + k * k * 4.0);
            let mut rotation = k * 0.07;
            let mut scale = 1.0;
            let mut z = i as i32;
            if let Some(h) = hovered {
                match i.cmp(&h) {
                    std::cmp::Ordering::Equal => {
                        pos.y = base_y - CARD.y * 0.32;
                        rotation = 0.0;
                        scale = 1.22;
                        z = 100;
                    }
                    std::cmp::Ordering::Less => pos.x -= CARD.x * 0.22,
                    std::cmp::Ordering::Greater => pos.x += CARD.x * 0.22,
                }
            }
            (Pose::at(pos).rotated(rotation).scaled(scale), z)
        })
        .collect()
}

/// Bake one sample card face at `scale` pixels per point.
fn bake_card(p: &Painter, art: &RoundedArt, textures: &[Texture2D], i: usize, scale: f32) -> Baked {
    let s = &SAMPLES[i];
    let theme = &p.theme;
    Baked::render(CARD, scale, |k| {
        let r = Rect::new(0.0, 0.0, CARD.x, CARD.y);
        fill_rounded(r, 10.0, theme.color.bg_primary);
        stroke_rounded(
            Rect::new(1.5, 1.5, CARD.x - 3.0, CARD.y - 3.0),
            9.0,
            3.0,
            theme.color.border,
        );
        if let Some(tex) = textures.get(i % textures.len().max(1)) {
            art.draw(tex, Rect::new(8.0, 30.0, CARD.x - 16.0, 56.0), 6.0, WHITE);
        }

        // Name: wraps right of the cost, stepping down the ramp until it fits
        // the 26-point band above the art.
        let band = 26.0;
        let (name, nl) = [TextSize::Base, TextSize::Sm, TextSize::Xs]
            .into_iter()
            .map(|size| {
                let mut name = RichText::new();
                let st = name.style(TextStyle::new(
                    p.font.cloned(),
                    p.size(size),
                    theme.color.text_primary,
                ));
                name.text(s.name, st);
                let l = name.layout(&MacroquadMeasure, CARD.x - 36.0, Align::Start, -1.0);
                (name, l)
            })
            .find_map(|(n, l)| (l.size.y <= band).then_some((n, l)))
            .unwrap_or_else(|| {
                let mut name = RichText::new();
                let st = name.style(TextStyle::new(
                    p.font.cloned(),
                    p.size(TextSize::Xs),
                    theme.color.text_primary,
                ));
                name.text(s.name, st);
                let l = name.layout(&MacroquadMeasure, CARD.x - 36.0, Align::Start, -1.0);
                (name, l)
            });
        nl.draw(
            &name,
            vec2(30.0, 2.0 + (band - nl.size.y).max(0.0) / 2.0),
            k,
            1.0,
            |_, _| {},
        );

        // Cost.
        draw_circle(16.0, 15.0, 10.0, theme.color.accent_blue);
        let cost = s.cost.to_string();
        let cs = p.size(TextSize::Md);
        let cd = p.measure(&cost, cs);
        draw_text_ex(
            &cost,
            16.0 - cd.width / 2.0,
            15.0 + cd.offset_y / 2.0 - 1.0,
            TextParams {
                font: p.font,
                font_size: (cs * k).round() as u16,
                font_scale: 1.0 / k,
                color: theme.color.bg_primary,
                ..Default::default()
            },
        );

        // Body, centred, with emphasis and icons.
        let mut body = RichText::new();
        let plain = body.style(TextStyle::new(
            p.font.cloned(),
            p.size(TextSize::Sm),
            theme.color.text_secondary,
        ));
        let strong = body.style(TextStyle::new(
            p.font.cloned(),
            p.size(TextSize::Md),
            theme.color.accent,
        ));
        for (text, emphasised, icon) in s.body {
            body.text(text, if *emphasised { strong } else { plain });
            if let Some(id) = icon {
                body.icon(*id, vec2(11.0, 11.0), plain);
            }
        }
        let bl = body.layout(&MacroquadMeasure, CARD.x - 16.0, Align::Center, 2.0);
        bl.draw(&body, vec2(8.0, 94.0), k, 1.0, |id, rect| {
            let c = rect.center();
            match id {
                ICON_ENERGY => draw_circle(c.x, c.y, rect.w / 2.0, theme.color.accent_yellow),
                ICON_GUARD => {
                    draw_rectangle(
                        rect.x + 1.0,
                        rect.y,
                        rect.w - 2.0,
                        rect.h * 0.7,
                        theme.color.accent_blue,
                    );
                    draw_triangle(
                        vec2(rect.x + 1.0, rect.y + rect.h * 0.7),
                        vec2(rect.x + rect.w - 1.0, rect.y + rect.h * 0.7),
                        vec2(c.x, rect.y + rect.h),
                        theme.color.accent_blue,
                    );
                }
                _ => {}
            }
        });
    })
}

/// Each curve: a plot, and a dot riding it on a loop.
fn easing(p: &Painter, area: Rect, t: f64) {
    const RUN: f64 = 1.2;
    const REST: f64 = 0.5;
    let phase = (t % (RUN + REST)).min(RUN) / RUN;
    let label = p.size(TextSize::Sm);
    let (plot_w, plot_h, row_h) = (70.0, 34.0, 46.0);
    for (i, e) in Ease::ALL.iter().enumerate() {
        let y = area.y + 8.0 + i as f32 * row_h;
        p.text(
            e.name(),
            area.x,
            y + plot_h / 2.0 + 4.0,
            label,
            p.theme.color.text_secondary,
        );
        let px = area.x + 110.0;
        draw_rectangle_lines(px, y, plot_w, plot_h, 1.0, p.theme.color.bg_highlight);
        let mut prev = vec2(px, y + plot_h);
        for s in 1..=40 {
            let u = s as f32 / 40.0;
            let pt = vec2(px + u * plot_w, y + plot_h - e.apply(u) * plot_h);
            draw_line(prev.x, prev.y, pt.x, pt.y, 1.5, p.theme.color.accent);
            prev = pt;
        }
        let track_x = px + plot_w + 24.0;
        let track_w = (area.w - (track_x - area.x) - 24.0).max(60.0);
        draw_line(
            track_x,
            y + plot_h / 2.0,
            track_x + track_w,
            y + plot_h / 2.0,
            1.0,
            p.theme.color.bg_highlight,
        );
        let v = e.apply(phase as f32);
        draw_circle(
            track_x + v * track_w,
            y + plot_h / 2.0,
            7.0,
            p.theme.color.accent_cyan,
        );
    }
}

/// One paragraph at three widths and three alignments, boxes drawn to show
/// the wrap width, plus a width that follows the mouse.
fn text_story(p: &Painter, area: Rect) {
    let mut t = RichText::new();
    let plain = t.style(TextStyle::new(
        p.font.cloned(),
        p.size(TextSize::Md),
        p.theme.color.text_primary,
    ));
    let strong = t.style(
        TextStyle::new(p.font.cloned(), p.size(TextSize::Lg), p.theme.color.accent)
            .shadow(BLACK, vec2(1.0, 1.0)),
    );
    let muted = t.style(TextStyle::new(
        p.font.cloned(),
        p.size(TextSize::Sm),
        p.theme.color.text_muted,
    ));
    t.text("Deal ", plain)
        .text("8", strong)
        .text(" damage. Gain ", plain)
        .icon(ICON_ENERGY, vec2(14.0, 14.0), plain)
        .text(" if the enemy is guarding.\n", plain)
        .text("A run that continues mid-word: un", plain)
        .text("breakable", strong)
        .text(" stays together.", plain)
        .text(" (small print)", muted);
    let icon = |id: u32, r: Rect| {
        if id == ICON_ENERGY {
            let c = r.center();
            draw_circle(c.x, c.y, r.w / 2.0, p.theme.color.accent_yellow);
        }
    };
    let dpi = screen_dpi_scale();
    let mouse = Vec2::from(mouse_position());
    let live = if area.contains(mouse) {
        (mouse.x - area.x - 16.0).max(60.0)
    } else {
        300.0
    };
    let mut y = area.y + 8.0;
    for (w, align, name) in [
        (180.0, Align::Start, "180 · start"),
        (240.0, Align::Center, "240 · centre"),
        (300.0, Align::End, "300 · end"),
        (live, Align::Start, "follows the mouse"),
    ] {
        p.text(
            name,
            area.x + 16.0,
            y + 12.0,
            p.size(TextSize::Xs),
            p.theme.color.text_muted,
        );
        let l = t.layout(&MacroquadMeasure, w, align, 3.0);
        let origin = vec2(area.x + 16.0, y + 20.0);
        draw_rectangle_lines(
            origin.x,
            origin.y,
            w,
            l.size.y,
            1.0,
            p.theme.color.bg_highlight,
        );
        l.draw(&t, origin, dpi, 1.0, icon);
        y += l.size.y + 40.0;
    }
}
