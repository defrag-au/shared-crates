//! Dump-to-dump picks, orbits, and drags. No raster.

use scenegraph::{Fonts, Geometry, Images, Materials, Scene, Stack, Vec3};
use sceneinput::{Event, Session, Tool};

fn cards() -> (Scene, scenegraph::DrawList) {
    let scene = Scene::from_dump(include_str!("../../scenegraph/tests/golden/cards.sg1")).unwrap();
    let mats =
        Materials::from_dump(include_str!("../../scenegraph/tests/golden/surfaces.mt1")).unwrap();
    let list = scene
        .draw_list_with(&Geometry::new(), &mats, &mut Fonts::new())
        .unwrap();
    (scene, list)
}

fn room() -> (Scene, scenegraph::DrawList) {
    let scene = Scene::from_dump(include_str!("../../scenegraph/tests/golden/room.sg1")).unwrap();
    let mut geom = Geometry::new();
    geom.insert_tet("tet").unwrap();
    let list = scene
        .draw_list_with(&geom, &Materials::new(), &mut Fonts::new())
        .unwrap();
    (scene, list)
}

fn ndc_of(list: &scenegraph::DrawList, p: Vec3, aspect: f32) -> [f32; 2] {
    let m = list.view_proj(aspect);
    let x = m[0][0] * p.x + m[1][0] * p.y + m[2][0] * p.z + m[3][0];
    let y = m[0][1] * p.x + m[1][1] * p.y + m[2][1] * p.z + m[3][1];
    let w = m[0][3] * p.x + m[1][3] * p.y + m[2][3] * p.z + m[3][3];
    [x / w, y / w]
}

#[test]
fn pick_a_is_golden() {
    let (scene, list) = cards();
    let rest = scene.dump();
    let c = list.meshes.iter().find(|m| m.name == "a").unwrap().centre;
    let ndc = ndc_of(&list, c, 1.0);
    let sess = Session::new();
    let (out, sess, wrote) = sess
        .step(&scene, &list, Event::down(ndc[0], ndc[1], 1.0))
        .unwrap();
    assert!(!wrote);
    let sel = sess.sel.as_ref().expect("hit a");
    assert_eq!(sel.name, "a");
    assert_eq!(sel.face, scenegraph::HitFace::Front);
    assert_eq!(out.dump(), rest, "pick must not write rest");
    assert_eq!(
        sess.dump(),
        include_str!("golden/pick-a.in1"),
        "{}",
        sess.dump()
    );
}

/// The crosshair pick a walk uses selects what `Tool::Pick` would, and
/// leaves the walking session alone.
#[test]
fn pick_ndc_is_the_pick_tool_without_a_session() {
    let (scene, list) = cards();
    let c = list.meshes.iter().find(|m| m.name == "a").unwrap().centre;
    let ndc = ndc_of(&list, c, 1.0);
    let (_, sess, _) = Session::new()
        .step(&scene, &list, Event::down(ndc[0], ndc[1], 1.0))
        .unwrap();
    let alone = sceneinput::pick_ndc(&scene, &list, ndc, 1.0, &Images::new());
    assert_eq!(alone, sess.sel);
    assert!(sceneinput::pick_ndc(&scene, &list, [0.99, 0.99], 1.0, &Images::new()).is_none());
}

#[test]
fn pick_miss_clears() {
    let (scene, list) = cards();
    let c = list.meshes.iter().find(|m| m.name == "a").unwrap().centre;
    let ndc = ndc_of(&list, c, 1.0);
    let sess = Session::new();
    let (_, sess, _) = sess
        .step(&scene, &list, Event::down(ndc[0], ndc[1], 1.0))
        .unwrap();
    assert!(sess.sel.is_some());
    let (_, sess, _) = sess
        .step(&scene, &list, Event::down(0.99, 0.99, 1.0))
        .unwrap();
    assert!(sess.sel.is_none());
}

#[test]
fn orbit_moves_camera_not_crate() {
    let (scene, list) = room();
    let crate_line = "crate Mesh Box c=e07050 p=room t=0,0.4,0 s=0.6,0.8,0.6";
    assert!(scene.dump().contains(crate_line));
    let sess = Session::new().with_tool(Tool::Orbit);
    let (s1, sess, wrote) = sess
        .step(&scene, &list, Event::down(0.0, 0.0, 1.0))
        .unwrap();
    assert!(!wrote);
    assert_eq!(s1.dump(), scene.dump());
    let (s2, _, wrote) = sess.step(&s1, &list, Event::motion(0.5, 0.0, 1.0)).unwrap();
    assert!(wrote);
    let dump = s2.dump();
    assert!(dump.contains(crate_line), "{dump}");
    assert!(
        !dump.contains("cam Camera eye=3.2,2.4,4.2 at=0,0.6,0"),
        "{dump}"
    );
}

