//! The shell's side of [`scenegraph::problems`]: which tags are controls,
//! where in a folder a problem was seen, and — when the view moves — how a
//! walk through a scene is judged.

pub use scenegraph::problems::Coverage;
use scenegraph::problems::{coverage, problems, REF_ASPECT};
use scenegraph::{DrawList, Fonts, Kind, Problem, Scene};
use sceneinput::Tool;

use crate::error::Error;
use crate::game::Game;
use crate::load::App;

/// The tag panes lift and inspect on a pick (`scene-play`, the table fixture).
pub const INSPECT_TAG: &str = "hand";

/// The folder's view moves: its pointer is `walk` or `orbit`, or its replay
/// has camera steps. Its scenes are judged across the views, on what is in
/// focus, not from rest's camera on everything.
pub fn moving(app: &App) -> bool {
    matches!(app.session.tool, Tool::Walk | Tool::Orbit)
        || app.replay.as_ref().is_some_and(|r| r.moves_the_view())
}

/// Within this of the centre (ndc), and unobstructed, a node is in focus.
const FOCUS: f32 = 0.6;

/// What one look at a scene found.
#[derive(Clone, Debug, Default)]
pub struct Look {
    /// Report now: every problem of a fixed view; of a moving one, the
    /// document's (`off-print`, `missing`, `z-fight`, `contrast`).
    pub found: Vec<Problem>,
    /// A moving view's verdict on each label and control in focus: the
    /// problem it has from here (`small`, `covered`), or none.
    pub focus: Vec<(String, Option<Problem>)>,
    pub coverage: Coverage,
}

/// Look at `scene` (from its own camera) at the reference stage. Controls
/// are the inspect tag plus the game's own.
///
/// When the view is `moving`, one view is one of many. `off-camera` is not
/// reported (the far wall is meant to be behind you). `small` and `covered`
/// become verdicts on what is in focus, for a [`Judge`] to weigh across the
/// walk: a sign is too small only if it never read from anywhere it was
/// looked at, so walking up to it is how it passes.
pub fn look(
    app: &App,
    scene: &Scene,
    fonts: &mut Fonts,
    game: Option<&dyn Game>,
    moving: bool,
) -> Result<Look, Error> {
    let list = scene
        .draw_list_with(&app.geom, &app.materials, fonts)
        .map_err(|e| Error::Scene {
            path: "problems".into(),
            source: e,
        })?;
    // Every tag marks something meant to be seen: the game's controls, the
    // inspect tag, and an author's own (`tag=exhibit`). Each must show
    // somewhere on its face, or it is `covered`.
    let mut controls = vec![INSPECT_TAG];
    if let Some(g) = game {
        controls.extend_from_slice(g.control_tags());
    }
    for n in scene.nodes() {
        for t in &n.tags {
            if !controls.contains(&t.as_str()) {
                controls.push(t.as_str());
            }
        }
    }
    let mut all = problems(scene, &list, REF_ASPECT, &controls);
    // Text on a textured face, judged from the folder's own pixels.
    all.extend(scenegraph::problems::textured_contrast(
        scene,
        &list,
        &app.images,
    ));
    let coverage = coverage(scene, &list, &controls);
    if !moving {
        return Ok(Look {
            found: all,
            focus: Vec::new(),
            coverage,
        });
    }
    let (view, found): (Vec<Problem>, Vec<Problem>) =
        all.into_iter().partition(Problem::of_the_view);
    let mut focus = Vec::new();
    let is_control = |name: &str| {
        scene
            .get(name)
            .is_some_and(|n| n.tags.iter().any(|t| controls.contains(&t.as_str())))
    };
    let candidates = list
        .labels
        .iter()
        .flat_map(|l| {
            let n = l.name.as_str();
            [("small", n), ("cropped", n), ("backwards", n)]
        })
        .chain(
            scene
                .nodes()
                .iter()
                .filter(|n| matches!(n.kind, Kind::Quad { .. } | Kind::Mesh { .. }))
                .filter(|n| is_control(&n.name))
                .map(|n| ("covered", n.name.as_str())),
        );
    for (kind, node) in candidates {
        if !in_focus(scene, &list, node) {
            continue;
        }
        let problem = view
            .iter()
            .find(|p| p.kind() == kind && p.node() == node)
            .cloned();
        focus.push((format!("{kind} {node}"), problem));
    }
    Ok(Look {
        found,
        focus,
        coverage,
    })
}

/// `node` is near the centre of the view and a pick at it lands on it, its
/// parent, or something it carries.
fn in_focus(scene: &Scene, list: &DrawList, node: &str) -> bool {
    let Some(aim) = list.aim_point(scene, node) else {
        return false;
    };
    let Some((x, y)) = list.project_ndc(aim, REF_ASPECT) else {
        return false;
    };
    if x.abs() > FOCUS || y.abs() > FOCUS {
        return false;
    }
    let Some(hit) = list.hit_ndc(x, y, REF_ASPECT) else {
        return false;
    };
    let parent = |n: &str| scene.get(n).and_then(|n| n.parent.as_deref());
    hit.name == node || parent(node) == Some(hit.name) || parent(hit.name) == Some(node)
}

/// Weighs a moving view's verdicts across every look: a label or control
/// that was in focus somewhere and failed every time is a problem, reported
/// at the first look that saw it fail. One that passed once is fine; one
/// never in focus is not judged.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Judge {
    cases: Vec<Case>,
}

