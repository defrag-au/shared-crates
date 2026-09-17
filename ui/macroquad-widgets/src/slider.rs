//! `AmountSlider` — pick an amount from a short ladder of snapping stops, for
//! surfaces with no text input.
//!
//! A row of preset buttons works for three values and falls apart at seven: the
//! targets shrink below a thumb, and a row of equal buttons stops reading as a
//! SCALE — you cannot see that one end is small and the other is reckless. A
//! slider keeps the ladder legible, shows where the chosen amount sits between
//! the extremes, and is one gesture instead of a hunt for a target.
//!
//! ## Stops, never a continuous range
//!
//! macroquad has no text input, so this is the only way to name an amount — and
//! a continuous slider would happily produce 37.4, which nobody meant to ask
//! for. The value is always exactly one of the caller's stops; dragging snaps
//! to the nearest.
//!
//! Labels come from the CALLER. What counts as a meaningful amount is a
//! decision about the surface, not about sliders — a widget that formatted
//! "420K" on its own would be making that call for every future caller.
//!
//! ## Why it is handed a drag position
//!
//! [`crate::Painter`] carries only the RESOLVED tap, which by design does not
//! exist until release — that is what makes swipes possible. A slider has to
//! follow the pointer while it is still down, so the host passes
//! [`crate::Gesture::drag`] in alongside.

use macroquad::prelude::*;

use crate::{Painter, draw_rounded_rect, painter::with_alpha};

/// Distance from the rect's edges to the first and last stop, so an end handle
/// is not half outside the control.
const EDGE: f32 = 16.0;
/// Clear space required between two labels before both may be drawn.
const LABEL_GAP: f32 = 8.0;
const LABEL_SIZE: f32 = 11.0;
const TRACK_H: f32 = 5.0;
const HANDLE_R: f32 = 9.0;

/// One position on the ladder.
pub struct SliderStop {
    pub value: u64,
    /// What to print under the tick. Short — these sit side by side.
    pub label: String,
}

impl SliderStop {
    pub fn new(value: u64, label: impl Into<String>) -> Self {
        Self {
            value,
            label: label.into(),
        }
    }
}

pub struct AmountSliderVm<'a> {
    /// Ascending. The slider only ever selects one of these.
    pub stops: &'a [SliderStop],
    /// Index into `stops`. Clamped everywhere it is used, so a caller whose
    /// index drifts past the end renders the last stop rather than panicking.
    pub index: usize,
    /// A disabled slider still DRAWS — the ladder is information even when it
    /// cannot be moved, and blanking it would make a disconnected wallet look
    /// like a broken one.
    pub enabled: bool,
}

impl AmountSliderVm<'_> {
    /// The amount currently chosen. 0 when there are no stops at all, which is
    /// the only honest answer to "how much" from an empty ladder.
    pub fn value(&self) -> u64 {
        match self.stops.len() {
            0 => 0,
            n => self.stops[self.index.min(n - 1)].value,
        }
    }
}

/// The x a stop's tick sits at.
fn stop_x(rect: Rect, count: usize, index: usize) -> f32 {
    let (a, b) = (rect.x + EDGE, rect.x + rect.w - EDGE);
    if count <= 1 {
        return a;
    }
    let t = index.min(count - 1) as f32 / (count - 1) as f32;
    a + (b - a) * t
}

/// The stop nearest an x position, clamped to the ends.
///
/// Nearest rather than "the one under the finger": at the extremes there is no
/// stop under the finger at all, and a slider that refuses to reach its own end
/// because you overshot by four pixels feels broken.
fn nearest_stop(rect: Rect, count: usize, x: f32) -> usize {
    if count <= 1 {
        return 0;
    }
    let (a, b) = (rect.x + EDGE, rect.x + rect.w - EDGE);
    if b <= a {
        return 0;
    }
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    (t * (count - 1) as f32).round() as usize
}

