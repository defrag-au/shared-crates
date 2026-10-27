use sceneanim::pose;
use scenedrv::present;
use sceneinput::{step, Event};

use crate::error::Error;
use crate::game::{Game, Pointed, Step};
use crate::load::App;
use crate::problems::{look, note, Judge};

/// How many moments of `clip.an1` rest is judged at (its start, then
/// evenly through it).
const CLIP_LOOKS: usize = 8;

/// Makes a fresh game at its start. Each check that plays starts from one,
/// so checks never depend on each other's moves.
pub type NewGame<'a> = &'a dyn Fn() -> Result<Box<dyn Game>, Error>;

/// Score an app folder. Dump is the result; screenshots are not.
///
/// With `new_game`, the game decides every move: its emit must equal
/// `scene.sg1`, a session hit is a [`Game::pick`], `clock.n` is that many
/// [`Game::tick`]s, and `replay.rp1` plays from its start. Without one, no
/// node is a move: `act.act1` alone writes rest.
///
/// The dump leads with `problems N` ([`scenegraph::problems`]): what rest
/// and every replay step's scene show that a glance would miss. Any problem
/// fails the gauntlet ([`Error::Problems`] carries the whole dump).
pub fn gauntlet(app: &App, new_game: Option<NewGame>) -> Result<String, Error> {
    gauntlet_with(app, new_game, Options::default())
}

/// What a gauntlet run adds beyond the score.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// A text sketch of the frame after every replay step, not only rest's.
    pub sketch_steps: bool,
    /// A close-up sketch of this node after every sketch of the frame: for
    /// a thing too small in the whole view to read its shape.
    pub close_up: Option<String>,
}

