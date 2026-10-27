//! Hilbert fill → `sw1`. No raster.

use scenegraph::Scene;
use sceneworld::{hilbert_slot, Print, Wall, World};

const HILBERT_4: &str = include_str!("golden/hilbert-4.sw1");

fn prints(n: usize) -> Vec<Print> {
    (0..n)
        .map(|i| {
            let ch = (b'A' + i as u8) as char;
            Print::new(format!("m:{}", ch.to_ascii_lowercase()), ch.to_string())
        })
        .collect()
}

#[test]
fn fill_four_is_one_cell() {
    let mut w = World::new("palace");
    let order = w.fill_hilbert(prints(4)).unwrap();
    assert_eq!(order, 0);
    assert_eq!(w.dump(), HILBERT_4);
    World::from_dump(&w.dump()).unwrap();
}

#[test]
fn rust_hook_same_as_dump() {
    let mut w = World::new("palace");
    w.fill_hilbert([
        Print::new("m:a", "A"),
        Print::new("m:b", "B"),
        Print::new("m:c", "C"),
        Print::new("m:d", "D"),
    ])
    .unwrap();
    assert_eq!(w.dump(), HILBERT_4);
}

#[test]
fn fifth_hang_steps_south() {
    let mut w = World::new("palace");
    let order = w.fill_hilbert(prints(5)).unwrap();
    assert_eq!(order, 1);
    assert_eq!(hilbert_slot(1, 4).unwrap(), ((0, 1), Wall::N));
    let last = w.hangs().last().unwrap();
    assert_eq!(last.c, (0, 1));
    assert_eq!(last.wall, Wall::N);
    assert_eq!(last.name, "p-e");
    assert!(w
        .dump()
        .contains("hang c=0,1 wall=n name=p-e m=m:e title=E"));
}

#[test]
fn repeated_titles_uniquify() {
    let mut w = World::new("palace");
    w.fill_hilbert([
        Print::new("m:1", "Toolhead0220"),
        Print::new("m:2", "Toolhead0220"),
    ])
    .unwrap();
    let names: Vec<_> = w.hangs().iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, ["p-toolhead0220", "p-toolhead0220-2"]);
}

#[test]
fn fixed_order_rejects_overflow() {
    let mut w = World::new("palace");
    match w.fill_hilbert_order(0, prints(5)) {
        Err(sceneworld::Error::HilbertFull { order, i }) => {
            assert_eq!(order, 0);
            assert_eq!(i, 4);
        }
        other => panic!("{other:?}"),
    }
    assert!(w.hangs().is_empty(), "must not partial-fill");
}

#[test]
fn emit_window_keeps_the_slice() {
    let mut w = World::new("palace");
    w.fill_hilbert(prints(16)).unwrap();
    w.span = 1;
    let scene = w.emit().unwrap();
    let hangs: Vec<_> = scene.tagged("hang").map(|n| n.name.as_str()).collect();
    // order 1 is a 2×2: (0,0),(0,1),(1,1),(1,0). Span=1 around origin
    // includes those four plus empty neighbours — 16 hangs, all in window.
    assert_eq!(hangs.len(), 16, "{hangs:?}");
    assert!(scene.get("p-a").is_some());
    assert!(scene.get("c0_1").is_some() || scene.get("p-e").is_some());
    Scene::from_dump(&scene.dump()).unwrap();
}
