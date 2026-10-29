//! Where a frame's time went: named scopes, nested, per frame.
//!
//! [`Frames`](crate::Frames) says a frame cost 7 ms. This says what the 7 ms
//! was: every scope the frame entered, nested, with its total and self time.
//!
//! No clock and no framework, like the rest of this crate: the host passes
//! `now_ms` from whatever clock its runtime has (macroquad's
//! `miniquad::date::now`, which needs no wasm-bindgen; `Instant` natively).
//! Names are `&'static str`, so entering a scope never allocates, and frame
//! buffers are reused once the ring is full.
//!
//! Two readings, both as text, because text can be searched, diffed and
//! pasted into a bug report, and a capture's report is text:
//!
//! - [`Scopes::busiest`]: the most expensive recent frame, the one worth
//!   reading. "Latest" is a poor default: it is usually a cheap frame, which
//!   is exactly why the expensive ones are worth finding.
//! - [`Scopes::window`]: every scope's mean and worst cost per frame across
//!   the recent frames, for "what does this always cost".
//!
//! Siblings with the same name merge into one row with a call count, so a
//! loop that draws 228 tiles reads as one line, `228 × tile`, not 228.

use std::collections::VecDeque;
use std::fmt::Write;

/// How many frames the ring keeps: two seconds at 60 Hz.
pub const SCOPE_FRAMES: usize = 120;

/// One scope as it ran.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScopeRecord {
    pub name: &'static str,
    /// 0 for the frame's top-level scopes.
    pub depth: u16,
    pub start_ms: f64,
    pub dur_ms: f64,
}

/// One frame's scopes, in the order they began.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScopeFrame {
    /// Which frame, counting from the first recorded.
    pub index: u64,
    /// What kind of frame it was (`"frame"`, `"update only"`): frames that
    /// did different work are compared with their own kind.
    pub label: &'static str,
    pub start_ms: f64,
    pub total_ms: f64,
    /// Time in the frame's top-level wait scopes ([`Scopes::wait_on`]): the
    /// part of `total_ms` that was waiting, not working.
    pub wait_ms: f64,
    pub scopes: Vec<ScopeRecord>,
}

impl ScopeFrame {
    /// What the frame cost, less its waiting: the number that says whether
    /// work is too expensive.
    pub fn work_ms(&self) -> f64 {
        (self.total_ms - self.wait_ms).max(0.0)
    }
}

/// The recorder.
#[derive(Debug, Default)]
pub struct Scopes {
    current: Option<ScopeFrame>,
    /// Indices into `current.scopes` of the scopes still open.
    open: Vec<usize>,
    frames: VecDeque<ScopeFrame>,
    count: u64,
    /// Scope buffers from frames that left the ring, for the next frames.
    spare: Vec<Vec<ScopeRecord>>,
    /// Top-level scope names that are waiting, not working.
    waits: Vec<&'static str>,
}

impl Scopes {
    pub fn new() -> Self {
        Self::default()
    }

    /// Count a top-level scope called `name` as waiting (for the display, for
    /// a vsync): it stays in the tree, but not in a frame's work, so the
    /// busiest frame is the one that worked hardest, not the one that waited
    /// longest.
    pub fn wait_on(mut self, name: &'static str) -> Self {
        self.waits.push(name);
        self
    }

    /// Start a frame. An unfinished frame before it is finished first.
    pub fn begin_frame(&mut self, label: &'static str, now_ms: f64) {
        if self.current.is_some() {
            self.end_frame(now_ms);
        }
        let mut scopes = self.spare.pop().unwrap_or_default();
        scopes.clear();
        self.current = Some(ScopeFrame {
            index: self.count,
            label,
            start_ms: now_ms,
            total_ms: 0.0,
            wait_ms: 0.0,
            scopes,
        });
        self.open.clear();
    }

