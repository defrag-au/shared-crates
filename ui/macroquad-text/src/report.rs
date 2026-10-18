//! What the text report records. Off until [`enable`]; every run drawn
//! through this crate calls in, so nothing else has to. A capture
//! (`macroquad_gamekit::harness::Capture`) writes it beside its PNG.

use std::cell::RefCell;
use std::fmt::Write as _;

use macroquad::prelude::*;

/// Below this many design units, text is reported as too small to read.
pub const MIN_TEXT: f32 = 8.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub text: String,
    /// Its box: left, top, width, height, in the surface's units.
    pub rect: Rect,
    pub size: f32,
    pub alpha: f32,
}

/// A surface text is drawn on: the screen, or a baked face.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Surface {
    pub size: Vec2,
    pub lines: Vec<Line>,
}

#[derive(Default)]
struct State {
    on: bool,
    screen: Surface,
    /// Faces baked since recording began, each once, in bake order.
    bakes: Vec<Surface>,
    /// The bake being drawn, if one is.
    baking: Option<Surface>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Start recording. Call before anything is baked, so faces are caught.
pub fn enable() {
    STATE.with(|s| s.borrow_mut().on = true);
}

pub fn on() -> bool {
    STATE.with(|s| s.borrow().on)
}

/// A new screen frame of `size` units: forget the last frame's lines.
pub fn begin_screen(size: Vec2) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.screen = Surface {
            size,
            lines: Vec::new(),
        };
    });
}

/// Text drawn from here until [`end_bake`] lands on a face of `size`.
pub fn begin_bake(size: Vec2) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.on {
            s.baking = Some(Surface {
                size,
                lines: Vec::new(),
            });
        }
    });
}

/// Forget what this bake has drawn so far: it is drawing again.
pub fn restart_bake() {
    STATE.with(|s| {
        if let Some(b) = s.borrow_mut().baking.as_mut() {
            b.lines.clear();
        }
    });
}

pub fn end_bake() {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if let Some(b) = s.baking.take()
            && !b.lines.is_empty()
            && !s.bakes.contains(&b)
        {
            s.bakes.push(b);
        }
    });
}

/// A line of text was drawn.
pub fn line(text: &str, rect: Rect, size: f32, alpha: f32) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if !s.on {
            return;
        }
        let line = Line {
            text: text.to_owned(),
            rect,
            size,
            alpha,
        };
        match s.baking.as_mut() {
            Some(b) => b.lines.push(line),
            None => s.screen.lines.push(line),
        }
    });
}

/// The screen and the faces, as recorded so far.
pub fn surfaces() -> (Surface, Vec<Surface>) {
    STATE.with(|s| {
        let s = s.borrow();
        (s.screen.clone(), s.bakes.clone())
    })
}

/// One problem a reviewer would flag.
#[derive(Clone, Debug, PartialEq)]
pub enum Problem {
    /// Not wholly on its surface.
    Off { line: Line },
    /// Cut short with an ellipsis.
    Cut { line: Line },
    /// Below [`MIN_TEXT`].
    Small { line: Line },
    /// On top of another line.
    Overlap { a: Line, b: Line },
}

/// Text that can be seen: fully transparent lines are animation states.
fn visible(l: &Line) -> bool {
    l.alpha > 0.05
}

