//! What the frontend is costing right now, as one console line.
//!
//! # Why this exists as a log and not only a HUD
//!
//! [`crate::perf_strip::PerfStrip`] shows the same readings on screen, which is
//! the right shape while you are looking at the app. It is the wrong shape for
//! the two questions that actually get asked about a slow frontend:
//!
//! - *"It burns CPU when I am not even looking at it."* A HUD cannot answer
//!   that, because reading it requires looking at it. A log line can: if the
//!   lines keep arriving while the tab is in the background, the app is
//!   painting when it should be asleep — and if they stop, the cost is
//!   somewhere other than the frame loop.
//! - *"Where is the memory?"* A single total says nothing actionable. These
//!   are split by the three places it actually accumulates, which have three
//!   different fixes.
//!
//! # The three memory figures, and why they are separate
//!
//! - **wasm** — linear memory, a HIGH-WATER MARK. It never shrinks: freeing a
//!   large decode returns it to the Rust allocator, which keeps the pages. So
//!   this only ever rises, and a flat line after a big load means "reusing
//!   what we took", not "leaked". It is still the number the browser charges
//!   us for.
//! - **textures** — decoded pixels egui holds, width × height × 4 each. The
//!   one the app controls directly, by deciding what size to decode at and
//!   what to keep.
//! - **image bytes** — compressed bytes the fetch loader is holding. Nearly
//!   always the smallest of the three, and the cheapest to keep; worth showing
//!   precisely so it can be ruled out rather than guessed at.
//!
//! Splitting them is the point. A frontend sitting at 776 MB is a different
//! problem depending on whether that is textures (decode smaller), wasm
//! (a transient that permanently raised the mark) or neither.

use std::time::Duration;

/// Per-frame sampler that reports periodically.
///
/// Hold one on the app and call [`Vitals::tick`] once per frame.
pub struct Vitals {
    every: Duration,
    /// `None` until the first tick, so the first report is a full interval in
    /// rather than immediately on load — when nothing has happened yet and
    /// every figure is meaningless.
    last_report: Option<f64>,
    frames_since: u32,
    /// Named things that happened since the last report, shown as rates.
    ///
    /// A `Vec` rather than a map: there are a handful of these, insertion
    /// order is the order they read best in, and a linear scan over five
    /// entries beats hashing on a path that runs per event.
    ///
    /// These exist because the frame rate alone cannot explain cost that is
    /// NOT frame-driven. A backgrounded tab suspends `requestAnimationFrame`
    /// and drops to a fraction of a frame per second — so anything still
    /// burning CPU at that point is arriving from somewhere else, and the only
    /// way to see it is to count it where it arrives.
    counts: Vec<(&'static str, u32)>,
}

impl Default for Vitals {
    fn default() -> Self {
        Self::every(Duration::from_secs(5))
    }
}

impl Vitals {
    /// Report at most this often. Five seconds by default: frequent enough to
    /// watch a number climb, rare enough to leave the console readable.
    pub fn every(every: Duration) -> Self {
        Self {
            every,
            last_report: None,
            frames_since: 0,
            counts: Vec::new(),
        }
    }

    /// Record one occurrence of `name`, reported as a per-second rate.
    ///
    /// Call it wherever the work actually happens — a socket message handled,
    /// a decode started. Cheap enough to sit on a hot path: a scan of a
    /// handful of entries and an increment.
    pub fn count(&mut self, name: &'static str) {
        match self.counts.iter_mut().find(|(known, _)| *known == name) {
            Some((_, n)) => *n += 1,
            None => self.counts.push((name, 1)),
        }
    }

