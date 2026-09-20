//! `Profiler` — the in-app flame chart panel: which frame, what it cost, and a capture you can paste.
//!
//! Feature-gated (`puffin`), because it carries a profiler. Lives here rather
//! than in one frontend because the second app that needed it wanted exactly
//! the same panel, and a 300-line window copied between them would drift
//! within a week.
//!
//! A host supplies three things: a [`Profiler`] on its app state, a call to
//! [`Profiler::ui`] drawn LAST, and a call to [`Profiler::end_frame`] after
//! it. It must also put [`Profiler::toggle`] somewhere persistent — the window
//! closes, and without a way back that is a one-way door.
//!
//! # What this is for
//!
//! `vitals` says a frame costs 4.2ms. It cannot say what the 4.2ms was. This
//! does: every scope in the frame, nested, on a zoomable axis.
//!
//! # Which frame you are looking at
//!
//! "Latest" is a poor default: it is replaced before it can be read, and it is
//! usually a cheap frame, because expensive frames are by definition the
//! minority — which is exactly what makes them worth finding. So the panel
//! opens on [`Showing::Busiest`] and carries a strip of the last [`HISTORY`]
//! frames that can be clicked to [`Showing::Pinned`] one and hold it still.
//!
//! Holding one still is the point. An outlier only means something next to its
//! neighbours, and a frame that moves under the reader cannot be compared with
//! anything.
//!
//! # The instrument must not move the needle
//!
//! Nothing here requests a repaint unless "Drive frames" is ticked, and it is
//! not ticked by default. An earlier version drove the frame loop
//! unconditionally so there would always be something to sample; the effect
//! was that every frame-rate reading taken with the panel open was a reading
//! of the panel. An app that rests and yields no frames is not a broken
//! capture — it is the measurement.
//!
//! # The cost of measuring
//!
//! puffin timestamps every scope, so this is not free and is not shipped:
//! the feature is off by default and `cargo tree` on a normal build reports no
//! puffin at all. Even enabled, `set_scopes_on(false)` stops collection
//! without rebuilding, which is what the capture toggle does.
//!
//! # Getting a capture out
//!
//! "Copy" puts the shown frame on the clipboard as text — self time per scope,
//! then the tree. A flame chart is read by pointing at it, which makes it
//! useless in a bug report; the same data as text can be searched, diffed
//! between two builds, and pasted into a conversation.

use crate::flame_chart::{FlameChart, Span};
use crate::theme::{Ink, InkExt, TextSize, ThemeExt, Token};

/// Frames kept on the history strip. Enough to see a pattern — a spike every
/// tenth frame is a different problem from one slow frame — without the bars
/// becoming too narrow to hit.
const HISTORY: usize = 60;
const STRIP_HEIGHT: f32 = 28.0;
/// Manually kept frames held at once. A debugging aid, so it is bounded —
/// an unbounded one becomes its own memory problem in a session left open.
const KEPT_MAX: usize = 8;

/// `HH:MM:SS` of a puffin timestamp, in UTC.
///
/// puffin reports nanoseconds since the unix epoch, so the seconds fall out
/// directly. UTC rather than local: this is a label for telling two specimens
/// apart, not a clock, and a timezone would be one more thing that could be
/// wrong in a pasted report.
fn time_of_day(start_ns: i64) -> String {
    let secs = start_ns.div_euclid(1_000_000_000).rem_euclid(86_400);
    let (h, m, s) = (secs / 3600, secs % 3600 / 60, secs % 60);
    format!("{h:02}:{m:02}:{s:02}")
}

/// Which frame the chart is showing.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Showing {
    /// The frame that did the most, still in puffin's buffer.
    ///
    /// Ranked by SCOPE COUNT, not by duration, and the distinction matters
    /// once the profiler stops driving the frame loop. A puffin frame spans
    /// the wall clock between repaints, so a resting app's frames are half a
    /// second of nothing; picking the longest would reliably select the
    /// longest PAUSE and show an empty chart. Scope count is on `FrameMeta`,
    /// so ranking costs no unpacking.
    Busiest,
    /// Whatever was drawn most recently.
    Latest,
    /// A frame the reader picked off the history strip, held until they pick
    /// another.
    ///
    /// The mode that makes the panel a diagnostic rather than a dashboard:
    /// "latest" is gone before it can be read, and an outlier is only useful
    /// if it can be kept still and compared with its neighbours.
    Pinned(u64),
}