/// [`gauntlet`] with [`Options`].
pub fn gauntlet_with(
    app: &App,
    new_game: Option<NewGame>,
    options: Options,
) -> Result<String, Error> {
    let mut out = String::new();
    out.push_str("roundtrip ");
    out.push_str(&app.files().join(" "));
    out.push('\n');
    roundtrip_all(app)?;

    let first = new_game.map(|f| f()).transpose()?;
    if let Some(game) = &first {
        let emitted = game.emit().map_err(Error::Game)?;
        if emitted.dump() != app.scene.dump() {
            return Err(Error::scenes(
                "scene.sg1 != the game's start (want is the game's emit; scene-gauntlet DIR --emit writes it)",
                &emitted.dump(),
                &app.scene.dump(),
            ));
        }
        out.push_str(&format!("game emit {}\n", crate::load::SCENE));
    }

    if let Some(clip) = &app.clip {
        let p0 = pose(&app.scene, clip, 0.0).map_err(|e| Error::Anim {
            path: "clip.an1".into(),
            source: e,
        })?;
        let t1 = clip.duration();
        let p1 = pose(&app.scene, clip, t1).map_err(|e| Error::Anim {
            path: "clip.an1".into(),
            source: e,
        })?;
        out.push_str(&format!(
            "pose t=0 nodes={} t={} nodes={} judged={CLIP_LOOKS}\n",
            p0.len(),
            compact_f(t1),
            p1.len()
        ));
    }

    let mixer = sceneanim::Mixer::new();
    let underlay = app.clip.as_ref().map(|c| (c, 0.0));
    let display =
        present(&app.scene, &mixer, |_| "Inspect.", underlay).map_err(|e| Error::Drive {
            path: "present".into(),
            source: e,
        })?;
    let mut fonts = crate::load::stock_fonts()?;
    let list = display
        .draw_list_with(&app.geom, &app.materials, &mut fonts)
        .map_err(|e| Error::Scene {
            path: "flatten".into(),
            source: e,
        })?;
    let mut seen = Vec::new();
    let moving = crate::problems::moving(app);
    let rest = look(app, &display, &mut fonts, first.as_deref(), moving)?;
    note(&mut seen, rest.found, 0);
    // The clip is the folder's idle life, so rest is judged through it: a
    // model can walk off the view or float mid-stride. Each problem names
    // the first moment that showed it.
    if let Some(clip) = &app.clip {
        let d = clip.duration();
        // Between moments, a node with `front=` must travel the way it faces.
        let mut before = display.clone();
        for i in 1..CLIP_LOOKS {
            let t = d * i as f32 / CLIP_LOOKS as f32;
            let posed =
                present(&app.scene, &mixer, |_| "Inspect.", Some((clip, t))).map_err(|e| {
                    Error::Drive {
                        path: "present".into(),
                        source: e,
                    }
                })?;
            let seen_then = look(app, &posed, &mut fonts, first.as_deref(), moving)?;
            crate::problems::note_clip(&mut seen, seen_then.found, t);
            let turned = scenegraph::problems::headings(&before, &posed);
            crate::problems::note_clip(&mut seen, turned, t);
            before = posed;
        }
    }
    let mut judge = Judge::default();
    judge.weigh(rest.focus, 0);
    let mut checked = rest.coverage;
    out.push_str(&list.census());
    // Each `models/KEY.glb` as interned, in its own units: the bounds are
    // what to read when `scale` asks for an `s=`.
    let geometry = app.geom.dump();
    let models: Vec<&str> = geometry.lines().filter(|l| l.contains(" model ")).collect();
    if !models.is_empty() {
        out.push_str("models\n");
        for line in models {
            out.push_str(line);
            out.push('\n');
        }
    }
    // The rest frame as text: what fills it and what is in front.
    out.push_str(&crate::replay::frame(
        app,
        &display,
        &mut fonts,
        options.close_up.as_deref(),
    )?);
    // Which textures the checks could read, and which they could not.
    let images = scenegraph::problems::image_census(&list, &app.images);
    if images != "images local=0 remote=0 absent=-\n" {
        out.push_str(&images);
    }
    out.push_str(&format!(
        "clock t={} dt={} n={}\n",
        compact_f(app.clock.t),
        compact_f(app.clock.dt),
        app.clock.n,
    ));
    if app.clock.n > 0 {
        score_clock(app, new_game, &mut out)?;
    }

    let ndc = app.session.ndc;
    let aspect = app.session.aspect;
    if ndc != [0.0, 0.0] || app.session.sel.is_some() {
        let (_, sess, _) = step(
            &app.scene,
            &list,
            &app.session,
            Event::down(ndc[0], ndc[1], aspect),
        )
        .map_err(|e| Error::Input {
            path: "session.in1".into(),
            source: e,
        })?;
        let got = sess.sel.as_ref().map(|s| s.name.as_str()).unwrap_or("-");
        if let Some(want) = &app.session.sel {
            if got != want.name {
                return Err(Error::HitMismatch {
                    want: want.name.clone(),
                    got: got.to_string(),
                });
            }
        }
        out.push_str(&format!(
            "hit ndc={},{} name={got}\n",
            compact_f(ndc[0]),
            compact_f(ndc[1])
        ));
        if let Some(sel) = &sess.sel {
            let at = Pointed {
                eye: list.camera.eye,
                point: sel.point,
            };
            score_ply(app, new_game, got, &at, &mut out)?;
        }
    }

    if let Some(replay) = &app.replay {
        let game = new_game.map(|f| f()).transpose()?;
        let (_, score) = crate::replay::play_with(app, replay, game, false, &options)?;
        out.push_str(&score.dump());
        for (line, frame) in &score.sketches {
            out.push_str(&format!("after @replay.rp1:{line}\n{frame}"));
        }
        checked.add(score.coverage);
        judge.absorb(&score.judge);
        for s in score.problems {
            note(&mut seen, vec![s.problem], s.line);
        }
    }
    for s in judge.verdicts() {
        note(&mut seen, vec![s.problem], s.line);
    }

    let mut dump = format!("gauntlet\nproblems {}\n", seen.len());
    for s in &seen {
        dump.push_str(&s.dump());
        dump.push('\n');
    }
    let mut checked_line = checked.dump();
    if moving {
        // A moving view judges only what it brought into focus: say how much.
        checked_line.pop();
        checked_line.push_str(&format!(" in-focus={}\n", judge.judged()));
        // And by name, so "the key was looked at" is read, not inferred.
        // All of them: a truncated list hides the very sign being checked.
        // Each with where it was first read: `@rest`, `@LINE` of the replay,
        // or `@never` (that one is also a problem above).
        let names: Vec<String> = judge
            .read_at()
            .into_iter()
            .map(|(name, at)| match at {
                Some(0) => format!("{name}@rest"),
                Some(line) => format!("{name}@{line}"),
                None => format!("{name}@never"),
            })
            .collect();
        if !names.is_empty() {
            checked_line.push_str(&format!("in-focus {}\n", names.join(" ")));
        }
    }
    dump.push_str(&checked_line);
    dump.push_str(&out);
    if !seen.is_empty() {
        return Err(Error::Problems(dump));
    }
    dump.push_str("ok\n");
    Ok(dump)
}

