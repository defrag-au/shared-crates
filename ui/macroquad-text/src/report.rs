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
    /// Depth of [`muted`] scopes: text drawn inside one is not recorded.
    muted: u32,
    screen: Surface,
    /// Where the screen's content begins in the drawing coordinate space.
    origin: Vec2,
    /// Clips in force, in the drawing units of the surface they were opened
    /// on. Text wholly outside one is not drawn, so it is not recorded.
    clips: Vec<Rect>,
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
    begin_screen_at(Vec2::ZERO, size);
}

/// A new screen frame of `size` units drawn at `origin`: forget the last
/// frame's lines, and report this frame's in coordinates relative to the
/// surface.
///
/// Use this when the screen is a panel inside a larger frame — a storybook
/// story, a capture crop — so the report measures against the panel rather
/// than the window it happens to be drawn in.
pub fn begin_screen_at(origin: Vec2, size: Vec2) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.origin = origin;
        s.clips.clear();
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

/// Text clipped to a rectangle in the current camera's units is text that is
/// drawn, so record it only when it falls inside the clip. Called by
/// `macroquad_gamekit::with_clip`, which sets the scissor itself.
pub fn push_clip(rect: Rect) {
    STATE.with(|s| s.borrow_mut().clips.push(rect));
}

/// Leave the innermost clip opened by [`push_clip`].
pub fn pop_clip() {
    STATE.with(|s| {
        s.borrow_mut().clips.pop();
    });
}

/// Run `draw` without recording its text.
///
/// For text drawn into an intermediate target that is composited somewhere
/// else every frame — one half of a split screen — whose coordinates are the
/// target's, not the screen's, so it would be reported in the wrong place.
/// Draw the screen's own text (a HUD) on the screen instead, where it is
/// recorded where it lands. A face baked once and kept belongs in a bake
/// ([`begin_bake`]), not here.
pub fn muted<R>(draw: impl FnOnce() -> R) -> R {
    STATE.with(|s| s.borrow_mut().muted += 1);
    let out = draw();
    STATE.with(|s| s.borrow_mut().muted -= 1);
    out
}

/// A line of text was drawn.
pub fn line(text: &str, rect: Rect, size: f32, alpha: f32) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if !s.on || s.muted > 0 {
            return;
        }
        // Text wholly outside a clip is scissored away: not visible, not a
        // problem. A clip with no area draws nothing at all.
        if !s
            .clips
            .iter()
            .all(|c| c.w > 0.0 && c.h > 0.0 && c.overlaps(&rect))
        {
            return;
        }
        let line = Line {
            text: text.to_owned(),
            rect,
            size,
            alpha,
        };
        let origin = s.origin;
        match s.baking.as_mut() {
            Some(b) => b.lines.push(line),
            None => {
                // The screen's own coordinates start at its origin: a story
                // drawn at (238, 24) in a window reports from (0, 0).
                let mut line = line;
                line.rect.x -= origin.x;
                line.rect.y -= origin.y;
                s.screen.lines.push(line);
            }
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
    /// Cut short with a trailing ellipsis.
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
        if l.text.ends_with('…') {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn screen_at(origin: Vec2, size: Vec2) {
        reset();
        enable();
        begin_screen_at(origin, size);
    }

    #[test]
    fn a_screen_origin_reports_lines_relative_to_the_surface() {
        screen_at(vec2(238.0, 24.0), vec2(390.0, 844.0));
        line("inside", Rect::new(258.0, 44.0, 60.0, 16.0), 12.0, 1.0);
        let (screen, _) = surfaces();
        assert_eq!(screen.lines[0].rect, Rect::new(20.0, 20.0, 60.0, 16.0));
        assert!(problems(&screen).is_empty());
        reset();
    }

    #[test]
    fn text_left_of_the_origin_is_off_its_surface() {
        screen_at(vec2(238.0, 24.0), vec2(390.0, 844.0));
        line("left of it", Rect::new(220.0, 44.0, 40.0, 16.0), 12.0, 1.0);
        let (screen, _) = surfaces();
        assert!(matches!(
            problems(&screen).as_slice(),
            [Problem::Off { .. }]
        ));
        reset();
    }

    #[test]
    fn a_bake_keeps_its_own_coordinates() {
        screen_at(vec2(238.0, 24.0), vec2(390.0, 844.0));
        begin_bake(vec2(64.0, 64.0));
        line("face", Rect::new(8.0, 8.0, 20.0, 10.0), 10.0, 1.0);
        end_bake();
        let (screen, bakes) = surfaces();
        assert!(screen.lines.is_empty());
        assert_eq!(bakes[0].lines[0].rect, Rect::new(8.0, 8.0, 20.0, 10.0));
        reset();
    }

    #[test]
    fn text_wholly_outside_a_clip_is_not_recorded() {
        screen_at(Vec2::ZERO, vec2(200.0, 200.0));
        push_clip(Rect::new(0.0, 0.0, 100.0, 100.0));
        line("inside", Rect::new(10.0, 10.0, 40.0, 14.0), 12.0, 1.0);
        line("below", Rect::new(10.0, 140.0, 40.0, 14.0), 12.0, 1.0);
        pop_clip();
        let (screen, _) = surfaces();
        assert_eq!(screen.lines.len(), 1);
        assert_eq!(screen.lines[0].text, "inside");
        reset();
    }

    #[test]
    fn a_middle_elision_is_not_a_cut() {
        screen_at(Vec2::ZERO, vec2(200.0, 100.0));
        line("0f3a…9c2e", Rect::new(0.0, 0.0, 60.0, 14.0), 12.0, 1.0);
        assert!(problems(&surfaces().0).is_empty());
        reset();
    }

    #[test]
    fn a_trailing_ellipsis_is_a_cut() {
        screen_at(Vec2::ZERO, vec2(200.0, 100.0));
        line("A name far…", Rect::new(0.0, 0.0, 60.0, 14.0), 12.0, 1.0);
        assert!(matches!(
            problems(&surfaces().0).as_slice(),
            [Problem::Cut { .. }]
        ));
        reset();
    }
}
