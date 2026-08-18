//! Tap and swipe recognition for touch surfaces.
//!
//! [`crate::frame_tap`] reports a tap the instant a finger *lands*
//! (`TouchPhase::Started`). That is fine for a surface where every touch is a
//! tap, but it makes a swipe impossible: dragging across a grid would select
//! whatever card the finger started on *and* scroll the page. It also means a
//! finger that lands on a button and slides off still activates it, which is
//! not how a native control behaves.
//!
//! [`Gestures`] resolves a touch on *release* instead, and only calls it a tap
//! if the finger barely moved. Travel beyond that, mostly sideways, is a swipe.
//!
//! # Why this is a struct and not a function
//!
//! Recognition needs memory across frames — where the finger landed, how far it
//! has come. The charter keeps widgets stateless, so this state belongs to the
//! host, exactly like a VM: the host owns a `Gestures`, updates it once a
//! frame, and passes the resulting tap into [`crate::Painter::new`].

use macroquad::prelude::*;

/// Movement (px) still counted as a tap rather than a drag. Roughly a finger's
/// natural wobble; below this, releasing where you landed is clearly a tap.
const TAP_SLOP: f32 = 12.0;

/// Travel (px) before a drag counts as a swipe. Deliberately well above
/// [`TAP_SLOP`] so the two can't both fire from one ambiguous gesture.
const SWIPE_MIN: f32 = 60.0;

/// How much more horizontal than vertical a swipe must be. Guards against a
/// vertical scroll attempt being read as a page turn.
const SWIPE_RATIO: f32 = 1.5;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SwipeDir {
    /// Content moved left — the *next* page, like turning a page forward.
    Left,
    Right,
}

/// What the user did this frame.
#[derive(Default, Clone, Copy)]
pub struct Gesture {
    /// A completed tap, at the release point. Feed to [`crate::Painter::new`].
    pub tap: Option<Vec2>,
    pub swipe: Option<SwipeDir>,
}

/// One in-progress pointer — a finger, or the mouse held down.
#[derive(Clone, Copy)]
struct Active {
    id: u64,
    start: Vec2,
    /// Furthest the pointer has been from `start`. Tracked as a maximum rather
    /// than measured at release, so an out-and-back drag doesn't read as a tap
    /// — the user clearly meant to drag, then changed their mind.
    travel: f32,
}

/// Host-owned gesture recogniser. Call [`Self::update`] once per frame, before
/// drawing, and pass `gesture.tap` to the `Painter`.
#[derive(Default)]
pub struct Gestures {
    active: Option<Active>,
}

/// The mouse is one pointer; this id can never collide with a real touch id
/// because `touches()` ids come from the platform's own sequence.
const MOUSE_ID: u64 = u64::MAX;

