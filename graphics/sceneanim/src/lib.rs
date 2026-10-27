//! Pure clip evaluator for [`scenegraph::Scene`].
//!
//! The graph does not know about time. [`pose`] clones a scene and writes
//! sampled TRS / camera channels. Dumps (`an1`) and posed `sg1` snapshots
//! are the tests — not screenshots.

mod clip;
mod dump;
mod ease;
mod error;
mod interp;
mod mixer;
mod pose;
mod quat;

pub use clip::{Channel, Clip, Interp, Key, Track, Wrap};
pub use dump::VERSION;
pub use ease::Ease;
pub use error::Error;
pub use mixer::{Layer, Mixer, VERSION as MIX_VERSION};
pub use pose::{pose, pose_layers};

#[cfg(test)]
mod tests {
    use super::*;
    use scenegraph::{Kind, Node, Scene, Vec3};

    fn rest() -> Scene {
        let mut s = Scene::new();
        s.insert(Node::group("crate").at(Vec3::new(0.0, 0.4, 0.0)))
            .unwrap();
        s.insert(Node::camera(
            "cam",
            Vec3::new(3.2, 2.4, 4.2),
            Vec3::new(0.0, 0.6, 0.0),
        ))
        .unwrap();
        s
    }

    #[test]
    fn dump_roundtrip() {
        let src = "an1\nbob wrap=loop\ncrate t 0:0,0.4,0 0.5:0,0.7,0 1:0,0.4,0\n";
        let c = Clip::from_dump(src).unwrap();
        assert_eq!(c.dump(), src);
        assert_eq!(c.wrap, Wrap::Loop);
        assert!((c.duration() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn pose_hits_keys() {
        let c = Clip::from_dump("an1\nbob\ncrate t 0:0,0.4,0 0.5:0,0.7,0 1:0,0.4,0\n").unwrap();
        let s = rest();
        let a = c.pose(&s, 0.0).unwrap();
        assert!(a
            .get("crate")
            .unwrap()
            .xf
            .t
            .is_near(Vec3::new(0.0, 0.4, 0.0), 1e-5));
        let b = c.pose(&s, 0.5).unwrap();
        assert!(b
            .get("crate")
            .unwrap()
            .xf
            .t
            .is_near(Vec3::new(0.0, 0.7, 0.0), 1e-5));
        let mid = c.pose(&s, 0.25).unwrap();
        assert!(mid
            .get("crate")
            .unwrap()
            .xf
            .t
            .is_near(Vec3::new(0.0, 0.55, 0.0), 1e-4));
    }

    #[test]
    fn unknown_target_errors() {
        let c = Clip::from_dump("an1\nbob\nnope t 0:0,0,0\n").unwrap();
        match c.pose(&rest(), 0.0) {
            Err(Error::UnknownTarget(n)) => assert_eq!(n, "nope"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn rest_scene_unmoved() {
        let c = Clip::from_dump("an1\nbob\ncrate t 0:0,0.4,0 1:0,0.9,0\n").unwrap();
        let s = rest();
        let _ = c.pose(&s, 1.0).unwrap();
        assert!(s
            .get("crate")
            .unwrap()
            .xf
            .t
            .is_near(Vec3::new(0.0, 0.4, 0.0), 1e-5));
    }

    #[test]
    fn wrap_once_loop_ping() {
        let once = Clip::from_dump("an1\nbob\ncrate t 0:0,0,0 1:0,1,0\n").unwrap();
        assert_eq!(once.wrap, Wrap::Once);
        assert!((once.wrap_time(1.5) - 1.0).abs() < 1e-6);
        let looped = Clip::from_dump("an1\nbob wrap=loop\ncrate t 0:0,0,0 1:0,1,0\n").unwrap();
        assert!((looped.wrap_time(1.25) - 0.25).abs() < 1e-6);
        assert!(looped.wrap_time(1.0).abs() < 1e-6);
        let ping = Clip::from_dump("an1\nbob wrap=ping\ncrate t 0:0,0,0 1:0,1,0\n").unwrap();
        assert!((ping.wrap_time(0.25) - 0.25).abs() < 1e-6);
        assert!((ping.wrap_time(1.5) - 0.5).abs() < 1e-6);
        assert!(ping.wrap_time(2.0).abs() < 1e-6);
    }

    #[test]
    fn step_holds_until_next_key() {
        let c = Clip::from_dump("an1\nbob\ncrate t Step 0:0,0,0 1:0,1,0\n").unwrap();
        let s = rest();
        let mid = c.pose(&s, 0.9).unwrap();
        assert!(mid.get("crate").unwrap().xf.t.is_near(Vec3::ZERO, 1e-5));
        let end = c.pose(&s, 1.0).unwrap();
        assert!(end
            .get("crate")
            .unwrap()
            .xf
            .t
            .is_near(Vec3::new(0.0, 1.0, 0.0), 1e-5));
    }

    #[test]
    fn camera_eye_channel() {
        let c = Clip::from_dump("an1\npan\ncam eye 0:3.2,2.4,4.2 1:0,2.4,4.2\n").unwrap();
        let s = c.pose(&rest(), 1.0).unwrap();
        match &s.get("cam").unwrap().kind {
            Kind::Camera { eye, .. } => {
                assert!(eye.is_near(Vec3::new(0.0, 2.4, 4.2), 1e-5));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn eye_on_group_is_bad_track() {
        let c = Clip::from_dump("an1\nbob\ncrate eye 0:0,0,0\n").unwrap();
        match c.pose(&rest(), 0.0) {
            Err(Error::BadTrack(n, Track::Eye)) => assert_eq!(n, "crate"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn linear_rotation_slerp_midpoint() {
        let c = Clip::from_dump("an1\nspin\ncrate r 0:0,0,0 1:0,1.5708,0\n").unwrap();
        let mid = c.pose(&rest(), 0.5).unwrap();
        let r = mid.get("crate").unwrap().xf.r;
        assert!(
            r.is_near(Vec3::new(0.0, std::f32::consts::FRAC_PI_2 * 0.5, 0.0), 1e-3),
            "{r:?}"
        );
    }

    #[test]
    fn ease_dump_roundtrip() {
        let src = "an1\nbob\ncrate t ease=in-cubic 0:0,0,0 1:0,1,0\n";
        let c = Clip::from_dump(src).unwrap();
        assert_eq!(c.dump(), src);
        assert_eq!(c.channels[0].ease, Ease::InCubic);
    }

    #[test]
    fn ease_in_cubic_lags_linear() {
        let c = Clip::from_dump("an1\nbob\ncrate t ease=in-cubic 0:0,0,0 1:0,1,0\n").unwrap();
        let mid = c.pose(&rest(), 0.5).unwrap();
        let y = mid.get("crate").unwrap().xf.t.y;
        assert!((y - 0.125).abs() < 1e-4, "in-cubic 0.5^3 = 0.125, got {y}");
    }

    #[test]
    fn pose_layers_overlay_two_nodes() {
        let a = Clip::from_dump("an1\na\ncrate t 0:0,0.4,0 1:0,1,0\n").unwrap();
        let b = Clip::from_dump("an1\nb\ncam eye 0:3.2,2.4,4.2 1:0,2.4,4.2\n").unwrap();
        let s = pose_layers(&rest(), &[(&a, 1.0), (&b, 1.0)]).unwrap();
        assert!(s
            .get("crate")
            .unwrap()
            .xf
            .t
            .is_near(Vec3::new(0.0, 1.0, 0.0), 1e-5));
        match &s.get("cam").unwrap().kind {
            Kind::Camera { eye, .. } => {
                assert!(eye.is_near(Vec3::new(0.0, 2.4, 4.2), 1e-5));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn pose_layers_empty_is_clone() {
        let s = rest();
        let out = pose_layers(&s, &[]).unwrap();
        assert_eq!(out.dump(), s.dump());
    }

    #[test]
    fn alpha_fades_label() {
        let mut s = Scene::new();
        s.insert(Node::label("hi", "ibm", 0.2, "Hi")).unwrap();
        s.insert(Node::camera("cam", Vec3::new(0.0, 0.0, 3.0), Vec3::ZERO))
            .unwrap();
        let c = Clip::from_dump("an1\nfade\nhi a 0:0,0,0 1:1,0,0\n").unwrap();
        assert_eq!(c.dump(), "an1\nfade\nhi a 0:0,0,0 1:1,0,0\n");
        let a0 = c.pose(&s, 0.0).unwrap();
        assert!(a0.dump().contains("c=ffffff00"), "{}", a0.dump());
        let mid = c.pose(&s, 0.5).unwrap();
        assert!(mid.dump().contains("c=ffffff80"), "{}", mid.dump());
        let a1 = c.pose(&s, 1.0).unwrap();
        assert!(!a1.dump().contains("c="), "opaque omits c=: {}", a1.dump());
        assert_eq!(s.dump().matches("c=").count(), 0);
    }

    #[test]
    fn alpha_on_group_is_bad_track() {
        let c = Clip::from_dump("an1\nbob\ncrate a 0:0,0,0\n").unwrap();
        match c.pose(&rest(), 0.0) {
            Err(Error::BadTrack(n, Track::Alpha)) => assert_eq!(n, "crate"),
            other => panic!("{other:?}"),
        }
    }

    /// A `pose` track runs a posed model's own clip: the node keeps its
    /// clip and the track writes its time. A node with no `pose=` to run
    /// is a bad track, as alpha on a group is.
    #[test]
    fn a_pose_track_runs_the_models_clip() {
        let scene = Scene::from_dump(
            "sg1\nfox Mesh Geom g=fox pose=Walk@0\ncam Camera eye=0,1,4 at=0,0,0\n",
        )
        .unwrap();
        let c = Clip::from_dump("an1\nstroll wrap=loop\nfox pose 0:0,0,0 2:2,0,0\n").unwrap();
        assert_eq!(
            c.dump(),
            "an1\nstroll wrap=loop\nfox pose 0:0,0,0 2:2,0,0\n"
        );
        let at = c.pose(&scene, 0.5).unwrap();
        assert!(
            at.dump().contains("fox Mesh Geom g=fox pose=Walk@0.5"),
            "{}",
            at.dump()
        );
        assert!(scene.dump().contains("pose=Walk@0\n"), "rest is unmoved");

        let still =
            Scene::from_dump("sg1\nfox Mesh Geom g=fox\ncam Camera eye=0,1,4 at=0,0,0\n").unwrap();
        match c.pose(&still, 0.5) {
            Err(Error::BadTrack(n, Track::Pose)) => assert_eq!(n, "fox"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn mixer_go_retreat_tick() {
        let clip = Clip::from_dump("an1\ncrate\ncrate t 0:0,0.4,0 1:0,1,0\n").unwrap();
        let s = rest();
        let mut mx = Mixer::new();
        mx.go(clip);
        assert_eq!(mx.dump(), "mx1\ncrate\n");
        mx.tick(1.0);
        assert!((mx.layers[0].time - 1.0).abs() < 1e-5);
        assert!(mx.layers[0].rate.abs() < 1e-5);
        let up = mx.pose(&s).unwrap();
        assert!(up
            .get("crate")
            .unwrap()
            .xf
            .t
            .is_near(Vec3::new(0.0, 1.0, 0.0), 1e-4));
        mx.retreat("crate");
        assert_eq!(mx.dump(), "mx1\ncrate t=1 rate=-1\n");
        mx.tick(1.0);
        assert!(mx.is_empty(), "{}", mx.dump());
        let home = mx.pose(&s).unwrap();
        assert!(home
            .get("crate")
            .unwrap()
            .xf
            .t
            .is_near(Vec3::new(0.0, 0.4, 0.0), 1e-4));
        assert_eq!(s.dump(), rest().dump());
    }

    #[test]
    fn mixer_then_waits_then_reverses_first() {
        let a = Clip::from_dump("an1\nlift\ncrate t 0:0,0.4,0 0.5:0,1,0\n").unwrap();
        let b = Clip::from_dump("an1\nfade\ncrate s 0:1,1,1 0.2:2,2,1\n").unwrap();
        let s = rest();
        let mut mx = Mixer::new();
        mx.go(a);
        mx.then(b, "lift");
        assert_eq!(mx.dump(), "mx1\nlift\nfade after=lift\n");
        mx.tick(0.5);
        assert!((mx.layers[0].time - 0.5).abs() < 1e-4);
        assert!(mx.layers[1].time.abs() < 1e-4, "fade waits, {}", mx.dump());
        mx.tick(0.2);
        assert!(
            mx.layers[1].time > 0.15,
            "fade runs after park, {}",
            mx.dump()
        );
        mx.retreat_all();
        assert!(
            mx.dump().contains("fade") && mx.dump().contains("rate=-1"),
            "fade reverses first: {}",
            mx.dump()
        );
        assert!(
            mx.layers
                .iter()
                .find(|l| l.clip.name == "lift")
                .is_some_and(|l| l.rate.abs() < 1e-4),
            "lift stays parked: {}",
            mx.dump()
        );
        mx.tick(0.2);
        assert!(
            mx.layers.iter().all(|l| l.clip.name != "fade"),
            "{}",
            mx.dump()
        );
        assert!(
            mx.layers
                .iter()
                .find(|l| l.clip.name == "lift")
                .is_some_and(|l| l.rate < 0.0),
            "lift reverses after fade: {}",
            mx.dump()
        );
        mx.tick(0.5);
        assert!(mx.is_empty(), "{}", mx.dump());
        assert!(s
            .get("crate")
            .unwrap()
            .xf
            .t
            .is_near(Vec3::new(0.0, 0.4, 0.0), 1e-4));
    }
}