    /// Enter a scope. Outside a frame, nothing is recorded.
    pub fn enter(&mut self, name: &'static str, now_ms: f64) {
        let Some(frame) = &mut self.current else {
            return;
        };
        frame.scopes.push(ScopeRecord {
            name,
            depth: self.open.len() as u16,
            start_ms: now_ms,
            dur_ms: 0.0,
        });
        self.open.push(frame.scopes.len() - 1);
    }

    /// Leave the innermost open scope.
    pub fn exit(&mut self, now_ms: f64) {
        let (Some(frame), Some(i)) = (&mut self.current, self.open.pop()) else {
            return;
        };
        let s = &mut frame.scopes[i];
        s.dur_ms = (now_ms - s.start_ms).max(0.0);
    }

    /// Finish the frame: close anything left open and keep it in the ring.
    pub fn end_frame(&mut self, now_ms: f64) {
        while !self.open.is_empty() {
            self.exit(now_ms);
        }
        let Some(mut frame) = self.current.take() else {
            return;
        };
        frame.total_ms = (now_ms - frame.start_ms).max(0.0);
        frame.wait_ms = frame
            .scopes
            .iter()
            .filter(|s| s.depth == 0 && self.waits.contains(&s.name))
            .map(|s| s.dur_ms)
            .sum();
        self.count += 1;
        if self.frames.len() == SCOPE_FRAMES
            && let Some(old) = self.frames.pop_front()
        {
            self.spare.push(old.scopes);
        }
        self.frames.push_back(frame);
    }

    /// The recent frames, oldest first.
    pub fn frames(&self) -> impl Iterator<Item = &ScopeFrame> {
        self.frames.iter()
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// The recent frame of a kind (`None` for any kind) that did the most
    /// work: its total less its waiting.
    pub fn busiest(&self, label: Option<&str>) -> Option<&ScopeFrame> {
        self.frames
            .iter()
            .filter(|f| label.is_none_or(|l| f.label == l))
            .max_by(|a, b| a.work_ms().total_cmp(&b.work_ms()))
    }

    /// Every scope path's cost per frame across the recent frames of a kind:
    /// mean and worst total, and mean calls.
    pub fn window(&self, label: Option<&str>) -> Vec<WindowRow> {
        let frames: Vec<&ScopeFrame> = self
            .frames
            .iter()
            .filter(|f| label.is_none_or(|l| f.label == l))
            .collect();
        let mut rows: Vec<WindowRow> = Vec::new();
        for frame in &frames {
            for row in rows_of(frame) {
                match rows.iter_mut().find(|r| r.path == row.path) {
                    Some(r) => {
                        r.sum_ms += row.total_ms;
                        r.max_ms = r.max_ms.max(row.total_ms);
                        r.calls += row.calls;
                    }
                    None => rows.push(WindowRow {
                        path: row.path,
                        depth: row.depth,
                        name: row.name,
                        sum_ms: row.total_ms,
                        max_ms: row.total_ms,
                        calls: row.calls,
                    }),
                }
            }
        }
        let n = frames.len().max(1) as f64;
        for r in &mut rows {
            r.sum_ms /= n;
        }
        rows.sort_by(|a, b| a.path.cmp(&b.path));
        rows
    }
}

/// A scope path's merged cost in one frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// The names from the top level down, joined with `/`.
    pub path: String,
    pub depth: u16,
    pub name: &'static str,
    /// How many times it ran under the same parent.
    pub calls: u32,
    pub total_ms: f64,
    /// Total, less the time inside its children.
    pub self_ms: f64,
}

/// A scope path's cost across a window of frames.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowRow {
    pub path: String,
    pub depth: u16,
    pub name: &'static str,
    /// The mean total per frame (named `sum` while it is being summed).
    pub sum_ms: f64,
    pub max_ms: f64,
    pub calls: u32,
}

