//! expand → pose → sg1. No raster.

use sceneanim::{pose_layers, Clip, Mixer};
use scenedrv::{
    flavor_node, inspect, present, Driver, Fan, Orbit, To, FADE_OPEN, LIFT_OPEN, LIFT_T, OPEN,
};
use scenegraph::{mat4_inverse, transform_point, Kind, Node, Prim, Scene, Vec3};

fn token() -> Scene {
    let mut s = Scene::new();
    s.insert(Node::group("token")).unwrap();
    s.insert(
        Node::quad("skin", 2.0, 2.0)
            .parent("token")
            .tag("layer")
            .at(Vec3::new(0.0, 0.0, 0.0)),
    )
    .unwrap();
    s.insert(
        Node::quad("hat", 2.0, 2.0)
            .parent("token")
            .tag("layer")
            .at(Vec3::new(0.0, 0.0, 0.001)),
    )
    .unwrap();
    s.insert(Node::camera("cam", Vec3::new(0.0, 0.0, 6.0), Vec3::ZERO))
        .unwrap();
    s
}

#[test]
fn golden_roundtrip() {
    let src = include_str!("golden/showcase.drv1");
    let d = Driver::from_dump(src).unwrap();
    assert_eq!(d.dump(), src);
}

#[test]
fn clip_pose_opens_then_orbits() {
    let scene = token();
    let rest = scene.dump();
    let drv = Driver::from_dump(include_str!("golden/showcase.drv1")).unwrap();
    let clip = drv.clip(&scene).unwrap();
    assert_eq!(clip.name, "showcase");

    let t0 = clip.pose(&scene, 0.0).unwrap();
    let hat0 = t0.get("hat").unwrap().xf.t;
    assert!(
        (hat0 - scene.get("hat").unwrap().xf.t).length() < 1e-4,
        "t=0 hat {hat0:?}"
    );
    let Kind::Camera { eye, .. } = t0.get("cam").unwrap().kind else {
        panic!("cam");
    };
    assert!(eye.x.abs() < 1e-3 && eye.z > 5.0, "t=0 frontal eye={eye:?}");

    let open = clip.pose(&scene, OPEN).unwrap();
    let hat_open = open.get("hat").unwrap().xf.t;
    assert!(hat_open.z > 0.4, "fan must move hat along z, {hat_open:?}");
    let Kind::Camera { eye, .. } = open.get("cam").unwrap().kind else {
        panic!("cam");
    };
    assert!(eye.x.abs() < 0.5, "OPEN still frontal, eye={eye:?}");

    let show = clip.pose(&scene, 1.0).unwrap();
    let hat_show = show.get("hat").unwrap().xf.t;
    assert!(
        (hat_show - hat_open).length() < 1e-3,
        "layers hold while the camera moves"
    );
    let Kind::Camera { eye, at, .. } = show.get("cam").unwrap().kind else {
        panic!("cam");
    };
    assert!(eye.x.abs() > 1.0, "t=1 camera off axis, eye={eye:?}");
    assert!(at.z > 0.1, "look-at in the fan, at={at:?}");
    assert_eq!(scene.dump(), rest, "clip must not write rest");
}

#[test]
fn rust_hook_same_as_dump() {
    let scene = token();
    let from_dump = Driver::from_dump("drv1\nshowcase\nfan tag=layer\norbit cam=cam\n")
        .unwrap()
        .clip(&scene)
        .unwrap();
    let from_rust = Driver::new("showcase")
        .op(Fan::tag("layer"))
        .op(Orbit::cam("cam"))
        .clip(&scene)
        .unwrap();
    assert_eq!(from_dump.dump(), from_rust.dump());
}

#[test]
fn fan_missing_tag_errors() {
    let scene = token();
    let drv = Driver::new("x").op(Fan::tag("nope"));
    match drv.clip(&scene) {
        Err(scenedrv::Error::EmptyTag(t)) => assert_eq!(t, "nope"),
        other => panic!("{other:?}"),
    }
}

