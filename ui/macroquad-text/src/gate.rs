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

/// Run `draw` with the gate open: text in it caches its glyphs as it draws and
/// is never held back.
///
/// For drawing that happens once and is kept — a card face baked into a
/// texture. Held back, its text would be missing from the bake for good, not
/// for a frame. The cost is what the gate otherwise saves: an atlas upload
/// mid-frame when the bake brings new glyphs, which a bake, done once per face,
/// can afford. `macroquad_gamekit`'s bakes run inside this.
pub fn ungated<R>(draw: impl FnOnce() -> R) -> R {
    GATE.with_borrow_mut(|gate| gate.ungated += 1);
    let out = draw();
    GATE.with_borrow_mut(|gate| gate.ungated -= 1);
    out
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
/// rasterises a set of characters — all of printable ASCII for body text,
/// the digits for a large size — in every registered font: a few
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
        let numeric: Vec<char> = NUMERIC.chars().collect();
        while let Some(job) = gate.state.next_job() {
            let fonts = gate.fonts.iter().chain(default_font.as_ref());
            match job {
                Job::Set(size, set) => {
                    if sizes.is_some_and(|limit| warmed >= limit) {
                        break;
                    }
                    warmed += 1;
                    let chars = match set {
                        Set::Ascii => &ascii,
                        Set::Numeric => &numeric,
                    };
                    fonts.for_each(|font| font.populate_font_cache(chars, size));
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
        if !gate.on || gate.ungated > 0 {
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
    /// Depth of [`ungated`] scopes: inside one, everything is admitted.
    ungated: u32,
    fonts: Vec<Font>,
    /// A style without a font was seen: warm macroquad's own too.
    default_font: bool,
    state: GateState,
}

/// A set of characters warmed at a size in one go.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Set {
    /// All of printable ASCII: what a small size is warmed with.
    Ascii,
    /// Digits and the punctuation numbers are written with: what a large
    /// size is warmed with, so a score or a total that changes every frame
    /// never waits for a glyph.
    Numeric,
}

/// The characters [`Set::Numeric`] holds.
const NUMERIC: &str = "0123456789.,:;+-–%$/()x";

/// One unit of caching work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Job {
    /// A set of characters at an atlas size.
    Set(u16, Set),
    /// One other character at an atlas size.
    Char(u16, char),
}

#[derive(Debug, Default)]
struct GateState {
    /// Sets cached at an atlas size, in every font.
    warm: HashSet<(u16, Set)>,
    /// Other characters cached at an atlas size.
    extra: HashSet<(u16, char)>,
    pending_sets: BTreeSet<(u16, Set)>,
    pending_chars: BTreeSet<(u16, char)>,
}

/// The largest atlas size, in device pixels, warmed with all of printable
/// ASCII at once. Body text (8–20 points at 2–3 pixels a point) is under it.
/// Above it — titles, banners, a 96-point countdown — a size holds a few
/// strings, and warming 95 glyphs of that size in every registered font filled
/// turbo-turns' atlas with 250 MB of glyphs no one drew and took 1.3 s. So above
/// it, digits come as a set (a big number changes every frame) and every
/// other character is cached as it appears.
const FULL_WARM_MAX: u16 = 64;

/// The largest atlas size warmed with [`Set::Numeric`]. Above it — a
/// countdown at 190 pixels — even the digits are a few big glyphs each, and
/// are cached one by one as they appear.
const NUMERIC_WARM_MAX: u16 = 160;

impl GateState {
    /// Whether every glyph `text` needs at atlas size `size` is cached;
    /// otherwise remember what is missing.
    fn admit(&mut self, text: &str, size: u16) -> bool {
        let mut ready = true;
        let mut need = |gate: &mut Self, set: Set| {
            if !gate.warm.contains(&(size, set)) {
                gate.pending_sets.insert((size, set));
                ready = false;
            }
        };
        // The set this size is warmed with, if any, and what it covers.
        let set = if size <= FULL_WARM_MAX {
            Some(Set::Ascii)
        } else if size <= NUMERIC_WARM_MAX {
            Some(Set::Numeric)
        } else {
            None
        };
        let covers = |ch: char| match set {
            Some(Set::Ascii) => ch.is_ascii_graphic(),
            Some(Set::Numeric) => NUMERIC.contains(ch),
            None => false,
        };
        match set {
            Some(Set::Ascii) => need(self, Set::Ascii),
            Some(Set::Numeric) if text.chars().any(covers) => need(self, Set::Numeric),
            _ => {}
        }
        for ch in text.chars() {
            // Layout breaks on whitespace, so it never reaches the atlas.
            if ch.is_whitespace() || covers(ch) {
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
            .or_else(|| {
                self.pending_sets
                    .first()
                    .map(|&(size, set)| Job::Set(size, set))
            })
    }

    /// Mark a job cached.
    fn done(&mut self, job: Job) {
        match job {
            Job::Set(size, set) => {
                self.pending_sets.remove(&(size, set));
                self.warm.insert((size, set));
            }
            Job::Char(size, ch) => {
                self.pending_chars.remove(&(size, ch));
                self.extra.insert((size, ch));
            }
        }
    }

    fn len(&self) -> usize {
        self.pending_sets.len() + self.pending_chars.len()
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
        assert_eq!(drain(&mut gate), vec![Job::Set(24, Set::Ascii)]);
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
    fn a_large_size_caches_only_the_characters_it_draws() {
        let mut gate = GateState::default();
        assert!(!gate.admit("GO!", 220));
        let mut jobs = drain(&mut gate);
        jobs.sort_by_key(|job| format!("{job:?}"));
        assert_eq!(
            jobs,
            vec![
                Job::Char(220, '!'),
                Job::Char(220, 'G'),
                Job::Char(220, 'O')
            ],
            "three glyphs, not all of ASCII"
        );
        assert!(gate.admit("GO", 220));
    }

    #[test]
    fn a_large_number_warms_every_digit_at_once() {
        let mut gate = GateState::default();
        assert!(
            !gate.admit("27,730", 98),
            "the first number at a large size"
        );
        assert_eq!(drain(&mut gate), vec![Job::Set(98, Set::Numeric)]);
        assert!(gate.admit("31,894", 98), "any other number draws at once");
        assert!(!gate.admit("ADA", 98), "letters are still one at a time");
    }

    #[test]
    fn a_huge_size_caches_even_its_digits_one_by_one() {
        let mut gate = GateState::default();
        assert!(!gate.admit("3", 190));
        assert_eq!(drain(&mut gate), vec![Job::Char(190, '3')]);
        assert!(!gate.admit("2", 190), "the next digit waits a frame");
    }

    #[test]
    fn a_bake_is_never_held_back_even_with_the_gate_on() {
        enable_glyph_gate();
        assert!(!admits("label", None, 31), "held back on the screen");
        assert!(
            ungated(|| admits("face text", None, 33)),
            "admitted inside a bake"
        );
        assert!(!admits("label", None, 35), "and the gate is back after it");
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
        assert_eq!(second, Job::Set(12, Set::Ascii));
        gate.done(second);
        assert_eq!(gate.len(), 1, "18 waits for the next frame");
        assert!(!gate.admit("b", 18), "and its text stays held back");
        assert!(gate.admit("é a", 12));
    }
}