    /// Count this frame, and report if the interval has elapsed.
    ///
    /// The frame RATE is the reading that makes the rest interpretable, and it
    /// is measured rather than assumed: egui only paints when something asks
    /// it to, so "60" and "0.2" are both normal and mean opposite things about
    /// whether the app is resting.
    pub fn tick(&mut self, ctx: &egui::Context) {
        self.frames_since += 1;

        let now = ctx.input(|i| i.time);
        let last = *self.last_report.get_or_insert(now);
        let elapsed = now - last;
        if elapsed < self.every.as_secs_f64() {
            return;
        }

        let fps = self.frames_since as f64 / elapsed.max(f64::EPSILON);
        self.last_report = Some(now);
        self.frames_since = 0;

        let snapshot = Snapshot::now(ctx);
        let textures = snapshot.textures;
        let wasm = snapshot.wasm_bytes;

        // Frame COST, not just cadence. The two answer different questions and
        // only together say whether there is a problem: 10 fps at 0.4ms a
        // frame is an app resting politely, 10 fps at 12ms is one that would
        // stutter the moment anything else asked for a frame. Fed by
        // [`crate::perf_strip::FrameScope`], so it reads zero unless the host
        // has one at the top of its frame.
        let frames = snapshot.frames;
        let build = if frames.samples == 0 {
            " · build n/a (no FrameScope)".to_string()
        } else {
            format!(
                " · build p50 {:.1}ms p95 {:.1}ms max {:.1}ms",
                frames.build_p50_ms, frames.build_p95_ms, frames.build_max_ms
            )
        };

        let mut rates = String::new();
        for (name, n) in self.counts.drain(..) {
            let per_sec = n as f64 / elapsed.max(f64::EPSILON);
            rates.push_str(&format!(" · {name} {per_sec:.1}/s"));
        }

        log::info!(
            "vitals: {fps:.1} fps{build} · wasm {} · textures {} ({}, {} avg) · image bytes {}{rates}",
            wasm.map(bytes).unwrap_or_else(|| "n/a".into()),
            bytes(textures.bytes as u64),
            textures.count,
            // See `TextureUse::mean_bytes` for why the average is the figure
            // that matters here.
            bytes(textures.mean_bytes() as u64),
            bytes(snapshot.image_bytes as u64),
        );
    }
}

/// Decoded texture memory egui is holding.
#[derive(Debug, Clone, Copy)]
pub struct TextureUse {
    pub bytes: usize,
    pub count: usize,
}

impl TextureUse {
    /// Mean bytes per texture — **the number that says whether decoding is
    /// sized right**, and the one a total cannot substitute for.
    ///
    /// 64KB is a 128px tile, 640KB a 400px card, 11MB a full-resolution decode
    /// that slipped through unhinted. A grid whose average sits in the
    /// megabytes is not holding too many images, it is holding the wrong size
    /// of them — which is a different fix, and the one that was needed when
    /// `card_browser` was passing `SizeHint::default()`.
    pub fn mean_bytes(self) -> usize {
        self.bytes.checked_div(self.count).unwrap_or(0)
    }
}

/// Everything [`Vitals`] reports, readable at any instant rather than only on
/// its logging cadence.
///
/// Every figure here is global — egui's texture manager, `perf_probe`'s frame
/// window, wasm linear memory — so a surface that wants to display them needs
/// nothing plumbed through from the host. The only thing it cannot see is the
/// host's named counters, which are the app's own.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub textures: TextureUse,
    /// `None` off wasm, where there is no linear memory to measure.
    pub wasm_bytes: Option<u64>,
    /// Compressed bytes the fetch loader holds — an order of magnitude below
    /// the textures they decoded into, which is why a cap on THIS alone never
    /// controlled memory.
    pub image_bytes: usize,
    pub frames: perf_probe::FrameStats,
}

impl Snapshot {
    pub fn now(ctx: &egui::Context) -> Self {
        Self {
            textures: texture_bytes(ctx),
            wasm_bytes: perf_probe::mem::linear_memory_bytes(),
            image_bytes: image_bytes(ctx),
            frames: perf_probe::frame_stats(),
        }
    }

    /// One line, inline-labelled, for a compact capture — see
    /// [`crate::flame_chart::report_compact`]. Self-describing rather than
    /// needing the legend, because it is appended after it.
    pub fn compact_line(&self) -> String {
        format!(
            "V tex={} n={} avg={} wasm={} img={} fps={:.1} p50={:.1}ms p95={:.1}ms",
            bytes(self.textures.bytes as u64),
            self.textures.count,
            bytes(self.textures.mean_bytes() as u64),
            self.wasm_bytes.map(bytes).unwrap_or_else(|| "n/a".into()),
            bytes(self.image_bytes as u64),
            self.frames.fps,
            self.frames.build_p50_ms,
            self.frames.build_p95_ms,
        )
    }
}

/// Byte counts, formatted the way `Vitals` formats them.
pub fn format_bytes(n: u64) -> String {
    bytes(n)
}

/// What egui's texture manager currently holds.
///
/// Summed per texture: `bytes_used` belongs to `TextureMeta` (width × height ×
/// bytes-per-pixel) and the manager keeps no total of its own.
pub fn texture_bytes(ctx: &egui::Context) -> TextureUse {
    let textures = ctx.tex_manager();
    let textures = textures.read();
    TextureUse {
        bytes: textures
            .allocated()
            .map(|(_, meta)| meta.bytes_used())
            .sum(),
        count: textures.num_allocated(),
    }
}

/// Compressed bytes the image fetch loader is holding, where it is installed.
fn image_bytes(_ctx: &egui::Context) -> usize {
    #[cfg(target_arch = "wasm32")]
    {
        crate::image_loader::fetch::loads(_ctx)
            .map(|loads| loads.byte_size())
            .unwrap_or(0)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        0
    }
}

/// Three significant figures and a unit. These are read at a glance next to
/// each other, so they are formatted the same way.
fn bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_are_scaled_to_a_readable_unit() {
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(2048), "2.0 KB");
        assert_eq!(bytes(776 * 1024 * 1024), "776.0 MB");
        assert_eq!(bytes(3 * 1024 * 1024 * 1024), "3.0 GB");
    }
}
