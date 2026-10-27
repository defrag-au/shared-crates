//! Flatten/present census. Timing is printed; counts are the score.

use std::time::Instant;

use scenegraph::{Fonts, Geometry, Materials};
use sceneworld::World;

const PALACE: &str = include_str!("golden/palace.sw1");

#[test]
fn palace_flatten_census() {
    let w = World::from_dump(PALACE).unwrap();
    let t0 = Instant::now();
    let scene = w.emit().unwrap();
    let emit_us = t0.elapsed().as_micros();

    let mut fonts = Fonts::new();
    fonts
        .insert("ibm", scenegraph::NOTO_SANS_BOLD, Vec::new())
        .unwrap();
    let geom = Geometry::new();
    let mats = Materials::new();
    let t1 = Instant::now();
    let list = scene.draw_list_with(&geom, &mats, &mut fonts).unwrap();
    let flatten_us = t1.elapsed().as_micros();
    let t2 = Instant::now();
    let _ = scene.draw_list_with(&geom, &mats, &mut fonts).unwrap();
    let warm_us = t2.elapsed().as_micros();
    let census = list.census();
    eprintln!(
        "palace emit={emit_us}us flatten_cold={flatten_us}us flatten_warm={warm_us}us nodes={} {census}",
        scene.len()
    );

    assert!(
        scene.len() < 800,
        "curve window must stay small, nodes={}",
        scene.len()
    );
    assert!(census.contains("quads="), "{census}");
    let quads = list.quads.len();
    assert!(quads < 400, "too many quads for a stretch: {quads}");
    let meshes = list.meshes.len();
    assert!(meshes < 200, "too many meshes for a stretch: {meshes}");
}