/// Draw the ladder and report a newly chosen index, or `None` if unchanged.
///
/// `drag` is [`crate::Gesture::drag`] — where the pointer is while still down.
pub fn amount_slider(
    p: &Painter,
    vm: &AmountSliderVm,
    drag: Option<Vec2>,
    rect: Rect,
) -> Option<usize> {
    let count = vm.stops.len();
    if count == 0 {
        return None;
    }
    let index = vm.index.min(count - 1);
    let t = &p.theme;

    // A press or drag ANYWHERE on the control moves the handle — you never have
    // to catch the handle itself, which is what makes it workable with a thumb.
    // The release frame reports a tap and no drag, and by then the caller has
    // already taken the drag's value, so the tap resolves to the same stop and
    // changes nothing.
    let mut chosen = None;
    if vm.enabled {
        for at in drag.into_iter().chain(p.tap) {
            if rect.contains(at) {
                let i = nearest_stop(rect, count, at.x);
                if i != index {
                    chosen = Some(i);
                }
            }
        }
    }

    let dim = |c: Color| if vm.enabled { c } else { with_alpha(c, 0.35) };
    let track_y = rect.y + rect.h * 0.34;
    let (a, b) = (rect.x + EDGE, rect.x + rect.w - EDGE);
    let handle_x = stop_x(rect, count, index);

    draw_rounded_rect(a, track_y, b - a, TRACK_H, TRACK_H * 0.5, dim(t.track));
    if handle_x > a {
        draw_rounded_rect(
            a,
            track_y,
            handle_x - a,
            TRACK_H,
            TRACK_H * 0.5,
            dim(t.accent),
        );
    }

    for i in 0..count {
        let x = stop_x(rect, count, i);
        let passed = i <= index;
        draw_circle(
            x,
            track_y + TRACK_H * 0.5,
            2.0,
            dim(if passed { t.accent } else { t.muted }),
        );
    }

    draw_circle(handle_x, track_y + TRACK_H * 0.5, HANDLE_R, dim(t.accent));
    draw_circle(
        handle_x,
        track_y + TRACK_H * 0.5,
        HANDLE_R * 0.45,
        dim(t.bg),
    );

    // Labels, left to right, skipping any that would collide with one already
    // placed. Overlapping labels are worse than absent ones — the selected stop
    // is reserved FIRST so it can never be the one dropped.
    let label_top = track_y + TRACK_H + HANDLE_R + 2.0;
    let mut spans: Vec<(f32, f32)> = Vec::with_capacity(count);
    let span_of = |i: usize| {
        let w = p.measure(&vm.stops[i].label, LABEL_SIZE).width;
        let cx = stop_x(rect, count, i);
        (cx - w * 0.5, cx + w * 0.5)
    };
    let mut draw_order: Vec<usize> = vec![index];
    spans.push(span_of(index));
    for i in 0..count {
        if i == index {
            continue;
        }
        let (l, r) = span_of(i);
        if spans
            .iter()
            .all(|(sl, sr)| r + LABEL_GAP < *sl || l > *sr + LABEL_GAP)
        {
            spans.push((l, r));
            draw_order.push(i);
        }
    }
    for i in draw_order {
        let (l, _) = span_of(i);
        let colour = if i == index { t.fg } else { t.muted };
        p.text(
            &vm.stops[i].label,
            l,
            p.top_baseline(label_top, LABEL_SIZE),
            LABEL_SIZE,
            dim(colour),
        );
    }

    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::new(20.0, 100.0, 300.0, 44.0)
    }

    fn ladder() -> Vec<SliderStop> {
        [10u64, 25, 50, 69, 100, 250, 420]
            .iter()
            .map(|v| SliderStop::new(*v, v.to_string()))
            .collect()
    }

    #[test]
    fn the_ends_of_the_ladder_are_reachable() {
        let r = rect();
        let n = 7;
        assert_eq!(stop_x(r, n, 0), r.x + EDGE);
        assert_eq!(stop_x(r, n, n - 1), r.x + r.w - EDGE);
    }

    /// Overshooting the end must still select the end. A slider that refuses to
    /// reach 420 because the thumb went four pixels past it feels broken.
    #[test]
    fn dragging_past_either_end_clamps() {
        let r = rect();
        assert_eq!(nearest_stop(r, 7, r.x - 500.0), 0);
        assert_eq!(nearest_stop(r, 7, r.x + r.w + 500.0), 6);
    }

    #[test]
    fn a_position_snaps_to_its_nearest_stop() {
        let r = rect();
        for i in 0..7 {
            let exact = stop_x(r, 7, i);
            assert_eq!(nearest_stop(r, 7, exact), i, "exact hit on {i}");
            // Nudged either way, still the same stop.
            assert_eq!(nearest_stop(r, 7, exact - 8.0), i, "left of {i}");
            assert_eq!(nearest_stop(r, 7, exact + 8.0), i, "right of {i}");
        }
    }

    #[test]
    fn stops_run_left_to_right_without_repeating() {
        let r = rect();
        let xs: Vec<f32> = (0..7).map(|i| stop_x(r, 7, i)).collect();
        for pair in xs.windows(2) {
            assert!(pair[1] > pair[0], "{xs:?}");
        }
    }

    /// Degenerate ladders must not divide by zero or panic.
    #[test]
    fn a_single_stop_is_not_a_division_by_zero() {
        let r = rect();
        assert_eq!(stop_x(r, 1, 0), r.x + EDGE);
        assert_eq!(nearest_stop(r, 1, 999.0), 0);
        assert_eq!(nearest_stop(r, 0, 999.0), 0);
    }

    #[test]
    fn the_value_is_always_one_of_the_stops() {
        let stops = ladder();
        for (i, expected) in [10u64, 25, 50, 69, 100, 250, 420].iter().enumerate() {
            let vm = AmountSliderVm {
                stops: &stops,
                index: i,
                enabled: true,
            };
            assert_eq!(vm.value(), *expected);
        }
    }

    /// An index past the end renders the last stop rather than panicking — the
    /// ladder can shrink under a caller holding an older index.
    #[test]
    fn an_index_past_the_end_clamps_to_the_last_stop() {
        let stops = ladder();
        let vm = AmountSliderVm {
            stops: &stops,
            index: 99,
            enabled: true,
        };
        assert_eq!(vm.value(), 420);

        let empty: Vec<SliderStop> = Vec::new();
        let vm = AmountSliderVm {
            stops: &empty,
            index: 0,
            enabled: true,
        };
        assert_eq!(vm.value(), 0, "an empty ladder cannot name an amount");
    }
}