#[test]
fn drag_moves_card_a() {
    let (scene, list) = cards();
    let start = scene.get("a").unwrap().xf.t;
    let c = list.meshes.iter().find(|m| m.name == "a").unwrap().centre;
    let ndc = ndc_of(&list, c, 1.0);
    let sess = Session::new().with_tool(Tool::Drag);
    let (s1, sess, wrote) = sess
        .step(&scene, &list, Event::down(ndc[0], ndc[1], 1.0))
        .unwrap();
    assert!(!wrote);
    assert_eq!(sess.sel.as_ref().unwrap().name, "a");
    let (s2, _, wrote) = sess
        .step(&s1, &list, Event::motion(ndc[0] + 0.2, ndc[1], 1.0))
        .unwrap();
    assert!(wrote);
    let end = s2.get("a").unwrap().xf.t;
    assert!(
        (end.x - start.x).abs() > 0.05,
        "expected drag to move x, start={start:?} end={end:?}"
    );
}

fn gallery() -> (Scene, scenegraph::DrawList) {
    let scene =
        Scene::from_dump(include_str!("../../scenegraph/tests/golden/gallery.sg1")).unwrap();
    let mats =
        Materials::from_dump(include_str!("../../scenegraph/tests/golden/gallery.mt1")).unwrap();
    let mut fonts = Fonts::new();
    fonts
        .insert("ibm", scenegraph::NOTO_SANS_BOLD, Vec::new())
        .unwrap();
    let list = scene
        .draw_list_with(&Geometry::new(), &mats, &mut fonts)
        .unwrap();
    (scene, list)
}

fn cam_eye_at(scene: &Scene) -> (Vec3, Vec3) {
    match &scene.get("cam").unwrap().kind {
        scenegraph::Kind::Camera { eye, at, .. } => (*eye, *at),
        other => panic!("{other:?}"),
    }
}

#[test]
fn walk_look_turns_at_not_eye() {
    let (scene, list) = gallery();
    let (eye0, at0) = cam_eye_at(&scene);
    let sess = Session::new().with_tool(Tool::Walk);
    let (s1, sess, wrote) = sess
        .step(&scene, &list, Event::down(0.0, 0.0, 1.0))
        .unwrap();
    assert!(!wrote);
    assert_eq!(s1.dump(), scene.dump());
    let (s2, _, wrote) = sess.step(&s1, &list, Event::motion(0.4, 0.0, 1.0)).unwrap();
    assert!(wrote);
    let (eye1, at1) = cam_eye_at(&s2);
    assert!(
        (eye1 - eye0).length() < 1e-4,
        "walk look keeps eye {eye1:?}"
    );
    assert!(
        at1.x > at0.x + 0.2,
        "ndc x+ looks right (toward +X), {at0:?} → {at1:?}"
    );
}

#[test]
fn walk_strafe_is_right() {
    let (scene, list) = gallery();
    let (eye0, _) = cam_eye_at(&scene);
    let out = sceneinput::stride(&scene, &list, 0.0, 1.0);
    let (eye1, _) = cam_eye_at(&out);
    assert!(
        eye1.x > eye0.x + 0.3,
        "strafe+ is +X when looking −Z, {eye0:?} → {eye1:?}"
    );
}

#[test]
fn walk_wheel_steps_forward() {
    let (scene, list) = gallery();
    let (eye0, _) = cam_eye_at(&scene);
    let sess = Session::new().with_tool(Tool::Walk);
    let (out, _, wrote) = sess
        .step(&scene, &list, Event::wheel(0.0, 0.0, 1.0, -0.2))
        .unwrap();
    assert!(wrote);
    let (eye1, _) = cam_eye_at(&out);
    assert!((eye1.y - eye0.y).abs() < 1e-4, "keep height");
    assert!(
        eye1.z < eye0.z - 0.5,
        "scroll-up walks toward north hang, {eye0:?} → {eye1:?}"
    );
}

