//! Named scene graph. The document is the frame.
//!
//! Pure: dump, query, flatten to [`DrawList`], pick, judge, sketch. The
//! renderers are edge crates over the same `DrawList` — `scenegraph-mq`
//! (macroquad) and `scenegraph-wgpu` (wgpu, egui) — which report what they
//! submit as a [`SubmitCensus`], so the two can be held to the same frame.
//! `gltf` adds the glTF importer (models, rigs, clips).

mod census;
mod draw;
mod dump;
mod error;
mod font;
mod geom;
mod images;
mod kind;
mod material;
mod math;
mod mesh;
mod node;
pub mod problems;
pub mod rig;
mod scene;
pub mod sketch;
mod stack;

pub use draw::{
    CameraParams, DrawList, FlattenCost, Hit, HitFace, LightParams, WorldGlyph, WorldLabel,
    WorldLamp, WorldMesh, WorldQuad, ALPHA_CUTOFF,
};
pub use dump::VERSION;
pub use error::Error;
pub use font::{Fonts, Layout, LayoutGlyph};
pub use geom::{Geometry, MeshBlob, ModelPart};
pub use images::{Images, RgbaImage};
pub use kind::{
    Front, Kind, KindOf, Lamp, LampOf, Pose, Prim, DEFAULT_FOV, DEFAULT_RANGE, DEFAULT_SLAB_DEPTH,
    DEFAULT_SLAB_RADIUS, MAX_DIR_LIGHTS, MAX_POINT_LIGHTS,
};
pub use material::{Materials, Shade, Surface, SurfaceBind, VERSION as MAT_VERSION};
pub use math::{mat4_inverse, transform_point, unproject, Mat4, Vec3};
pub use mesh::{LocalVert, Tessellation};
pub use node::{Node, Transform};
pub use problems::Problem;
pub use scene::Scene;
pub use stack::{dump_id, Stack, StackLayer};

