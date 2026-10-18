//! The glyph gate: text waits a frame for glyphs that are not cached, and they
//! are cached in one batch after the frame's last draw.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashSet};

use macroquad::prelude::*;

/// Turn on the glyph gate: from now on, text whose glyphs are not cached yet is
/// held back for a frame, and [`flush_glyphs`] caches it all at the end of the
/// frame, so each font atlas uploads at most once per frame.
///
/// ## Why
///
/// macroquad keeps each font's glyphs in one CPU image and one texture. Caching
/// a glyph marks the image dirty, and the next `draw_text_ex` uploads the
/// *whole* image again (`Atlas::texture`). Draws are batched to the end of the
/// frame, but uploads are not: a frame that lays out and draws a hundred runs,
/// each bringing a new glyph or size, uploads the atlas a hundred times. On
/// macOS's OpenGL each upload is staged in its own buffer until the frame is
/// submitted, and the driver keeps the buffers afterwards; on the web each one
/// is a stall. A 2048² atlas is 16 MB, so one text-heavy first frame left
/// gigabytes of graphics memory behind (measured in solada-ledger: 166 uploads
/// in one frame, 129 retained 16 MB regions).
///
/// ## How
///
/// Call this once, before the first frame, and load fonts with [`load_font`]
/// (or pass them to [`register_font`]). Then call [`flush_glyphs`] once per
/// frame, after the last text is drawn. Everything else is automatic: the
/// measures, [`draw_run`](crate::draw_run) and
/// [`TextLayout::draw`](crate::TextLayout::draw) consult the gate, so text
/// whose glyphs are missing is measured with an estimate and not drawn, and
/// appears the next frame from one upload. `macroquad_gamekit::app::run` does
/// all of this for a game.
///
/// A new size is warmed with all of printable ASCII, so numbers that change
/// every frame never miss; other characters are cached as they appear. The
/// cost is that text at a size or character never seen before appears one
/// frame late; a preloader hides the first of those.
///
/// Until this is called the gate admits everything and text caches as it
/// draws, as macroquad does on its own.
///
/// The gate keys on raster sizes, not fonts: [`flush_glyphs`] warms every
/// registered font at each held-back size. A font that is used without being
/// registered still draws, but caches its glyphs mid-frame.
pub fn enable_glyph_gate() {
    GATE.with_borrow_mut(|gate| gate.on = true);
}

/// Load a TTF font and register it with the glyph gate.
pub fn load_font(bytes: &[u8]) -> Result<Font, macroquad::Error> {
    let font = load_ttf_font_from_bytes(bytes)?;
    register_font(&font);
    Ok(font)
}

/// Tell the glyph gate about a font loaded some other way. Register each font
/// once: the gate cannot tell two handles to one font apart.
pub fn register_font(font: &Font) {
    GATE.with_borrow_mut(|gate| gate.fonts.push(font.clone()));
}

/// Whether the gate is holding text back for glyphs it has not cached yet.
pub fn glyphs_pending() -> bool {
    GATE.with_borrow(|gate| !gate.state.is_empty())
}

/// What one [`flush_glyphs`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GlyphFlush {
    /// Sizes and characters cached this call.
    pub cached: usize,
    /// Left for the next call because the size limit was reached.
    pub remaining: usize,
}

/// Cache what the gate held back this frame. Call once per frame, after the
/// last text is drawn.
///
/// `sizes` limits how many new sizes are warmed this call. Warming a size
/// rasterises all of printable ASCII in every registered font: a few
/// milliseconds in a release build, several times that in debug, and on the
/// web it happens on the one thread that also draws. With a limit, the rest
/// wait for the next frame, so a screen that suddenly needs many sizes fills
/// in over a few frames rather than pausing. Single characters are cheap and
/// always cached. `None` caches everything now: right behind a preloader.
///
/// The limit is a count, not a time, so a capture caches the same glyphs on
/// the same frame on every machine.
pub fn flush_glyphs(sizes: Option<usize>) -> GlyphFlush {
    GATE.with_borrow_mut(|gate| {
        let ascii: Vec<char> = printable_ascii().collect();
        let default_font = gate.default_font.then(Font::default);
        let mut cached = 0;
        let mut warmed = 0;
        while let Some(job) = gate.state.next_job() {
            let fonts = gate.fonts.iter().chain(default_font.as_ref());
            match job {
                Job::Size(size) => {
                    if sizes.is_some_and(|limit| warmed >= limit) {
                        break;
                    }
                    warmed += 1;
                    fonts.for_each(|font| font.populate_font_cache(&ascii, size));
                }
                Job::Char(size, ch) => fonts.for_each(|font| font.populate_font_cache(&[ch], size)),
            }
            gate.state.done(job);
            cached += 1;
        }
        GlyphFlush {
            cached,
            remaining: gate.state.len(),
        }
    })
}

