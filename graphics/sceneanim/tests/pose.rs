//! Posed sg1 snapshots — the score, not the raster.

use sceneanim::Clip;
use scenegraph::Scene;

fn room() -> Scene {
    Scene::from_dump(include_str!("../../scenegraph/tests/golden/room.sg1")).unwrap()
}

fn cards() -> Scene {
    Scene::from_dump(include_str!("../../scenegraph/tests/golden/cards.sg1")).unwrap()
}

fn table() -> Scene {
    Scene::from_dump(include_str!("../../scenegraph/tests/golden/table.sg1")).unwrap()
}

#[test]
fn bob_dump_is_golden() {
    let src = include_str!("golden/bob.an1");
    let clip = Clip::from_dump(src).unwrap();
    assert_eq!(clip.dump(), src);
}

#[test]
fn hover_dump_is_golden() {
    let src = include_str!("golden/hover.an1");
    let clip = Clip::from_dump(src).unwrap();
    assert_eq!(clip.dump(), src);
}

#[test]
fn bob_poses_crate_height() {
    let clip = Clip::from_dump(include_str!("golden/bob.an1")).unwrap();
    let rest = room();
    let a = clip.pose(&rest, 0.0).unwrap();
    assert!(a
        .dump()
        .contains("crate Mesh Box c=e07050 p=room t=0,0.4,0 s=0.6,0.8,0.6"));
    let b = clip.pose(&rest, 0.5).unwrap();
    assert!(
        b.dump()
            .contains("crate Mesh Box c=e07050 p=room t=0,0.7,0 s=0.6,0.8,0.6"),
        "{}",
        b.dump()
    );
    assert!(rest.dump().contains("t=0,0.4,0"));
}

#[test]
fn hover_poses_hand_slabs() {
    let clip = Clip::from_dump(include_str!("golden/hover.an1")).unwrap();
    let rest = cards();
    let p = clip.pose(&rest, 0.5).unwrap();
    assert!(
        p.dump()
            .contains("a Mesh Slab 2.2x3.1 mat=print-a p=stage t=-1.3,1.75,-0.2 tag=hand"),
        "{}",
        p.dump()
    );
    assert!(p.dump().contains("t=1.3,1.35,-0.2"), "{}", p.dump());
}

#[test]
fn table_hover_dump_is_golden() {
    let src = include_str!("golden/table-hover.an1");
    let clip = Clip::from_dump(src).unwrap();
    assert_eq!(clip.dump(), src);
}

#[test]
fn table_hover_poses_hand() {
    let clip = Clip::from_dump(include_str!("golden/table-hover.an1")).unwrap();
    let rest = table();
    let p = clip.pose(&rest, 0.5).unwrap();
    assert!(
        p.dump().contains(
            "th-shredder Mesh Slab 1.5x2 mat=th-shredder p=hand t=-1.55,0.28,0.1 r=0,0,0.2 tag=hand"
        ),
        "{}",
        p.dump()
    );
    assert!(p.dump().contains("t=0,0.38,0.16"), "{}", p.dump());
    assert!(p.dump().contains("t=1.55,0.28,0.1"), "{}", p.dump());
}

#[test]
fn bob_loop_matches_next_cycle() {
    let clip = Clip::from_dump(include_str!("golden/bob.an1")).unwrap();
    let rest = room();
    let a = clip.pose(&rest, 0.5).unwrap();
    let b = clip.pose(&rest, 1.5).unwrap();
    assert_eq!(a.dump(), b.dump());
}