pub struct Profiler {
    /// Registers a sink with the global profiler on construction and removes
    /// it on drop, so simply holding this is what makes frames available.
    view: puffin::GlobalFrameView,
    open: bool,
    capturing: bool,
    /// Whether to keep asking for frames.
    ///
    /// OFF by default, and that is the whole point. A profiler that drives the
    /// frame loop pins the app at display rate and then reports the rate it
    /// caused — the first version did exactly that, and every reading taken
    /// with the panel open was a reading of the panel.
    ///
    /// Left off, the app paints as it otherwise would: idle means no frames,
    /// which is the honest answer to "what does this cost at rest". Turn it on
    /// to sample a surface that is genuinely busy, knowing the rate is now
    /// yours and not the app's.
    driving: bool,
    showing: Showing,
    /// The busiest frame seen since the panel opened, held so it OUTLIVES
    /// puffin's ring.
    ///
    /// Without this, "busiest" means "busiest of the last few dozen", and the
    /// spike worth looking at is gone by the time anyone reaches for it —
    /// which is exactly when a reader goes looking. Holding an `Arc` keeps the
    /// frame alive after eviction; frames are reference-counted, so this costs
    /// one pointer plus the frame nobody else is holding any more.
    ///
    /// Reset explicitly, because a high-water mark that quietly decayed would
    /// be a different measurement every time it was read.
    high_water: Option<std::sync::Arc<puffin::FrameData>>,
    /// Only frames NEWER than this count towards the peak.
    ///
    /// Clearing `high_water` alone does not reset anything: the frame that set
    /// it is still sitting in puffin's ring, so the very next refresh scans
    /// the ring, finds it again and re-adopts it. The button appeared to do
    /// nothing because it did nothing. A reset has to move a floor, not empty
    /// a slot.
    peak_since: u64,
    /// Frames the reader chose to hold onto, for comparing two of them.
    kept: Vec<std::sync::Arc<puffin::FrameData>>,
    /// The spans currently drawn, and which frame they came from.
    ///
    /// Cached because converting a frame allocates a `Span` per scope — a few
    /// thousand — and doing that every frame would make the profiler the most
    /// expensive thing in the profile. It is rebuilt only when the frame being
    /// shown actually changes.
    shown: Option<(u64, Vec<Span>)>,
}

impl Default for Profiler {
    fn default() -> Self {
        // Loud, once, at error level. This build can reach a real deployment
        // — `PROFILING=1 wrangler deploy` is the whole point of it — and the
        // one thing that must not happen is it quietly STAYING there. The
        // panel is open by default and hard to miss, but a console line
        // survives someone closing it.
        log::error!(
            "PROFILING BUILD — puffin is collecting and the app holds a frame open to \
             feed the flame chart, so it idles hotter than a normal build. Redeploy \
             without PROFILING=1 when finished."
        );
        // Collection starts ON: a profiler you have to remember to switch on
        // records nothing for the first interesting thing you do.
        puffin::set_scopes_on(true);
        Self {
            view: puffin::GlobalFrameView::default(),
            open: true,
            capturing: true,
            driving: false,
            showing: Showing::Busiest,
            high_water: None,
            peak_since: 0,
            kept: Vec::new(),
            shown: None,
        }
    }
}

impl Profiler {
    /// Close the frame puffin is recording into. Call once per frame, last.
    ///
    /// Without it puffin never sees a frame boundary, and every scope in the
    /// session accumulates into one frame that is never delivered to a sink —
    /// which looks exactly like the profiler being broken.
    pub fn end_frame(&self) {
        profiling::finish_frame!();
    }

    /// Whether the panel is showing — for a host drawing a toggle.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Show or hide the panel.
    ///
    /// A host MUST offer this somewhere persistent. The window has a close
    /// button, and without a way back that button is a one-way door: the panel
    /// is gone for the session and the only route to it is a reload, which
    /// throws away the capture that prompted closing it in the first place.
    pub fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        // SCOPED, because the panel is drawn inside the frame it measures and
        // therefore shows up in its own captures. Without this its cost lands
        // on `App::ui` as unattributed self time — a capture came back with
        // 1.9ms of `text_layout::layout` sitting directly under `App::ui`,
        // after `ui::draw` had returned, with nothing else drawn there but
        // this window. An instrument that cannot be seen in its own output
        // cannot be subtracted from it.
        profiling::function_scope!();