/// What `scene-gauntlet DIR --emit` writes for a game folder, so nothing has
/// to be copied by hand: `scene.sg1` is the game's start; when the folder
/// has one ply (a `session.in1` hit, or `clock.tk1` `n=` ticks), `after.sg1`
/// and `delta.sgd1` are that ply. Read them before keeping them.
pub fn emit_files(app: &App, new_game: NewGame) -> Result<Vec<(&'static str, String)>, Error> {
    let rest = new_game()?.emit().map_err(Error::Game)?;
    let mut files = vec![(crate::load::SCENE, rest.dump())];
    let mut played = None;
    if app.session.ndc != [0.0, 0.0] || app.session.sel.is_some() {
        let mut fonts = crate::load::stock_fonts()?;
        let list = rest
            .draw_list_with(&app.geom, &app.materials, &mut fonts)
            .map_err(|e| Error::Scene {
                path: "flatten".into(),
                source: e,
            })?;
        let ndc = app.session.ndc;
        let (_, sess, _) = step(
            &rest,
            &list,
            &app.session,
            Event::down(ndc[0], ndc[1], app.session.aspect),
        )
        .map_err(|e| Error::Input {
            path: "session.in1".into(),
            source: e,
        })?;
        if let Some(sel) = sess.sel {
            let mut game = new_game()?;
            let at = Pointed {
                eye: list.camera.eye,
                point: sel.point,
            };
            if game.pick_at(&sel.name, &at).map_err(Error::Game)? == Step::Applied {
                played = Some(game.emit().map_err(Error::Game)?);
            }
        }
    } else if app.clock.n > 0 {
        let mut game = new_game()?;
        for _ in 0..app.clock.n {
            if game.tick().map_err(Error::Game)? == Step::Blocked {
                break;
            }
        }
        played = Some(game.emit().map_err(Error::Game)?);
    }
    if let Some(played) = played.filter(|p| p.dump() != rest.dump()) {
        files.push((crate::load::AFTER, played.dump()));
        files.push((
            crate::load::DELTA,
            crate::patch::diff(&rest, &played).dump(),
        ));
    }
    Ok(files)
}

/// `clock.n` ticks. With a game, its ticks are the truth and `act.act1`
/// (if present) must describe the same rest write; without one, the act is.
fn score_clock(app: &App, new_game: Option<NewGame>, out: &mut String) -> Result<(), Error> {
    let mut clock = app.clock;
    let by_act = match &app.act {
        Some(act) => {
            let mut scene = app.scene.clone();
            for _ in 0..clock.n {
                scene = crate::apply(&scene, act)?;
            }
            Some(scene)
        }
        None => None,
    };
    let played = match new_game {
        Some(new_game) => {
            let mut game = new_game()?;
            for _ in 0..clock.n {
                if game.tick().map_err(Error::Game)? == Step::Blocked {
                    break;
                }
            }
            Some(game.emit().map_err(Error::Game)?)
        }
        None => by_act.clone(),
    };
    for _ in 0..clock.n {
        clock.step(clock.dt);
    }
    out.push_str(&format!("tick t={}\n", compact_f(clock.t)));
    let Some(played) = played else {
        crate::drive(&app.scene, &app.clock)?;
        return Ok(());
    };
    if let Some(by_act) = &by_act {
        if by_act.dump() != played.dump() {
            return Err(Error::scenes(
                "act.act1 writes another rest than the game's ticks (want is the game's)",
                &played.dump(),
                &by_act.dump(),
            ));
        }
        out.push_str("act act.act1\n");
    }
    if played.dump() != app.scene.dump() {
        score_after(app, &played, out)?;
        score_delta(app, &app.scene, &played, out)?;
    }
    Ok(())
}

/// One ply at the session hit. With a game it is a [`Game::pick_at`], from
/// rest's camera; without one only `act.act1` can write rest, and its
/// target must be the hit.
fn score_ply(
    app: &App,
    new_game: Option<NewGame>,
    hit: &str,
    at: &Pointed,
    out: &mut String,
) -> Result<(), Error> {
    if let Some(act) = &app.act {
        if let Some(want) = act.target() {
            if want != hit {
                return Err(Error::ActTarget {
                    want: want.to_string(),
                    got: hit.to_string(),
                });
            }
        }
    }
    let by_act = app
        .act
        .as_ref()
        .map(|act| crate::apply(&app.scene, act))
        .transpose()?;
    let played = match new_game {
        Some(new_game) => {
            let mut game = new_game()?;
            let step = game.pick_at(hit, at).map_err(Error::Game)?;
            out.push_str(&format!(
                "pick {hit} {}\n",
                if step == Step::Blocked {
                    "blocked"
                } else {
                    "applied"
                }
            ));
            Some(game.emit().map_err(Error::Game)?)
        }
        None => by_act.clone(),
    };
    if let Some(by_act) = &by_act {
        if let Some(played) = &played {
            if by_act.dump() != played.dump() {
                return Err(Error::scenes(
                    "act.act1 writes another rest than the game's pick (want is the game's)",
                    &played.dump(),
                    &by_act.dump(),
                ));
            }
        }
        out.push_str("act act.act1\n");
    }
    if let Some(played) = played {
        if played.dump() != app.scene.dump() {
            score_after(app, &played, out)?;
            score_delta(app, &app.scene, &played, out)?;
        }
    }
    Ok(())
}

