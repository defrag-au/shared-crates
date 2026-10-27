//! World document + Chebyshev window as [`scenegraph::Scene`].
//!
//! The hang table — or a Hilbert generator (`order=` + prints) — is the
//! palace. [`World::emit`] writes only the cells that matter right now.

mod dump;
mod error;
mod hilbert;
mod world;

pub use dump::VERSION;
pub use error::Error;
pub use hilbert::{
    hilbert_index, hilbert_order_for, hilbert_side, hilbert_slot, hilbert_xy, Print, HILBERT_WALLS,
    MAX_ORDER,
};
pub use world::{
    transplant_cam, Hang, Wall, World, CELL, DOOR, HANG_Z, HEIGHT, PALACE_ORDER, PALACE_SPAN,
    PRINT, TITLE_SIZE, TITLE_Y,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dump_roundtrip_compact() {
        let src = "sw1\ngallery\nhang c=0,0 wall=n name=p-a m=demo:a title=A\n";
        let w = World::from_dump(src).unwrap();
        assert_eq!(w.dump(), src);
        assert_eq!(w.cell, CELL);
        assert_eq!(w.span, 0);
        assert_eq!(w.at, (0, 0));
        assert_eq!(w.hangs().len(), 1);
    }

    #[test]
    fn unknown_key_errors() {
        match World::from_dump("sw1\nx maze=1\n") {
            Err(Error::UnknownKey(s)) => assert_eq!(s, "maze"),
            other => panic!("{other:?}"),
        }
        match World::from_dump("sw1\nx\nhang c=0,0 wall=n name=a m=b glow=1\n") {
            Err(Error::UnknownKey(s)) => assert_eq!(s, "glow"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn unknown_wall_errors() {
        match World::from_dump("sw1\nx\nhang c=0,0 wall=up name=a m=b\n") {
            Err(Error::UnknownWall(s)) => assert_eq!(s, "up"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn hang_needs_closed_keys() {
        match World::from_dump("sw1\nx\nhang wall=n name=a m=b\n") {
            Err(Error::HangNeedsCell) => {}
            other => panic!("{other:?}"),
        }
        match World::from_dump("sw1\nx\nhang c=0,0 name=a m=b\n") {
            Err(Error::HangNeedsWall) => {}
            other => panic!("{other:?}"),
        }
        match World::from_dump("sw1\nx\nhang c=0,0 wall=n m=b\n") {
            Err(Error::HangNeedsName) => {}
            other => panic!("{other:?}"),
        }
        match World::from_dump("sw1\nx\nhang c=0,0 wall=n name=a\n") {
            Err(Error::HangNeedsImage) => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn duplicate_slot_and_name() {
        let mut w = World::new("g");
        w.push(Hang::new((0, 0), Wall::N, "a", "m:a")).unwrap();
        match w.push(Hang::new((0, 0), Wall::N, "b", "m:b")) {
            Err(Error::DuplicateSlot { wall, x, z }) => {
                assert_eq!(wall, Wall::N);
                assert_eq!((x, z), (0, 0));
            }
            other => panic!("{other:?}"),
        }
        match w.push(Hang::new((1, 0), Wall::N, "a", "m:c")) {
            Err(Error::DuplicateHang(n)) => assert_eq!(n, "a"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn rust_hook_matches_dump() {
        let mut w = World::new("gallery");
        w.push(Hang::new((0, 0), Wall::N, "p-a", "demo:a").title("A"))
            .unwrap();
        w.span = 1;
        w.at = (2, -1);
        w.cell = 10.0;
        assert_eq!(
            w.dump(),
            "sw1\ngallery cell=10 span=1 at=2,-1\nhang c=0,0 wall=n name=p-a m=demo:a title=A\n"
        );
    }

    #[test]
    fn cell_of_centred_origin() {
        let w = World::new("g");
        assert_eq!(w.cell_of(0.0, 0.0), (0, 0));
        assert_eq!(w.cell_of(3.9, -3.9), (0, 0));
        assert_eq!(w.cell_of(4.0, 0.0), (1, 0));
        assert_eq!(w.cell_of(-4.1, 0.0), (-1, 0));
        assert_eq!(w.cell_of(0.0, 4.0), (0, 1));
    }
}
