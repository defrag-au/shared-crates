//! A scene **app** is a folder of dumps. The runner presents; the gauntlet
//! scores. No fixture enum, no shaders in the document, no game rules: a
//! game crate decides what a pick or a key does, behind [`Game`].

mod act;
mod clock;
mod error;
mod fx;
mod game;
mod gauntlet;
mod load;
mod mismatch;
mod patch;
mod problems;
mod replay;
pub mod view;

pub use act::{apply, Act, Op, VERSION as ACT_VERSION};
pub use clock::{drive, propose_tick, Clock, DT, VERSION as CLOCK_VERSION};
pub use error::Error;
pub use fx::{Fx, Op as FxOp, VERSION as FX_VERSION};
pub use game::{Game, Pointed, Step};
pub use gauntlet::{emit_files, gauntlet, gauntlet_with, NewGame, Options};
pub use load::{stock_fonts, App};
pub use patch::{diff, Delta, Line as DeltaLine, VERSION as DELTA_VERSION};
pub use problems::{look, moving, Judge, Look, Seen, INSPECT_TAG};
pub use replay::{
    play, play_with, Aim, Replay, ReplayStep, Score, Verb, VERSION as REPLAY_VERSION,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Four app folders kept as fixtures: a room, a card table, a paged
    /// gallery and a tic-tac-toe board (scored here without its game).
    fn apps_root() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/apps")
    }

    #[test]
    fn room_gauntlet_ok() {
        let app = App::load(apps_root().join("room")).unwrap();
        let d = gauntlet(&app, None).unwrap();
        assert!(d.starts_with("gauntlet\n"), "{d}");
        assert!(d.contains("roundtrip scene.sg1"), "{d}");
        assert!(d.contains("census\n"), "{d}");
        assert!(d.contains("ok\n"), "{d}");
    }

    #[test]
    fn table_gauntlet_ok() {
        let app = App::load(apps_root().join("table")).unwrap();
        let d = gauntlet(&app, None).unwrap();
        assert!(
            d.contains("roundtrip scene.sg1 materials.mt1 clip.an1 session.in1"),
            "{d}"
        );
        assert!(d.contains("pose t=0"), "{d}");
        assert!(d.contains("ok\n"), "{d}");
    }

    #[test]
    fn gallery_gauntlet_ok() {
        let app = App::load(apps_root().join("gallery")).unwrap();
        let d = gauntlet(&app, None).unwrap();
        assert!(d.contains("roundtrip world.sw1"), "{d}");
        assert!(d.contains("ok\n"), "{d}");
        let golden = include_str!("../../scenegraph/tests/golden/gallery.sg1");
        assert_eq!(app.scene.dump(), golden);
    }

    #[test]
    fn without_a_game_a_cell_hit_writes_nothing() {
        // The shell holds no rules: the ttt folder's hit alone is not a move.
        let mut app = App::load(apps_root().join("ttt")).unwrap();
        app.act = None;
        app.replay = None;
        let d = gauntlet(&app, None).unwrap();
        assert!(d.contains("hit ndc=0,0 name=c11"), "{d}");
        assert!(!d.contains("play after.sg1"), "{d}");
    }

    #[test]
    fn without_a_game_the_act_alone_is_the_ply() {
        let mut app = App::load(apps_root().join("ttt")).unwrap();
        app.replay = None;
        let d = gauntlet(&app, None).unwrap();
        assert!(d.contains("act act.act1"), "{d}");
        assert!(d.contains("play after.sg1"), "{d}");
        assert!(d.contains("delta delta.sgd1"), "{d}");
    }

    #[test]
    fn replay_without_a_game_rejects_moves() {
        let mut app = App::load(apps_root().join("ttt")).unwrap();
        let replay = app.replay.take().expect("ttt has a replay");
        match play(&app, &replay, None, false) {
            Err(Error::ReplayNeedsGame { .. }) => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn replay_picks_assert_hits_without_a_game() {
        let app = App::load(apps_root().join("room")).unwrap();
        let replay = Replay::from_dump("rp1\npick ndc=0,0\n").unwrap();
        let (blessed, score) = play(&app, &replay, None, true).unwrap();
        assert_eq!(score.picks, 1);
        let Verb::Pick { hit, .. } = &blessed.steps[0].verb else {
            panic!("pick");
        };
        assert!(!hit.is_empty());
        let (_, again) = play(&app, &blessed, None, false).unwrap();
        assert_eq!(again, score);
    }

    #[test]
    fn missing_rest_errors() {
        let err = match App::load(apps_root()) {
            Err(e) => e,
            Ok(_) => panic!("expected missing rest"),
        };
        assert!(matches!(err, Error::MissingRest), "{err}");
    }

    #[test]
    fn from_source_reads_without_a_filesystem() {
        let app = App::from_source("mem", |name| match name {
            // The fixture folder's room: the golden one's tet floats
            // (`floating`), which its census tests do not mind.
            "scene.sg1" => Some(include_str!("../tests/fixtures/apps/room/scene.sg1").to_string()),
            _ => None,
        })
        .unwrap();
        assert_eq!(app.files(), ["scene.sg1".to_string()]);
        assert!(gauntlet(&app, None).unwrap().ends_with("ok\n"));
    }

    // A dark cell: the white test labels on it read (`contrast`).
    const BOARD: &str = "sg1
b Group
cell Quad 1x1 sh=Unlit c=2a2a2a p=b tag=cell
cam Camera eye=0,0,4 at=0,0,0
";

    fn board_app(extra: &str, replay: Option<&str>) -> App {
        let scene = format!("{BOARD}{extra}");
        let replay = replay.map(str::to_string);
        App::from_source("mem", move |name| match name {
            "scene.sg1" => Some(scene.clone()),
            "replay.rp1" => replay.clone(),
            _ => None,
        })
        .unwrap()
    }

    /// Any tagged node is meant to be seen, not only a game's controls: an
    /// exhibit wholly behind the board's cell is `covered`, and the sketch
    /// lists it as unseen.
    #[test]
    fn a_tagged_node_hidden_behind_another_is_covered() {
        let app = board_app(
            "hidden Mesh Box c=aa5533 t=0,0,-1 s=0.3,0.3,0.3 tag=exhibit\n",
            None,
        );
        match gauntlet(&app, None) {
            Err(Error::Problems(dump)) => {
                assert!(dump.contains("\ncovered hidden by=cell\n"), "{dump}");
                assert!(dump.contains("\nunseen hidden\n"), "{dump}");
            }
            other => panic!("{other:?}"),
        }
    }

    /// A clip that slides a model facing +Z along -Z walks it backwards:
    /// `heading`, at the first moment that showed it. Turned round, it is
    /// walking forwards.
    #[test]
    fn a_clip_walking_a_model_backwards_is_heading() {
        let app = |r: &str| {
            let scene = format!(
                "{BOARD}walker Mesh Geom g=tet front=+z t=0,-1,0 r=0,{r},0 s=0.3,0.3,0.3\n"
            );
            App::from_source("mem", move |name| match name {
                "scene.sg1" => Some(scene.clone()),
                "clip.an1" => Some("an1\nback\nwalker t 0:0,-1,0 1:0,-1,-1\n".into()),
                _ => None,
            })
            .unwrap()
        };
        match gauntlet(&app("0"), None) {
            Err(Error::Problems(dump)) => assert!(
                dump.contains(
                    "\nheading walker travels 180° away from its front=+z @clip.an1 t=0.12\n"
                ),
                "{dump}"
            ),
            other => panic!("{other:?}"),
        }
        let turned = gauntlet(&app("3.1416"), None).unwrap();
        assert!(!turned.contains("heading"), "{turned}");
    }

    /// A clip that carries a sign out of the view halfway is reported at
    /// the moment it first showed, though rest and the clip's start are
    /// fine.
    #[test]
    fn rest_is_judged_through_its_clip() {
        let scene = format!("{BOARD}sign Label font=ibm size=0.3 \"Hi\" t=0,1,0\n");
        let app = App::from_source("mem", move |name| match name {
            "scene.sg1" => Some(scene.clone()),
            "clip.an1" => Some("an1\ndrift\nsign t 0:0,1,0 0.5:0,1,0 1:40,1,0\n".into()),
            _ => None,
        })
        .unwrap();
        match gauntlet(&app, None) {
            Err(Error::Problems(dump)) => {
                assert!(dump.contains("off-camera sign @clip.an1 t=0.62"), "{dump}");
                assert!(dump.contains("judged=8"), "{dump}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn rest_problems_fail_the_gauntlet_and_lead_its_dump() {
        let app = board_app("fine Label font=ibm size=0.01 \"tiny\" t=0,1,0\n", None);
        match gauntlet(&app, None) {
            Err(Error::Problems(dump)) => {
                assert!(
                    dump.starts_with("gauntlet\nproblems 1\nsmall fine em="),
                    "{dump}"
                );
                assert!(dump.contains("\ncensus\n"), "{dump}");
                assert!(!dump.ends_with("ok\n"), "{dump}");
            }
            other => panic!("{other:?}"),
        }
    }

    /// A room with a sign on its far wall, written small: readable up close,
    /// not from the door.
    const ROOM: &str = "sg1
floor Quad 8x12 c=4a4038 r=-1.5708,0,0
wall Quad 8x3 c=2a2a2a t=0,1.5,-6
sign Label font=ibm size=0.08 \"Exit\" p=wall t=0,0,0.02
cam Camera eye=0,1.5,5 at=0,1.5,0
";

    fn room_app(replay: &str) -> App {
        let replay = replay.to_string();
        App::from_source("room", move |name| match name {
            "scene.sg1" => Some(ROOM.to_string()),
            "replay.rp1" => Some(replay.clone()),
            _ => None,
        })
        .unwrap()
    }

    #[test]
    fn camera_steps_assert_their_view_and_survive_a_bless() {
        let app = room_app("rp1\nwalk f=2\nturn dx=0.25\norbit dx=0.1 zoom=-0.1\n");
        let (blessed, score) = play(&app, app.replay.as_ref().unwrap(), None, true).unwrap();
        assert_eq!(score.moves, 3);
        assert!(score.dump().contains(" moves=3 "), "{}", score.dump());
        assert!(blessed.steps.iter().all(|s| s.view.starts_with("eye=")));
        assert!(
            blessed.steps[0].view.starts_with("eye=0,1.5,3 "),
            "{}",
            blessed.steps[0].view
        );
        // A view is not rest: walking writes no delta.
        assert!(blessed.steps.iter().all(|s| s.delta.is_empty()));
        let back = Replay::from_dump(&blessed.dump()).unwrap();
        assert_eq!(back.dump(), blessed.dump());
        assert!(play(&app, &back, None, false).is_ok());
    }

    /// The room as a game that sees the camera: the wall is a move only
    /// within arm's reach, and crossing the room's midline is a move too.
    struct Reach {
        touched: bool,
        crossed: bool,
        fx: Fx,
    }

    impl Game for Reach {
        fn dump(&self) -> String {
            format!("reach {} {}\n", self.touched, self.crossed)
        }
        fn emit(&self) -> Result<scenegraph::Scene, String> {
            let mut d = ROOM.to_string();
            if self.crossed {
                d.push_str("mark Label font=ibm size=0.3 \"Halfway\" p=wall t=0,-0.8,0.02\n");
            }
            scenegraph::Scene::from_dump(&d).map_err(|e| e.to_string())
        }
        fn tick(&mut self) -> Result<Step, String> {
            Ok(Step::Blocked)
        }
        fn input(&mut self, _token: &str) -> Result<Step, String> {
            Ok(Step::Blocked)
        }
        fn pick_at(&mut self, name: &str, at: &Pointed) -> Result<Step, String> {
            if name != "wall" || self.touched || at.reach() > 2.0 {
                self.fx = Fx::new();
                return Ok(Step::Blocked);
            }
            self.touched = true;
            self.fx = Fx::new().sound("touch");
            Ok(Step::Applied)
        }
        fn moved(&mut self, eye: scenegraph::Vec3) -> Result<Step, String> {
            if self.crossed || eye.z > 0.0 {
                return Ok(Step::Blocked);
            }
            self.crossed = true;
            self.fx = Fx::new().sound("halfway");
            Ok(Step::Applied)
        }
        fn bindings(&self) -> &'static [(&'static str, &'static str)] {
            &[]
        }
        fn fx(&self) -> &Fx {
            &self.fx
        }
    }

    #[test]
    fn a_game_sees_how_far_away_a_pick_was_and_where_the_eye_went() {
        let app = room_app("rp1\npick node=wall\nwalk f=3\nwalk f=6\npick node=wall\n");
        let game = || {
            Box::new(Reach {
                touched: false,
                crossed: false,
                fx: Fx::new(),
            }) as Box<dyn Game>
        };
        let (blessed, score) =
            play(&app, app.replay.as_ref().unwrap(), Some(game()), true).unwrap();
        let s = &blessed.steps;
        // From the door the wall is 11 away: out of reach.
        assert!(s[0].blocked, "{}", blessed.dump());
        // The first walk stays on the near half: nothing happens.
        assert!(
            s[1].fx.is_empty() && s[1].delta.is_empty(),
            "{}",
            blessed.dump()
        );
        // The second crosses the midline: the game's move, with its cue and delta.
        assert_eq!(s[2].fx, ["halfway"], "{}", blessed.dump());
        assert!(
            s[2].delta[0].dump().starts_with("+ mark "),
            "{}",
            blessed.dump()
        );
        // Now within reach: the same pick is a move.
        assert!(!s[3].blocked && s[3].fx == ["touch"], "{}", blessed.dump());
        assert_eq!(score.blocked, 1);
        // Blessed, it replays exactly.
        let back = Replay::from_dump(&blessed.dump()).unwrap();
        assert!(play(&app, &back, Some(game()), false).is_ok());
    }

    #[test]
    fn an_aim_at_something_off_screen_is_never_an_outcome() {
        // Walked up so close to the wall that its sign is out of view: the
        // blessed `hit=undrawn:sign` must not pass the next run.
        let app = room_app("rp1\nturn dx=1.2\npick node=sign\n");
        let (blessed, _) = play(&app, app.replay.as_ref().unwrap(), None, true).unwrap();
        let d = blessed.dump();
        assert!(d.contains("pick node=sign hit=undrawn:sign"), "{d}");
        let again = room_app(&d);
        match play(&again, again.replay.as_ref().unwrap(), None, false) {
            Err(Error::ReplayMismatch {
                line, what, got, ..
            }) => {
                assert_eq!((line, what.as_str()), (4, "aim"));
                assert!(got.contains("not on screen"), "{got}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_wrong_view_names_its_line() {
        let app = room_app("rp1\nwalk f=2\nview eye=0,1.5,9 at=0,1.5,0\n");
        match play(&app, app.replay.as_ref().unwrap(), None, false) {
            Err(Error::ReplayMismatch {
                line, what, got, ..
            }) => {
                assert_eq!((line, what.as_str()), (2, "view"));
                assert!(got.starts_with("eye=0,1.5,3"), "{got}");
            }
            other => panic!("{other:?}"),
        }
        assert!(Replay::from_dump("rp1\npick ndc=0,0\nview eye=0,0,0 at=0,0,1\n").is_err());
        assert!(Replay::from_dump("rp1\nwalk f=1 blocked\n").is_err());
        assert!(Replay::from_dump("rp1\nwalk dx=1\n").is_err());
    }

    /// The gauntlet's verdict on `replay` in the room, as its problem lines.
    fn room_problems(replay: &str) -> Vec<String> {
        let app = room_app(replay);
        let (blessed, _) = play(&app, app.replay.as_ref().unwrap(), None, true).unwrap();
        let app = room_app(&blessed.dump());
        match gauntlet(&app, None) {
            Ok(_) => Vec::new(),
            Err(Error::Problems(d)) => d
                .lines()
                .skip(2)
                .take_while(|l| !l.starts_with("checked "))
                .map(str::to_string)
                .collect(),
            Err(e) => panic!("{e}"),
        }
    }

    #[test]
    fn a_walk_judges_text_by_the_best_look_it_got() {
        // Stood at the door looking at it the whole time: never readable.
        let far = room_problems("rp1\nwalk f=0.5\n");
        assert_eq!(far.len(), 1, "{far:?}");
        assert!(far[0].starts_with("small sign em="), "{far:?}");
        // Walk up to it: one close look is enough, and nothing is reported.
        assert_eq!(
            room_problems("rp1\nwalk f=0.5\nwalk f=9\n"),
            Vec::<String>::new()
        );
        // A fixed view judges everything from where rest's camera is.
        let fixed =
            App::from_source("room", |n| (n == "scene.sg1").then(|| ROOM.to_string())).unwrap();
        let mut fonts = stock_fonts().unwrap();
        let seen = look(&fixed, &fixed.scene, &mut fonts, None, false).unwrap();
        assert!(
            seen.found.iter().any(|p| p.kind() == "small"),
            "{:?}",
            seen.found
        );
        assert!(seen.focus.is_empty());
    }

    #[test]
    fn what_a_walk_never_looks_at_is_not_judged() {
        // Facing the side of the room from the start, and walking that way:
        // the sign is never in focus, so it is not judged, and the checked
        // line says nothing was.
        let sideways = ROOM.replace("at=0,1.5,0", "at=5,1.5,5");
        let replay = "rp1\nwalk f=0.5\n".to_string();
        let read = move |name: &str| match name {
            "scene.sg1" => Some(sideways.clone()),
            "replay.rp1" => Some(replay.clone()),
            _ => None,
        };
        let app = App::from_source("room", read.clone()).unwrap();
        let (blessed, _) = play(&app, app.replay.as_ref().unwrap(), None, true).unwrap();
        let blessed = blessed.dump();
        let app = App::from_source("room", move |name| {
            (name == "replay.rp1")
                .then(|| blessed.clone())
                .or_else(|| read(name))
        })
        .unwrap();
        let d = gauntlet(&app, None).unwrap_or_else(|e| panic!("{e}"));
        assert!(d.contains("problems 0\n"), "{d}");
        assert!(d.contains(" in-focus=0\n"), "{d}");
    }

    /// Grows a label too wide for its cell on its first input.
    struct Spill {
        moved: bool,
    }

    impl Game for Spill {
        fn dump(&self) -> String {
            format!("spill {}\n", self.moved)
        }
        fn emit(&self) -> Result<scenegraph::Scene, String> {
            let extra = if self.moved {
                "mark Label font=ibm size=0.5 \"TOO WIDE\" p=cell t=0,0,0.05\n"
            } else {
                ""
            };
            scenegraph::Scene::from_dump(&format!("{BOARD}{extra}")).map_err(|e| e.to_string())
        }
        fn tick(&mut self) -> Result<Step, String> {
            Ok(Step::Blocked)
        }
        fn input(&mut self, _token: &str) -> Result<Step, String> {
            self.moved = true;
            Ok(Step::Applied)
        }
        fn bindings(&self) -> &'static [(&'static str, &'static str)] {
            &[]
        }
        fn control_tags(&self) -> &'static [&'static str] {
            &["cell"]
        }
    }

    /// Takes one drag: `cell` dropped on `slot` moves the cell there.
    struct Slide {
        moved: bool,
    }

    const SLOT: &str = "slot Quad 1x1 sh=Unlit p=b t=1.5,0,-0.1\n";

    impl Game for Slide {
        fn dump(&self) -> String {
            format!("slide {}\n", self.moved)
        }
        fn emit(&self) -> Result<scenegraph::Scene, String> {
            let board = if self.moved {
                BOARD.replace("tag=cell", "t=1.5,0,0 tag=cell")
            } else {
                BOARD.to_string()
            };
            scenegraph::Scene::from_dump(&format!("{board}{SLOT}")).map_err(|e| e.to_string())
        }
        fn tick(&mut self) -> Result<Step, String> {
            Ok(Step::Blocked)
        }
        fn input(&mut self, _token: &str) -> Result<Step, String> {
            Ok(Step::Blocked)
        }
        fn drag(&mut self, from: &str, to: &str) -> Result<Step, String> {
            if self.moved || (from, to) != ("cell", "slot") {
                return Ok(Step::Blocked);
            }
            self.moved = true;
            Ok(Step::Applied)
        }
        fn bindings(&self) -> &'static [(&'static str, &'static str)] {
            &[]
        }
    }

    #[test]
    fn a_drag_is_the_games_and_both_ends_are_asserted() {
        let app = board_app(SLOT, None);
        let replay = Replay::from_dump(
            "rp1\ndrag node=cell to=cell\ndrag node=cell to=slot\ndrag node=cell to=0.9,0.9\n",
        )
        .unwrap();
        let (blessed, score) =
            play(&app, &replay, Some(Box::new(Slide { moved: false })), true).unwrap();
        assert_eq!(score.drags, 3);
        assert_eq!(score.picks, 0);
        assert_eq!(score.blocked, 2);
        let dump = blessed.dump();
        assert!(
            dump.starts_with(
                "rp1\ndrag node=cell to=cell hit=cell drop=cell blocked\n\
                 drag node=cell to=slot hit=cell drop=slot\n\
                 ~ cell Quad 1x1 sh=Unlit c=2a2a2a p=b t=1.5,0,0 tag=cell\n\
                 drag node=cell to=0.9,0.9 hit=cell drop=- blocked\n"
            ),
            "{dump}"
        );
        let again = Replay::from_dump(&dump).unwrap();
        let (_, rescored) =
            play(&app, &again, Some(Box::new(Slide { moved: false })), false).unwrap();
        assert_eq!(rescored, score);
        let wrong = Replay::from_dump(&dump.replacen("drop=slot", "drop=cell", 1)).unwrap();
        match play(&app, &wrong, Some(Box::new(Slide { moved: false })), false) {
            Err(Error::ReplayMismatch { line, what, .. }) => {
                assert_eq!((line, what.as_str()), (3, "drop"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn without_a_game_a_drag_asserts_its_hits_only() {
        let app = board_app(SLOT, None);
        let replay = Replay::from_dump("rp1\ndrag node=cell to=slot hit=cell drop=slot\n").unwrap();
        let (_, score) = play(&app, &replay, None, false).unwrap();
        assert_eq!((score.drags, score.blocked), (1, 0));
        let refused =
            Replay::from_dump("rp1\ndrag node=cell to=slot hit=cell drop=slot blocked\n").unwrap();
        assert!(matches!(
            play(&app, &refused, None, false),
            Err(Error::ReplayNeedsGame { .. })
        ));
    }

    #[test]
    fn a_problem_a_step_makes_is_pinned_to_its_replay_line() {
        let app = board_app("", Some("rp1\n# first\ninput go\n"));
        let replay = app.replay.clone().unwrap();
        let (_, score) = play(&app, &replay, Some(Box::new(Spill { moved: false })), true).unwrap();
        let seen: Vec<String> = score.problems.iter().map(Seen::dump).collect();
        assert_eq!(
            seen,
            ["off-print mark parent=cell spills past the face @replay.rp1:3"]
        );
    }
}
