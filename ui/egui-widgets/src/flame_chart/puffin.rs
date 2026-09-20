//! Turning a `puffin` frame into [`Span`]s.
//!
//! Feature-gated (`puffin`) and deliberately the only place in this crate that
//! knows a profiler exists. [`super::FlameChart`] takes plain data, so a second
//! source — a worker's trace, a server timing — plugs in beside this without
//! touching the widget.
//!
//! # The stream is a tree written flat
//!
//! puffin stores each thread's scopes as a byte stream in which a scope carries
//! the OFFSETS of its children. `Reader::from_start` walks siblings only;
//! descending means opening another reader at `child_begin_position`. So the
//! recursion below is not incidental — it is how depth is recovered, because
//! nothing in the stream records a level.
//!
//! # Names live apart from the stream
//!
//! A scope on the wire is a [`puffin::ScopeId`], not a string; the same
//! function profiled ten thousand times costs ten thousand integers and one
//! name. Resolving them needs a [`puffin::ScopeCollection`], which a consumer
//! holds via `FrameView::scope_collection()` — `GlobalProfiler` keeps one but
//! exposes no accessor, so it is the caller's to pass rather than ours to find.
//!
//! # Why the recursion is split out and tested
//!
//! [`spans_of_stream`] takes a bare `Stream` and a name lookup, so the tree
//! walk can be checked against a stream built by hand. The first version of
//! this file was written from memory of the API and was wrong in two places at
//! once — a field that does not exist, and an offset passed where a stream
//! goes. Neither showed up until a browser drew an empty chart.

use super::Span;
use std::sync::Arc;

/// Flatten every thread in `frame` into spans, depth-first.
///
/// Threads are concatenated rather than interleaved: their scopes share a clock
/// but not a stack, so laying them on one set of rows would draw a child inside
/// a parent it never ran under. Each thread restarts at depth 0.
pub fn spans_of(frame: &puffin::UnpackedFrameData, scopes: &puffin::ScopeCollection) -> Vec<Span> {
    let name_of = |id: &puffin::ScopeId| {
        scopes
            .fetch_by_id(id)
            .map(|details| Arc::from(details.name().as_ref()))
    };

    let mut out = Vec::new();
    for stream_info in frame.thread_streams.values() {
        spans_into(&stream_info.stream, &name_of, &mut out);
    }
    out
}

/// One stream's scopes, depth-first, as spans.
///
/// Split from [`spans_of`] so the tree walk is reachable from a test with a
/// hand-built stream — no frame, no profiler, no browser.
pub fn spans_of_stream(
    stream: &puffin::Stream,
    name_of: &dyn Fn(&puffin::ScopeId) -> Option<Arc<str>>,
) -> Vec<Span> {
    let mut out = Vec::new();
    spans_into(stream, name_of, &mut out);
    out
}

fn spans_into(
    stream: &puffin::Stream,
    name_of: &dyn Fn(&puffin::ScopeId) -> Option<Arc<str>>,
    out: &mut Vec<Span>,
) {
    push_siblings(stream, 0, 0, name_of, out);
}

/// One level of siblings starting at `offset`, then each of their children.
///
/// A stream that fails to parse contributes what it managed before the error
/// rather than aborting the capture: a partial flame chart is worth more than
/// no picture, which is the alternative when one truncated stream can take the
/// lot with it.
fn push_siblings(
    stream: &puffin::Stream,
    offset: u64,
    depth: u16,
    name_of: &dyn Fn(&puffin::ScopeId) -> Option<Arc<str>>,
    out: &mut Vec<Span>,
) {
    // Guard the recursion as well as the offset. A malformed stream pointing a
    // child back at its parent would otherwise recurse until the stack gave
    // out, and a profiler taking the app down is a poor trade for a
    // diagnostic.
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(reader) = puffin::Reader::with_offset(stream, offset) else {
        return;
    };

    for scope in reader {
        let Ok(scope) = scope else {
            return;
        };

        out.push(Span {
            depth,
            start_ns: scope.record.start_ns,
            duration_ns: scope.record.duration_ns,
            label: name_of(&scope.id).unwrap_or_else(|| Arc::from("<unnamed scope>")),
        });

        // `depth` is ours to track: the stream records parentage as offsets and
        // never as a level.
        if scope.child_begin_position < scope.child_end_position {
            push_siblings(stream, scope.child_begin_position, depth + 1, name_of, out);
        }
    }
}

