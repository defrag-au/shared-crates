//! `motion` — a sprite's animation measured as it plays: how long each pose shows at 60 Hz, how far the moving part travels at each change, whether the mass holds, and the path the moving part traces, with the problems that make motion look cheap named.
//!
//! An animation is judged by eye in motion, and a review that reads still
//! images and text cannot watch it. So this module turns the motion into
//! numbers a still review can check, against the craft's rules of thumb:
//!
//! - **Spacing.** How far the pixels travel at each pose change, in art
//!   pixels: for each cell that changed, the distance to the nearest cell
//!   of its colour in the other pose. Over [`POP`] the part teleports: the
//!   eye sees two pictures, not one thing moving, and the cure is an
//!   in-between pose. The moving part's centre is reported too, and
//!   neighbouring changes that differ by more than [`UNEVEN`] times jerk.
//! - **Timing.** Each pose's hold in display frames at [`HZ`]. A hold under
//!   [`FLICKER`] frames strobes: the pose is barely seen (or, under one,
//!   never drawn at all).
//! - **Mass.** The silhouette's area at each pose. A change over [`MASS`]
//!   reads as the body swelling and shrinking rather than moving.
//! - **Arcs.** Living things move on arcs. The moving part's centre, pose
//!   by pose (the centroid of the cells that are not the same in every
//!   frame), should trace a curve; on a straight line it reads as a piston.
//! - **In step.** A gait with no jitter plays every clone in lockstep.
//!
//! [`Cycle::text`] leads with the problems, then the timing, each change,
//! and the path. The sprite sheet (`--sprites`) draws the same cycle: the
//! poses as played with the one before ghosted, and the path over all of
//! them. Pure: no window.

use crate::gait::Gait;
use crate::pixel::Grid;

/// The display rate the cycle is played at.
pub const HZ: f32 = 60.0;
/// The mean travel, in art pixels, past which a pose change is a pop.
pub const POP: f32 = 2.0;
/// Neighbouring changes whose travel differs by more than this many times
/// are uneven spacing.
pub const UNEVEN: f32 = 3.0;
/// Pose changes a second below which a part that travels more than a pixel
/// a change reads as a slideshow: pixel art moves at 8 to 12 poses a
/// second, holds on the extremes included.
pub const SLIDESHOW: f32 = 6.0;
/// A hold shorter than this many display frames flickers.
pub const FLICKER: u32 = 2;
/// The share the silhouette's area may change by between poses.
pub const MASS: f32 = 0.12;
/// Below this ratio of the path's narrow spread to its long one, the moving
/// part travels on a line.
pub const LINE: f32 = 0.2;
/// Degrees the moving part's long axis must turn across the cycle for it to
/// count as pivoting rather than sliding.
pub const PIVOT: f32 = 15.0;

/// What changed from one pose to the next.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shift {
    /// The share of the cells drawn in either pose that changed.
    pub changed: f32,
    /// The mean distance, in art pixels, from each changed cell to the
    /// nearest cell of its colour in the other pose: how far the pixels had
    /// to travel.
    pub travel: f32,
    /// The furthest any changed cell had to come.
    pub travel_max: f32,
    /// The opaque cells before and after.
    pub area: (usize, usize),
}

