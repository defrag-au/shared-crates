//! Closed clip drivers. Ops compile to [`sceneanim::Clip`].
//!
//! The graph has no playhead. [`Driver::clip`] reads rest and writes a
//! clip; [`sceneanim::pose`] evaluates it. [`present`] is insert-then-pose
//! for a mixer (pane and gauntlet share it).

mod driver;
mod dump;
mod error;
mod inspect;
mod op;

pub use driver::Driver;
pub use dump::VERSION;
pub use error::Error;
pub use inspect::{flavor_node, inspect, present, FLAVOR_FONT, FLAVOR_SIZE};
pub use op::{
    Fan, Op, Orbit, To, FADE_OPEN, LIFT_OPEN, LIFT_S, LIFT_T, OPEN, SPREAD_X, SPREAD_Y, SPREAD_Z,
};

#[cfg(test)]
mod tests {
    use super::*;
    use sceneanim::Wrap;

    #[test]
    fn dump_roundtrip_compact() {
        let src = "drv1\nshowcase\nfan tag=layer\norbit cam=cam\n";
        let d = Driver::from_dump(src).unwrap();
        assert_eq!(d.dump(), src);
        assert_eq!(d.wrap, Wrap::Once);
        assert_eq!(d.ops.len(), 2);
    }

    #[test]
    fn unknown_op_errors() {
        match Driver::from_dump("drv1\nx\nwiggle node=crate\n") {
            Err(Error::UnknownOp(s)) => assert_eq!(s, "wiggle"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn rust_hooks_match_dump() {
        let d = Driver::new("showcase")
            .op(Fan::tag("layer"))
            .op(Orbit::cam("cam"));
        assert_eq!(d.dump(), "drv1\nshowcase\nfan tag=layer\norbit cam=cam\n");
        let lift = Driver::new("inspect").op(To::lift("th-shredder"));
        assert_eq!(
            lift.dump(),
            "drv1\ninspect\nto node=th-shredder open=0.7 t=0,2.32,2.2 r=0,0,0 s=1.6,1.6,1 ease=inout-cubic\n"
        );
        let fade = Driver::new("flavor-shredder").op(To::fade("flavor-shredder"));
        assert_eq!(
            fade.dump(),
            "drv1\nflavor-shredder\nto node=flavor-shredder open=0.22 a=1 ease=inout-cubic\n"
        );
    }

    #[test]
    fn to_needs_dest() {
        match Driver::from_dump("drv1\nx\nto node=th-shredder\n") {
            Err(Error::ToNeedsDest) => {}
            other => panic!("{other:?}"),
        }
    }
}
