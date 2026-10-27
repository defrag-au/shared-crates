//! Pure pointer session for [`scenegraph::Scene`].
//!
//! The graph has no widgets. [`step`] takes a rest scene, the [`DrawList`]
//! the pointer sees, and a closed event. Pick writes the session; orbit,
//! drag, and walk write a new rest scene. Dumps (`in1`) are the tests.

mod dump;
mod error;
mod session;
mod step;
mod tool;

pub use dump::VERSION;
pub use error::Error;
pub use session::{Sel, Session};
pub use step::{pick_ndc, step, step_with, stride};
pub use tool::{Event, Tool};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dump_roundtrip_empty() {
        let src = "in1\npick\n";
        let s = Session::from_dump(src).unwrap();
        assert_eq!(s.dump(), src);
        assert_eq!(s.tool, Tool::Pick);
        assert!(s.sel.is_none());
    }

    #[test]
    fn dump_roundtrip_walk() {
        let src = "in1\nwalk\n";
        let s = Session::from_dump(src).unwrap();
        assert_eq!(s.dump(), src);
        assert_eq!(s.tool, Tool::Walk);
    }

    #[test]
    fn dump_roundtrip_sel() {
        let src = "in1\npick\nndc=-0.3,0.1\nsel=a face=front uv=0.5,0.5 at=-1.3,1.55,-0.15\n";
        let s = Session::from_dump(src).unwrap();
        assert_eq!(s.dump(), src);
        assert_eq!(s.sel.as_ref().unwrap().name, "a");
    }

    #[test]
    fn unknown_tool_errors() {
        match Session::from_dump("in1\nbutton\n") {
            Err(Error::UnknownTool(t)) => assert_eq!(t, "button"),
            other => panic!("{other:?}"),
        }
    }
}