/// How far `b` moved from `a`. Recoloured cells count as both vanished and
/// appeared in place, so a colour shimmer travels nothing.
pub fn shift(a: &Grid, b: &Grid) -> Shift {
    let (w, h) = (a.w.min(b.w), a.h.min(b.h));
    let mut gone: Vec<(f32, f32)> = Vec::new();
    let mut came: Vec<(f32, f32)> = Vec::new();
    let (mut drawn, mut changed) = (0usize, 0usize);
    let (mut area_a, mut area_b) = (0usize, 0usize);
    for y in 0..h {
        for x in 0..w {
            let (pa, pb) = (a.at(x, y), b.at(x, y));
            area_a += pa.is_some() as usize;
            area_b += pb.is_some() as usize;
            if pa.is_none() && pb.is_none() {
                continue;
            }
            drawn += 1;
            if pa == pb {
                continue;
            }
            changed += 1;
            let p = (x as f32, y as f32);
            if pa.is_some() {
                gone.push(p);
            }
            if pb.is_some() {
                came.push(p);
            }
        }
    }
    // How far a pixel had to come: the nearest cell of its colour in the
    // other pose. A lower bound on the flow, so a part that sweeps past
    // more of its own colour reads a little short.
    let nearest = |p: (f32, f32), colour: Option<[u8; 4]>, other: &Grid| {
        let mut best = f32::INFINITY;
        for y in 0..h {
            for x in 0..w {
                if other.at(x, y).is_some() && other.at(x, y) == colour {
                    let d = ((p.0 - x as f32).powi(2) + (p.1 - y as f32).powi(2)).sqrt();
                    best = best.min(d);
                }
            }
        }
        best
    };
    let (mut sum, mut count, mut max) = (0.0f32, 0usize, 0.0f32);
    for (cells, this, other) in [(&gone, a, b), (&came, b, a)] {
        for p in cells.iter() {
            let d = nearest(*p, this.at(p.0 as usize, p.1 as usize), other);
            if d.is_finite() {
                sum += d;
                count += 1;
                max = max.max(d);
            }
        }
    }
    Shift {
        changed: if drawn == 0 {
            0.0
        } else {
            changed as f32 / drawn as f32
        },
        travel: if count == 0 { 0.0 } else { sum / count as f32 },
        travel_max: max,
        area: (area_a, area_b),
    }
}

/// One pose shown for a run of display frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
    pub frame: usize,
    /// Display frames at [`HZ`].
    pub ticks: u32,
}

/// A pose change as played: from the run before to this one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    pub from: usize,
    pub to: usize,
    pub shift: Shift,
}

/// A sprite's animation played once through its gait, measured.
#[derive(Clone, Debug, PartialEq)]
pub struct Cycle {
    /// The poses in the order shown, with their holds; the last runs into
    /// the first.
    pub runs: Vec<Run>,
    /// The change into each run from the one before it (into run 0 from the
    /// last).
    pub steps: Vec<Step>,
    /// The moving part's centre in each frame, by frame index: `None` for a
    /// frame with nothing in the moving part.
    pub centres: Vec<Option<(f32, f32)>>,
    /// How far, in degrees, the moving part's long axis turns across the
    /// frames: a pivoting tail turns, a sliding one does not.
    pub turn: f32,
    /// Each gait step's hold, `(frame, display frames)`, unrounded: a step
    /// under one display frame is never drawn, so never appears in `runs`.
    pub holds: Vec<(usize, f32)>,
    /// The art pixel's size in design units.
    pub scale: f32,
    /// The gait's period and jitter.
    pub period: f32,
    pub spread: f32,
    /// What looks wrong, worst first, each a sentence naming what to do.
    pub problems: Vec<String>,
}