        // `open` is lifted out of `self` for the call: `Window::open` holds a
        // mutable borrow for the whole builder, and the closure below needs
        // `self` too. Copied back after.
        let mut open = self.open;
        // Raised inside the closure, acted on after: keeping a frame needs the
        // frame, and `self.shown` is borrowed for drawing while the button is
        // being drawn next to it.
        let mut keep = false;
        let mut reset_peak = false;
        // Raised in the kept-frames list, acted on after the window closes.
        let mut view_kept = None;
        let mut copy_kept = None;
        let mut drop_kept = None;
        egui::Window::new("Profiler")
            .open(&mut open)
            .default_size([900.0, 400.0])
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.checkbox(&mut self.capturing, "Capture").changed() {
                        // Stops the timestamping without a rebuild, so the
                        // cost can be taken out of the picture while leaving
                        // the last capture on screen to read.
                        puffin::set_scopes_on(self.capturing);
                    }
                    ui.checkbox(&mut self.driving, "Drive frames")
                        .on_hover_text(
                            "Keep asking for repaints so there is always something to \
                             sample. Off by default: with it on the app paints at display \
                             rate and the frame RATE you read is this panel's, not the \
                             app's. The per-scope costs stay honest either way.",
                        );
                    ui.separator();
                    ui.selectable_value(&mut self.showing, Showing::Busiest, "Busiest frame")
                        .on_hover_text(
                            "The busiest frame since this panel opened — RETAINED, so it \
                             survives puffin's ring. Reset to start watching again.",
                        );
                    ui.selectable_value(&mut self.showing, Showing::Latest, "Latest frame");
                    if ui
                        .button("Reset peak")
                        .on_hover_text("Forget the retained busiest frame and watch afresh.")
                        .clicked()
                    {
                        reset_peak = true;
                    }
                });

                // Asked for from INSIDE the panel, so it stops the moment the
                // panel is closed — a driver that outlived its window would be
                // an invisible hand on the frame rate.
                if self.driving {
                    ui.ctx().request_repaint();
                }

                Self::vitals_strip(ui);
                Self::repaint_strip(ui);

                if let Some(picked) = self.history_strip(ui) {
                    self.showing = Showing::Pinned(picked);
                }

                self.refresh();

                ui.add_space(6.0);
                match &self.shown {
                    Some((_, spans)) if !spans.is_empty() => {
                        let total: i64 = spans
                            .iter()
                            .filter(|s| s.depth == 0)
                            .map(|s| s.duration_ns)
                            .sum();
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} · {} scopes",
                                    crate::flame_chart::format_duration_ns(total),
                                    spans.len()
                                ))
                                .color(Ink::Token(Token::TextMuted).of(ui))
                                .size(ui.text_size(TextSize::Sm)),
                            );
                            // A capture that cannot leave the browser can only
                            // be described, and a description of a flame chart
                            // is worth very little. This is the same data as
                            // text: searchable, diffable between two builds,
                            // and pasteable into a conversation.
                            if ui
                                .button("Copy")
                                .on_hover_text(
                                    "Copy this frame in the compact FLAME/1 encoding — \
                                     every scope, its self/max/total, and a \
                                     repeat-collapsed tree, in a fraction of the text. \
                                     Carries its own legend line, so whatever reads it \
                                     can decode it.",
                                )
                                .clicked()
                            {
                                ui.ctx().copy_text(Self::capture_text(ui.ctx(), spans));
                            }
                            if ui
                                .button("Keep")
                                .on_hover_text(
                                    "Hold this frame past puffin's ring, so it can still \
                                     be compared with a later one.",
                                )
                                .clicked()
                            {
                                keep = true;
                            }
                        });
                        ui.add_space(4.0);
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            FlameChart::new(spans).id_salt("mirror-profiler").show(ui);
                        });
                    }
                    // Distinct from "captured an empty frame": before the
                    // first frame is delivered there is nothing to say, and
                    // saying "0 scopes" would read as a broken profiler.
                    _ => {
                        ui.label(
                            egui::RichText::new(
                                "Waiting for a frame. If this does not clear, \
                                 `end_frame` is not being called.",
                            )
                            .color(Ink::Token(Token::TextMuted).of(ui))
                            .size(ui.text_size(TextSize::Sm)),
                        );
                    }
                }

                // ── kept frames ──────────────────────────────────────────
                //
                // BENEATH the chart rather than mixed into the history strip:
                // the strip is a live window that scrolls away, and these are
                // the opposite — specimens, held deliberately, that must stay
                // exactly where they were put. Timestamped, because "the slow
                // one" stops being identifiable the moment there are two.
                //
                // Snapshotted into plain values first: the rows are drawn
                // while `self` is borrowed for the buttons that mutate it.
                let rows: Vec<(u64, i64, usize)> = self
                    .kept
                    .iter()
                    .map(|frame| {
                        (
                            frame.frame_index(),
                            frame.range_ns().0,
                            frame.meta().num_scopes,
                        )
                    })
                    .collect();
                if !rows.is_empty() {
                    ui.separator();
                    ui.label(
                        egui::RichText::new(format!("Kept frames ({}/{KEPT_MAX})", rows.len()))
                            .color(Ink::Token(Token::TextMuted).of(ui))
                            .size(ui.text_size(TextSize::Sm)),
                    );
                    for (index, start_ns, scopes) in rows {
                        ui.horizontal(|ui| {
                            let on_screen = self
                                .shown
                                .as_ref()
                                .is_some_and(|(shown, _)| *shown == index);
                            ui.label(
                                egui::RichText::new(format!(
                                    "{}  ·  {scopes} scopes",
                                    time_of_day(start_ns)
                                ))
                                .color(if on_screen {
                                    Ink::Token(Token::TextPrimary).of(ui)
                                } else {
                                    Ink::Token(Token::TextMuted).of(ui)
                                })
                                .size(ui.text_size(TextSize::Sm)),
                            );
                            if ui.small_button("View").clicked() {
                                view_kept = Some(index);
                            }
                            if ui.small_button("Copy").clicked() {
                                copy_kept = Some(index);
                            }
                            // `PhosphorIcon`, not "✕" — the bare multiplication
                            // sign is not in the font and renders as tofu.
                            // `no_broken_glyphs` catches it, which is how this
                            // one was found.
                            if ui
                                .small_button(crate::icons::PhosphorIcon::X.codepoint().to_string())
                                .clicked()
                            {
                                drop_kept = Some(index);
                            }
                        });
                    }
                }
            });
        self.open = open;

        if keep {
            self.keep_shown();
        }
        if reset_peak {
            // Both halves: forget what is held AND move the floor past every
            // frame currently in the ring, so the peak is rebuilt only from
            // what happens next.
            self.high_water = None;
            self.peak_since = self
                .view
                .lock()
                .latest_frame()
                .map(|frame| frame.frame_index())
                .unwrap_or(0);
        }
        if let Some(index) = view_kept {
            self.showing = Showing::Pinned(index);
        }
        if let Some(index) = copy_kept {
            self.copy_kept(ctx, index);
        }
        if let Some(index) = drop_kept {
            self.kept.retain(|frame| frame.frame_index() != index);
        }
    }

    /// Put a kept frame on the clipboard without disturbing what is on screen.
    ///
    /// Unpacked on demand rather than held as spans: a kept frame is a held
    /// `Arc` costing nothing extra until someone asks for it, and most never
    /// get asked.
    fn copy_kept(&self, ctx: &egui::Context, index: u64) {
        let Some(frame) = self.kept.iter().find(|f| f.frame_index() == index) else {
            return;
        };
        let Ok(unpacked) = frame.unpacked() else {
            return;
        };
        let view = self.view.lock();
        let spans = crate::flame_chart::puffin::spans_of(&unpacked, view.scope_collection());
        ctx.copy_text(Self::capture_text(ctx, &spans));
    }

    /// What either Copy button puts on the clipboard: the compact frame
    /// encoding, plus one line of what the app was HOLDING at the time.
    ///
    /// Together, because they are only conclusive together. A frame breakdown
    /// with no memory figures cannot distinguish "this frame decoded a lot of
    /// images" from "this frame decoded the same images for the fourth time
    /// because they keep being evicted" — and those have opposite fixes.
    fn capture_text(ctx: &egui::Context, spans: &[crate::flame_chart::Span]) -> String {
        let causes = Self::repaint_causes(ctx);
        let repaint = if causes.is_empty() {
            "R none".to_owned()
        } else {
            let listed: Vec<String> = causes
                .iter()
                .map(|(site, n)| if *n > 1 { format!("{site} x{n}") } else { site.clone() })
                .collect();
            format!("R {}", listed.join(" "))
        };
        format!(
            "{}{}\n{repaint}\n",
            crate::flame_chart::report_compact(spans),
            crate::vitals::Snapshot::now(ctx).compact_line(),
        )
    }

    /// Hold the frame currently on screen past puffin's ring.
    ///
    /// Deduplicated by index, so pressing Keep twice on the same frame does
    /// not grow the list; capped, because this is a debugging aid and an
    /// unbounded one would be its own memory problem in a session left open.
    fn keep_shown(&mut self) {
        let Some((index, _)) = self.shown.as_ref().map(|(i, s)| (*i, s)) else {
            return;
        };
        if self.kept.iter().any(|frame| frame.frame_index() == index) {
            return;
        }
        let found = {
            let view = self.view.lock();
            view.all_uniq()
                .find(|frame| frame.frame_index() == index)
                .cloned()
                .or_else(|| self.high_water.clone().filter(|f| f.frame_index() == index))
        };
        if let Some(frame) = found {
            if self.kept.len() >= KEPT_MAX {
                self.kept.remove(0);
            }
            self.kept.push(frame);
        }
    }

    /// **Who asked for this frame.**
    ///
    /// The question a flame chart cannot answer and that keeps being the
    /// actual problem. A frame breakdown tells you what a frame cost; it says
    /// nothing about why there were sixty of them when the app was idle, and
    /// an egui app only paints when something asks it to. Twice in one
    /// investigation the answer was found by reading code and grepping for
    /// `request_repaint` — a widget's animation timer once, a polling
    /// transport the next time. egui has known all along: every request
    /// carries the file and line that made it.
    ///
    /// Deduplicated by call site with a count, because one cause firing forty
    /// times is a different bug from forty causes firing once.
    fn repaint_causes(ctx: &egui::Context) -> Vec<(String, usize)> {
        let mut counts: Vec<(String, usize)> = Vec::new();
        for cause in ctx.repaint_causes() {
            let key = format!("{}:{}", short_path(cause.file), cause.line);
            match counts.iter_mut().find(|(seen, _)| *seen == key) {
                Some((_, n)) => *n += 1,
                None => counts.push((key, 1)),
            }
        }
        // Loudest first — the one firing every frame is the one to chase.
        counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        counts
    }

    fn repaint_strip(ui: &mut egui::Ui) {
        let causes = Self::repaint_causes(ui.ctx());
        let muted = Ink::Token(Token::TextMuted).of(ui);
        let size = ui.text_size(crate::theme::TextSize::Xs);
        let text = if causes.is_empty() {
            // Genuinely reactive: this frame happened because of input, and
            // the app will now sleep. The state to aim for when idle.
            "repaint: none requested (reactive)".to_owned()
        } else {
            let listed: Vec<String> = causes
                .iter()
                .take(4)
                .map(|(site, n)| {
                    if *n > 1 {
                        format!("{site} ×{n}")
                    } else {
                        site.clone()
                    }
                })
                .collect();
            let more = causes.len().saturating_sub(4);
            let suffix = if more > 0 {
                format!(" +{more} more")
            } else {
                String::new()
            };
            // ASCII: the arrow that reads best here is U+2190, which egui's
            // default font does not carry — `tests/no_broken_glyphs` exists
            // because it renders as a tofu box.
            format!("repaint by: {}{suffix}", listed.join(", "))
        };
        ui.label(egui::RichText::new(text).color(muted).size(size));
        ui.add_space(4.0);
    }

    /// Memory and frame cost, beside the frame breakdown.
    ///
    /// Here because a flame chart answers "where did this frame go" and cannot
    /// answer "and is the app holding a sane amount of memory while it does
    /// that" — and the two questions are constantly confused for each other. A
    /// frame that looks fine while textures climb is a leak in progress; a
    /// frame full of image work while the average texture sits in the
    /// megabytes is a SIZING bug, not a volume one.
    ///
    /// Reads global state directly, so a host gets this by opening the panel
    /// and has nothing to wire up. `Vitals` still logs the same figures on its
    /// own cadence for the case where nobody is watching.
    fn vitals_strip(ui: &mut egui::Ui) {
        let vitals = crate::vitals::Snapshot::now(ui.ctx());
        let muted = Ink::Token(Token::TextMuted).of(ui);
        let mean = vitals.textures.mean_bytes();

        // The average is the one figure here worth colouring, because it is
        // the one with a threshold: past a 400px card the decode ladder has
        // gone wrong, and no amount of eviction fixes a wrong rung.
        let mean_ink = if mean > 1_000_000 {
            Ink::Token(Token::AccentRed).of(ui)
        } else if mean > 640 * 1024 {
            Ink::Token(Token::AccentYellow).of(ui)
        } else {
            muted
        };

        let size = ui.text_size(crate::theme::TextSize::Xs);
        ui.horizontal_wrapped(|ui| {
            let mut chip = |text: String, ink: egui::Color32| {
                ui.label(egui::RichText::new(text).color(ink).size(size));
            };
            chip(
                format!(
                    "textures {} ({})",
                    crate::vitals::format_bytes(vitals.textures.bytes as u64),
                    vitals.textures.count
                ),
                muted,
            );
            chip(
                format!("avg {}", crate::vitals::format_bytes(mean as u64)),
                mean_ink,
            );
            if let Some(wasm) = vitals.wasm_bytes {
                chip(format!("wasm {}", crate::vitals::format_bytes(wasm)), muted);
            }
            chip(
                format!(
                    "img {}",
                    crate::vitals::format_bytes(vitals.image_bytes as u64)
                ),
                muted,
            );
            if vitals.frames.samples > 0 {
                chip(
                    format!(
                        "{:.0} fps · p50 {:.1}ms · p95 {:.1}ms",
                        vitals.frames.fps,
                        vitals.frames.build_p50_ms,
                        vitals.frames.build_p95_ms
                    ),
                    muted,
                );
            }
        });
        ui.add_space(4.0);
    }

    /// The last [`HISTORY`] frames as clickable bars; returns the one picked.
    ///
    /// Bars are scaled by SCOPE COUNT, which is on `FrameMeta` and therefore
    /// free — ranking by anything truer would mean unpacking sixty frames
    /// every time the panel draws, which is a profiler that is worth profiling.
    /// Scope count is a good enough proxy to make an outlier stand out, and
    /// standing out is all this has to do; the flame chart below answers what
    /// the outlier actually was.
    fn history_strip(&self, ui: &mut egui::Ui) -> Option<u64> {
        let frames: Vec<(u64, usize)> = {
            let view = self.view.lock();
            let mut recent: Vec<(u64, usize)> = view
                .all_uniq()
                .map(|frame| (frame.frame_index(), frame.meta().num_scopes))
                .collect();
            // Newest last, so the strip reads left-to-right as time.
            recent.sort_unstable();
            recent.split_off(recent.len().saturating_sub(HISTORY))
        };
        if frames.is_empty() {
            return None;
        }

        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), STRIP_HEIGHT),
            egui::Sense::click(),
        );
        let painter = ui.painter_at(rect);
        let tallest = frames.iter().map(|(_, n)| *n).max().unwrap_or(1).max(1);
        let bar_w = rect.width() / frames.len() as f32;

        let shown = match self.showing {
            Showing::Pinned(index) => Some(index),
            _ => self.shown.as_ref().map(|(index, _)| *index),
        };

        let mut picked = None;
        for (slot, (index, scopes)) in frames.iter().enumerate() {
            let x = rect.left() + slot as f32 * bar_w;
            let height = (*scopes as f32 / tallest as f32) * rect.height();
            let bar = egui::Rect::from_min_max(
                egui::pos2(x, rect.bottom() - height.max(1.0)),
                egui::pos2(x + bar_w - 1.0, rect.bottom()),
            );

            // The frame on screen is marked, so clicking around the strip is
            // navigation rather than guesswork.
            let colour = if Some(*index) == shown {
                Ink::Token(Token::TextPrimary).of(ui)
            } else {
                Ink::Token(Token::TextMuted).of(ui).gamma_multiply(0.6)
            };
            painter.rect_filled(bar, 0, colour);

            if response.clicked()
                && let Some(pos) = response.interact_pointer_pos()
                && (x..x + bar_w).contains(&pos.x)
            {
                picked = Some(*index);
            }
        }
        picked
    }

    /// Pick the frame to show and convert it, if it is not the one already
    /// converted.
    fn refresh(&mut self) {
        let view = self.view.lock();

        // Advance the high-water mark BEFORE choosing, so a spike that arrived
        // this instant is already retained. Scanning the whole ring rather
        // than just the newest frame means a spike is still caught when the
        // app paints in bursts and several frames land between reads.
        let busiest = view
            .all_uniq()
            // Newer than the last reset, or the frame that set the old peak is
            // simply re-adopted from the ring and the reset is invisible.
            .filter(|frame| frame.frame_index() > self.peak_since)
            .max_by_key(|frame| frame.meta().num_scopes);
        if let Some(busiest) = busiest {
            let beats = self
                .high_water
                .as_ref()
                .is_none_or(|held| busiest.meta().num_scopes > held.meta().num_scopes);
            if beats {
                self.high_water = Some(busiest.clone());
            }
        }

        let chosen = match self.showing {
            Showing::Latest => view.latest_frame(),
            // The RETAINED busiest, not the ring's current best — see
            // `high_water`.
            Showing::Busiest => self.high_water.clone(),
            // Falls back to nothing rather than to "latest" when the pinned
            // frame ages out of puffin's ring. Silently sliding to a different
            // frame under a reader comparing two of them is worse than an
            // empty panel that says so.
            // Looks in what the reader KEPT first, then the ring. A frame
            // pinned from the strip and then kept must stay reachable after
            // eviction, or "keep" would not mean anything.
            Showing::Pinned(index) => self
                .kept
                .iter()
                .find(|frame| frame.frame_index() == index)
                .cloned()
                .or_else(|| {
                    view.all_uniq()
                        .find(|frame| frame.frame_index() == index)
                        .cloned()
                }),
        };
        let Some(frame) = chosen else {
            return;
        };

        let index = frame.frame_index();
        if self
            .shown
            .as_ref()
            .is_some_and(|(shown, _)| *shown == index)
        {
            return;
        }
        let Ok(unpacked) = frame.unpacked() else {
            return;
        };

        let spans = crate::flame_chart::puffin::spans_of(&unpacked, view.scope_collection());
        self.shown = Some((index, spans));
    }
}

