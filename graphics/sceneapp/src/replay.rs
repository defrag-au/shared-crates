//! Multi-step replay (`rp1`). A whole game as ordered steps, each followed by
//! the outcome it must have: the hit, applied or `blocked`, the cues, and the
//! named delta of the emitted scene. The gauntlet plays it headless, from the
//! game's start; [`play`] with `bless` writes what actually happened instead
//! of checking it.
//!
//! ```text
//! rp1
//! # X takes the centre
//! pick ndc=0,0 hit=c11
//! fx mark
//! + mark-c11 Label font=ibm size=0.55 "X" c=1a1210 p=c11 t=0,0,0.05
//! ~ turn Label font=ibm size=0.14 "O to play" p=ttt t=0,-2.15,0.04
//! pick node=c11 hit=c11 blocked
//! input reset
//! tick blocked
//! ```
//!
//! A pick aims at an `ndc=` point, or at `node=NAME`: wherever that node's
//! origin is drawn at the step's aspect. The hit is asserted either way, so a
//! node aimed at but covered by another is a mismatch, not a silent miss.
//!
//! A drag goes down at one aim and up at another, and asserts both picks:
//! `drag node=t07 to=gap hit=t07 drop=gap` (`to=x,y` is an ndc point). Both
//! are picked on the scene before the drag, then [`Game::drag`] decides.

use scenegraph::Scene;
use sceneinput::{step, Event, Session};

use crate::error::Error;
use crate::game::{Game, Pointed, Step};
use crate::load::App;
use crate::patch::{diff, parse_line, Line};
use crate::problems::{look, note, Coverage, Judge, Seen};

pub const VERSION: &str = "rp1";

/// Where a pick lands.
#[derive(Clone, Debug, PartialEq)]
pub enum Aim {
    /// A point in ndc, y up.
    Ndc([f32; 2]),
    /// Where this node's origin is drawn.
    Node(String),
}