impl Cycle {
    /// Play `frames` once through `gait` at [`HZ`] and measure it.
    pub fn play(frames: &[Grid], gait: &Gait, scale: f32) -> Self {
        let ticks = (gait.period() * HZ).round().max(1.0) as u32;
        let mut runs: Vec<Run> = Vec::new();
        for i in 0..ticks {
            let frame = gait
                .at_beat(i as f32 / ticks as f32)
                .frame
                .min(frames.len().saturating_sub(1));
            match runs.last_mut() {
                Some(r) if r.frame == frame => r.ticks += 1,
                _ => runs.push(Run { frame, ticks: 1 }),
            }
        }
        // The cycle wraps: a last run on the first run's pose is one hold.
        if runs.len() > 1 && runs[0].frame == runs[runs.len() - 1].frame {
            let last = runs.pop().expect("more than one run");
            runs[0].ticks += last.ticks;
        }
        let steps: Vec<Step> = if runs.len() < 2 {
            Vec::new()
        } else {
            (0..runs.len())
                .map(|i| {
                    let from = runs[(i + runs.len() - 1) % runs.len()].frame;
                    let to = runs[i].frame;
                    Step {
                        from,
                        to,
                        shift: shift(&frames[from], &frames[to]),
                    }
                })
                .collect()
        };
        let centres = moving_centres(frames);
        let turn = moving_turn(frames);
        let total: f32 = gait.steps().iter().map(|(_, w)| w).sum();
        let holds = gait
            .steps()
            .iter()
            .map(|(f, w)| (*f, w / total.max(1e-6) * gait.period() * HZ))
            .collect();
        let mut cycle = Self {
            runs,
            steps,
            centres,
            turn,
            holds,
            scale,
            period: gait.period(),
            spread: gait.spread(),
            problems: Vec::new(),
        };
        cycle.problems = cycle.find_problems(frames.len());
        cycle
    }

    fn find_problems(&self, frames: usize) -> Vec<String> {
        let mut out = Vec::new();
        for s in &self.steps {
            if s.shift.travel > POP {
                out.push(format!(
                    "pop: frame {} to {} moves the part {:.1} art px at once (over {POP}); draw an in-between pose",
                    s.from, s.to, s.shift.travel
                ));
            }
        }
        let n = self.steps.len();
        let mean_travel = self.steps.iter().map(|s| s.shift.travel).sum::<f32>() / n.max(1) as f32;
        if n > 0 && self.changes_per_second() < SLIDESHOW && mean_travel > 1.0 {
            let longest = self.runs.iter().map(|r| r.ticks).max().unwrap_or(0);
            out.push(format!(
                "slideshow: {:.1} pose changes a second (under {SLIDESHOW}), the longest held {:.0} ms, each moving {mean_travel:.1} px; add in-betweens rather than slowing the poses",
                self.changes_per_second(),
                longest as f32 * 1000.0 / HZ
            ));
        }
        for i in 0..n {
            let (a, b) = (self.steps[i], self.steps[(i + 1) % n]);
            let (lo, hi) = if a.shift.travel < b.shift.travel {
                (a.shift.travel, b.shift.travel)
            } else {
                (b.shift.travel, a.shift.travel)
            };
            if n > 2 && hi > 1.0 && hi > lo.max(0.25) * UNEVEN {
                out.push(format!(
                    "uneven: {} to {} travels {:.1} px, {} to {} {:.1} px; space the poses more evenly",
                    a.from, a.to, a.shift.travel, b.from, b.to, b.shift.travel
                ));
            }
        }
        for (frame, ticks) in &self.holds {
            if *ticks < FLICKER as f32 {
                out.push(format!(
                    "flicker: frame {frame} holds for {ticks:.1} display frames at {HZ} Hz{}; hold it longer or drop it",
                    if *ticks < 1.0 { ", so is never drawn" } else { "" }
                ));
            }
        }
        for s in &self.steps {
            let (a, b) = s.shift.area;
            let big = a.max(b).max(1) as f32;
            let d = (a as f32 - b as f32).abs() / big;
            if d > MASS {
                out.push(format!(
                    "mass: frame {} to {} changes the silhouette by {:.0}% ({a} to {b} px); keep the volume",
                    s.from,
                    s.to,
                    d * 100.0
                ));
            }
        }
        // A part that pivots traces an arc even when the arc is shallow (a
        // tail swung 25° either way moves sideways a tenth as far as up and
        // down), so a flat path is only a piston when the part does not
        // turn as it goes.
        if let Some(ratio) = self.path_spread()
            && frames >= 3
            && ratio < LINE
            && self.turn < PIVOT
        {
            out.push(format!(
                "line: the moving part slides on a straight line (narrow/long spread {ratio:.2}) without turning ({:.0}°); pivot it, so its tip swings on an arc",
                self.turn
            ));
        }
        if self.runs.len() > 1 && self.spread == 0.0 {
            out.push(
                "lockstep: the gait has no jitter, so every clone moves in step; add Gait::jitter"
                    .to_string(),
            );
        }
        out
    }

