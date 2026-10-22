//! `plan` — a capture plan (`--plan plan.txt`): many conditions, many
//! captures, one launch. Each section resets the game, puts it in a named
//! condition through [`Game::setup`](crate::app::Game::setup), scripts its
//! keys and captures the frames it lists, on the fixed clock.
//!
//! ```txt
//! # A section: its name, then what it does. Frames count from the
//! # section's start, sixty a second.
//! [walk-front]
//! set leader DaKing          # handed to the game as it is
//! set facing front
//! keys 0-200:Down            # the --keys syntax
//! frames 60-83 walk.png      # walk-060.png … walk-083.png
//!
//! [walk-side]
//! set facing left
//! shot 90 side.png           # one frame
//! ```
//!
//! Lines before the first `[section]` make an unnamed one. Blank lines and
//! `#` comments are skipped. A capture writes its PNG and text report as a
//! `--shot` does; paths are as given, from where the game was launched. A
//! section ends after its last capture; the run exits after the last
//! section's.
//!
//! The plan is checked whole before anything runs, and a mistake names its
//! line: a plan that fails halfway through a long run is the frustration it
//! exists to remove.

use std::collections::BTreeMap;

use crate::harness::KeyScript;

/// A parsed plan.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub sections: Vec<Section>,
}

/// One condition and its captures.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Section {
    pub name: String,
    /// `set` lines, in order, with their line numbers for errors.
    pub sets: Vec<(usize, String)>,
    pub keys: Vec<KeyScript>,
    /// Captures by frame (from the section's start): where each is written.
    pub shots: BTreeMap<u32, String>,
}

impl Plan {
    /// Read and parse the plan at `path`.
    pub fn load(path: &str) -> Result<Plan, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("plan {path}: {e}"))?;
        Plan::parse(&text).map_err(|e| format!("plan {path}: {e}"))
    }

    /// Parse a plan, or say which line is wrong and why.
    pub fn parse(text: &str) -> Result<Plan, String> {
        let mut plan = Plan::default();
        for (i, raw) in text.lines().enumerate() {
            let n = i + 1;
            let line = raw.split_once('#').map_or(raw, |(code, _)| code).trim();
            if line.is_empty() {
                continue;
            }
            if let Some(name) = line.strip_prefix('[') {
                let name = name
                    .strip_suffix(']')
                    .ok_or_else(|| format!("line {n}: `{line}`: a section is `[name]`"))?;
                plan.sections.push(Section {
                    name: name.trim().to_string(),
                    ..Section::default()
                });
                continue;
            }
            if plan.sections.is_empty() {
                plan.sections.push(Section::default());
            }
            let section = plan.sections.last_mut().expect("a section");
            let (word, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
            let rest = rest.trim();
            let shot = |section: &mut Section, frame: u32, path: String| match section
                .shots
                .insert(frame, path)
            {
                Some(_) => Err(format!("line {n}: two captures at frame {frame}")),
                None => Ok(()),
            };
            match word {
                "set" if !rest.is_empty() => section.sets.push((n, rest.to_string())),
                "keys" => section
                    .keys
                    .extend(KeyScript::parse_all(rest).map_err(|e| format!("line {n}: {e}"))?),
                "shot" => {
                    let (frame, path) = frame_and_path(rest).ok_or_else(|| {
                        format!("line {n}: `{line}`: expected `shot FRAME file.png`")
                    })?;
                    let frame: u32 = frame
                        .parse()
                        .map_err(|_| format!("line {n}: `{frame}` is not a frame number"))?;
                    shot(section, frame, path.to_string())?;
                }
                "frames" => {
                    let (range, path) = frame_and_path(rest).ok_or_else(|| {
                        format!("line {n}: `{line}`: expected `frames FROM-TO file.png`")
                    })?;
                    let (from, to) = range
                        .split_once('-')
                        .and_then(|(a, b)| Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?)))
                        .filter(|(a, b)| a <= b)
                        .ok_or_else(|| {
                            format!("line {n}: `{range}`: expected FROM-TO, in order")
                        })?;
                    for frame in from..=to {
                        shot(section, frame, numbered(path, frame))?;
                    }
                }
                _ => {
                    return Err(format!(
                        "line {n}: `{line}`: expected `[section]`, `set …`, `keys …`, `shot …` or `frames …`"
                    ));
                }
            }
        }
        if plan.sections.iter().all(|s| s.shots.is_empty()) {
            return Err("nothing to capture: no `shot` or `frames` line".to_string());
        }
        Ok(plan)
    }

    /// How many captures it takes.
    pub fn captures(&self) -> usize {
        self.sections.iter().map(|s| s.shots.len()).sum()
    }
}