fn score_delta(
    app: &App,
    before: &scenegraph::Scene,
    after: &scenegraph::Scene,
    out: &mut String,
) -> Result<(), Error> {
    let got = crate::patch::diff(before, after);
    if let Some(want) = &app.delta {
        if got.dump() != want.dump() {
            return Err(Error::lines(
                "delta.sgd1 != rest to after (got is the real delta; scene-gauntlet DIR --emit writes it)",
                &want.dump(),
                &got.dump(),
            ));
        }
        out.push_str("delta delta.sgd1\n");
    } else if !got.lines.is_empty() {
        return Err(Error::MissingDelta);
    }
    Ok(())
}

fn score_after(app: &App, played: &scenegraph::Scene, out: &mut String) -> Result<(), Error> {
    let want = app.after.as_ref().ok_or(Error::MissingAfter)?;
    if played.dump() != want.dump() {
        return Err(Error::scenes(
            "after.sg1 != the ply (got is what was played; scene-gauntlet DIR --emit writes it)",
            &want.dump(),
            &played.dump(),
        ));
    }
    out.push_str("play after.sg1\n");
    Ok(())
}

/// Every document the folder holds must survive dump → parse → dump.
fn roundtrip_all(app: &App) -> Result<(), Error> {
    roundtrip_scene(&app.scene, "scene.sg1")?;
    if let Some(after) = &app.after {
        roundtrip_scene(after, "after.sg1")?;
    }
    if !app.materials.is_empty() {
        let d = app.materials.dump();
        let back = scenegraph::Materials::from_dump(&d).map_err(|e| Error::Scene {
            path: "materials.mt1".into(),
            source: e,
        })?;
        if back.dump() != d {
            return Err(Error::RoundTrip("materials.mt1".into()));
        }
    }
    if let Some(clip) = &app.clip {
        let back = sceneanim::Clip::from_dump(&clip.dump()).map_err(|e| Error::Anim {
            path: "clip.an1".into(),
            source: e,
        })?;
        if back.dump() != clip.dump() {
            return Err(Error::RoundTrip("clip.an1".into()));
        }
    }
    if let Some(driver) = &app.driver {
        let back = scenedrv::Driver::from_dump(&driver.dump()).map_err(|e| Error::Drive {
            path: "drive.drv1".into(),
            source: e,
        })?;
        if back.dump() != driver.dump() {
            return Err(Error::RoundTrip("drive.drv1".into()));
        }
        driver.clip(&app.scene).map_err(|e| Error::Drive {
            path: "drive.drv1".into(),
            source: e,
        })?;
    }
    if let Some(world) = &app.world {
        let back = sceneworld::World::from_dump(&world.dump()).map_err(|e| Error::World {
            path: "world.sw1".into(),
            source: e,
        })?;
        if back.dump() != world.dump() {
            return Err(Error::RoundTrip("world.sw1".into()));
        }
    }
    let sess_dump = app.session.dump();
    let sess_back = sceneinput::Session::from_dump(&sess_dump).map_err(|e| Error::Input {
        path: "session.in1".into(),
        source: e,
    })?;
    if sess_back.dump() != sess_dump {
        return Err(Error::RoundTrip("session.in1".into()));
    }
    if let Some(act) = &app.act {
        let back = crate::act::Act::from_dump(&act.dump())?;
        if back.dump() != act.dump() {
            return Err(Error::RoundTrip("act.act1".into()));
        }
    }
    if let Some(delta) = &app.delta {
        let back = crate::patch::Delta::from_dump(&delta.dump())?;
        if back.dump() != delta.dump() {
            return Err(Error::RoundTrip("delta.sgd1".into()));
        }
    }
    if app.clock_live {
        let back = crate::clock::Clock::from_dump(&app.clock.dump())?;
        if back.dump() != app.clock.dump() {
            return Err(Error::RoundTrip("clock.tk1".into()));
        }
    }
    if let Some(replay) = &app.replay {
        let back = crate::replay::Replay::from_dump(&replay.dump())?;
        if back.dump() != replay.dump() {
            return Err(Error::RoundTrip("replay.rp1".into()));
        }
    }
    Ok(())
}

fn roundtrip_scene(scene: &scenegraph::Scene, path: &str) -> Result<(), Error> {
    let d = scene.dump();
    let back = scenegraph::Scene::from_dump(&d).map_err(|e| Error::Scene {
        path: path.into(),
        source: e,
    })?;
    if back.dump() != d {
        return Err(Error::RoundTrip(path.into()));
    }
    Ok(())
}

fn compact_f(v: f32) -> String {
    if v == 0.0 {
        return "0".into();
    }
    let s = format!("{v:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}