    /// The moving part's path through the poses as played: the centre of
    /// each run's frame, in order.
    pub fn path(&self) -> Vec<(f32, f32)> {
        self.runs
            .iter()
            .filter_map(|r| self.centres.get(r.frame).copied().flatten())
            .collect()
    }

    /// The path's narrow spread over its long one (`0` a line, `1` round),
    /// or `None` when it spans under a pixel.
    pub fn path_spread(&self) -> Option<f32> {
        let mut pts = self.path();
        pts.dedup();
        if pts.len() < 2 {
            return None;
        }
        let n = pts.len() as f32;
        let (mx, my) = pts
            .iter()
            .fold((0.0, 0.0), |a, p| (a.0 + p.0 / n, a.1 + p.1 / n));
        let (mut xx, mut yy, mut xy) = (0.0f32, 0.0f32, 0.0f32);
        for (x, y) in &pts {
            xx += (x - mx).powi(2) / n;
            yy += (y - my).powi(2) / n;
            xy += (x - mx) * (y - my) / n;
        }
        let mid = (xx + yy) / 2.0;
        let r = (((xx - yy) / 2.0).powi(2) + xy * xy).sqrt();
        let (long, narrow) = (mid + r, (mid - r).max(0.0));
        if long.sqrt() < 0.5 {
            return None;
        }
        Some((narrow / long).sqrt())
    }

    /// Distinct pose changes a second, at the nominal rate.
    pub fn changes_per_second(&self) -> f32 {
        self.steps.len() as f32 / self.period
    }

    /// The report: problems first, then timing, each change, and the path.
    pub fn text(&self) -> String {
        let mut out = String::new();
        if self.problems.is_empty() {
            out.push_str("problems: none\n");
        } else {
            out.push_str("problems:\n");
            for p in &self.problems {
                out.push_str(&format!("- {p}\n"));
            }
        }
        let timing: Vec<String> = self
            .runs
            .iter()
            .map(|r| {
                format!(
                    "f{} {}f/{:.0}ms",
                    r.frame,
                    r.ticks,
                    r.ticks as f32 * 1000.0 / HZ
                )
            })
            .collect();
        out.push_str(&format!(
            "timing at {HZ} Hz over {:.2}s, {:.1} changes/s, clones ±{:.0}% rate: {}\n",
            self.period,
            self.changes_per_second(),
            self.spread * 100.0,
            timing.join(" > ")
        ));
        for s in &self.steps {
            let (a, b) = s.shift.area;
            let part = match (
                self.centres.get(s.from).copied().flatten(),
                self.centres.get(s.to).copied().flatten(),
            ) {
                (Some(p), Some(q)) => format!(
                    ", part centre moves {:.1} px",
                    ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt()
                ),
                _ => String::new(),
            };
            out.push_str(&format!(
                "  {} to {}: {:.0}% changed, travel {:.1} px (max {:.1}; {:.1} design units){part}, area {a} to {b}\n",
                s.from,
                s.to,
                s.shift.changed * 100.0,
                s.shift.travel,
                s.shift.travel_max,
                s.shift.travel * self.scale
            ));
        }
        let path: Vec<String> = self
            .path()
            .iter()
            .map(|(x, y)| format!("({x:.1}, {y:.1})"))
            .collect();
        if !path.is_empty() {
            out.push_str(&format!(
                "moving part's centre as played: {}{}, its long axis turning {:.0}°\n",
                path.join(" > "),
                self.path_spread()
                    .map_or(String::new(), |r| format!(", narrow/long spread {r:.2}")),
                self.turn
            ));
        }
        out
    }
}