fn table() -> Scene {
    Scene::from_dump(include_str!("../../scenegraph/tests/golden/table.sg1")).unwrap()
}

/// Parent-local origin. A Label on a Slab print: `|x|≤w/2`, `|y|≤h/2`, `z>0`.
fn local_in(scene: &Scene, child: &str, parent: &str) -> Vec3 {
    let inv = mat4_inverse(scene.world_matrix(parent).unwrap()).expect("invert");
    transform_point(
        inv,
        transform_point(scene.world_matrix(child).unwrap(), Vec3::ZERO),
    )
}

fn on_print(scene: &Scene, child: &str, slab: &str) -> String {
    let (w, h) = match scene.get(slab).unwrap().kind {
        Kind::Mesh {
            prim: Prim::Slab { w, h, .. },
            ..
        } => (w, h),
        _ => panic!("{slab} is not a Slab"),
    };
    let p = local_in(scene, child, slab);
    let on = p.x.abs() <= w * 0.5 + 1e-4 && p.y.abs() <= h * 0.5 + 1e-4 && p.z > 0.0;
    format!(
        "{child} on {slab} local={:.4},{:.4},{:.4} face={w}x{h} {}\n",
        p.x,
        p.y,
        p.z,
        if on { "print" } else { "OFF" }
    )
}

#[test]
fn inspect_dump_is_golden() {
    let src = include_str!("golden/inspect.drv1");
    let d = Driver::from_dump(src).unwrap();
    assert_eq!(d.dump(), src);
    assert_eq!(
        Driver::new("inspect").op(To::lift("th-shredder")).dump(),
        src
    );
    let fade = include_str!("golden/inspect-fade.drv1");
    let f = Driver::from_dump(fade).unwrap();
    assert_eq!(f.dump(), fade);
    assert_eq!(
        Driver::new("flavor-shredder")
            .op(To::fade("flavor-shredder"))
            .dump(),
        fade
    );
}

#[test]
fn to_lifts_named_node() {
    let scene = table();
    let rest = scene.dump();
    let clip = Driver::from_dump(include_str!("golden/inspect.drv1"))
        .unwrap()
        .clip(&scene)
        .unwrap();
    assert_eq!(clip.name, "inspect");
    assert!(clip.dump().contains("ease=inout-cubic"), "{}", clip.dump());

    let t0 = clip.pose(&scene, 0.0).unwrap();
    assert!(
        t0.dump().contains(
            "th-shredder Mesh Slab 1.5x2 mat=th-shredder p=hand t=-1.55,0,0 r=0,0,0.2 tag=hand"
        ),
        "{}",
        t0.dump()
    );
    assert!(
        !t0.dump().contains("flavor-shredder"),
        "pose must not insert labels: {}",
        t0.dump()
    );

    let posed = clip.pose(&scene, 1.0).unwrap();
    assert!(
        posed.dump().contains(
            "th-shredder Mesh Slab 1.5x2 mat=th-shredder p=hand t=0,2.32,2.2 s=1.6,1.6,1 tag=hand"
        ),
        "{}",
        posed.dump()
    );
    assert!(
        !posed.dump().contains("r=0,0,0.2"),
        "lift flattens rest twist: {}",
        posed.dump()
    );
    let welder = posed.get("th-welder").unwrap().xf.t;
    let rest_w = scene.get("th-welder").unwrap().xf.t;
    assert!(
        (welder - rest_w).length() < 1e-4,
        "only the named node moves, welder {welder:?}"
    );
    assert_eq!(scene.dump(), rest, "clip must not write rest");
}

