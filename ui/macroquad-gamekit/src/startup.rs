//! `Startup` — the phases a screen goes through between opening and drawing
//! whole: settling while the window finds its size, warming glyphs behind a
//! [`Preloader`], then live.
//!
//! ```text
//! Settling ──scale still──▶ Warming ──nothing held back──▶ Live
//!     ▲                                                     │
//!     └──────────────────── scale changed ──────────────────┘
//! ```
//!
//! A tiling window manager resizes a window after it opens, and a browser
//! settles its canvas a moment after load. Text cached during that time is
//! cached at a scale the screen is about to leave, so it waits. Once the scale
//! holds still, text goes on, the [glyph gate](crate::text::enable_glyph_gate)
//! holds back what it has not cached, and every frame caches all of it. The
//! first frame that holds nothing back is the screen whole: the preloader
//! lifts, and from then on glyphs are cached a few milliseconds a frame.
//!
//! A frame calls [`Startup::begin`] before drawing, reads
//! [`Startup::text`] to know whether to draw text, and calls
//! [`Startup::finish`] after its last draw. Time comes in as `dt`, so the
//! phases are unit-tested and stepped like everything else in the kit.

use macroquad::prelude::*;

use crate::preload::Preloader;
use crate::text::{GlyphFlush, flush_glyphs};

/// Frames and seconds the pixel scale must hold still before text goes on.
/// The time floor matters: a window manager's resize can arrive a second
/// after launch.
pub const SETTLE_FRAMES: u32 = 30;
pub const SETTLE_SECONDS: f32 = 1.2;

/// New text sizes a live frame warms: one size is a few milliseconds in a
/// release build, so a view needing several fills in over as many frames.
pub const LIVE_SIZES_PER_FRAME: usize = 1;

/// Where a screen is between opening and drawing whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// The window is still finding its size: no text, so no glyph is cached
    /// at a scale it is about to leave.
    Settling { stable_frames: u32 },
    /// Text is on and the glyph gate holds back what it has not cached; each
    /// frame caches everything it was asked for.
    Warming,
    /// The screen draws whole. Sizes a new view needs are warmed
    /// [`LIVE_SIZES_PER_FRAME`] at a time, so it fills in rather than pausing.
    Live,
}

impl Phase {
    /// Whether a frame in this phase draws text.
    pub fn text(self) -> bool {
        !matches!(self, Self::Settling { .. })
    }

    pub fn live(self) -> bool {
        self == Self::Live
    }

    /// The size limit to hand [`flush_glyphs`]: none unless live.
    pub fn glyph_sizes(self) -> Option<usize> {
        self.live().then_some(LIVE_SIZES_PER_FRAME)
    }

    /// Before drawing: whether the pixel scale changed, and how long the
    /// screen has been open.
    pub fn observe_scale(&mut self, scale_changed: bool, clock: f32) {
        *self = match *self {
            _ if scale_changed => Self::Settling { stable_frames: 0 },
            Self::Settling { stable_frames }
                if stable_frames + 1 >= SETTLE_FRAMES && clock >= SETTLE_SECONDS =>
            {
                Self::Warming
            }
            Self::Settling { stable_frames } => Self::Settling {
                stable_frames: stable_frames + 1,
            },
            phase => phase,
        };
    }

    /// After the frame's glyph flush: go live once `ready` (the screen's data
    /// is in) and nothing was held back. True on the frame it does.
    pub fn observe_flush(&mut self, flushed: GlyphFlush, ready: bool) -> bool {
        let whole = *self == Self::Warming && ready && flushed == GlyphFlush::default();
        if whole {
            *self = Self::Live;
        }
        whole
    }
}

/// What a frame's [`Startup::finish`] did, for a frame log.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Finished {
    pub flushed: GlyphFlush,
    /// This frame drew the screen whole for the first time since settling.
    pub went_live: bool,
}

/// Drives the [`Phase`]s, the glyph flush and the [`Preloader`] for a screen.
#[derive(Clone, Debug)]
pub struct Startup {
    phase: Phase,
    preloader: Preloader,
    scale: Option<f32>,
    clock: f32,
    /// A capture's scale is fixed, so a scale change never sends it back.
    fixed: bool,
}

impl Startup {
    /// A screen opening in a window: settle, warm behind the preloader, then
    /// show.
    pub fn new() -> Self {
        Self {
            phase: Phase::Settling { stable_frames: 0 },
            preloader: Preloader::new(),
            scale: None,
            clock: 0.0,
            fixed: false,
        }
    }