impl Gestures {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self) -> Gesture {
        self.update_from(
            &touches(),
            is_mouse_button_pressed(MouseButton::Left),
            is_mouse_button_released(MouseButton::Left),
            Vec2::from(mouse_position()),
        )
    }

    /// The recognition itself, with inputs passed in so it can be tested
    /// without a window.
    fn update_from(
        &mut self,
        touches: &[Touch],
        mouse_pressed: bool,
        mouse_released: bool,
        mouse_at: Vec2,
    ) -> Gesture {
        let mut out = Gesture::default();

        // ── touch ───────────────────────────────────────────────────────
        for t in touches {
            match t.phase {
                // Only track one pointer. A second finger during a swipe is
                // almost always a grip adjustment, not a new gesture.
                TouchPhase::Started if self.active.is_none() => {
                    self.active = Some(Active {
                        id: t.id,
                        start: t.position,
                        travel: 0.0,
                    });
                }
                TouchPhase::Moved | TouchPhase::Stationary => {
                    if let Some(a) = &mut self.active {
                        if a.id == t.id {
                            a.travel = a.travel.max(a.start.distance(t.position));
                        }
                    }
                }
                TouchPhase::Ended => {
                    if let Some(a) = self.active.filter(|a| a.id == t.id) {
                        self.active = None;
                        out = resolve(a.start, t.position, a.travel);
                    }
                }
                TouchPhase::Cancelled => {
                    // The platform took the gesture (a system edge swipe, a
                    // call). Neither a tap nor a swipe happened.
                    if self.active.is_some_and(|a| a.id == t.id) {
                        self.active = None;
                    }
                }
                TouchPhase::Started => {}
            }
        }

        // ── mouse ───────────────────────────────────────────────────────
        // Same release semantics, so a desktop drag can exercise swipe in the
        // storybook rather than needing a phone to test a page turn.
        if mouse_pressed && self.active.is_none() {
            self.active = Some(Active {
                id: MOUSE_ID,
                start: mouse_at,
                travel: 0.0,
            });
        } else if let Some(a) = &mut self.active {
            if a.id == MOUSE_ID {
                a.travel = a.travel.max(a.start.distance(mouse_at));
            }
        }
        if mouse_released {
            if let Some(a) = self.active.filter(|a| a.id == MOUSE_ID) {
                self.active = None;
                out = resolve(a.start, mouse_at, a.travel);
            }
        }

        out
    }

    /// True while a pointer is down and has travelled far enough that it is
    /// clearly a drag. Widgets can use this to suppress hover highlighting
    /// mid-swipe, so dragging across a grid doesn't light up every card.
    pub fn dragging(&self) -> bool {
        self.active.is_some_and(|a| a.travel > TAP_SLOP)
    }
}