/// `"12 out.png"` as `("12", "out.png")`.
fn frame_and_path(rest: &str) -> Option<(&str, &str)> {
    let (frame, path) = rest.split_once(char::is_whitespace)?;
    let path = path.trim();
    (!path.is_empty()).then_some((frame, path))
}

/// `walk.png` at frame 7 as `walk-007.png`: numbered, so a sequence sorts.
fn numbered(path: &str, frame: u32) -> String {
    match path.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.contains('/') => {
            format!("{stem}-{frame:03}.{ext}")
        }
        _ => format!("{path}-{frame:03}"),
    }
}

/// Where a run is in its plan.
#[derive(Debug)]
pub struct PlanRun {
    plan: Plan,
    section: usize,
    /// Frames into the section.
    frame: u32,
    /// Whether the plan has begun: it waits for the game to be ready.
    started: bool,
    /// Whether the current section has yet to be set up.
    fresh: bool,
    written: usize,
}

/// What a plan's run does next.
#[derive(Debug, PartialEq, Eq)]
pub enum Next {
    Continue,
    /// Every capture is written.
    Done,
}

impl PlanRun {
    pub fn new(plan: Plan) -> Self {
        Self {
            plan,
            section: 0,
            frame: 0,
            started: false,
            fresh: true,
            written: 0,
        }
    }

    /// Begin the plan, once the game is ready to be captured.
    pub fn start(&mut self) {
        self.started = true;
    }

    pub fn started(&self) -> bool {
        self.started
    }

    /// The section's `set` lines, the first time it's asked of a section
    /// (empty after, and before the plan starts): the loop resets the game
    /// and applies them.
    pub fn take_setup(&mut self) -> Option<&Section> {
        if !self.started || !self.fresh {
            return None;
        }
        self.fresh = false;
        self.plan.sections.get(self.section)
    }

    /// The keys held and pressed at this frame of the section, and which
    /// frame of their script it is.
    pub fn keys(&self) -> (&[KeyScript], u32) {
        match self.plan.sections.get(self.section) {
            Some(s) if self.started => (&s.keys, self.frame),
            _ => (&[], 0),
        }
    }

    /// Where this frame's capture goes, if it has one.
    pub fn due(&self) -> Option<&str> {
        if !self.started {
            return None;
        }
        self.plan
            .sections
            .get(self.section)?
            .shots
            .get(&self.frame)
            .map(String::as_str)
    }

    /// Frames until the section's next capture (0 if this one has one).
    pub fn until_next(&self) -> Option<u32> {
        if !self.started {
            return None;
        }
        let section = self.plan.sections.get(self.section)?;
        section
            .shots
            .range(self.frame..)
            .next()
            .map(|(f, _)| f - self.frame)
    }

    /// Note a capture written.
    pub fn wrote(&mut self) {
        self.written += 1;
    }

    pub fn written(&self) -> usize {
        self.written
    }

    pub fn total(&self) -> usize {
        self.plan.captures()
    }

    /// The section's name, for the report.
    pub fn section_name(&self) -> &str {
        self.plan
            .sections
            .get(self.section)
            .map_or("", |s| s.name.as_str())
    }

    /// The frame into the section.
    pub fn frame(&self) -> u32 {
        self.frame
    }