#[test]
fn inspect_inserts_flavor_rest_untouched() {
    let scene = table();
    let rest = scene.dump();
    assert!(!rest.contains("flavor-shredder"));

    let shown = inspect(&scene, "th-shredder", "Tear it apart.").unwrap();
    assert_eq!(scene.dump(), rest, "inspect must clone");
    assert!(!rest.contains("flavor-shredder"));
    assert_eq!(flavor_node("th-shredder"), "flavor-shredder");
    assert!(
        shown.dump().contains(
            "th-shredder Mesh Slab 1.5x2 mat=th-shredder p=hand t=-1.55,0,0 r=0,0,0.2 tag=hand"
        ),
        "inspect does not lift: {}",
        shown.dump()
    );
    assert!(
        shown.dump().contains(
            r#"flavor-shredder Label font=ibm size=0.12 "Tear it apart." p=th-shredder t=0,-0.35,0.05"#
        ),
        "{}",
        shown.dump()
    );
    let line = on_print(&shown, "flavor-shredder", "th-shredder");
    assert!(line.contains(" print"), "{line}");

    let clip = Driver::from_dump(include_str!("golden/inspect.drv1"))
        .unwrap()
        .clip(&scene)
        .unwrap();
    let posed = clip.pose(&shown, 1.0).unwrap();
    assert!(
        posed.dump().contains(
            "th-shredder Mesh Slab 1.5x2 mat=th-shredder p=hand t=0,2.32,2.2 s=1.6,1.6,1 tag=hand"
        ),
        "{}",
        posed.dump()
    );
    assert!(
        posed.dump().contains(
            r#"flavor-shredder Label font=ibm size=0.12 "Tear it apart." p=th-shredder t=0,-0.35,0.05"#
        ),
        "{}",
        posed.dump()
    );
    let posed_line = on_print(&posed, "flavor-shredder", "th-shredder");
    assert!(posed_line.contains(" print"), "{posed_line}");
    assert_eq!(
        scene.dump(),
        rest,
        "pose of inspect clone must not write rest"
    );
}

#[test]
fn inspect_inks_flavor_to_read_on_its_card() {
    let src = "sg1
pale Mesh Slab 1.5x2 c=efe6d8
dark Mesh Slab 1.5x2 c=1b3a2c
art Mesh Slab 1.5x2 m=print
cam Camera eye=0,0,6 at=0,0,0
";
    let scene = scenegraph::Scene::from_dump(src).unwrap();
    let ink = |card: &str| {
        let shown = inspect(&scene, card, "x").unwrap();
        let line = shown
            .dump()
            .lines()
            .find(|l| l.starts_with(&format!("flavor-{card} ")))
            .unwrap()
            .to_string();
        line
    };
    assert!(ink("pale").contains("c=1a1210"), "{}", ink("pale"));
    // White is the default and is omitted from the dump.
    assert!(!ink("dark").contains(" c="), "{}", ink("dark"));
    assert!(
        !ink("art").contains(" c="),
        "a textured face keeps the default: {}",
        ink("art")
    );
}