    /// A capture: its scale is fixed from the first frame, and it wants the
    /// screen itself, so there is no settling and no cover. Text is still
    /// held back for the frame its glyphs are cached in.
    pub fn capture() -> Self {
        Self {
            phase: Phase::Warming,
            preloader: Preloader::lifted(),
            scale: None,
            clock: 0.0,
            fixed: true,
        }
    }

    /// Before drawing. `pixel_scale` is device pixels per design unit this
    /// frame. Returns whether it changed, for a frame log.
    pub fn begin(&mut self, dt: f32, pixel_scale: f32) -> bool {
        self.clock += dt;
        let changed = self
            .scale
            .is_none_or(|scale| (scale - pixel_scale).abs() > 0.001);
        self.scale = Some(pixel_scale);
        self.phase.observe_scale(changed && !self.fixed, self.clock);
        changed
    }

    /// Whether this frame draws text.
    pub fn text(&self) -> bool {
        self.phase.text()
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// After the frame's last text: cache what the gate held back, move the
    /// phase on, and advance the preloader. `ready` is the screen's word that
    /// its data is in. Draw the cover with [`draw_cover`](Self::draw_cover)
    /// after this.
    pub fn finish(&mut self, dt: f32, ready: bool) -> Finished {
        let flushed = flush_glyphs(self.phase.glyph_sizes());
        let went_live = self.phase.observe_flush(flushed, ready);
        self.preloader.update(dt, self.phase.live());
        Finished { flushed, went_live }
    }

    /// The preloader's cover, over `area`; see [`Preloader::draw`].
    pub fn draw_cover(&self, area: Rect, top: Color, bottom: Color, accent: Color) {
        self.preloader.draw(area, top, bottom, accent);
    }
}

impl Default for Startup {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    #[test]
    fn text_waits_for_the_time_floor_as_well_as_the_frame_count() {
        let mut phase = Phase::Settling { stable_frames: 0 };
        let mut clock = 0.0;
        let mut frames = 0;
        while !phase.text() {
            clock += DT;
            frames += 1;
            phase.observe_scale(false, clock);
            assert!(frames < 1000, "never settled");
        }
        assert_eq!(phase, Phase::Warming);
        assert!(frames as f32 * DT >= SETTLE_SECONDS, "{frames} frames");
    }

    #[test]
    fn it_goes_live_only_when_the_data_is_in_and_nothing_was_held_back() {
        let mut phase = Phase::Warming;
        let held = GlyphFlush {
            cached: 6,
            remaining: 0,
        };
        assert!(!phase.observe_flush(held, true), "glyphs were held back");
        assert!(
            !phase.observe_flush(GlyphFlush::default(), false),
            "no data"
        );
        assert!(phase.observe_flush(GlyphFlush::default(), true));
        assert!(phase.live());
        assert_eq!(phase.glyph_sizes(), Some(LIVE_SIZES_PER_FRAME));
        assert!(!phase.observe_flush(GlyphFlush::default(), true), "once");
    }

    #[test]
    fn a_resize_while_live_hides_text_until_the_scale_settles_again() {
        let mut phase = Phase::Live;
        phase.observe_scale(true, 30.0);
        assert!(!phase.text());
        assert_eq!(phase.glyph_sizes(), None);
        phase.observe_scale(false, 30.1);
        assert_eq!(phase, Phase::Settling { stable_frames: 1 });
    }

    #[test]
    fn a_window_settles_after_its_last_resize_not_its_first_frame() {
        let mut startup = Startup::new();
        assert!(startup.begin(DT, 2.0), "the first frame is a change");
        // Well past the time floor, so only the frame count is in play.
        for _ in 0..90 {
            startup.begin(DT, 2.0);
        }
        // The window manager tiles the window late.
        assert!(startup.begin(DT, 2.31));
        assert!(!startup.text());
        for _ in 0..SETTLE_FRAMES - 1 {
            startup.begin(DT, 2.31);
        }
        assert!(!startup.text(), "one frame short");
        startup.begin(DT, 2.31);
        assert!(startup.text());
    }

    #[test]
    fn a_capture_draws_text_from_its_first_frame_and_ignores_its_scale() {
        let mut startup = Startup::capture();
        startup.begin(DT, 2.0);
        assert!(startup.text());
        startup.begin(DT, 3.0);
        assert!(startup.text());
    }
}