/// A frame's scopes merged by path, in the order each path first ran.
pub fn rows_of(frame: &ScopeFrame) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    // The path of each open depth, and the row each record went into.
    let mut stack: Vec<&'static str> = Vec::new();
    let mut row_of: Vec<usize> = Vec::with_capacity(frame.scopes.len());
    for s in &frame.scopes {
        stack.truncate(usize::from(s.depth));
        stack.push(s.name);
        let path = stack.join("/");
        let i = match rows.iter().position(|r| r.path == path) {
            Some(i) => {
                rows[i].calls += 1;
                rows[i].total_ms += s.dur_ms;
                rows[i].self_ms += s.dur_ms;
                i
            }
            None => {
                rows.push(Row {
                    path,
                    depth: s.depth,
                    name: s.name,
                    calls: 1,
                    total_ms: s.dur_ms,
                    self_ms: s.dur_ms,
                });
                rows.len() - 1
            }
        };
        row_of.push(i);
    }
    // A child's time is not its parent's own.
    for (k, s) in frame.scopes.iter().enumerate() {
        if s.depth == 0 {
            continue;
        }
        let parent = frame.scopes[..k]
            .iter()
            .rposition(|p| p.depth + 1 == s.depth)
            .map(|p| row_of[p]);
        if let Some(p) = parent {
            rows[p].self_ms -= s.dur_ms;
        }
    }
    for r in &mut rows {
        r.self_ms = r.self_ms.max(0.0);
    }
    rows
}

/// A frame as a table: total, self, calls and the scope, indented.
pub fn frame_text(frame: &ScopeFrame) -> String {
    let mut out = String::new();
    if frame.wait_ms > 0.0 {
        let _ = writeln!(
            out,
            "frame {} ({}) · {:.2} ms of work, {:.2} ms in all with waiting",
            frame.index,
            frame.label,
            frame.work_ms(),
            frame.total_ms
        );
    } else {
        let _ = writeln!(
            out,
            "frame {} ({}) · {:.2} ms",
            frame.index, frame.label, frame.total_ms
        );
    }
    let _ = writeln!(out, "  total ms   self ms   calls  scope");
    for r in rows_of(frame) {
        let _ = writeln!(
            out,
            "  {:>8.3}  {:>8.3}  {:>6}  {}{}",
            r.total_ms,
            r.self_ms,
            r.calls,
            "  ".repeat(usize::from(r.depth)),
            r.name
        );
    }
    out
}