#[test]
fn inspect_unknown_node() {
    match inspect(&table(), "nope", "x") {
        Err(scenedrv::Error::UnknownNode(n)) => assert_eq!(n, "nope"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn to_unknown_node() {
    let drv = Driver::new("x").op(To::lift("nope"));
    match drv.clip(&table()) {
        Err(scenedrv::Error::UnknownNode(n)) => assert_eq!(n, "nope"),
        other => panic!("{other:?}"),
    }
}

fn lift_clip(scene: &Scene, name: &str) -> Clip {
    Driver::new(name).op(To::lift(name)).clip(scene).unwrap()
}

const LIFT_DUMP: &str =
    "th-shredder Mesh Slab 1.5x2 mat=th-shredder p=hand t=0,2.32,2.2 s=1.6,1.6,1 tag=hand";

#[test]
fn to_returns_to_rest() {
    let scene = table();
    let rest = scene.dump();
    let clip = lift_clip(&scene, "th-shredder");
    let up = clip.pose(&scene, 1.0).unwrap();
    assert!(up.dump().contains(LIFT_DUMP), "{}", up.dump());
    let home = clip.pose(&scene, 0.0).unwrap();
    assert!(
        home.dump().contains(
            "th-shredder Mesh Slab 1.5x2 mat=th-shredder p=hand t=-1.55,0,0 r=0,0,0.2 tag=hand"
        ),
        "{}",
        home.dump()
    );
    assert_eq!(scene.dump(), rest);
}

#[test]
fn ease_lags_linear_at_quarter() {
    let scene = table();
    let clip = lift_clip(&scene, "th-shredder");
    let p = clip.pose(&scene, LIFT_OPEN * 0.25).unwrap();
    let t = p.get("th-shredder").unwrap().xf.t;
    let rest = scene.get("th-shredder").unwrap().xf.t;
    let linear_y = rest.y + (LIFT_T.y - rest.y) * 0.25;
    assert!(
        t.y < linear_y * 0.5,
        "inout-cubic at 0.25 should lag linear {linear_y}, got {t:?}"
    );
    assert!(t.y > rest.y, "must have left rest, {t:?}");
}

#[test]
fn pose_layers_switch_two_cards() {
    let scene = table();
    let rest = scene.dump();
    let a = lift_clip(&scene, "th-shredder");
    let b = lift_clip(&scene, "th-welder");
    let both = pose_layers(&scene, &[(&a, 0.5), (&b, 0.5)]).unwrap();
    let sa = both.get("th-shredder").unwrap().xf.t;
    let sb = both.get("th-welder").unwrap().xf.t;
    let ra = scene.get("th-shredder").unwrap().xf.t;
    let rb = scene.get("th-welder").unwrap().xf.t;
    assert!(sa.y > ra.y + 0.8, "shredder lifting {sa:?}");
    assert!(sb.y > rb.y + 0.8, "welder lifting {sb:?}");
    assert!((sa.x - sb.x).abs() < 0.9, "both heading to center");

    let swapped = pose_layers(&scene, &[(&a, 0.0), (&b, 1.0)]).unwrap();
    assert!(
        swapped.get("th-shredder").unwrap().xf.t.is_near(ra, 1e-4),
        "shredder home"
    );
    assert!(
        swapped.get("th-welder").unwrap().xf.t.is_near(LIFT_T, 1e-4),
        "welder in highlight {:?}",
        swapped.get("th-welder").unwrap().xf.t
    );
    assert_eq!(scene.dump(), rest, "layers must not write rest");
}

#[test]
fn mixer_switch_ticks_home_and_in() {
    let scene = table();
    let rest = scene.dump();
    let mut mx = Mixer::new();
    mx.go(lift_clip(&scene, "th-shredder"));
    mx.tick(LIFT_OPEN);
    assert!(
        mx.pose(&scene).unwrap().dump().contains(LIFT_DUMP),
        "{}",
        mx.pose(&scene).unwrap().dump()
    );
    mx.retreat_all();
    mx.go(lift_clip(&scene, "th-welder"));
    assert_eq!(
        mx.dump(),
        "mx1\nth-shredder t=0.7 rate=-1\nth-welder\n",
        "{}",
        mx.dump()
    );
    mx.tick(LIFT_OPEN * 0.5);
    let mid = mx.pose(&scene).unwrap();
    let sa = mid.get("th-shredder").unwrap().xf.t;
    let sb = mid.get("th-welder").unwrap().xf.t;
    assert!(
        sa.y > 0.8 && sa.y < LIFT_T.y - 0.5,
        "shredder returning {sa:?}"
    );
    assert!(
        sb.y > 0.8 && sb.y < LIFT_T.y - 0.5,
        "welder arriving {sb:?}"
    );

    mx.tick(LIFT_OPEN * 0.5);
    assert_eq!(mx.layers.len(), 1, "{}", mx.dump());
    assert_eq!(mx.layers[0].clip.name, "th-welder");
    let done = mx.pose(&scene).unwrap();
    assert!(done
        .get("th-shredder")
        .unwrap()
        .xf
        .t
        .is_near(scene.get("th-shredder").unwrap().xf.t, 1e-4));
    assert!(done.get("th-welder").unwrap().xf.t.is_near(LIFT_T, 1e-4));
    assert_eq!(scene.dump(), rest);
}

fn fade_clip(scene: &Scene, card: &str) -> Clip {
    let flavor = flavor_node(card);
    Driver::new(&flavor)
        .op(To::fade(&flavor))
        .clip(scene)
        .unwrap()
}

fn pose_inspect(scene: &Scene, mx: &Mixer) -> scenegraph::Scene {
    present(scene, mx, |_| "Tear it apart.", None).unwrap()
}

#[test]
fn flavor_waits_for_focus_then_fades() {
    let scene = table();
    let rest = scene.dump();
    let mut mx = Mixer::new();
    mx.go(lift_clip(&scene, "th-shredder"));
    mx.then(fade_clip(&scene, "th-shredder"), "th-shredder");

    mx.tick(LIFT_OPEN * 0.5);
    let mid = pose_inspect(&scene, &mx);
    assert!(
        !mid.dump().contains("flavor-shredder"),
        "no flavor mid-lift: {}",
        mid.dump()
    );
    assert!(mid.get("th-shredder").unwrap().xf.t.y > 0.5);

    mx.tick(LIFT_OPEN * 0.5);
    let parked = pose_inspect(&scene, &mx);
    assert!(parked.dump().contains(LIFT_DUMP), "{}", parked.dump());
    assert!(
        !parked.dump().contains("flavor-shredder"),
        "flavor waits for fade: {}",
        parked.dump()
    );

    mx.tick(FADE_OPEN * 0.5);
    let fading = pose_inspect(&scene, &mx);
    let fade_line = fading
        .dump()
        .lines()
        .find(|l| l.starts_with("flavor-shredder"))
        .unwrap_or("")
        .to_string();
    assert!(fade_line.contains("c="), "alpha still rising: {fade_line}");
    assert!(fading.dump().contains(LIFT_DUMP), "{}", fading.dump());

    mx.tick(FADE_OPEN);
    let focused = pose_inspect(&scene, &mx);
    assert!(
        focused.dump().contains(
            r#"flavor-shredder Label font=ibm size=0.12 "Tear it apart." p=th-shredder t=0,-0.35,0.05"#
        ),
        "{}",
        focused.dump()
    );

    mx.retreat_all();
    mx.tick(FADE_OPEN * 0.5);
    let out = pose_inspect(&scene, &mx);
    assert!(
        out.dump().contains(LIFT_DUMP),
        "card holds dest while text fades: {}",
        out.dump()
    );
    let out_line = out
        .dump()
        .lines()
        .find(|l| l.starts_with("flavor-shredder"))
        .unwrap_or("")
        .to_string();
    assert!(out_line.contains("c="), "text fading out: {out_line}");

    mx.tick(FADE_OPEN);
    let homeward = pose_inspect(&scene, &mx);
    assert!(
        !homeward.dump().contains("flavor-shredder"),
        "text gone before the card returns: {}",
        homeward.dump()
    );
    assert!(
        mx.layers
            .iter()
            .find(|l| l.clip.name == "th-shredder")
            .is_some_and(|l| l.rate < 0.0),
        "card reversing after fade: {}",
        mx.dump()
    );
    assert_eq!(scene.dump(), rest);
}

#[test]
fn hover_under_inspect_overlay() {
    let scene = table();
    let hover =
        Clip::from_dump(include_str!("../../sceneanim/tests/golden/table-hover.an1")).unwrap();
    let lift = lift_clip(&scene, "th-shredder");
    let posed = pose_layers(&scene, &[(&hover, 0.5), (&lift, 1.0)]).unwrap();
    assert!(
        posed.dump().contains(LIFT_DUMP),
        "inspect wins shredder: {}",
        posed.dump()
    );
    assert!(
        posed.dump().contains("t=0,0.38,0.16"),
        "welder still hovers: {}",
        posed.dump()
    );
}