/// What a step does. Closed.
#[derive(Clone, Debug, PartialEq)]
pub enum Verb {
    /// Pointer down at `at` on the emitted scene. `hit` is the node the
    /// [`scenegraph::DrawList`] must return: a name, `-` for nothing, or empty
    /// while unblessed.
    Pick { at: Aim, aspect: f32, hit: String },
    /// Pointer down at `from`, carried, and released at `to`. Both are
    /// picked on the emitted scene as it was before the drag (what is under
    /// the pointer, not the carried node): `hit` where it went down, `drop`
    /// where it came up. Then [`Game::drag`]`(hit, drop)`.
    Drag {
        from: Aim,
        to: Aim,
        aspect: f32,
        hit: String,
        drop: String,
    },
    /// A game token, as a key binding would send it.
    Input(String),
    /// One game tick (the shell clock's `every`, without the wait).
    Tick,
    /// A camera step (`orbit`, `turn`, `walk`). Moves the live view, not
    /// rest: the step asserts `view eye=… at=…`. In a `world.sw1` folder a
    /// walk can page the window, which is that step's delta.
    Move(crate::view::Move),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReplayStep {
    /// `#` lines before the step, kept through a bless.
    pub notes: Vec<String>,
    pub verb: Verb,
    /// The game refused the step.
    pub blocked: bool,
    /// Cue keys (`fx1` `sound KEY`), in order. Empty unless applied.
    pub fx: Vec<String>,
    /// Named delta of the emitted scene across the step.
    pub delta: Vec<Line>,
    /// A camera step's view afterwards (`eye=… at=…`); empty otherwise, or
    /// while unblessed.
    pub view: String,
    /// A pick's distance from the eye to the point it landed on, in metres
    /// to two places (`reach=2.31` on its line). Written only where the view
    /// moves — how far away a pick was is what a walk game judges — and
    /// empty for a miss, on a board, or while unblessed.
    pub reach: String,
    /// 1-based source line of the step; 0 when built in code.
    pub line: usize,
}

impl ReplayStep {
    pub fn new(verb: Verb) -> Self {
        Self {
            notes: Vec::new(),
            verb,
            blocked: false,
            fx: Vec::new(),
            delta: Vec::new(),
            view: String::new(),
            reach: String::new(),
            line: 0,
        }
    }
}

impl Replay {
    /// The replay moves the camera: its scenes are judged from each view.
    pub fn moves_the_view(&self) -> bool {
        self.steps.iter().any(|s| matches!(s.verb, Verb::Move(_)))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Replay {
    pub steps: Vec<ReplayStep>,
    /// `#` lines after the last step.
    pub tail: Vec<String>,
}

/// Counts from one played replay, and the problems its steps' scenes showed
/// (each once, at the first step that made it). Dump-shaped through
/// [`Score::dump`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Score {
    pub steps: usize,
    pub picks: usize,
    pub drags: usize,
    pub moves: usize,
    pub inputs: usize,
    pub ticks: usize,
    pub blocked: usize,
    pub problems: Vec<Seen>,
    /// What the problem checks looked at across the steps' scenes.
    pub coverage: Coverage,
    /// A moving view's in-focus verdicts across the steps, for the gauntlet
    /// to weigh with rest's.
    pub judge: Judge,
    /// The frame after each step as text ([`scenegraph::sketch`]), by its
    /// replay line: only when [`play_with`] was asked for them.
    pub sketches: Vec<(usize, String)>,
}

impl Score {
    /// `drags=` appears only when the replay has one, so a replay without
    /// drags scores the same line it always did.
    pub fn dump(&self) -> String {
        let drags = if self.drags > 0 {
            format!(" drags={}", self.drags)
        } else {
            String::new()
        };
        let moves = if self.moves > 0 {
            format!(" moves={}", self.moves)
        } else {
            String::new()
        };
        format!(
            "replay replay.rp1 steps={} picks={}{drags}{moves} inputs={} ticks={} blocked={}\n",
            self.steps, self.picks, self.inputs, self.ticks, self.blocked
        )
    }
}

impl Replay {
    pub fn dump(&self) -> String {
        let mut out = String::from(VERSION);
        out.push('\n');
        for s in &self.steps {
            for n in &s.notes {
                out.push_str(n);
                out.push('\n');
            }
            match &s.verb {
                Verb::Pick { at, aspect, hit } => {
                    out.push_str("pick ");
                    push_aim(&mut out, at);
                    push_aspect(&mut out, *aspect);
                    if !hit.is_empty() {
                        out.push_str(" hit=");
                        out.push_str(hit);
                    }
                    if !s.reach.is_empty() {
                        out.push_str(" reach=");
                        out.push_str(&s.reach);
                    }
                }
                Verb::Drag {
                    from,
                    to,
                    aspect,
                    hit,
                    drop,
                } => {
                    out.push_str("drag ");
                    push_aim(&mut out, from);
                    out.push_str(" to=");
                    match to {
                        Aim::Ndc(ndc) => out.push_str(&ndc_text(*ndc)),
                        Aim::Node(name) => out.push_str(name),
                    }
                    push_aspect(&mut out, *aspect);
                    if !hit.is_empty() {
                        out.push_str(" hit=");
                        out.push_str(hit);
                    }
                    if !drop.is_empty() {
                        out.push_str(" drop=");
                        out.push_str(drop);
                    }
                }
                Verb::Input(token) => {
                    out.push_str("input ");
                    out.push_str(token);
                }
                Verb::Tick => out.push_str("tick"),
                Verb::Move(mv) => out.push_str(&move_text(mv)),
            }
            if s.blocked {
                out.push_str(" blocked");
            }
            out.push('\n');
            if !s.view.is_empty() {
                out.push_str("view ");
                out.push_str(&s.view);
                out.push('\n');
            }
            if !s.fx.is_empty() {
                out.push_str("fx ");
                out.push_str(&s.fx.join(" "));
                out.push('\n');
            }
            for d in &s.delta {
                out.push_str(&d.dump());
                out.push('\n');
            }
        }
        for n in &self.tail {
            out.push_str(n);
            out.push('\n');
        }
        out
    }

