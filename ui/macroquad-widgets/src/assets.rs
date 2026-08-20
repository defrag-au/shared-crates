//! Loading external resources with progress, the way a game boots.
//!
//! macroquad fetches over the network on wasm, so every font, texture and data
//! file is a round trip that happens *after* the wasm is already running. A
//! surface with more than one of them needs to answer two questions the player
//! is asking within the first second: is anything happening, and how much
//! longer.
//!
//! [`Fonts`] answers neither, on purpose — it degrades to the built-in face and
//! swaps when the real one lands, which is right when a font is the only thing
//! outstanding. It stops being right the moment a surface cannot draw itself
//! until its textures arrive.
//!
//! # Progressive or gated is the caller's decision
//!
//! This reports progress; it does not impose a loading screen. A lobby whose
//! only asset is a font should keep drawing and swap the face silently; a game
//! that needs its sprite sheet should hold on [`Loader::complete`] first.
//! Gating unconditionally would put a flash of loading screen in front of
//! surfaces that never needed one, which is worse than the wait it replaces.
//!
//! # Failures are not fatal
//!
//! A missing asset counts toward completion and is reported in
//! [`Progress::failed`]. A surface that refuses to start because one texture
//! 404'd is a surface that is down; one that starts with a placeholder is a
//! surface with a visible bug, which is strictly better.

use std::collections::HashMap;

use macroquad::experimental::coroutines::{start_coroutine, Coroutine};
use macroquad::prelude::*;

use crate::fonts::{FontFiles, Fonts, Slot};

/// How far along a load is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    /// Assets resolved — loaded *or* failed.
    pub done: usize,
    pub total: usize,
    /// How many of `done` failed. Non-zero means the surface is running with
    /// something missing.
    pub failed: usize,
}

impl Progress {
    /// `0.0..=1.0`. An empty load is complete, not divided by zero.
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            return 1.0;
        }
        self.done as f32 / self.total as f32
    }

    pub fn complete(&self) -> bool {
        self.done >= self.total
    }

    /// Sum of two progresses, for a loader made of parts.
    pub fn and(self, other: Progress) -> Progress {
        Progress {
            done: self.done + other.done,
            total: self.total + other.total,
            failed: self.failed + other.failed,
        }
    }
}

/// One thing to fetch.
///
/// URLs are resolved by macroquad relative to the page on wasm, so a caller
/// decides absolute-vs-relative — which matters inside a Discord Activity,
/// where only same-origin paths are permitted.
pub enum Asset {
    Font { slot: Slot, url: String },
    /// An image, keyed for later lookup.
    Texture { key: String, url: String },
    /// Anything else — a level, a config, a sound file the caller decodes.
    Bytes { key: String, url: String },
}

impl Asset {
    pub fn font(slot: Slot, url: impl Into<String>) -> Self {
        Self::Font {
            slot,
            url: url.into(),
        }
    }

    pub fn texture(key: impl Into<String>, url: impl Into<String>) -> Self {
        Self::Texture {
            key: key.into(),
            url: url.into(),
        }
    }

    pub fn bytes(key: impl Into<String>, url: impl Into<String>) -> Self {
        Self::Bytes {
            key: key.into(),
            url: url.into(),
        }
    }
}

enum Pending {
    Texture(String, Coroutine<Result<Texture2D, macroquad::Error>>),
    Bytes(String, Coroutine<Result<Vec<u8>, macroquad::Error>>),
}

/// Fetches a set of assets, reporting progress.
#[derive(Default)]
pub struct Loader {
    fonts: Fonts,
    textures: HashMap<String, Texture2D>,
    blobs: HashMap<String, Vec<u8>>,
    pending: Vec<Pending>,
    non_font_total: usize,
    non_font_failed: usize,
    /// Set once every asset has resolved, so a caller can tell "nothing to
    /// load" from "not started".
    started: bool,
}

impl Loader {
    /// Begin fetching. Returns immediately; poll with [`Loader::update`].
    pub fn start(assets: Vec<Asset>) -> Self {
        let mut files = FontFiles::default();
        let mut pending = Vec::new();

        for asset in assets {
            match asset {
                Asset::Font { slot, url } => match slot {
                    Slot::Regular => files.regular = Some(url),
                    Slot::Bold => files.bold = Some(url),
                    Slot::Mono => files.mono = Some(url),
                },
                Asset::Texture { key, url } => {
                    pending.push(Pending::Texture(
                        key,
                        start_coroutine(async move { load_texture(url.as_str()).await }),
                    ));
                }
                Asset::Bytes { key, url } => {
                    pending.push(Pending::Bytes(
                        key,
                        start_coroutine(async move { load_file(url.as_str()).await }),
                    ));
                }
            }
        }

        Self {
            non_font_total: pending.len(),
            pending,
            fonts: Fonts::start(files),
            started: true,
            ..Default::default()
        }
    }