/// Deeper than any real call stack a frame produces, and shallow enough that
/// recursing to it cannot exhaust the stack.
const MAX_DEPTH: u16 = 128;

#[cfg(test)]
mod tests {
    use super::*;
    use puffin::{ScopeId, Stream};

    /// `ScopeId::new` is `pub(crate)` and test-only inside puffin, but the
    /// tuple field is public — so a test here builds one directly rather than
    /// going through a profiler to obtain one.
    fn scope_id(id: u32) -> ScopeId {
        ScopeId(std::num::NonZeroU32::new(id).expect("non-zero"))
    }

    /// Names by id, so a test can assert on something readable.
    fn named(id: &ScopeId) -> Option<Arc<str>> {
        Some(Arc::from(format!("scope-{}", id.0).as_str()))
    }

    /// The shape puffin's own tests use: one top scope with two children.
    fn nested() -> Stream {
        let mut stream = Stream::default();
        let (top, _) = stream.begin_scope(|| 100, scope_id(1), "");
        let (a, _) = stream.begin_scope(|| 200, scope_id(2), "");
        stream.end_scope(a, 300);
        let (b, _) = stream.begin_scope(|| 300, scope_id(3), "");
        stream.end_scope(b, 400);
        stream.end_scope(top, 400);
        stream
    }

    /// The whole point of the recursion: depth is RECOVERED, because the
    /// stream stores parentage as byte offsets and never as a level. Getting
    /// this wrong draws every scope on one row, which is not a flame chart.
    #[test]
    fn children_are_one_level_deeper_than_their_parent() {
        let spans = spans_of_stream(&nested(), &named);

        assert_eq!(spans.len(), 3, "one top scope and its two children");
        assert_eq!(spans[0].depth, 0);
        assert_eq!(spans[1].depth, 1);
        assert_eq!(spans[2].depth, 1);
    }

    #[test]
    fn times_and_names_survive_the_walk() {
        let spans = spans_of_stream(&nested(), &named);

        assert_eq!(spans[0].start_ns, 100);
        assert_eq!(spans[0].duration_ns, 300);
        assert_eq!(spans[0].label.as_ref(), "scope-1");

        assert_eq!(spans[1].start_ns, 200);
        assert_eq!(spans[1].duration_ns, 100);
        assert_eq!(spans[1].label.as_ref(), "scope-2");

        assert_eq!(spans[2].start_ns, 300);
        assert_eq!(spans[2].duration_ns, 100);
    }

    /// Every child must be visited, not just the first. A sibling walk that
    /// stopped early would silently drop most of a frame.
    #[test]
    fn every_sibling_is_visited() {
        let mut stream = Stream::default();
        let (top, _) = stream.begin_scope(|| 0, scope_id(1), "");
        for i in 0..8 {
            let (child, _) = stream.begin_scope(|| i * 10, scope_id(2), "");
            stream.end_scope(child, i * 10 + 5);
        }
        stream.end_scope(top, 100);

        let spans = spans_of_stream(&stream, &named);
        assert_eq!(spans.iter().filter(|s| s.depth == 1).count(), 8);
    }

    /// Depth is unbounded in principle and the walk must handle a real stack.
    #[test]
    fn a_deep_stack_keeps_its_levels() {
        let mut stream = Stream::default();
        let mut open = Vec::new();
        for level in 0..16i64 {
            let (offset, _) = stream.begin_scope(|| level, scope_id(1), "");
            open.push(offset);
        }
        for (level, offset) in open.into_iter().enumerate().rev() {
            stream.end_scope(offset, 100 - level as i64);
        }

        let spans = spans_of_stream(&stream, &named);
        let depths: Vec<u16> = spans.iter().map(|s| s.depth).collect();
        assert_eq!(depths, (0..16).collect::<Vec<u16>>());
    }

    /// A scope whose id is not in the collection is still drawn. Dropping it
    /// would leave a hole in the stack under a parent that still claims the
    /// time, which reads as the profiler losing work.
    #[test]
    fn an_unresolvable_name_still_yields_a_span() {
        let spans = spans_of_stream(&nested(), &|_| None);

        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].label.as_ref(), "<unnamed scope>");
    }

    #[test]
    fn an_empty_stream_yields_nothing() {
        assert!(spans_of_stream(&Stream::default(), &named).is_empty());
    }
}
