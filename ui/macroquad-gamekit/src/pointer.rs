//! `Pointer` — the press, drag, long-press and tap a card game needs, built on
//! the widgets crate's `Gesture`; `Swipes`, the four-way swipe an arcade game
//! needs; and `HoverGrace`, which keeps a hover alive for a moment between
//! targets.
//!
//! `macroquad_widgets::Gestures` resolves a tap or a swipe when the pointer
//! lifts, and reports where it is while down. A card needs more, while the
//! pointer is still down:
//!
//! - a drag that starts only after the pointer has moved a set distance
//!   (a fraction of the card's height), so a press is not a drag;
//! - a long-press that fires once if the pointer is held still, for a detail
//!   view;
//! - a tap only if neither of those happened.
//!
//! [`Pointer::update`] turns each frame's `Gesture` into [`PointerEvent`]s.
//! It reads the gesture and `dt`, never the clock or the input state, so it is
//! tested without a window.

use macroquad::prelude::Vec2;
use macroquad_widgets::Gesture;

use crate::dir::Dir4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerConfig {
    /// Points the pointer must move from where it went down before a drag
    /// starts.
    pub drag_threshold: f32,
    /// Seconds held still before a long-press fires.
    pub long_press: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointerEvent {
    Pressed {
        at: Vec2,
    },
    DragStarted {
        from: Vec2,
        at: Vec2,
    },
    Dragged {
        at: Vec2,
    },
    LongPress {
        at: Vec2,
    },
    /// The pointer lifted. `dragged` says whether a drag had started.
    Released {
        at: Vec2,
        dragged: bool,
    },
    /// A press that was neither a drag nor a long-press.
    Tap {
        at: Vec2,
    },
}

#[derive(Clone, Copy, Debug)]
struct Press {
    start: Vec2,
    at: Vec2,
    held: f32,
    dragging: bool,
    long_fired: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Pointer {
    pub config: PointerConfig,
    press: Option<Press>,
}

impl Pointer {
    pub fn new(config: PointerConfig) -> Self {
        Self {
            config,
            press: None,
        }
    }

    /// Where the pointer went down, while it is down.
    pub fn pressed_at(&self) -> Option<Vec2> {
        self.press.map(|p| p.start)
    }

    /// Whether a drag is in progress.
    pub fn dragging(&self) -> bool {
        self.press.is_some_and(|p| p.dragging)
    }

    /// Feed this frame's gesture. Events come back in the order they happened.
    pub fn update(&mut self, g: &Gesture, dt: f32) -> Vec<PointerEvent> {
        let mut out = Vec::new();
        match (g.drag, &mut self.press) {
            (Some(at), None) => {
                self.press = Some(Press {
                    start: at,
                    at,
                    held: 0.0,
                    dragging: false,
                    long_fired: false,
                });
                out.push(PointerEvent::Pressed { at });
            }
            (Some(at), Some(p)) => {
                p.at = at;
                p.held += dt;
                if !p.dragging
                    && !p.long_fired
                    && p.start.distance(at) >= self.config.drag_threshold
                {
                    p.dragging = true;
                    out.push(PointerEvent::DragStarted { from: p.start, at });
                }
                if p.dragging {
                    out.push(PointerEvent::Dragged { at });
                } else if !p.long_fired && p.held >= self.config.long_press {
                    p.long_fired = true;
                    out.push(PointerEvent::LongPress { at });
                }
            }
            (None, Some(p)) => {
                let p = *p;
                self.press = None;
                out.push(PointerEvent::Released {
                    at: p.at,
                    dragged: p.dragging,
                });
                if !p.dragging
                    && !p.long_fired
                    && let Some(at) = g.tap
                {
                    out.push(PointerEvent::Tap { at });
                }
            }
            (None, None) => {
                // Down and up within one frame: the gesture saw a tap we never
                // saw pressed.
                if let Some(at) = g.tap {
                    out.push(PointerEvent::Pressed { at });
                    out.push(PointerEvent::Released { at, dragged: false });
                    out.push(PointerEvent::Tap { at });
                }
            }
        }
        out
    }
}

/// A finished four-way swipe: where the pointer went down, where it last was
/// before lifting, and which way it mostly went.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Swipe {
    pub from: Vec2,
    pub to: Vec2,
    pub dir: Dir4,
}

/// Four-way swipes, for games: a hop in Frogger, a turn in Snake.
///
/// The widgets' `Gesture::swipe` is a page turn — sideways only, and only when
/// clearly horizontal — and stays that way. This reads the same `Gesture` and
/// resolves any press that travelled at least `min` (in the gesture's units:
/// stage units for [`Frame::gesture`](crate::Frame::gesture)) into its
/// dominant direction when it lifts. A shorter press resolves to nothing, and
/// the gesture's own tap still fires for it.
#[derive(Clone, Copy, Debug)]
pub struct Swipes {
    pub min: f32,
    press: Option<(Vec2, Vec2)>,
}

impl Swipes {
    pub fn new(min: f32) -> Self {
        Self { min, press: None }
    }

    /// Feed this frame's gesture; a swipe comes back on the frame it lifts.
    pub fn update(&mut self, g: &Gesture) -> Option<Swipe> {
        match (g.drag, &mut self.press) {
            (Some(at), Some((_, last))) => {
                *last = at;
                None
            }
            (Some(at), None) => {
                self.press = Some((at, at));
                None
            }
            (None, Some((from, to))) => {
                let (from, to) = (*from, *to);
                self.press = None;
                (from.distance(to) >= self.min)
                    .then(|| Dir4::of(to - from))
                    .flatten()
                    .map(|dir| Swipe { from, to, dir })
            }
            (None, None) => None,
        }
    }

    /// Forget a press in progress (a scene change mid-swipe).
    pub fn cancel(&mut self) {
        self.press = None;
    }
}

/// Keeps the last hovered target for `grace` seconds after the pointer leaves
/// it, so moving between neighbouring cards does not drop and re-lift the
/// hand.
#[derive(Clone, Copy, Debug)]
pub struct HoverGrace<K> {
    current: Option<K>,
    left: f32,
    pub grace: f32,
}

impl<K: Copy + PartialEq> HoverGrace<K> {
    pub fn new(grace: f32) -> Self {
        Self {
            current: None,
            left: 0.0,
            grace,
        }
    }

    /// Feed what is under the pointer now; get what counts as hovered.
    pub fn update(&mut self, hit: Option<K>, dt: f32) -> Option<K> {
        match hit {
            Some(k) => {
                self.current = Some(k);
                self.left = self.grace;
            }
            None => {
                self.left -= dt;
                if self.left <= 0.0 {
                    self.current = None;
                }
            }
        }
        self.current
    }

    pub fn get(&self) -> Option<K> {
        self.current
    }

    pub fn clear(&mut self) {
        self.current = None;
        self.left = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use macroquad::prelude::vec2;

    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn cfg() -> PointerConfig {
        PointerConfig {
            drag_threshold: 7.0,
            long_press: 0.45,
        }
    }

    fn down(at: Vec2) -> Gesture {
        Gesture {
            drag: Some(at),
            ..Gesture::default()
        }
    }

    fn up(tap: Option<Vec2>) -> Gesture {
        Gesture {
            tap,
            ..Gesture::default()
        }
    }

    #[test]
    fn a_short_still_press_is_a_tap() {
        let mut p = Pointer::new(cfg());
        let at = vec2(10.0, 10.0);
        assert_eq!(p.update(&down(at), DT), vec![PointerEvent::Pressed { at }]);
        p.update(&down(at), DT);
        let ev = p.update(&up(Some(at)), DT);
        assert_eq!(
            ev,
            vec![
                PointerEvent::Released { at, dragged: false },
                PointerEvent::Tap { at }
            ]
        );
    }

    #[test]
    fn a_drag_starts_only_past_the_threshold() {
        let mut p = Pointer::new(cfg());
        p.update(&down(vec2(0.0, 0.0)), DT);
        assert!(p.update(&down(vec2(0.0, -6.0)), DT).is_empty());
        let ev = p.update(&down(vec2(0.0, -7.5)), DT);
        assert_eq!(
            ev,
            vec![
                PointerEvent::DragStarted {
                    from: vec2(0.0, 0.0),
                    at: vec2(0.0, -7.5)
                },
                PointerEvent::Dragged {
                    at: vec2(0.0, -7.5)
                }
            ]
        );
        let ev = p.update(&up(None), DT);
        assert_eq!(
            ev,
            vec![PointerEvent::Released {
                at: vec2(0.0, -7.5),
                dragged: true
            }]
        );
    }

    #[test]
    fn holding_still_fires_one_long_press_and_no_tap() {
        let mut p = Pointer::new(cfg());
        let at = vec2(5.0, 5.0);
        p.update(&down(at), DT);
        let mut fired = 0;
        for _ in 0..60 {
            fired += p
                .update(&down(at), DT)
                .iter()
                .filter(|e| matches!(e, PointerEvent::LongPress { .. }))
                .count();
        }
        assert_eq!(fired, 1);
        let ev = p.update(&up(Some(at)), DT);
        assert!(!ev.iter().any(|e| matches!(e, PointerEvent::Tap { .. })));
    }

    #[test]
    fn a_click_inside_one_frame_is_still_a_tap() {
        let mut p = Pointer::new(cfg());
        let at = vec2(1.0, 2.0);
        let ev = p.update(&up(Some(at)), DT);
        assert_eq!(ev.last(), Some(&PointerEvent::Tap { at }));
    }

    fn swipe_over(s: &mut Swipes, path: &[Vec2]) -> Option<Swipe> {
        for &at in path {
            assert_eq!(s.update(&down(at)), None, "nothing resolves while down");
        }
        s.update(&up(None))
    }

    #[test]
    fn a_swipe_resolves_four_ways_on_release() {
        let mut s = Swipes::new(20.0);
        let o = vec2(100.0, 100.0);
        for (to, dir) in [
            (vec2(104.0, 60.0), Dir4::Up),
            (vec2(96.0, 140.0), Dir4::Down),
            (vec2(60.0, 108.0), Dir4::Left),
            (vec2(140.0, 92.0), Dir4::Right),
        ] {
            assert_eq!(
                swipe_over(&mut s, &[o, (o + to) / 2.0, to]),
                Some(Swipe { from: o, to, dir })
            );
        }
    }

    #[test]
    fn a_press_shorter_than_min_is_not_a_swipe() {
        let mut s = Swipes::new(20.0);
        let o = vec2(0.0, 0.0);
        assert_eq!(swipe_over(&mut s, &[o, vec2(0.0, -19.0)]), None);
        // A one-frame click: no press was seen down, so nothing to resolve.
        assert_eq!(s.update(&up(Some(o))), None);
    }

    #[test]
    fn a_cancelled_press_does_not_resolve() {
        let mut s = Swipes::new(20.0);
        s.update(&down(Vec2::ZERO));
        s.update(&down(vec2(0.0, 80.0)));
        s.cancel();
        assert_eq!(s.update(&up(None)), None);
    }

    #[test]
    fn hover_survives_a_short_gap_between_targets() {
        let mut h = HoverGrace::new(0.14);
        assert_eq!(h.update(Some(2), DT), Some(2));
        assert_eq!(h.update(None, 0.1), Some(2), "within grace");
        assert_eq!(h.update(Some(3), DT), Some(3), "next card takes over");
        assert_eq!(h.update(None, 0.2), None, "grace spent");
    }
}