    /// Poll once per frame. Adopts whatever has arrived and returns the
    /// progress *after* doing so, so a caller can act on completion the same
    /// frame rather than the next one.
    pub fn update(&mut self) -> Progress {
        self.fonts.update();

        let mut still_pending = Vec::new();
        for item in std::mem::take(&mut self.pending) {
            match item {
                Pending::Texture(key, co) => match co.retrieve() {
                    None => still_pending.push(Pending::Texture(key, co)),
                    Some(Ok(texture)) => {
                        self.textures.insert(key, texture);
                    }
                    Some(Err(e)) => {
                        macroquad::logging::error!("{}", format!("asset {key} failed: {e:?}"));
                        self.non_font_failed += 1;
                    }
                },
                Pending::Bytes(key, co) => match co.retrieve() {
                    None => still_pending.push(Pending::Bytes(key, co)),
                    Some(Ok(bytes)) => {
                        self.blobs.insert(key, bytes);
                    }
                    Some(Err(e)) => {
                        macroquad::logging::error!("{}", format!("asset {key} failed: {e:?}"));
                        self.non_font_failed += 1;
                    }
                },
            }
        }
        self.pending = still_pending;

        self.progress()
    }

    pub fn progress(&self) -> Progress {
        let non_font = Progress {
            done: self.non_font_total - self.pending.len(),
            total: self.non_font_total,
            failed: self.non_font_failed,
        };
        self.fonts.progress().and(non_font)
    }

    /// Whether everything has resolved, successfully or not.
    pub fn complete(&self) -> bool {
        self.started && self.progress().complete()
    }

    pub fn fonts(&self) -> &Fonts {
        &self.fonts
    }

    pub fn texture(&self, key: &str) -> Option<&Texture2D> {
        self.textures.get(key)
    }

    pub fn bytes(&self, key: &str) -> Option<&[u8]> {
        self.blobs.get(key).map(Vec::as_slice)
    }
}

/// Draw a centred loading indicator: a label, a bar, and a count.
///
/// Deliberately plain and self-contained — it runs before the surface's own
/// assets exist, so it cannot depend on any of them. Uses the built-in face
/// when `font` is `None`, which is the usual case since the font is often
/// exactly what is still loading.
pub fn draw_loading(progress: Progress, label: &str, font: Option<&Font>, accent: Color) {
    let cx = screen_width() * 0.5;
    let cy = screen_height() * 0.5;

    let bar_w = (screen_width() * 0.4).clamp(160.0, 420.0);
    let bar_h = 6.0;
    let bar_x = cx - bar_w * 0.5;

    let text_size = 18;
    let dims = measure_text(label, font, text_size, 1.0);
    draw_text_ex(
        label,
        cx - dims.width * 0.5,
        cy - 24.0,
        TextParams {
            font,
            font_size: text_size,
            color: Color::new(0.6, 0.6, 0.66, 1.0),
            ..Default::default()
        },
    );

    // Track, then fill. Drawn even at zero so the bar is visibly *there*
    // before anything has arrived — an empty region reads as a broken screen.
    draw_rectangle(bar_x, cy, bar_w, bar_h, Color::new(1.0, 1.0, 1.0, 0.12));
    draw_rectangle(
        bar_x,
        cy,
        bar_w * progress.fraction().clamp(0.0, 1.0),
        bar_h,
        accent,
    );

    let count = if progress.failed > 0 {
        format!(
            "{}/{}  ({} failed)",
            progress.done, progress.total, progress.failed
        )
    } else {
        format!("{}/{}", progress.done, progress.total)
    };
    let count_dims = measure_text(&count, font, 14, 1.0);
    draw_text_ex(
        &count,
        cx - count_dims.width * 0.5,
        cy + 28.0,
        TextParams {
            font,
            font_size: 14,
            color: Color::new(0.45, 0.45, 0.5, 1.0),
            ..Default::default()
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_load_is_complete() {
        let p = Progress::default();
        assert_eq!(p.fraction(), 1.0);
        assert!(p.complete());
    }

    #[test]
    fn fraction_tracks_done_over_total() {
        let p = Progress {
            done: 1,
            total: 4,
            failed: 0,
        };
        assert_eq!(p.fraction(), 0.25);
        assert!(!p.complete());
    }

    /// A failure still counts as resolved — otherwise a gated surface waits
    /// forever on an asset that is never coming.
    #[test]
    fn failures_count_toward_completion() {
        let p = Progress {
            done: 2,
            total: 2,
            failed: 1,
        };
        assert!(p.complete());
        assert_eq!(p.fraction(), 1.0);
    }

    #[test]
    fn parts_sum() {
        let a = Progress {
            done: 1,
            total: 2,
            failed: 0,
        };
        let b = Progress {
            done: 2,
            total: 3,
            failed: 1,
        };
        assert_eq!(
            a.and(b),
            Progress {
                done: 3,
                total: 5,
                failed: 1
            }
        );
    }
}
