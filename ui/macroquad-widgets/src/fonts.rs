//! Host-owned font loading, for the faces [`crate::Painter`] borrows.
//!
//! The painter takes `font` and `mono` as `Option<&Font>` and never loads
//! them — that is deliberate, and it is what lets a web app fetch faces over
//! HTTP while the storybook embeds them. This module is the other half: the
//! loader every macroquad surface would otherwise write again.
//!
//! Sits here with [`crate::Gestures`] as a **host-owned** helper rather than a
//! widget. Both hold state across frames, which the widget charter forbids in
//! widgets precisely so that it lives somewhere explicit instead.
//!
//! # Non-blocking by design
//!
//! The fetch is a second round trip after the wasm, and a boot screen that
//! waited on it would sit blank for exactly that long. The loop starts
//! immediately and each face is adopted the frame its bytes land. macroquad's
//! built-in face is ASCII-only and rough, but it is *there* — a screen that
//! appears late is worse than one that briefly looks plain.
//!
//! # Paths are relative
//!
//! `load_ttf_font` resolves against the page, so a file sits beside the wasm
//! and reaches Discord's proxy as `<mapped root>/…`, like `gl.js`. An absolute
//! host would be blocked by the Activity CSP.

use macroquad::experimental::coroutines::{start_coroutine, Coroutine};
use macroquad::prelude::*;

/// Which face a file is being loaded for.
///
/// Three faces rather than a weight × family matrix, because that is what
/// [`crate::Painter`] takes: a proportional face, its bold, and a fixed-width
/// one. A surface needing more than this wants its own type, not a bigger enum.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Slot {
    /// UI chrome — headings, labels, buttons.
    Regular,
    /// Emphasis. Falls back to [`Slot::Regular`] when absent.
    Bold,
    /// Fixed-width. **Not** interchangeable with the others: a terminal grid
    /// measures one glyph and positions every cell from it, so a proportional
    /// face here leaves ragged gaps in every column.
    Mono,
}

/// Which files to fetch, per slot. `None` leaves that slot on the built-in
/// face, which is a supported state rather than a degraded one — a surface
/// that only needs mono should not pull two proportional faces to get it.
///
/// Paths are owned `String`s rather than `&'static str` because on the web the
/// right URL is decided at runtime: the same file is reached through a proxy
/// prefix inside a Discord Activity and from an asset host directly outside
/// one, and neither is known at compile time.
#[derive(Default, Clone)]
pub struct FontFiles {
    pub regular: Option<String>,
    pub bold: Option<String>,
    pub mono: Option<String>,
}

impl FontFiles {
    /// Just a monospace face — a terminal-styled surface.
    pub fn mono(file: impl Into<String>) -> Self {
        Self {
            mono: Some(file.into()),
            ..Default::default()
        }
    }

    /// A regular/bold proportional pair — ordinary UI chrome.
    pub fn ui(regular: impl Into<String>, bold: impl Into<String>) -> Self {
        Self {
            regular: Some(regular.into()),
            bold: Some(bold.into()),
            ..Default::default()
        }
    }

    pub fn with_mono(mut self, file: impl Into<String>) -> Self {
        self.mono = Some(file.into());
        self
    }
}

/// Faces fetched beside the wasm, adopted as they arrive.
#[derive(Default)]
pub struct Fonts {
    regular: Option<Font>,
    bold: Option<Font>,
    mono: Option<Font>,
    pending: Vec<(Slot, Coroutine<Result<Font, macroquad::Error>>)>,
    /// How many were asked for, kept so progress can be reported after
    /// `pending` has drained.
    total: usize,
    failed: usize,
}

impl Fonts {
    /// Start fetching. Returns immediately; poll with [`Fonts::update`].
    pub fn start(files: FontFiles) -> Self {
        let load =
            |file: String| start_coroutine(async move { load_ttf_font(file.as_str()).await });

        let mut pending = Vec::new();
        for (slot, file) in [
            (Slot::Regular, files.regular),
            (Slot::Bold, files.bold),
            (Slot::Mono, files.mono),
        ] {
            if let Some(file) = file {
                pending.push((slot, load(file)));
            }
        }

        Self {
            total: pending.len(),
            pending,
            ..Default::default()
        }
    }

    /// How far along the fetches are, for [`crate::assets::Loader`].
    pub fn progress(&self) -> crate::assets::Progress {
        crate::assets::Progress {
            done: self.total - self.pending.len(),
            total: self.total,
            failed: self.failed,
        }
    }

    /// Poll once per frame; adopts each face as it lands.
    pub fn update(&mut self) {
        let mut failed = 0;
        self.pending.retain(|(slot, co)| match co.retrieve() {
            None => true,
            Some(Ok(font)) => {
                match slot {
                    Slot::Regular => self.regular = Some(font),
                    Slot::Bold => self.bold = Some(font),
                    Slot::Mono => self.mono = Some(font),
                }
                false
            }
            // Not fatal — the built-in face carries on. Loud in the console
            // because a missing font in production is a deploy that forgot the
            // file, not something a player should have to report.
            Some(Err(e)) => {
                macroquad::logging::error!("{}", format!("a UI font failed to load: {e:?}"));
                failed += 1;
                false
            }
        });
        self.failed += failed;
    }

    /// Whether every requested face has resolved, one way or the other.
    ///
    /// For a surface that wants to measure a grid once rather than every
    /// frame: measurements taken before this is true describe the built-in
    /// face and will be wrong afterwards.
    pub fn settled(&self) -> bool {
        self.pending.is_empty()
    }

    /// The face for a slot, or `None` while it is in flight or absent.
    ///
    /// `Bold` falls back to the proportional face; `Mono` never falls back,
    /// because a proportional substitute is worse than the built-in fixed
    /// width face it would replace.
    pub fn face(&self, slot: Slot) -> Option<&Font> {
        match slot {
            Slot::Regular => self.regular.as_ref(),
            Slot::Bold => self.bold.as_ref().or(self.regular.as_ref()),
            Slot::Mono => self.mono.as_ref(),
        }
    }

    pub fn regular(&self) -> Option<&Font> {
        self.face(Slot::Regular)
    }

    pub fn bold(&self) -> Option<&Font> {
        self.face(Slot::Bold)
    }

    pub fn mono(&self) -> Option<&Font> {
        self.face(Slot::Mono)
    }

    pub fn params(&self, slot: Slot, size: f32, colour: Color) -> TextParams<'_> {
        TextParams {
            font: self.face(slot),
            font_size: size as u16,
            color: colour,
            ..Default::default()
        }
    }
}