#[test]
fn walk_stops_at_wall() {
    let (scene, list) = gallery();
    let (eye0, _) = cam_eye_at(&scene);
    let out = sceneinput::stride(&scene, &list, 20.0, 0.0);
    let (eye1, _) = cam_eye_at(&out);
    assert!((eye1.y - eye0.y).abs() < 1e-4);
    assert!(
        eye1.z > -3.8,
        "must not pass the north hang/wall, eye.z={}",
        eye1.z
    );
    assert!(
        eye1.z < eye0.z - 0.5,
        "must still walk some way, {eye0:?} → {eye1:?}"
    );
}

fn combat() -> (Scene, scenegraph::DrawList) {
    let scene = Scene::from_dump(include_str!("../../scenegraph/tests/golden/table.sg1")).unwrap();
    let mats =
        Materials::from_dump(include_str!("../../scenegraph/tests/golden/table.mt1")).unwrap();
    let mut fonts = Fonts::new();
    fonts
        .insert("ibm", scenegraph::NOTO_SANS_BOLD, Vec::new())
        .unwrap();
    let list = scene
        .draw_list_with(&Geometry::new(), &mats, &mut fonts)
        .unwrap();
    (scene, list)
}

#[test]
fn pick_label_selects_parent() {
    let (scene, list) = combat();
    let aspect = 16.0 / 9.0;
    let lab = list
        .labels
        .iter()
        .find(|l| l.name == "end-turn-n")
        .expect("end-turn-n");
    let ndc = ndc_of(&list, lab.centre, aspect);
    let (_, sess, wrote) = Session::new()
        .step(&scene, &list, Event::down(ndc[0], ndc[1], aspect))
        .unwrap();
    assert!(!wrote);
    let sel = sess.sel.as_ref().expect("hit");
    assert_eq!(sel.name, "end-turn", "{}", sess.dump());

    let title = list
        .labels
        .iter()
        .find(|l| l.name == "name-shredder")
        .expect("name-shredder");
    let ndc = ndc_of(&list, title.centre, aspect);
    let (_, sess, _) = Session::new()
        .step(&scene, &list, Event::down(ndc[0], ndc[1], aspect))
        .unwrap();
    assert_eq!(
        sess.sel.as_ref().unwrap().name,
        "th-shredder",
        "{}",
        sess.dump()
    );
}

#[test]
fn pick_a_dump_file_roundtrips() {
    let src = include_str!("golden/pick-a.in1");
    let s = Session::from_dump(src).unwrap();
    assert_eq!(s.dump(), src);
    assert_eq!(s.tool, Tool::Pick);
    assert_eq!(s.sel.as_ref().unwrap().name, "a");
}

#[test]
fn pick_punches_through_transparent_hat() {
    let mut px = vec![0u8; 4 * 4 * 4];
    for y in 1..3 {
        for x in 1..3 {
            let i = ((y * 4 + x) * 4) as usize;
            px[i] = 200;
            px[i + 3] = 255;
        }
    }
    let mut images = Images::new();
    images.insert("hat", 4, 4, px).unwrap();
    let mut skin = vec![0u8; 64];
    for p in skin.chunks_mut(4) {
        p[0] = 80;
        p[3] = 255;
    }
    images.insert("skin", 4, 4, skin).unwrap();
    let mut stack = Stack::new("token", 2.0, 2.0);
    stack.push("skin", "skin", 0);
    stack.push("hat", "hat", 1);
    let (scene, mats) = stack.scene().unwrap();
    let list = scene
        .draw_list_with(&Geometry::new(), &mats, &mut Fonts::new())
        .unwrap();
    let corner = Vec3::new(0.9, 0.9, 0.04);
    let ndc = ndc_of(&list, corner, 1.0);
    let (_, sess, _) = Session::new()
        .step_with(&scene, &list, Event::down(ndc[0], ndc[1], 1.0), &images)
        .unwrap();
    assert_eq!(
        sess.sel.as_ref().map(|s| s.name.as_str()),
        Some("skin"),
        "{}",
        sess.dump()
    );
    let centre = list.quads.iter().find(|q| q.name == "hat").unwrap().centre;
    let ndc = ndc_of(&list, centre, 1.0);
    let (_, sess, _) = Session::new()
        .step_with(&scene, &list, Event::down(ndc[0], ndc[1], 1.0), &images)
        .unwrap();
    assert_eq!(
        sess.sel.as_ref().map(|s| s.name.as_str()),
        Some("hat"),
        "{}",
        sess.dump()
    );
}