/// The last two path components of a source file.
///
/// `RepaintCause::file` is whatever `file!()` produced, which for a registry
/// dependency is an absolute store path long enough to fill the strip on its
/// own. Two components keep the crate-relative part that identifies the call
/// site — `image_loader/fetch.rs`, `src/block_train.rs` — and drop the rest.
fn short_path(file: &str) -> &str {
    let normalised = file.trim_end_matches('/');
    match normalised.rmatch_indices('/').nth(1) {
        Some((index, _)) => &normalised[index + 1..],
        None => normalised,
    }
}

#[cfg(test)]
mod short_path_tests {
    use super::short_path;

    #[test]
    fn a_long_registry_path_keeps_only_what_identifies_the_call_site() {
        assert_eq!(
            short_path("/nix/store/abc/registry/egui-0.36.2/src/context.rs"),
            "src/context.rs"
        );
        assert_eq!(
            short_path("ui/egui-widgets/src/image_loader/fetch.rs"),
            "image_loader/fetch.rs"
        );
    }

    /// Short paths must survive unchanged rather than being truncated to
    /// nothing — a bare file name is already the answer.
    #[test]
    fn a_path_with_too_few_components_is_left_alone() {
        assert_eq!(short_path("src/lib.rs"), "src/lib.rs");
        assert_eq!(short_path("lib.rs"), "lib.rs");
        assert_eq!(short_path(""), "");
    }
}