/// The range, in degrees, of the moving part's long axis across the frames
/// (0 when the part is too round or too small to have one).
fn moving_turn(frames: &[Grid]) -> f32 {
    let Some(first) = frames.first() else {
        return 0.0;
    };
    let (w, h) = frames
        .iter()
        .fold((first.w, first.h), |(w, h), g| (w.min(g.w), h.min(g.h)));
    let moving = |x: usize, y: usize| frames.iter().any(|g| g.at(x, y) != first.at(x, y));
    let angles: Vec<f32> = frames
        .iter()
        .filter_map(|g| {
            let pts: Vec<(f32, f32)> = (0..h)
                .flat_map(|y| (0..w).map(move |x| (x, y)))
                .filter(|&(x, y)| g.at(x, y).is_some() && moving(x, y))
                .map(|(x, y)| (x as f32, y as f32))
                .collect();
            if pts.len() < 3 {
                return None;
            }
            let n = pts.len() as f32;
            let (mx, my) = pts
                .iter()
                .fold((0.0, 0.0), |a, p| (a.0 + p.0 / n, a.1 + p.1 / n));
            let (mut xx, mut yy, mut xy) = (0.0f32, 0.0f32, 0.0f32);
            for (x, y) in &pts {
                xx += (x - mx).powi(2);
                yy += (y - my).powi(2);
                xy += (x - mx) * (y - my);
            }
            // Too round to have a direction.
            let r = (((xx - yy) / 2.0).powi(2) + xy * xy).sqrt();
            if r < 0.2 * (xx + yy) / 2.0 {
                return None;
            }
            Some((0.5 * (2.0 * xy).atan2(xx - yy)).to_degrees())
        })
        .collect();
    if angles.len() < 2 {
        return 0.0;
    }
    let lo = angles.iter().copied().fold(f32::INFINITY, f32::min);
    let hi = angles.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    hi - lo
}