pub use census::SubmitCensus;
pub use font::NOTO_SANS_BOLD;
#[cfg(feature = "gltf")]
mod gltf_import;
#[cfg(feature = "gltf")]
pub use gltf_import::{
    model_from_gltf, scene_from_gltf, scene_from_gltf_path, GltfImport, ImportedModel,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::transform_point;

    fn table() -> Scene {
        let mut s = Scene::new();
        s.insert(Node::group("table")).unwrap();
        s.insert(
            Node::group("hand")
                .parent("table")
                .at(Vec3::new(0.0, 1.2, 0.0)),
        )
        .unwrap();
        s.insert(
            Node::quad("panel", 2.2, 3.1)
                .parent("hand")
                .tag("hand")
                .image("front")
                .at(Vec3::new(0.2, 0.0, 0.0)),
        )
        .unwrap();
        s.insert(Node::camera(
            "cam",
            Vec3::new(0.0, 1.6, 5.0),
            Vec3::new(0.0, 0.8, 0.0),
        ))
        .unwrap();
        s.insert(Node::light("sun", Vec3::new(-0.3, -1.0, -0.4)))
            .unwrap();
        s
    }

    #[test]
    fn dump_roundtrip() {
        let a = table();
        let dump = a.dump();
        let b = Scene::from_dump(&dump).unwrap();
        assert_eq!(a.dump(), b.dump());
        assert!(dump.starts_with("sg1\n"));
        assert!(dump.contains("panel Quad 2.2x3.1 m=front p=hand t=0.2,0,0 tag=hand"));
        assert!(!dump.contains("fov="), "default fov omitted: {dump}");
        assert!(!dump.contains("s=1,1,1"));
    }

    #[test]
    fn unknown_kind_is_error() {
        let src = "sg1\nfoo Spline\n";
        match Scene::from_dump(src) {
            Err(Error::UnknownKind(k)) => assert_eq!(k, "Spline"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn unknown_key_is_error() {
        let src = "sg1\nfoo Group wobble=1\n";
        match Scene::from_dump(src) {
            Err(Error::UnknownKey(k)) => assert_eq!(k, "wobble"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn duplicate_and_missing_parent() {
        let mut s = Scene::new();
        s.insert(Node::group("a")).unwrap();
        assert_eq!(
            s.insert(Node::group("a")),
            Err(Error::DuplicateName("a".into()))
        );
        assert_eq!(
            s.insert(Node::group("b").parent("nope")),
            Err(Error::UnknownParent("nope".into()))
        );
    }

    #[test]
    fn cycle_rejected() {
        let mut s = Scene::new();
        s.insert(Node::group("a")).unwrap();
        s.insert(Node::group("b").parent("a")).unwrap();
        assert_eq!(s.set_parent("a", Some("b")), Err(Error::Cycle("a".into())));
    }

    #[test]
    fn remove_reparents() {
        let mut s = Scene::new();
        s.insert(Node::group("root")).unwrap();
        s.insert(Node::group("mid").parent("root")).unwrap();
        s.insert(Node::group("leaf").parent("mid")).unwrap();
        s.remove("mid");
        assert_eq!(s.get("leaf").unwrap().parent.as_deref(), Some("root"));
    }

    #[test]
    fn tagged_and_kind_queries() {
        let s = table();
        let tags: Vec<_> = s.tagged("hand").map(|n| n.name.as_str()).collect();
        assert_eq!(tags, ["panel"]);
        assert_eq!(s.kind(KindOf::Quad).count(), 1);
        assert_eq!(s.kind(KindOf::Camera).count(), 1);
    }

    #[test]
    fn world_compose_and_radians() {
        let mut s = Scene::new();
        s.insert(Node::group("p").at(Vec3::new(1.0, 0.0, 0.0)))
            .unwrap();
        s.insert(Node::group("c").parent("p").at(Vec3::new(0.0, 2.0, 0.0)))
            .unwrap();
        let m = s.world_matrix("c").unwrap();
        let w = transform_point(m, Vec3::ZERO);
        assert!(w.is_near(Vec3::new(1.0, 2.0, 0.0), 1e-5), "{w:?}");
        let batched = s.world_matrices().unwrap();
        let bw = transform_point(batched["c"], Vec3::ZERO);
        assert!(bw.is_near(w, 1e-6), "{bw:?} vs {w:?}");

        let mut s = Scene::new();
        s.insert(Node::group("rx").rot(Vec3::new(std::f32::consts::FRAC_PI_2, 0.0, 0.0)))
            .unwrap();
        let m = s.world_matrix("rx").unwrap();
        let w = transform_point(m, Vec3::new(0.0, 1.0, 0.0));
        assert!(w.is_near(Vec3::new(0.0, 0.0, 1.0), 1e-4), "{w:?}");
    }

    #[test]
    fn draw_list_is_named() {
        let s = table();
        let d = s.draw_list().unwrap();
        assert_eq!(d.camera.name, "cam");
        assert_eq!(d.quads[0].name, "panel");
        assert_eq!(d.quads[0].image.as_deref(), Some("front"));
        assert!(d.cost.dump().starts_with("cost\n"), "{}", d.cost.dump());
        let dd = d.dump();
        assert!(dd.starts_with("draw\n"));
        assert!(dd.contains("quad panel 2.2x3.1"));
        assert!(dd.contains("m=front"));
        let c = d.census();
        assert!(c.starts_with("census\n"), "{c}");
        assert!(c.contains("quads=1"), "{c}");
        assert!(c.contains("lights=1"), "{c}");
    }

    #[test]
    fn retarget_camera_keeps_prims() {
        let mut s = table();
        let mut d = s.draw_list().unwrap();
        let n_quads = d.quads.len();
        let Kind::Camera { eye, .. } = &mut s.get_mut("cam").unwrap().kind else {
            panic!("cam");
        };
        *eye = Vec3::new(1.0, 2.0, 9.0);
        d.set_camera_from(&s).unwrap();
        assert!(
            d.camera.eye.is_near(Vec3::new(1.0, 2.0, 9.0), 1e-5),
            "{:?}",
            d.camera.eye
        );
        assert_eq!(d.quads.len(), n_quads);
    }

    #[test]
    fn put_camera_inserts_and_overwrites() {
        let mut s = Scene::new();
        let cam = CameraParams::look("cam", Vec3::new(1.0, 2.0, 3.0), Vec3::ZERO);
        s.put_camera(&cam).unwrap();
        let got = s.camera_params().unwrap();
        assert!(got.eye.is_near(cam.eye, 1e-5), "{:?}", got.eye);
        let cam2 = CameraParams::look("cam", Vec3::new(4.0, 5.0, 6.0), Vec3::new(0.0, 1.0, 0.0));
        s.put_camera(&cam2).unwrap();
        let got = s.camera_params().unwrap();
        assert!(got.eye.is_near(cam2.eye, 1e-5), "{:?}", got.eye);
        assert!(got.at.is_near(cam2.at, 1e-5), "{:?}", got.at);
    }

    #[test]
    fn mesh_parse_errors() {
        match Scene::from_dump("sg1\nc Mesh\n") {
            Err(Error::MeshNeedsPrim) => {}
            other => panic!("{other:?}"),
        }
        match Scene::from_dump("sg1\nc Mesh Torus\n") {
            Err(Error::UnknownPrim(p)) => assert_eq!(p, "Torus"),
            other => panic!("{other:?}"),
        }
        match Scene::from_dump("sg1\nc Mesh 1x1 Box\n") {
            Err(Error::KindKey(KindOf::Mesh, "WxH")) => {}
            other => panic!("{other:?}"),
        }
        match Scene::from_dump("sg1\nc Quad Box 1x1\n") {
            Err(Error::KindKey(KindOf::Quad, "Box|Plane|Geom|Slab|Sphere")) => {}
            other => panic!("{other:?}"),
        }
        match Scene::from_dump("sg1\nc Mesh Slab\n") {
            Err(Error::SlabNeedsSize) => {}
            other => panic!("{other:?}"),
        }
        match Scene::from_dump("sg1\nc Mesh Geom\n") {
            Err(Error::MeshNeedsGeom) => {}
            other => panic!("{other:?}"),
        }
        match Scene::from_dump("sg1\nc Mesh Box g=tet\n") {
            Err(Error::KindKey(KindOf::Mesh, "g")) => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn mesh_builder_dumps() {
        let mut s = Scene::new();
        s.insert(
            Node::mesh("crate", Prim::Box)
                .color([0xe0, 0x70, 0x50, 255])
                .at(Vec3::new(0.0, 0.4, 0.0))
                .scale(Vec3::new(0.6, 0.8, 0.6)),
        )
        .unwrap();
        s.insert(Node::camera(
            "cam",
            Vec3::new(3.2, 2.4, 4.2),
            Vec3::new(0.0, 0.6, 0.0),
        ))
        .unwrap();
        let dump = s.dump();
        assert!(dump.contains("crate Mesh Box c=e07050 t=0,0.4,0 s=0.6,0.8,0.6"));
        let d = s.draw_list().unwrap();
        assert_eq!(d.meshes.len(), 1);
        assert_eq!(d.meshes[0].prim, Prim::Box);
        assert_eq!(d.meshes[0].verts.len(), 24);
        assert_eq!(d.meshes[0].indices.len(), 36);
    }

    #[test]
    fn slab_dumps_as_stock() {
        let mut s = Scene::new();
        s.insert(
            Node::slab("a", 2.2, 3.1)
                .image("front-a")
                .at(Vec3::new(-1.3, 1.55, -0.2))
                .tag("hand"),
        )
        .unwrap();
        s.insert(Node::camera(
            "cam",
            Vec3::new(0.0, 2.2, 6.0),
            Vec3::new(0.0, 1.0, 0.0),
        ))
        .unwrap();
        let dump = s.dump();
        assert!(
            dump.contains("a Mesh Slab 2.2x3.1 m=front-a t=-1.3,1.55,-0.2 tag=hand"),
            "{dump}"
        );
        assert!(!dump.contains("d="), "{dump}");
        assert!(!dump.contains("rad="), "{dump}");
        let d = s.draw_list().unwrap();
        let hit = d
            .hit(Vec3::new(-1.3, 1.55, 4.0), Vec3::new(0.0, 0.0, -1.0))
            .unwrap();
        assert_eq!(hit.name, "a");
        assert_eq!(hit.face, HitFace::Front);
        let back = d
            .hit(Vec3::new(-1.3, 1.55, -4.0), Vec3::new(0.0, 0.0, 1.0))
            .unwrap();
        assert_eq!(back.face, HitFace::Back);
        let edge = d
            .hit(Vec3::new(2.0, 1.55, -0.2), Vec3::new(-1.0, 0.0, 0.0))
            .unwrap();
        assert_eq!(edge.name, "a");
        assert_eq!(edge.face, HitFace::Edge);
    }

    #[test]
    fn asset_id_image_key_roundtrips() {
        // Pane convention: intern key is policy:hex. Colon is legal in m=.
        let src = "sg1\ncard Mesh Slab 1.5x1.5 rad=0 sh=Unlit m=285c0b8e91ba323da4ca083c9db837e111dafbf3143ece4d03eba8f4:546f6f6c6865616430323230\ncam Camera eye=0,1.6,2 at=0,1.5,0\n";
        let s = Scene::from_dump(src).unwrap();
        let dump = s.dump();
        assert!(
            dump.contains("m=285c0b8e91ba323da4ca083c9db837e111dafbf3143ece4d03eba8f4:546f6f6c6865616430323230"),
            "{dump}"
        );
        assert_eq!(Scene::from_dump(&dump).unwrap().dump(), dump);
        let bind = s.get("card").unwrap().kind.surface_bind().unwrap();
        match bind {
            SurfaceBind::Inline(surf) => {
                assert_eq!(
                    surf.image.as_deref(),
                    Some("285c0b8e91ba323da4ca083c9db837e111dafbf3143ece4d03eba8f4:546f6f6c6865616430323230")
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn slab_back_print_dumps() {
        let mut s = Scene::new();
        s.insert(
            Node::slab("a", 2.2, 3.1)
                .image("front-a")
                .image_back("back-a"),
        )
        .unwrap();
        s.insert(Node::camera("cam", Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO))
            .unwrap();
        let dump = s.dump();
        assert!(dump.contains("mb=back-a"), "{dump}");
        let d = s.draw_list().unwrap();
        assert_eq!(d.meshes[0].image_back.as_deref(), Some("back-a"));
        assert!(d.meshes[0].back_index.is_some());
        assert!(d.dump().contains("mb=back-a"));
    }

    #[test]
    fn flatten_normals_are_inverse_transpose() {
        let mut s = Scene::new();
        s.insert(Node::group("p").scale(Vec3::new(2.0, 1.0, 1.0)))
            .unwrap();
        s.insert(Node::mesh("q", Prim::Plane).parent("p").rot(Vec3::new(
            0.0,
            std::f32::consts::FRAC_PI_4,
            0.0,
        )))
        .unwrap();
        s.insert(Node::camera("cam", Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO))
            .unwrap();
        let n = s.draw_list().unwrap().meshes[0].verts[0].1;
        let m = s.world_matrix("q").unwrap();
        let expect = crate::math::NormalXf::of(m).apply(Vec3::new(0.0, 0.0, 1.0));
        assert!(n.is_near(expect, 1e-3), "n={n:?} expect={expect:?}");
    }

    #[test]
    fn geom_lookup_at_flatten() {
        let mut s = Scene::new();
        s.insert(Node::geom("tet", "tet").at(Vec3::new(-1.2, 0.35, 0.5)))
            .unwrap();
        s.insert(Node::camera("cam", Vec3::new(3.0, 2.0, 4.0), Vec3::ZERO))
            .unwrap();
        assert!(s.dump().contains("tet Mesh Geom g=tet t=-1.2,0.35,0.5"));
        match s.draw_list() {
            Err(Error::UnknownGeom(k)) => assert_eq!(k, "tet"),
            other => panic!("{other:?}"),
        }
        let mut g = Geometry::new();
        g.insert_tet("tet").unwrap();
        let d = s
            .draw_list_with(&g, &Materials::new(), &mut Fonts::new())
            .unwrap();
        assert_eq!(d.meshes[0].prim, Prim::Geom);
        assert_eq!(d.meshes[0].geom.as_deref(), Some("tet"));
        assert_eq!(d.meshes[0].verts.len(), 12);
        assert!(d.dump().contains("mesh tet Geom"));
        assert!(d.dump().contains("g=tet"));
    }

    #[test]
    fn hit_picks_named_quad_and_box() {
        let mut s = Scene::new();
        s.insert(Node::quad("panel", 2.0, 2.0)).unwrap();
        s.insert(Node::camera("cam", Vec3::new(0.0, 0.0, 4.0), Vec3::ZERO))
            .unwrap();
        let d = s.draw_list().unwrap();
        let hit = d
            .hit(Vec3::new(0.0, 0.0, 4.0), Vec3::new(0.0, 0.0, -1.0))
            .unwrap();
        assert_eq!(hit.name, "panel");
        assert!((hit.t - 4.0).abs() < 1e-3, "{}", hit.t);
        assert_eq!(hit.face, HitFace::Front);
        assert!(hit.point.is_near(Vec3::ZERO, 1e-3), "{:?}", hit.point);
        assert!(
            (hit.uv[0] - 0.5).abs() < 0.05 && (hit.uv[1] - 0.5).abs() < 0.05,
            "{:?}",
            hit.uv
        );
        let back = d
            .hit(Vec3::new(0.0, 0.0, -4.0), Vec3::new(0.0, 0.0, 1.0))
            .unwrap();
        assert_eq!(back.name, "panel");
        assert_eq!(back.face, HitFace::Back);
        assert!(d
            .hit(Vec3::new(3.0, 0.0, 4.0), Vec3::new(0.0, 0.0, -1.0))
            .is_none());
        let ndc = d.hit_ndc(0.0, 0.0, 1.0).unwrap();
        assert_eq!(ndc.name, "panel");
        assert_eq!(ndc.face, HitFace::Front);

        let mut s = Scene::new();
        s.insert(Node::mesh("crate", Prim::Box)).unwrap();
        s.insert(Node::camera("cam", Vec3::new(0.0, 0.0, 3.0), Vec3::ZERO))
            .unwrap();
        let d = s.draw_list().unwrap();
        let hit = d
            .hit(Vec3::new(0.0, 0.0, 3.0), Vec3::new(0.0, 0.0, -1.0))
            .unwrap();
        assert_eq!(hit.name, "crate");
        assert!((hit.t - 2.5).abs() < 1e-3, "{}", hit.t);
        assert_eq!(hit.face, HitFace::Front);
    }

    #[test]
    fn dump_parse_any_order() {
        let src = "sg1\nchild Group p=root\nroot Group\ncam Camera eye=0,1,3 at=0,0,0\n";
        let s = Scene::from_dump(src).unwrap();
        assert_eq!(s.get("child").unwrap().parent.as_deref(), Some("root"));
        assert_eq!(s.dump().matches("root Group").count(), 1);
    }

    #[test]
    fn light_parse_errors() {
        match Scene::from_dump("sg1\nsun Light\n") {
            Err(Error::LightNeedsLamp) => {}
            other => panic!("{other:?}"),
        }
        match Scene::from_dump("sg1\nsun Light Sphere\n") {
            Err(Error::UnknownLamp(k)) => assert_eq!(k, "Sphere"),
            other => panic!("{other:?}"),
        }
        match Scene::from_dump("sg1\nsun Light Point dir=0,-1,0\n") {
            Err(Error::KindKey(KindOf::Light, "dir")) => {}
            other => panic!("{other:?}"),
        }
        match Scene::from_dump("sg1\nsun Light Dir range=2 dir=0,-1,0\n") {
            Err(Error::KindKey(KindOf::Light, "range")) => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn active_camera_is_named() {
        let src = "sg1\ncam Camera eye=0,1,4 at=0,0,0\nside Camera eye=4,1,0 at=0,0,0 active=1\n";
        let s = Scene::from_dump(src).unwrap();
        assert_eq!(s.dump(), src);
        assert_eq!(s.active_camera_name(), Some("side"));
        assert_eq!(s.draw_list().unwrap().camera.name, "side");
        match Scene::from_dump(
            "sg1\na Camera eye=0,1,4 at=0,0,0 active=1\nb Camera eye=1,1,4 at=0,0,0 active=1\n",
        ) {
            Err(Error::MultipleActive) => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn point_light_dumps() {
        let mut s = Scene::new();
        s.insert(Node::group("room")).unwrap();
        s.insert(
            Node::point("lamp")
                .parent("room")
                .color([255, 200, 120, 255])
                .at(Vec3::new(0.9, 1.4, 0.7)),
        )
        .unwrap();
        s.insert(Node::camera("cam", Vec3::new(3.0, 2.0, 4.0), Vec3::ZERO))
            .unwrap();
        let dump = s.dump();
        assert!(
            dump.contains("lamp Light Point c=ffc878 p=room t=0.9,1.4,0.7"),
            "{dump}"
        );
        let d = s.draw_list().unwrap();
        assert!(matches!(d.lights[0].lamp, WorldLamp::Point { .. }));
    }

    #[test]
    fn images_reject_bad_size() {
        let mut im = Images::new();
        assert_eq!(
            im.insert("x", 2, 2, vec![0; 3]),
            Err(Error::BadImage("x".into()))
        );
    }

    #[test]
    fn sphere_dumps_and_hits_front() {
        let mut s = Scene::new();
        s.insert(Node::mesh("orb", Prim::Sphere).color([0xe8, 0xd0, 0x80, 255]))
            .unwrap();
        s.insert(Node::camera("cam", Vec3::new(0.0, 0.0, 3.0), Vec3::ZERO))
            .unwrap();
        let dump = s.dump();
        assert!(dump.contains("orb Mesh Sphere c=e8d080"), "{dump}");
        let d = s.draw_list().unwrap();
        assert_eq!(d.meshes[0].prim, Prim::Sphere);
        let hit = d
            .hit(Vec3::new(0.0, 0.0, 3.0), Vec3::new(0.0, 0.0, -1.0))
            .unwrap();
        assert_eq!(hit.name, "orb");
        assert_eq!(hit.face, HitFace::Front);
        assert!((hit.t - 2.5).abs() < 0.05, "{}", hit.t);
        assert!(
            hit.point.is_near(Vec3::new(0.0, 0.0, 0.5), 0.05),
            "{:?}",
            hit.point
        );
    }

    #[test]
    fn unlit_dumps_and_flattens() {
        let src = "sg1\npanel Quad 2x2 sh=Unlit c=ff0000\ncam Camera eye=0,0,4 at=0,0,0\n";
        let s = Scene::from_dump(src).unwrap();
        assert_eq!(s.dump(), src);
        let d = s.draw_list().unwrap();
        assert_eq!(d.quads[0].shade, Shade::Unlit);
        assert!(d.dump().contains("sh=Unlit"), "{}", d.dump());
        match Scene::from_dump("sg1\npanel Quad 2x2 sh=Toon\n") {
            Err(Error::UnknownShade(k)) => assert_eq!(k, "Toon"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn mat_resolves_and_rejects_mix() {
        let src = "sg1\na Mesh Box mat=print-a\ncam Camera eye=0,1,4 at=0,0,0\n";
        let s = Scene::from_dump(src).unwrap();
        assert_eq!(s.dump(), src);
        match s.draw_list() {
            Err(Error::UnknownMaterial(k)) => assert_eq!(k, "print-a"),
            other => panic!("{other:?}"),
        }
        let mats = Materials::from_dump("mt1\nprint-a Unlit m=front-a mb=back-a\n").unwrap();
        let d = s
            .draw_list_with(&Geometry::new(), &mats, &mut Fonts::new())
            .unwrap();
        assert_eq!(d.meshes[0].shade, Shade::Unlit);
        assert_eq!(d.meshes[0].image.as_deref(), Some("front-a"));
        assert_eq!(d.meshes[0].image_back.as_deref(), Some("back-a"));
        match Scene::from_dump("sg1\na Mesh Box mat=print-a c=ff0000\n") {
            Err(Error::MatInline) => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn materials_dump_roundtrip() {
        let src = include_str!("../tests/golden/surfaces.mt1");
        let m = Materials::from_dump(src).unwrap();
        assert_eq!(m.dump(), src);
        assert_eq!(m.get("print-a").unwrap().shade, Shade::Unlit);
    }

    #[test]
    fn label_dump_roundtrip() {
        let src = "sg1\nhi Label font=ibm size=0.2 \"Arc Welder\"\ncam Camera eye=0,0,4 at=0,0,0\n";
        let s = Scene::from_dump(src).unwrap();
        assert_eq!(s.dump(), src);
        match s.draw_list() {
            Err(Error::UnknownFont(k)) => assert_eq!(k, "ibm"),
            other => panic!("{other:?}"),
        }
        let uni = "sg1\nhi Label font=ibm size=0.2 \"café\"\ncam Camera eye=0,0,4 at=0,0,0\n";
        let s = Scene::from_dump(uni).unwrap();
        assert_eq!(s.dump(), uni);
    }

    #[test]
    fn label_parse_errors() {
        match Scene::from_dump("sg1\nhi Label size=0.2 \"x\"\n") {
            Err(Error::LabelNeedsFont) => {}
            other => panic!("{other:?}"),
        }
        match Scene::from_dump("sg1\nhi Label font=ibm \"x\"\n") {
            Err(Error::LabelNeedsSize) => {}
            other => panic!("{other:?}"),
        }
        match Scene::from_dump("sg1\nhi Label font=ibm size=0.2\n") {
            Err(Error::LabelNeedsText) => {}
            other => panic!("{other:?}"),
        }
        match Scene::from_dump("sg1\nq Quad 1x1 \"nope\"\n") {
            Err(Error::Parse(m)) if m.contains("quoted string only on Label") => {}
            other => panic!("{other:?}"),
        }
        match Scene::from_dump("sg1\nhi Label font=ibm size=0.2 \"x\" 1x1\n") {
            Err(Error::KindKey(KindOf::Label, "WxH")) => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn label_flattens_and_hits() {
        let mut s = Scene::new();
        s.insert(Node::label("hi", "ibm", 0.4, "Hi")).unwrap();
        s.insert(Node::camera("cam", Vec3::new(0.0, 0.0, 3.0), Vec3::ZERO))
            .unwrap();
        let mut fonts = Fonts::new();
        fonts
            .insert("ibm", crate::NOTO_SANS_BOLD, Vec::new())
            .unwrap();
        let d = s
            .draw_list_with(&Geometry::new(), &Materials::new(), &mut fonts)
            .unwrap();
        assert_eq!(d.labels.len(), 1);
        assert!(
            d.labels[0].glyphs.len() >= 2,
            "{}",
            d.labels[0].glyphs.len()
        );
        assert!(d.labels[0].missing.is_empty());
        assert!(d.dump().contains("label hi"), "{}", d.dump());
        assert!(!d.dump().contains("missing="), "{}", d.dump());
        let hit = d
            .hit(Vec3::new(0.0, 0.0, 3.0), Vec3::new(0.0, 0.0, -1.0))
            .unwrap();
        assert_eq!(hit.name, "hi");
        assert_eq!(hit.face, HitFace::Front);

        let mut s = Scene::new();
        s.insert(Node::label("hp", "ibm", 0.2, "HP ☠ 12")).unwrap();
        s.insert(Node::camera("cam", Vec3::new(0.0, 0.0, 3.0), Vec3::ZERO))
            .unwrap();
        let d = s
            .draw_list_with(&Geometry::new(), &Materials::new(), &mut fonts)
            .unwrap();
        assert!(d.labels[0].missing.contains(&0x2620));
        assert!(d.dump().contains("missing=U+2620"), "{}", d.dump());
        assert!(!d.labels[0].glyphs.is_empty());
    }
}