/// A window as a table: mean and worst per frame, mean calls.
pub fn window_text(rows: &[WindowRow], frames: usize) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "  mean ms   worst ms   calls/frame  scope   (over {frames} frames)"
    );
    let n = frames.max(1) as f64;
    for r in rows {
        let _ = writeln!(
            out,
            "  {:>7.3}  {:>9.3}  {:>11.1}  {}{}",
            r.sum_ms,
            r.max_ms,
            f64::from(r.calls) / n,
            "  ".repeat(usize::from(r.depth)),
            r.name
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame: update (2 ms, inside it wall 1.5 ms), draw (5 ms, inside it
    /// three tiles of 1 ms).
    fn sample(s: &mut Scopes, at: f64, slow: f64) {
        s.begin_frame("frame", at);
        s.enter("update", at);
        s.enter("wall", at + 0.2);
        s.exit(at + 1.7);
        s.exit(at + 2.0);
        s.enter("draw", at + 2.0);
        for k in 0..3 {
            let t = at + 2.5 + f64::from(k);
            s.enter("tile", t);
            s.exit(t + 1.0 + if k == 2 { slow } else { 0.0 });
        }
        s.exit(at + 7.0 + slow);
        s.end_frame(at + 7.5 + slow);
    }

    #[test]
    fn a_frame_reads_as_a_tree_with_siblings_merged() {
        let mut s = Scopes::new();
        sample(&mut s, 0.0, 0.0);
        let frame = s.busiest(None).expect("a frame");
        let rows = rows_of(frame);
        let names: Vec<(&str, u32)> = rows.iter().map(|r| (r.name, r.calls)).collect();
        assert_eq!(
            names,
            [("update", 1), ("wall", 1), ("draw", 1), ("tile", 3)]
        );
        let draw = &rows[2];
        assert!((draw.total_ms - 5.0).abs() < 1e-9);
        assert!(
            (draw.self_ms - 2.0).abs() < 1e-9,
            "5 ms less three 1 ms tiles"
        );
        let update = &rows[0];
        assert!((update.self_ms - 0.5).abs() < 1e-9);
        assert!((frame.total_ms - 7.5).abs() < 1e-9);
        let text = frame_text(frame);
        assert!(text.contains("tile"), "{text}");
        assert!(text.contains("     3  "), "{text}");
    }

    #[test]
    fn the_busiest_frame_is_found_and_the_window_averages() {
        let mut s = Scopes::new();
        sample(&mut s, 0.0, 0.0);
        sample(&mut s, 10.0, 4.0);
        sample(&mut s, 30.0, 0.0);
        let busiest = s.busiest(Some("frame")).expect("frames");
        assert_eq!(busiest.index, 1);
        let window = s.window(None);
        let tile = window.iter().find(|r| r.path == "draw/tile").expect("tile");
        assert!(
            (tile.max_ms - 7.0).abs() < 1e-9,
            "the slow frame's tiles: 1+1+5"
        );
        assert!((tile.sum_ms - (3.0 + 7.0 + 3.0) / 3.0).abs() < 1e-9);
        assert_eq!(tile.calls, 9);
        assert!(window_text(&window, 3).contains("3.0"));
    }

    #[test]
    fn frames_of_another_kind_are_kept_apart() {
        let mut s = Scopes::new();
        s.begin_frame("update only", 0.0);
        s.end_frame(50.0);
        sample(&mut s, 60.0, 0.0);
        assert_eq!(s.busiest(Some("frame")).expect("drawn").label, "frame");
        assert_eq!(s.busiest(None).expect("any").label, "update only");
    }

    #[test]
    fn scopes_outside_a_frame_or_left_open_do_no_harm() {
        let mut s = Scopes::new();
        s.enter("stray", 0.0);
        s.exit(1.0);
        assert!(s.is_empty());
        s.begin_frame("frame", 0.0);
        s.enter("left open", 1.0);
        s.end_frame(4.0);
        let rows = rows_of(s.busiest(None).expect("frame"));
        assert!(
            (rows[0].total_ms - 3.0).abs() < 1e-9,
            "closed at the frame's end"
        );
    }

    #[test]
    fn waiting_is_left_out_of_a_frames_work_and_of_the_busiest() {
        let mut s = Scopes::new().wait_on("present");
        // A cheap frame that waited long for the display.
        s.begin_frame("frame", 0.0);
        s.enter("draw", 0.0);
        s.exit(1.0);
        s.enter("present", 1.0);
        s.exit(16.0);
        s.end_frame(16.0);
        // A heavier frame that waited less.
        s.begin_frame("frame", 20.0);
        s.enter("draw", 20.0);
        s.exit(26.0);
        s.enter("present", 26.0);
        s.exit(36.0);
        s.end_frame(36.0);
        let busiest = s.busiest(None).expect("frames");
        assert_eq!(busiest.index, 1, "the one that worked hardest");
        assert!((busiest.work_ms() - 6.0).abs() < 1e-9);
        assert!(frame_text(busiest).contains("6.00 ms of work, 16.00 ms in all"));
    }

    #[test]
    fn the_ring_keeps_the_last_frames_and_reuses_their_buffers() {
        let mut s = Scopes::new();
        for k in 0..(SCOPE_FRAMES as u64 + 10) {
            sample(&mut s, k as f64 * 10.0, 0.0);
        }
        assert_eq!(s.len(), SCOPE_FRAMES);
        assert_eq!(s.frames().next().expect("oldest").index, 10);
    }
}