/// The centre of the moving part in each frame: the opaque cells, among
/// those that are not the same in every frame.
fn moving_centres(frames: &[Grid]) -> Vec<Option<(f32, f32)>> {
    let Some(first) = frames.first() else {
        return Vec::new();
    };
    let (w, h) = frames
        .iter()
        .fold((first.w, first.h), |(w, h), g| (w.min(g.w), h.min(g.h)));
    let moving = |x: usize, y: usize| frames.iter().any(|g| g.at(x, y) != first.at(x, y));
    frames
        .iter()
        .map(|g| {
            let (mut sx, mut sy, mut n) = (0.0f32, 0.0f32, 0usize);
            for y in 0..h {
                for x in 0..w {
                    if g.at(x, y).is_some() && moving(x, y) {
                        sx += x as f32 + 0.5;
                        sy += y as f32 + 0.5;
                        n += 1;
                    }
                }
            }
            (n > 0).then(|| (sx / n as f32, sy / n as f32))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pixel::Palette;

    fn grid(rows: &[&str]) -> Grid {
        Grid::parse(rows, &Palette::new().with('a', 0x808080), 0).expect("parses")
    }

    /// A block that steps one pixel travels one; one that jumps five pops.
    #[test]
    fn travel_is_how_far_the_part_moved() {
        let a = grid(&["aa......", "aa......"]);
        let step = grid(&[".aa.....", ".aa....."]);
        let jump = grid(&[".....aa.", ".....aa."]);
        let s = shift(&a, &step);
        assert!((0.9..=1.1).contains(&s.travel), "{s:?}");
        let j = shift(&a, &jump);
        assert!(j.travel > 4.0, "{j:?}");
        assert_eq!(j.area, (4, 4));
        assert_eq!(shift(&a, &a).travel, 0.0);
    }

    /// Played at 60 Hz, holds come out as display frames, the wrap is one
    /// hold, and a two-pose snap with no in-between is a pop.
    #[test]
    fn a_cycle_names_pops_flicker_and_lockstep() {
        let a = grid(&["aa......"]);
        let b = grid(&[".....aa."]);
        let gait = Gait::new(&[(0, 1.0), (1, 1.0)], 0.5);
        let c = Cycle::play(&[a.clone(), b.clone()], &gait, 1.0);
        assert_eq!(c.runs.iter().map(|r| r.ticks).sum::<u32>(), 30);
        assert_eq!(c.runs.len(), 2);
        assert!(c.problems.iter().any(|p| p.starts_with("pop")), "{c:?}");
        assert!(c.problems.iter().any(|p| p.starts_with("lockstep")));

        let flick = Gait::new(&[(0, 30.0), (1, 1.0)], 0.5).jitter(0.1);
        let c = Cycle::play(&[a, b], &flick, 1.0);
        assert!(c.problems.iter().any(|p| p.starts_with("flicker")), "{c:?}");
        assert!(!c.problems.iter().any(|p| p.starts_with("lockstep")));
    }

    /// A part that goes up and down a column travels on a line; one that
    /// swings round a corner travels on an arc.
    #[test]
    fn a_straight_stroke_is_named_and_an_arc_is_not() {
        let line = [
            grid(&["a..", "...", "..."]),
            grid(&["...", "a..", "..."]),
            grid(&["...", "...", "a.."]),
        ];
        let gait = Gait::new(&[(0, 1.0), (1, 1.0), (2, 1.0), (1, 1.0)], 1.0).jitter(0.1);
        let c = Cycle::play(&line, &gait, 1.0);
        assert!(c.problems.iter().any(|p| p.starts_with("line")), "{c:?}");

        let arc = [
            grid(&["a..", "...", "..."]),
            grid(&["...", "...", ".a."]),
            grid(&["..a", "...", "..."]),
        ];
        let c = Cycle::play(&arc, &gait, 1.0);
        assert!(!c.problems.iter().any(|p| p.starts_with("line")), "{c:?}");
        assert!(c.path_spread().is_some_and(|r| r > 0.3));
    }

    /// A bar that slides up and down is a piston; the same bar pivoting
    /// about one end, its tip barely moving sideways, is an arc.
    #[test]
    fn a_pivot_is_an_arc_and_a_slide_is_not() {
        let gait = Gait::new(&[(0, 1.0), (1, 1.0), (2, 1.0), (1, 1.0)], 1.0).jitter(0.1);
        let slide = [
            grid(&["aaaa", "....", "....", "....", "...."]),
            grid(&["....", "....", "aaaa", "....", "...."]),
            grid(&["....", "....", "....", "....", "aaaa"]),
        ];
        let c = Cycle::play(&slide, &gait, 1.0);
        assert!(c.problems.iter().any(|p| p.starts_with("line")), "{c:?}");

        let pivot = [
            grid(&["a....", ".aa..", "...aa", ".....", "....."]),
            grid(&[".....", ".....", "aaaaa", ".....", "....."]),
            grid(&[".....", ".....", "...aa", ".aa..", "a...."]),
        ];
        let c = Cycle::play(&pivot, &gait, 1.0);
        assert!(c.turn > PIVOT, "{c:?}");
        assert!(!c.problems.iter().any(|p| p.starts_with("line")), "{c:?}");
    }

    /// A pose that swells the silhouette is named.
    #[test]
    fn mass_that_swells_is_named() {
        let a = grid(&["aa..", "aa.."]);
        let b = grid(&["aaaa", "aaaa"]);
        let gait = Gait::even(2, 0.5).jitter(0.1);
        let c = Cycle::play(&[a, b], &gait, 1.0);
        assert!(c.problems.iter().any(|p| p.starts_with("mass")), "{c:?}");
        assert!(c.text().starts_with("problems:\n- "));
    }
}