    /// Step past a frame (drawn or skipped): on to the next section once this
    /// one has no captures left.
    pub fn advance(&mut self) -> Next {
        if !self.started {
            return Next::Continue;
        }
        self.frame += 1;
        while self.until_next().is_none() {
            self.section += 1;
            self.frame = 0;
            self.fresh = true;
            if self.section >= self.plan.sections.len() {
                return Next::Done;
            }
        }
        Next::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAN: &str = "
        # Comments and blanks are skipped.
        [walk-front]
        set leader DaKing
        set facing front   # a trailing comment too
        keys 0-200:Down
        frames 2-4 walk.png

        [side]
        set facing left
        shot 1 .tmp/side.png
    ";

    #[test]
    fn a_plan_parses_into_sections_of_sets_keys_and_captures() {
        let plan = Plan::parse(PLAN).expect("parses");
        assert_eq!(plan.sections.len(), 2);
        let walk = &plan.sections[0];
        assert_eq!(walk.name, "walk-front");
        assert_eq!(walk.sets[0].1, "leader DaKing");
        assert_eq!(walk.sets[1].1, "facing front");
        assert_eq!(walk.keys.len(), 1);
        let shots: Vec<(&u32, &String)> = walk.shots.iter().collect();
        assert_eq!(shots.len(), 3);
        assert_eq!(shots[0], (&2, &"walk-002.png".to_string()));
        assert_eq!(
            plan.sections[1].shots.get(&1).map(String::as_str),
            Some(".tmp/side.png")
        );
        assert_eq!(plan.captures(), 4);
    }

    #[test]
    fn a_bad_line_is_refused_by_number() {
        let err = |text: &str| Plan::parse(text).unwrap_err();
        assert!(err("shot 1 a.png\nzoom 2").contains("line 2"));
        assert!(err("[a\nshot 1 a.png").contains("line 1"));
        assert!(err("shot x a.png").contains("not a frame number"));
        assert!(err("frames 4-2 a.png").contains("in order"));
        assert!(err("shot 1 a.png\nshot 1 b.png").contains("two captures"));
        assert!(err("keys 3:Nope\nshot 1 a.png").contains("line 1"));
        assert!(err("set facing left").contains("nothing to capture"));
    }

    #[test]
    fn numbered_frames_sort_and_keep_their_extension() {
        assert_eq!(numbered("a/walk.png", 7), "a/walk-007.png");
        assert_eq!(numbered("walk", 12), "walk-012");
        assert_eq!(numbered("./dir.v2/walk", 1), "./dir.v2/walk-001");
    }

    #[test]
    fn a_run_waits_to_start_then_walks_each_section_to_its_last_capture() {
        let mut run = PlanRun::new(Plan::parse(PLAN).expect("parses"));
        assert_eq!(run.due(), None);
        assert!(run.take_setup().is_none(), "not before it starts");
        assert_eq!(run.advance(), Next::Continue);
        run.start();
        assert_eq!(
            run.take_setup().map(|s| s.name.clone()),
            Some("walk-front".into())
        );
        assert!(run.take_setup().is_none(), "once a section");
        assert_eq!(run.until_next(), Some(2));
        let mut seen = Vec::new();
        loop {
            if let Some(path) = run.due() {
                seen.push((
                    run.section_name().to_string(),
                    run.frame(),
                    path.to_string(),
                ));
                run.wrote();
            }
            if run.advance() == Next::Done {
                break;
            }
            if run.frame() == 0 {
                assert_eq!(
                    run.take_setup().map(|s| s.name.clone()),
                    Some("side".into())
                );
            }
        }
        assert_eq!(
            seen,
            vec![
                ("walk-front".into(), 2, "walk-002.png".into()),
                ("walk-front".into(), 3, "walk-003.png".into()),
                ("walk-front".into(), 4, "walk-004.png".into()),
                ("side".into(), 1, ".tmp/side.png".into()),
            ]
        );
        assert_eq!(run.written(), run.total());
    }

    #[test]
    fn a_section_without_captures_is_passed_over() {
        let plan = Plan::parse("[empty]\nset x\n[real]\nshot 0 a.png").expect("parses");
        let mut run = PlanRun::new(plan);
        run.start();
        // The empty section ends before it begins; the loop moves on.
        assert_eq!(run.until_next(), None);
        assert_eq!(run.advance(), Next::Continue);
        assert_eq!(run.section_name(), "real");
        assert_eq!(run.due(), Some("a.png"));
    }
}