/// The visible text on `surface` as a reader sees it: a layout places
/// each word on its own, so words on one baseline, in one size and a
/// space apart, are joined back into the line they make.
pub fn joined(surface: &Surface) -> Vec<Line> {
    let mut words: Vec<&Line> = surface.lines.iter().filter(|l| visible(l)).collect();
    words.sort_by(|a, b| {
        (a.rect.y.round(), a.rect.x)
            .partial_cmp(&(b.rect.y.round(), b.rect.x))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut out: Vec<Line> = Vec::new();
    for w in words {
        if let Some(last) = out.last_mut() {
            let gap = w.rect.x - last.rect.right();
            let same_line = (last.rect.y - w.rect.y).abs() < 0.5
                && (last.size - w.size).abs() < 0.01
                && gap >= -0.5
                && gap < w.size * 0.6;
            if same_line {
                last.text = format!("{} {}", last.text, w.text);
                last.rect.w = w.rect.right() - last.rect.x;
                last.rect.h = last.rect.h.max(w.rect.h);
                continue;
            }
        }
        out.push(w.clone());
    }
    out
}

/// What is wrong with the text on `surface`.
pub fn problems(surface: &Surface) -> Vec<Problem> {
    let mut out = Vec::new();
    let bounds = Rect::new(0.0, 0.0, surface.size.x, surface.size.y);
    let joined = joined(surface);
    let lines: Vec<&Line> = joined.iter().collect();
    for l in &lines {
        let r = l.rect;
        let inside = r.x >= bounds.x - 0.5
            && r.y >= bounds.y - 0.5
            && r.right() <= bounds.right() + 0.5
            && r.bottom() <= bounds.bottom() + 0.5;
        if !inside {
            out.push(Problem::Off { line: (*l).clone() });
        }
        if l.text.contains('…') {
            out.push(Problem::Cut { line: (*l).clone() });
        }
        if l.size < MIN_TEXT {
            out.push(Problem::Small { line: (*l).clone() });
        }
    }
    for (i, a) in lines.iter().enumerate() {
        for b in &lines[i + 1..] {
            // A shadow or outline copy is not a second line.
            if a.text == b.text && a.rect.overlaps(&b.rect) {
                continue;
            }
            if let Some(o) = a.rect.intersect(b.rect) {
                let smaller = (a.rect.w * a.rect.h).min(b.rect.w * b.rect.h).max(1e-3);
                if o.w * o.h / smaller > 0.25 {
                    out.push(Problem::Overlap {
                        a: (*a).clone(),
                        b: (*b).clone(),
                    });
                }
            }
        }
    }
    out
}

fn rect(r: Rect) -> String {
    format!("({:.0}, {:.0}, {:.0}×{:.0})", r.x, r.y, r.w, r.h)
}

fn describe(p: &Problem, surface: &str) -> String {
    match p {
        Problem::Off { line } => format!(
            "off its surface: {:?} at {} on {surface}",
            line.text,
            rect(line.rect)
        ),
        Problem::Cut { line } => format!("cut short: {:?} on {surface}", line.text),
        Problem::Small { line } => format!(
            "too small: {:?} at {:.1} (minimum {MIN_TEXT}) on {surface}",
            line.text, line.size
        ),
        Problem::Overlap { a, b } => format!(
            "overlapping: {:?} at {} and {:?} at {} on {surface}",
            a.text,
            rect(a.rect),
            b.text,
            rect(b.rect)
        ),
    }
}

fn list(out: &mut String, surface: &Surface) {
    let mut lines = joined(surface);
    lines.dedup_by(|a, b| a.text == b.text && a.rect.overlaps(&b.rect));
    for l in lines {
        let _ = writeln!(out, "- {} {:.1} {:?}", rect(l.rect), l.size, l.text);
    }
}

/// The report for the capture written to `png`.
pub fn render(png: &str, note: &str) -> String {
    let (screen, bakes) = surfaces();
    let mut found: Vec<String> = problems(&screen)
        .iter()
        .map(|p| describe(p, "the screen"))
        .collect();
    for (i, b) in bakes.iter().enumerate() {
        let name = format!("face #{}", i + 1);
        found.extend(problems(b).iter().map(|p| describe(p, &name)));
    }
    let mut out = String::new();
    let _ = writeln!(out, "# Frame report: {png}");
    if !note.is_empty() {
        let _ = writeln!(out, "{note}");
    }
    let _ = writeln!(
        out,
        "\nBoxes are (left, top, width×height) in design units, then the text size and the text. \
         The screen is {:.0}×{:.0}. A face's text is in the face's own units; the face is drawn \
         scaled, so its on-screen size is its size times that scale.",
        screen.size.x, screen.size.y
    );
    let _ = writeln!(out, "\n## Problems ({})", found.len());
    if found.is_empty() {
        let _ = writeln!(out, "None found.");
    }
    for p in &found {
        let _ = writeln!(out, "- {p}");
    }
    let _ = writeln!(out, "\n## Screen text, top to bottom");
    list(&mut out, &screen);
    let _ = writeln!(out, "\n## Baked faces ({})", bakes.len());
    for (i, b) in bakes.iter().enumerate() {
        let _ = writeln!(
            out,
            "\n### Face #{} ({:.0}×{:.0})",
            i + 1,
            b.size.x,
            b.size.y
        );
        list(&mut out, b);
    }
    out
}

/// Forget everything recorded, and stop recording. For tests.
#[doc(hidden)]
pub fn reset() {
    STATE.with(|s| *s.borrow_mut() = State::default());
}