#[derive(Clone, Debug, PartialEq)]
struct Case {
    key: String,
    failed: Option<Seen>,
    /// The first look it passed at: 0 is rest, else a `replay.rp1` line.
    passed: Option<usize>,
}

impl Judge {
    pub fn weigh(&mut self, focus: Vec<(String, Option<Problem>)>, line: usize) {
        for (key, problem) in focus {
            let case = self.case(key);
            match problem {
                Some(problem) => {
                    if case.failed.is_none() {
                        case.failed = Some(Seen {
                            problem,
                            line,
                            clip: None,
                        });
                    }
                }
                None => case.passed = Some(case.passed.map_or(line, |p| p.min(line))),
            }
        }
    }

    fn case(&mut self, key: String) -> &mut Case {
        let i = match self.cases.iter().position(|c| c.key == key) {
            Some(i) => i,
            None => {
                self.cases.push(Case {
                    key,
                    failed: None,
                    passed: None,
                });
                self.cases.len() - 1
            }
        };
        &mut self.cases[i]
    }

    /// Fold another judge's looks in after this one's.
    pub fn absorb(&mut self, other: &Judge) {
        for c in &other.cases {
            let mine = self.case(c.key.clone());
            mine.passed = match (mine.passed, c.passed) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
            if mine.failed.is_none() {
                mine.failed.clone_from(&c.failed);
            }
        }
    }

    /// What never passed.
    pub fn verdicts(&self) -> Vec<Seen> {
        self.cases
            .iter()
            .filter(|c| c.passed.is_none())
            .filter_map(|c| c.failed.clone())
            .collect()
    }

    /// Labels and controls brought into focus at least once.
    pub fn judged(&self) -> usize {
        self.names().len()
    }

    /// Their names, in first-seen order: what the walk actually looked at.
    pub fn names(&self) -> Vec<&str> {
        self.read_at().into_iter().map(|(n, _)| n).collect()
    }

    /// Each name with where it was first read: the first look by which every
    /// check on it had passed once (0 is rest, else a `replay.rp1` line), or
    /// `None` if one never did — that one is also a verdict.
    pub fn read_at(&self) -> Vec<(&str, Option<usize>)> {
        let mut out: Vec<(&str, Option<usize>)> = Vec::new();
        for c in &self.cases {
            let name = c.key.split_once(' ').map_or(c.key.as_str(), |(_, n)| n);
            match out.iter_mut().find(|(n, _)| *n == name) {
                Some((_, at)) => *at = at.zip(c.passed).map(|(a, b)| a.max(b)),
                None => out.push((name, c.passed)),
            }
        }
        out
    }
}

/// A problem and where it was first seen: rest, a replay line, or a moment
/// of the folder's clip.
#[derive(Clone, Debug, PartialEq)]
pub struct Seen {
    pub problem: Problem,
    /// 0 is rest; otherwise the `replay.rp1` line of the step that made it.
    pub line: usize,
    /// Seconds into `clip.an1`, when rest posed by the clip showed it.
    pub clip: Option<f32>,
}

impl Seen {
    pub fn dump(&self) -> String {
        match (self.line, self.clip) {
            (_, Some(t)) => format!("{} @clip.an1 t={t:.2}", self.problem.dump()),
            (0, None) => self.problem.dump(),
            (n, None) => format!("{} @replay.rp1:{n}", self.problem.dump()),
        }
    }
}

/// Add each of `found` not already in `seen` (by its line, ignoring where).
pub fn note(seen: &mut Vec<Seen>, found: Vec<Problem>, line: usize) {
    note_at(seen, found, line, None);
}

/// [`note`] for rest posed by the folder's clip at `t` seconds.
pub fn note_clip(seen: &mut Vec<Seen>, found: Vec<Problem>, t: f32) {
    note_at(seen, found, 0, Some(t));
}

fn note_at(seen: &mut Vec<Seen>, found: Vec<Problem>, line: usize, clip: Option<f32>) {
    for problem in found {
        // A heading's angle differs a little at every moment of one walk:
        // one per node, the first.
        let same = |s: &Seen| match (&s.problem, &problem) {
            (Problem::Heading { name: a, .. }, Problem::Heading { name: b, .. }) => a == b,
            (a, b) => a.dump() == b.dump(),
        };
        if !seen.iter().any(same) {
            seen.push(Seen {
                problem,
                line,
                clip,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small(label: &str) -> Option<Problem> {
        Some(Problem::Small {
            label: label.into(),
            em: 4.0,
        })
    }

    /// A sign is read where its last check first passed; one seen only from
    /// behind is never read, and is the verdict.
    #[test]
    fn a_name_is_read_where_every_check_first_passed() {
        let mut j = Judge::default();
        j.weigh(
            vec![
                ("small sign".into(), small("sign")),
                ("backwards sign".into(), None),
                (
                    "backwards rear".into(),
                    Some(Problem::Backwards {
                        label: "rear".into(),
                    }),
                ),
            ],
            0,
        );
        j.weigh(vec![("small sign".into(), None)], 7);
        j.weigh(vec![("small sign".into(), None)], 3);
        assert_eq!(j.read_at(), vec![("sign", Some(3)), ("rear", None)]);
        let v: Vec<String> = j.verdicts().iter().map(Seen::dump).collect();
        assert_eq!(
            v,
            ["backwards rear is seen from behind (its face points away)"]
        );

        let mut rest = Judge::default();
        rest.weigh(vec![("backwards rear".into(), None)], 0);
        rest.absorb(&j);
        assert_eq!(rest.read_at()[0], ("rear", Some(0)), "rest read it first");
    }
}