    pub fn from_dump(src: &str) -> Result<Self, Error> {
        let mut lines = src.lines();
        let ver = lines
            .next()
            .ok_or_else(|| Error::ReplayParse("empty dump".into()))?
            .trim();
        if ver != VERSION {
            return Err(Error::ReplayParse(format!("expected {VERSION}, got {ver}")));
        }
        let mut replay = Replay::default();
        let mut notes = Vec::new();
        for (i, raw) in lines.enumerate() {
            let at = i + 2;
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('#') {
                notes.push(line.to_string());
                continue;
            }
            let err = |m: String| Error::ReplayParse(format!("line {at}: {m}"));
            if let Some(view) = line.strip_prefix("view ") {
                let s = replay
                    .steps
                    .last_mut()
                    .ok_or_else(|| err("view before any step".into()))?;
                if !matches!(s.verb, Verb::Move(_)) {
                    return Err(err("view follows a camera step (orbit, turn, walk)".into()));
                }
                s.view = view.trim().to_string();
                continue;
            }
            if line == "fx" || line.starts_with("fx ") {
                let rest = &line[2..];
                let s = replay
                    .steps
                    .last_mut()
                    .ok_or_else(|| err("fx before any step".into()))?;
                if !s.fx.is_empty() {
                    return Err(err("one fx line per step".into()));
                }
                s.fx = rest.split_whitespace().map(str::to_string).collect();
                if s.fx.is_empty() {
                    return Err(err("fx needs at least one key".into()));
                }
                continue;
            }
            if matches!(line.as_bytes().first(), Some(b'+' | b'~' | b'-')) {
                let s = replay
                    .steps
                    .last_mut()
                    .ok_or_else(|| err("delta before any step".into()))?;
                let d = parse_line(line).map_err(|e| err(e.to_string()))?;
                s.delta.push(d);
                continue;
            }
            let mut step = parse_step(line).map_err(err)?;
            step.line = at;
            step.notes = std::mem::take(&mut notes);
            replay.steps.push(step);
        }
        replay.tail = notes;
        Ok(replay)
    }
}

fn push_aim(out: &mut String, aim: &Aim) {
    match aim {
        Aim::Ndc(ndc) => {
            out.push_str("ndc=");
            out.push_str(&ndc_text(*ndc));
        }
        Aim::Node(name) => {
            out.push_str("node=");
            out.push_str(name);
        }
    }
}

fn push_aspect(out: &mut String, aspect: f32) {
    if (aspect - 1.0).abs() > 1e-6 {
        out.push_str(" aspect=");
        out.push_str(&compact_f(aspect));
    }
}

fn ndc_text(ndc: [f32; 2]) -> String {
    format!("{},{}", compact_f(ndc[0]), compact_f(ndc[1]))
}

fn parse_ndc(v: &str) -> Result<[f32; 2], String> {
    let (x, y) = v
        .split_once(',')
        .ok_or_else(|| format!("ndc is x,y, got '{v}'"))?;
    Ok([parse_f(x)?, parse_f(y)?])
}

/// `orbit dx= dy= [zoom=]`, `turn dx= dy=`, `walk f= [s=]`. Omitted keys
/// are 0; unknown keys are errors.
fn parse_move(verb: &str, args: &[&str]) -> Result<crate::view::Move, String> {
    use crate::view::Move;
    let allowed: &[&str] = match verb {
        "orbit" => &["dx", "dy", "zoom"],
        "turn" => &["dx", "dy"],
        _ => &["f", "s"],
    };
    let mut kv = std::collections::HashMap::new();
    for a in args {
        let (k, v) = a
            .split_once('=')
            .ok_or_else(|| format!("bad token '{a}'"))?;
        if !allowed.contains(&k) {
            return Err(format!("{verb} takes {}, not '{k}'", allowed.join(" ")));
        }
        kv.insert(k, parse_f(v)?);
    }
    let get = |k: &str| kv.get(k).copied().unwrap_or(0.0);
    Ok(match verb {
        "orbit" => Move::Orbit {
            dx: get("dx"),
            dy: get("dy"),
            zoom: get("zoom"),
        },
        "turn" => Move::Turn {
            dx: get("dx"),
            dy: get("dy"),
        },
        _ => Move::Walk {
            forward: get("f"),
            strafe: get("s"),
        },
    })
}

fn move_text(mv: &crate::view::Move) -> String {
    use crate::view::Move;
    let kv = |pairs: &[(&str, f32)]| {
        pairs
            .iter()
            .filter(|(_, v)| *v != 0.0)
            .map(|(k, v)| format!(" {k}={}", compact_f(*v)))
            .collect::<String>()
    };
    match *mv {
        Move::Orbit { dx, dy, zoom } => {
            format!("orbit{}", kv(&[("dx", dx), ("dy", dy), ("zoom", zoom)]))
        }
        Move::Turn { dx, dy } => format!("turn{}", kv(&[("dx", dx), ("dy", dy)])),
        Move::Walk { forward, strafe } => format!("walk{}", kv(&[("f", forward), ("s", strafe)])),
    }
}

/// `drag node=A|ndc=X,Y to=B|to=X,Y [aspect=] [hit=] [drop=]`.
fn parse_drag(args: &[&str]) -> Result<Verb, String> {
    let mut from = None;
    let mut to = None;
    let mut aspect = 1.0;
    let mut hit = String::new();
    let mut drop = String::new();
    for a in args {
        let (k, v) = a
            .split_once('=')
            .ok_or_else(|| format!("bad token '{a}'"))?;
        match k {
            "ndc" | "node" => {
                if from.is_some() {
                    return Err("drag takes ndc= or node=, not both".into());
                }
                from = Some(if k == "ndc" {
                    Aim::Ndc(parse_ndc(v)?)
                } else {
                    Aim::Node(v.to_string())
                });
            }
            // A comma is never in a node name: `to=x,y` is a point.
            "to" if v.contains(',') => to = Some(Aim::Ndc(parse_ndc(v)?)),
            "to" => to = Some(Aim::Node(v.to_string())),
            "aspect" => aspect = parse_f(v)?,
            "hit" => hit = v.to_string(),
            "drop" => drop = v.to_string(),
            other => return Err(format!("unknown key '{other}'")),
        }
    }
    Ok(Verb::Drag {
        from: from.ok_or("drag needs ndc=X,Y or node=NAME")?,
        to: to.ok_or("drag needs to=NAME or to=X,Y")?,
        aspect,
        hit,
        drop,
    })
}

fn parse_step(line: &str) -> Result<ReplayStep, String> {
    let mut words: Vec<&str> = line.split_whitespace().collect();
    let blocked = words.last() == Some(&"blocked");
    if blocked {
        words.pop();
    }
    let (&verb, args) = words.split_first().ok_or("empty step")?;
    let mut reach = String::new();
    let verb = match verb {
        "pick" => {
            let mut at = None;
            let mut aspect = 1.0;
            let mut hit = String::new();
            for a in args {
                let (k, v) = a
                    .split_once('=')
                    .ok_or_else(|| format!("bad token '{a}'"))?;
                let aim = match k {
                    "ndc" => {
                        let (x, y) = v
                            .split_once(',')
                            .ok_or_else(|| format!("ndc is x,y, got '{v}'"))?;
                        Some(Aim::Ndc([parse_f(x)?, parse_f(y)?]))
                    }
                    "node" => Some(Aim::Node(v.to_string())),
                    "aspect" => {
                        aspect = parse_f(v)?;
                        None
                    }
                    "hit" => {
                        hit = v.to_string();
                        None
                    }
                    "reach" => {
                        parse_f(v)?;
                        reach = v.to_string();
                        None
                    }
                    other => return Err(format!("unknown key '{other}'")),
                };
                if let Some(aim) = aim {
                    if at.is_some() {
                        return Err("pick takes ndc= or node=, not both".into());
                    }
                    at = Some(aim);
                }
            }
            Verb::Pick {
                at: at.ok_or("pick needs ndc=X,Y or node=NAME")?,
                aspect,
                hit,
            }
        }
        "drag" => parse_drag(args)?,
        "input" => match args {
            [token] => Verb::Input((*token).to_string()),
            _ => return Err("input takes one token".into()),
        },
        "tick" => {
            if !args.is_empty() {
                return Err("tick takes no arguments".into());
            }
            Verb::Tick
        }
        "orbit" | "turn" | "walk" => Verb::Move(parse_move(verb, args)?),
        other => return Err(format!("unknown step '{other}'")),
    };
    if blocked && matches!(verb, Verb::Move(_)) {
        return Err("a camera step is never refused: no `blocked`".into());
    }
    Ok(ReplayStep {
        blocked,
        reach,
        ..ReplayStep::new(verb)
    })
}

fn parse_f(s: &str) -> Result<f32, String> {
    s.parse().map_err(|_| format!("bad float '{s}'"))
}

fn compact_f(v: f32) -> String {
    if v == 0.0 {
        return "0".into();
    }
    let s = format!("{v:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Play `replay` from the start: the game's emit when there is one, else the
/// app's rest. Checks every step unless `bless`, and returns what happened
/// (notes kept) with its counts.
///
/// Without a game a step can only be a pick, and a pick only asserts its hit:
/// no node is a move, so `blocked`, `fx` and delta lines are errors there.
pub fn play(
    app: &App,
    replay: &Replay,
    game: Option<Box<dyn Game>>,
    bless: bool,
) -> Result<(Replay, Score), Error> {
    play_with(
        app,
        replay,
        game,
        bless,
        &crate::gauntlet::Options::default(),
    )
}

/// [`play`], and with [`crate::Options::sketch_steps`] the frame after
/// every step as text in [`Score::sketches`] (with a close-up of
/// [`crate::Options::close_up`]): what the view saw, for a reader with no
/// screenshot.
pub fn play_with(
    app: &App,
    replay: &Replay,
    mut game: Option<Box<dyn Game>>,
    bless: bool,
    options: &crate::gauntlet::Options,
) -> Result<(Replay, Score), Error> {
    let sketch = options.sketch_steps;
    let close_up = options.close_up.as_deref();
    let mut fonts = crate::load::stock_fonts()?;
    // `scene` is rest (the game's emit, or the folder's); `view` is the live
    // camera on top of it, moved only by camera steps. Picks and checks see
    // rest from the view; deltas compare rest with rest.
    let mut scene = match &game {
        Some(g) => g.emit().map_err(Error::Game)?,
        None => app.scene.clone(),
    };
    let mut view: Option<scenegraph::CameraParams> = None;
    let mut world = app.world.clone();
    let moving = crate::problems::moving(app);
    let mut out = Replay {
        steps: Vec::with_capacity(replay.steps.len()),
        tail: replay.tail.clone(),
    };
    let mut score = Score::default();
    for want in &replay.steps {
        let shown = crate::view::with_view(&scene, view.as_ref())?;
        let line = want.line;
        let mismatch = |what: &str, want: String, got: String| Error::ReplayMismatch {
            line,
            what: what.to_string(),
            want,
            got,
        };
        let mut got = ReplayStep {
            notes: want.notes.clone(),
            line,
            ..ReplayStep::new(want.verb.clone())
        };
        score.steps += 1;
        let outcome = match &want.verb {
            Verb::Pick { at, aspect, hit } => {
                score.picks += 1;
                let (name, pointed) = pick(app, &shown, &mut fonts, at, *aspect)?;
                if !bless {
                    missed(&name).map_err(|why| mismatch("aim", hit.clone(), why))?;
                }
                if !bless && hit != &name {
                    let want = if hit.is_empty() {
                        "(no hit=)".into()
                    } else {
                        hit.clone()
                    };
                    return Err(mismatch("hit", want, name));
                }
                got.verb = Verb::Pick {
                    at: at.clone(),
                    aspect: *aspect,
                    hit: name.clone(),
                };
                if moving {
                    if let Some(p) = &pointed {
                        got.reach = format!("{:.2}", p.reach());
                    }
                }
                if !bless && want.reach != got.reach {
                    let shown = |r: &str| {
                        if r.is_empty() {
                            "(no reach=)".to_string()
                        } else {
                            r.to_string()
                        }
                    };
                    return Err(mismatch("reach", shown(&want.reach), shown(&got.reach)));
                }
                // The game hears where the pick came from: the view's eye
                // and the point it landed on.
                match (game.as_mut(), pointed) {
                    (Some(g), Some(p)) => Some(g.pick_at(&name, &p).map_err(Error::Game)?),
                    (Some(_), None) => Some(Step::Blocked),
                    (None, _) => None,
                }
            }
            Verb::Drag {
                from,
                to,
                aspect,
                hit,
                drop,
            } => {
                score.drags += 1;
                // Both ends on the scene before the drag: the carried node is
                // under the pointer when it lifts, and must not be the drop.
                let (down, _) = pick(app, &shown, &mut fonts, from, *aspect)?;
                let (up, _) = pick(app, &shown, &mut fonts, to, *aspect)?;
                if !bless {
                    let want = |w: &String, key: &str| {
                        if w.is_empty() {
                            format!("(no {key}=)")
                        } else {
                            w.clone()
                        }
                    };
                    missed(&down).map_err(|why| mismatch("aim", hit.clone(), why))?;
                    missed(&up).map_err(|why| mismatch("aim", drop.clone(), why))?;
                    if hit != &down {
                        return Err(mismatch("hit", want(hit, "hit"), down));
                    }
                    if drop != &up {
                        return Err(mismatch("drop", want(drop, "drop"), up));
                    }
                }
                got.verb = Verb::Drag {
                    from: from.clone(),
                    to: to.clone(),
                    aspect: *aspect,
                    hit: down.clone(),
                    drop: up.clone(),
                };
                match game.as_mut() {
                    Some(g) if down != "-" && up != "-" => {
                        Some(g.drag(&down, &up).map_err(Error::Game)?)
                    }
                    Some(_) => Some(Step::Blocked),
                    None => None,
                }
            }
            Verb::Input(token) => {
                score.inputs += 1;
                let g = game.as_mut().ok_or(Error::ReplayNeedsGame { line })?;
                Some(g.input(token).map_err(Error::Game)?)
            }
            Verb::Tick => {
                score.ticks += 1;
                let g = game.as_mut().ok_or(Error::ReplayNeedsGame { line })?;
                Some(g.tick().map_err(Error::Game)?)
            }
            Verb::Move(mv) => {
                score.moves += 1;
                let list = flatten(app, &shown, &mut fonts)?;
                let next_view = crate::view::apply(&shown, &list, mv)?;
                got.view = crate::view::dump(&next_view);
                // A world pages its window when the eye crosses into a cell
                // far enough from the one it was emitted around.
                let before = scene.clone();
                if let Some(w) = world.as_mut() {
                    if let Some(next) = crate::view::page(w, &next_view)? {
                        scene = next;
                    }
                }
                // Then the game hears where the eye went: walking through a
                // doorway can be its move. Blocked is "nothing happened".
                if let Some(g) = game.as_mut() {
                    if g.moved(next_view.eye).map_err(Error::Game)? == Step::Applied {
                        got.fx = g
                            .fx()
                            .ops
                            .iter()
                            .map(|crate::fx::Op::Sound { key }| key.clone())
                            .collect();
                        scene = g.emit().map_err(Error::Game)?;
                    }
                }
                got.delta = diff(&before, &scene).lines;
                view = Some(next_view);
                let shown = crate::view::with_view(&scene, view.as_ref())?;
                let seen = look(app, &shown, &mut fonts, game.as_deref(), moving)?;
                note(&mut score.problems, seen.found, line);
                score.judge.weigh(seen.focus, line);
                score.coverage.add(seen.coverage);
                if !bless {
                    if want.view.is_empty() {
                        return Err(mismatch("view", "(no view line)".into(), got.view));
                    }
                    if want.view != got.view {
                        return Err(mismatch("view", want.view.clone(), got.view));
                    }
                    if want.delta != got.delta {
                        let lines =
                            |d: &[Line]| d.iter().map(Line::dump).collect::<Vec<_>>().join("\n");
                        return Err(mismatch("delta", lines(&want.delta), lines(&got.delta)));
                    }
                    if want.fx != got.fx {
                        return Err(mismatch("fx", want.fx.join(" "), got.fx.join(" ")));
                    }
                }
                if sketch {
                    let shown = crate::view::with_view(&scene, view.as_ref())?;
                    let f = frame(app, &shown, &mut fonts, close_up)?;
                    score.sketches.push((line, f));
                }
                out.steps.push(got);
                continue;
            }
        };
        match (outcome, &game) {
            (Some(step), Some(g)) => {
                got.blocked = step == Step::Blocked;
                if got.blocked {
                    score.blocked += 1;
                } else {
                    got.fx = g
                        .fx()
                        .ops
                        .iter()
                        .map(|crate::fx::Op::Sound { key }| key.clone())
                        .collect();
                }
                let next = g.emit().map_err(Error::Game)?;
                got.delta = diff(&scene, &next).lines;
                // A node with `front=` the step moved must have gone the way
                // it faces.
                let turned = scenegraph::problems::headings(&scene, &next);
                note(&mut score.problems, turned, line);
                if !got.delta.is_empty() {
                    // Judged from where the camera is, not where rest put it.
                    let shown = crate::view::with_view(&next, view.as_ref())?;
                    let seen = look(app, &shown, &mut fonts, Some(g.as_ref()), moving)?;
                    note(&mut score.problems, seen.found, line);
                    score.judge.weigh(seen.focus, line);
                    score.coverage.add(seen.coverage);
                }
                scene = next;
            }
            _ => {
                if want.blocked || !want.fx.is_empty() || !want.delta.is_empty() {
                    return Err(Error::ReplayNeedsGame { line });
                }
            }
        }
        if !bless {
            if want.blocked != got.blocked {
                let word = |b: bool| if b { "blocked" } else { "applied" }.to_string();
                return Err(mismatch("outcome", word(want.blocked), word(got.blocked)));
            }
            if want.fx != got.fx {
                return Err(mismatch("fx", want.fx.join(" "), got.fx.join(" ")));
            }
            if want.delta != got.delta {
                let lines = |d: &[Line]| d.iter().map(Line::dump).collect::<Vec<_>>().join("\n");
                return Err(mismatch("delta", lines(&want.delta), lines(&got.delta)));
            }
        }
        if sketch {
            let shown = crate::view::with_view(&scene, view.as_ref())?;
            let f = frame(app, &shown, &mut fonts, close_up)?;
            score.sketches.push((line, f));
        }
        out.steps.push(got);
    }
    Ok((out, score))
}

/// A `node=` aim that did not land on screen is never an outcome, even
/// once blessed: `hit=undrawn:door blocked` would score a step that never
/// pointed at the door (the escape room's first try). Bless writes it so it
/// can be read; the next run fails on it.
fn missed(name: &str) -> Result<(), String> {
    match name.split_once(':') {
        Some(("undrawn", node)) => Err(format!(
            "{node} is not on screen from this view; move the camera first"
        )),
        Some(("unknown", node)) => Err(format!("no node named {node}")),
        _ => Ok(()),
    }
}

/// `scene` as a text sketch at the reference stage, with the tagged nodes
/// no cell saw listed as `unseen`.
///
/// A tagged node too small for a ray at a cell centre still claims the
/// cell where a ray at its aim point hits it, so `unseen` agrees with a
/// pick. A node with `front=` says which way it faces in the key. With
/// `close_up`, that node's close-up follows when it is on screen.
pub(crate) fn frame(
    app: &App,
    scene: &Scene,
    fonts: &mut scenegraph::Fonts,
    close_up: Option<&str>,
) -> Result<String, Error> {
    use scenegraph::sketch::{self, SKETCH_COLS, SKETCH_ROWS};
    let list = flatten(app, scene, fonts)?;
    let aspect = scenegraph::problems::REF_ASPECT;
    let images = Some(&app.images);
    let tagged: Vec<&str> = scene
        .nodes()
        .iter()
        .filter(|n| !n.tags.is_empty())
        .map(|n| n.name.as_str())
        .collect();
    let dress = |s: &mut sketch::Sketch| {
        for name in &tagged {
            if !s.cells.iter().flatten().any(|c| c == name) {
                if let Some(aim) = list.aim_point(scene, name) {
                    s.claim(&list, images, name, aim);
                }
            }
        }
        for n in scene.nodes() {
            if let Some(word) = sketch::facing(scene, &list, &n.name) {
                s.note(&n.name, format!("faces={word}"));
            }
        }
    };
    let mut whole = sketch::sketch(&list, aspect, images, SKETCH_COLS, SKETCH_ROWS);
    dress(&mut whole);
    let mut out = whole.dump(&tagged);
    if let Some(name) = close_up {
        match sketch::close_up(&list, aspect, images, name, SKETCH_COLS, SKETCH_ROWS) {
            Some(mut near) => {
                dress(&mut near);
                out.push_str(&near.dump(&[]));
            }
            None => out.push_str(&format!("sketch {name} off screen\n")),
        }
    }
    Ok(out)
}

/// `scene` flattened with the folder's geometry and materials.
fn flatten(
    app: &App,
    scene: &Scene,
    fonts: &mut scenegraph::Fonts,
) -> Result<scenegraph::DrawList, Error> {
    scene
        .draw_list_with(&app.geom, &app.materials, fonts)
        .map_err(|e| Error::Scene {
            path: "replay.rp1".into(),
            source: e,
        })
}

/// The node a pick at `at` selects on `scene`, or `-`. Same path as the
/// panes: flatten, then a `sceneinput` pick. A `node=` aim that names no
/// node comes back as `unknown:NAME`, and one off screen or behind the eye
/// as `undrawn:NAME`. One token each, so a blessed replay still parses, and
/// no real node name matches them.
fn pick(
    app: &App,
    scene: &Scene,
    fonts: &mut scenegraph::Fonts,
    at: &Aim,
    aspect: f32,
) -> Result<(String, Option<Pointed>), Error> {
    let list = flatten(app, scene, fonts)?;
    let ndc = match at {
        Aim::Ndc(ndc) => *ndc,
        Aim::Node(name) => {
            if scene.get(name).is_none() {
                return Ok((format!("unknown:{name}"), None));
            }
            // The centre of what it draws, not its origin (a model's is at
            // its feet).
            let Some(aim) = list.aim_point(scene, name) else {
                return Ok((format!("unknown:{name}"), None));
            };
            match list.project_ndc(aim, aspect) {
                Some((x, y)) if x.abs() <= 1.0 && y.abs() <= 1.0 => [x, y],
                _ => return Ok((format!("undrawn:{name}"), None)),
            }
        }
    };
    let (_, sess, _) = step(
        scene,
        &list,
        &Session::new(),
        Event::down(ndc[0], ndc[1], aspect),
    )
    .map_err(|e| Error::Input {
        path: "replay.rp1".into(),
        source: e,
    })?;
    let eye = list.camera.eye;
    Ok(match sess.sel {
        Some(sel) => (
            sel.name,
            Some(Pointed {
                eye,
                point: sel.point,
            }),
        ),
        None => ("-".to_string(), None),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "\
rp1
# X takes the centre
pick ndc=0,0 hit=c11
fx mark
+ mark-c11 Label font=ibm size=0.55 \"X\" c=1a1210 p=c11 t=0,0,0.05
~ turn Label font=ibm size=0.14 \"O to play\" p=ttt t=0,-2.15,0.04
pick ndc=0.25,-0.5 aspect=1.5 hit=- blocked
input reset
tick blocked
# end
";

    #[test]
    fn dump_roundtrip() {
        let r = Replay::from_dump(SRC).unwrap();
        assert_eq!(r.steps.len(), 4);
        assert_eq!(r.steps[0].notes, vec!["# X takes the centre".to_string()]);
        assert_eq!(r.steps[0].fx, vec!["mark".to_string()]);
        assert_eq!(r.steps[0].delta.len(), 2);
        assert_eq!(r.steps[0].line, 3);
        assert!(r.steps[1].blocked);
        assert_eq!(r.steps[2].verb, Verb::Input("reset".into()));
        assert_eq!(r.steps[3].verb, Verb::Tick);
        assert_eq!(r.tail, vec!["# end".to_string()]);
        assert_eq!(r.dump(), SRC);
    }

    #[test]
    fn unknown_step_errors_with_line() {
        match Replay::from_dump("rp1\npick ndc=0,0 hit=a\njump\n") {
            Err(Error::ReplayParse(m)) => assert!(m.starts_with("line 3:"), "{m}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn delta_before_step_errors() {
        assert!(matches!(
            Replay::from_dump("rp1\n~ turn Label\n"),
            Err(Error::ReplayParse(_))
        ));
    }

    #[test]
    fn unblessed_pick_has_empty_hit() {
        let r = Replay::from_dump("rp1\npick ndc=0,0\n").unwrap();
        assert_eq!(
            r.steps[0].verb,
            Verb::Pick {
                at: Aim::Ndc([0.0, 0.0]),
                aspect: 1.0,
                hit: String::new()
            }
        );
        assert_eq!(r.dump(), "rp1\npick ndc=0,0\n");
    }

    #[test]
    fn node_aims_roundtrip_and_exclude_ndc() {
        let src = "rp1\npick node=c11 hit=c11\n";
        let r = Replay::from_dump(src).unwrap();
        assert_eq!(
            r.steps[0].verb,
            Verb::Pick {
                at: Aim::Node("c11".into()),
                aspect: 1.0,
                hit: "c11".into()
            }
        );
        assert_eq!(r.dump(), src);
        assert!(Replay::from_dump("rp1\npick node=a ndc=0,0\n").is_err());
        assert!(Replay::from_dump("rp1\npick hit=a\n").is_err());
    }

    #[test]
    fn drags_roundtrip_with_node_and_ndc_ends() {
        let src = "\
rp1
# t07 into the gap
drag node=t07 to=gap hit=t07 drop=gap
fx slide
drag ndc=0.1,-0.2 to=0.3,0 aspect=1.5 hit=a drop=- blocked
drag node=a to=b
";
        let r = Replay::from_dump(src).unwrap();
        assert_eq!(
            r.steps[0].verb,
            Verb::Drag {
                from: Aim::Node("t07".into()),
                to: Aim::Node("gap".into()),
                aspect: 1.0,
                hit: "t07".into(),
                drop: "gap".into(),
            }
        );
        assert_eq!(
            r.steps[1].verb,
            Verb::Drag {
                from: Aim::Ndc([0.1, -0.2]),
                to: Aim::Ndc([0.3, 0.0]),
                aspect: 1.5,
                hit: "a".into(),
                drop: "-".into(),
            }
        );
        assert!(r.steps[1].blocked);
        assert_eq!(r.dump(), src);
    }

    #[test]
    fn drags_need_both_ends() {
        for bad in [
            "rp1\ndrag node=a\n",
            "rp1\ndrag to=b\n",
            "rp1\ndrag node=a ndc=0,0 to=b\n",
            "rp1\ndrag node=a to=b over=c\n",
            "rp1\ndrag node=a to=1,x\n",
        ] {
            assert!(
                matches!(Replay::from_dump(bad), Err(Error::ReplayParse(_))),
                "{bad}"
            );
        }
    }

    #[test]
    fn score_names_drags_only_when_there_are_some() {
        let mut s = Score {
            steps: 2,
            picks: 1,
            ..Score::default()
        };
        assert_eq!(
            s.dump(),
            "replay replay.rp1 steps=2 picks=1 inputs=0 ticks=0 blocked=0\n"
        );
        s.drags = 1;
        assert_eq!(
            s.dump(),
            "replay replay.rp1 steps=2 picks=1 drags=1 inputs=0 ticks=0 blocked=0\n"
        );
    }
}