/// Classify a finished pointer as a tap, a swipe, or neither.
fn resolve(start: Vec2, end: Vec2, travel: f32) -> Gesture {
    if travel <= TAP_SLOP {
        return Gesture {
            tap: Some(end),
            swipe: None,
        };
    }

    let d = end - start;
    let horizontal = d.x.abs();
    if horizontal >= SWIPE_MIN && horizontal > d.y.abs() * SWIPE_RATIO {
        return Gesture {
            tap: None,
            swipe: Some(if d.x < 0.0 {
                SwipeDir::Left
            } else {
                SwipeDir::Right
            }),
        };
    }

    // Dragged, but not far enough or not straight enough. Deliberately
    // nothing: a half-gesture that fired a tap would be worse than one that
    // asks to be repeated.
    Gesture::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(id: u64, phase: TouchPhase, x: f32, y: f32) -> Touch {
        Touch {
            id,
            phase,
            position: vec2(x, y),
        }
    }

    /// Drive a full press → move → release, returning the final gesture.
    fn gesture_over(path: &[(f32, f32)]) -> Gesture {
        let mut g = Gestures::new();
        let (sx, sy) = path[0];
        g.update_from(&[touch(1, TouchPhase::Started, sx, sy)], false, false, Vec2::ZERO);
        for &(x, y) in &path[1..path.len() - 1] {
            g.update_from(&[touch(1, TouchPhase::Moved, x, y)], false, false, Vec2::ZERO);
        }
        let (ex, ey) = path[path.len() - 1];
        g.update_from(&[touch(1, TouchPhase::Ended, ex, ey)], false, false, Vec2::ZERO)
    }

    #[test]
    fn a_still_finger_is_a_tap_on_release() {
        // The whole point of resolving on release: nothing fires while the
        // finger is down, so a drag can still be a drag.
        let mut g = Gestures::new();
        let down = g.update_from(&[touch(1, TouchPhase::Started, 100.0, 100.0)], false, false, Vec2::ZERO);
        assert!(down.tap.is_none(), "a tap must not fire on touch-down");

        let up = g.update_from(&[touch(1, TouchPhase::Ended, 103.0, 101.0)], false, false, Vec2::ZERO);
        assert_eq!(up.tap, Some(vec2(103.0, 101.0)));
        assert!(up.swipe.is_none());
    }

    #[test]
    fn a_long_horizontal_drag_is_a_swipe_and_not_also_a_tap() {
        // The bug this whole module exists to prevent: a swipe across a grid
        // must not select the card it started on.
        let left = gesture_over(&[(300.0, 200.0), (240.0, 202.0), (140.0, 205.0)]);
        assert_eq!(left.swipe, Some(SwipeDir::Left));
        assert!(left.tap.is_none(), "a swipe must never also register a tap");

        let right = gesture_over(&[(140.0, 200.0), (240.0, 198.0), (300.0, 203.0)]);
        assert_eq!(right.swipe, Some(SwipeDir::Right));
        assert!(right.tap.is_none());
    }

    #[test]
    fn a_vertical_drag_is_not_a_page_turn() {
        // Someone trying to scroll the page must not turn it sideways.
        let g = gesture_over(&[(200.0, 100.0), (205.0, 200.0), (210.0, 300.0)]);
        assert_eq!(g.swipe, None);
        assert!(g.tap.is_none());
    }

    #[test]
    fn a_short_drag_is_neither() {
        // Between slop and the swipe threshold: ambiguous, so do nothing
        // rather than guess. Firing a tap here would select a card the user
        // was already dragging away from.
        let g = gesture_over(&[(200.0, 200.0), (215.0, 202.0), (230.0, 203.0)]);
        assert!(g.tap.is_none());
        assert_eq!(g.swipe, None);
    }

    #[test]
    fn an_out_and_back_drag_is_not_a_tap() {
        // Released where it started, but it clearly travelled. Measuring only
        // the endpoints would call this a tap.
        let g = gesture_over(&[(200.0, 200.0), (320.0, 200.0), (200.0, 200.0)]);
        assert!(g.tap.is_none());
        assert_eq!(g.swipe, None);
    }

    #[test]
    fn a_cancelled_touch_does_nothing() {
        // The OS took the gesture — an edge swipe, an incoming call.
        let mut g = Gestures::new();
        g.update_from(&[touch(1, TouchPhase::Started, 300.0, 200.0)], false, false, Vec2::ZERO);
        g.update_from(&[touch(1, TouchPhase::Moved, 140.0, 200.0)], false, false, Vec2::ZERO);
        let out = g.update_from(&[touch(1, TouchPhase::Cancelled, 140.0, 200.0)], false, false, Vec2::ZERO);
        assert!(out.tap.is_none());
        assert_eq!(out.swipe, None);
    }

    #[test]
    fn a_second_finger_does_not_hijack_the_gesture() {
        // A grip adjustment mid-swipe must not restart recognition, or the
        // swipe would be measured from the wrong origin and come up short.
        let mut g = Gestures::new();
        g.update_from(&[touch(1, TouchPhase::Started, 300.0, 200.0)], false, false, Vec2::ZERO);
        g.update_from(
            &[
                touch(2, TouchPhase::Started, 100.0, 400.0),
                touch(1, TouchPhase::Moved, 200.0, 200.0),
            ],
            false,
            false,
            Vec2::ZERO,
        );
        let out = g.update_from(&[touch(1, TouchPhase::Ended, 140.0, 202.0)], false, false, Vec2::ZERO);
        assert_eq!(out.swipe, Some(SwipeDir::Left));
    }

    #[test]
    fn the_mouse_follows_the_same_rules() {
        // So a desktop drag exercises swipe in the storybook.
        let mut g = Gestures::new();
        g.update_from(&[], true, false, vec2(300.0, 200.0));
        g.update_from(&[], false, false, vec2(200.0, 202.0));
        let out = g.update_from(&[], false, true, vec2(140.0, 204.0));
        assert_eq!(out.swipe, Some(SwipeDir::Left));
        assert!(out.tap.is_none());

        let mut g = Gestures::new();
        g.update_from(&[], true, false, vec2(300.0, 200.0));
        let out = g.update_from(&[], false, true, vec2(302.0, 201.0));
        assert_eq!(out.tap, Some(vec2(302.0, 201.0)));
    }
}