/// The characters a size is warmed with in one go.
pub(crate) fn printable_ascii() -> impl Iterator<Item = char> {
    (0x20u8..=0x7e).map(char::from)
}

/// Whether the gate lets `text` through at atlas size `size`; records what is
/// missing when it does not.
pub(crate) fn admits(text: &str, font: Option<&Font>, size: u16) -> bool {
    GATE.with_borrow_mut(|gate| {
        if !gate.on {
            return true;
        }
        if font.is_none() {
            gate.default_font = true;
        }
        gate.state.admit(text, size)
    })
}

thread_local! {
    /// macroquad draws from one thread, so the gate is per thread.
    static GATE: RefCell<Gate> = RefCell::new(Gate::default());
}

#[derive(Default)]
struct Gate {
    on: bool,
    fonts: Vec<Font>,
    /// A style without a font was seen: warm macroquad's own too.
    default_font: bool,
    state: GateState,
}

/// One unit of caching work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Job {
    /// All of printable ASCII at an atlas size.
    Size(u16),
    /// One other character at an atlas size.
    Char(u16, char),
}

#[derive(Debug, Default)]
struct GateState {
    /// Atlas sizes whose printable ASCII is cached in every font.
    warm: HashSet<u16>,
    /// Other characters cached at an atlas size.
    extra: HashSet<(u16, char)>,
    pending_sizes: BTreeSet<u16>,
    pending_chars: BTreeSet<(u16, char)>,
}

impl GateState {
    /// Whether every glyph `text` needs at atlas size `size` is cached;
    /// otherwise remember what is missing.
    fn admit(&mut self, text: &str, size: u16) -> bool {
        let mut ready = true;
        if !self.warm.contains(&size) {
            self.pending_sizes.insert(size);
            ready = false;
        }
        for ch in text.chars() {
            // Layout breaks on whitespace, so it never reaches the atlas.
            if ch.is_ascii_graphic() || ch.is_whitespace() {
                continue;
            }
            if !self.extra.contains(&(size, ch)) {
                self.pending_chars.insert((size, ch));
                ready = false;
            }
        }
        ready
    }

    /// The next thing to cache: odd characters first, as they are cheap.
    fn next_job(&self) -> Option<Job> {
        self.pending_chars
            .first()
            .map(|&(size, ch)| Job::Char(size, ch))
            .or_else(|| self.pending_sizes.first().map(|&size| Job::Size(size)))
    }

    /// Mark a job cached.
    fn done(&mut self, job: Job) {
        match job {
            Job::Size(size) => {
                self.pending_sizes.remove(&size);
                self.warm.insert(size);
            }
            Job::Char(size, ch) => {
                self.pending_chars.remove(&(size, ch));
                self.extra.insert((size, ch));
            }
        }
    }

    fn len(&self) -> usize {
        self.pending_sizes.len() + self.pending_chars.len()
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run every pending job, as an unlimited flush does.
    fn drain(gate: &mut GateState) -> Vec<Job> {
        let mut jobs = Vec::new();
        while let Some(job) = gate.next_job() {
            gate.done(job);
            jobs.push(job);
        }
        jobs
    }

    #[test]
    fn the_gate_holds_back_new_sizes_until_flushed_then_admits_them() {
        let mut gate = GateState::default();
        assert!(!gate.admit("12,345 ADA", 24), "a size never seen");
        assert!(!gate.admit("BUY", 24), "still pending this frame");
        assert_eq!(drain(&mut gate), vec![Job::Size(24)]);
        assert!(gate.admit("98,760 ADA", 24), "any ASCII at a warm size");
        assert!(!gate.admit("BUY", 18), "a different size is not warm");
    }

    #[test]
    fn the_gate_caches_other_characters_one_by_one_at_a_warm_size() {
        let mut gate = GateState::default();
        gate.admit("x", 24);
        drain(&mut gate);
        assert!(!gate.admit("A → B", 24));
        assert_eq!(
            drain(&mut gate),
            vec![Job::Char(24, '→')],
            "the size was already warm"
        );
        assert!(gate.admit("C → D", 24));
        assert!(
            gate.admit("tab\tand\u{a0}nbsp", 24),
            "whitespace is never drawn"
        );
    }

    #[test]
    fn a_limited_flush_leaves_the_rest_held_back_until_it_is_cached() {
        let mut gate = GateState::default();
        gate.admit("a", 12);
        gate.admit("b", 18);
        gate.admit("é", 12);
        // Characters first, then sizes, smallest first.
        let first = gate.next_job().unwrap();
        assert_eq!(first, Job::Char(12, 'é'));
        gate.done(first);
        let second = gate.next_job().unwrap();
        assert_eq!(second, Job::Size(12));
        gate.done(second);
        assert_eq!(gate.len(), 1, "18 waits for the next frame");
        assert!(!gate.admit("b", 18), "and its text stays held back");
        assert!(gate.admit("é a", 12));
    }
}
