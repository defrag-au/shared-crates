//! `conversation_history` — what people said to the bot, what it worked out,
//! and what it said back (feature `gateway`).
//!
//! Not a log. A log is a sequence of events for an operator to scan; this is
//! a **conversation**, and every design decision here follows from that. A
//! turn has three levels because that is the shape of the question actually
//! being asked of it:
//!
//! 1. **The request** — who said what. The thing you scan for.
//! 2. **The working out** — the steps, the tools, the failures. The thing you
//!    came for when the answer was wrong.
//! 3. **The answer** — the reply, and what it cost.
//!
//! A flat list told you a binding fired and nothing about *why the reply was
//! wrong*, which is the part that costs real time. The previous answer to "I
//! typed something and nothing happened" was a `wrangler tail`, by which
//! point the moment had passed.
//!
//! ## Misses matter more than hits
//!
//! A hit is already visible — it is right there in the channel. A **miss** is
//! invisible everywhere else, and "no binding matched" and "we don't know our
//! own user id yet" look identical from Discord while meaning entirely
//! different things. So a miss renders its reason prominently, and
//! [`HistoryState::misses_only`] exists because that is the actual workflow:
//! find the message that should have worked.
//!
//! ## Waiting is a state, not an absence
//!
//! A trace arrives seconds after the message it explains, out of order — so
//! an untraced turn is normally *pending*, and says so. But a trace whose
//! entry has already aged out of the ring is dropped silently, and a turn
//! that will never be traced looks exactly like one still in flight. Past
//! [`STALE_WAIT_SECS`] the copy stops promising an answer that is not coming.

use egui::Ui;
use gateway_wiring::{RecentActivity, TraceKind, TraceStep};

use crate::relative_time::relative_label;
use crate::theme;

/// After this long with no trace, a turn stops saying "waiting" and admits
/// nothing is coming. Comfortably longer than a slow model turn — this is
/// about a trace that was DROPPED, not one that is thinking.
pub const STALE_WAIT_SECS: i64 = 120;

/// Cross-frame state for the feed.
#[derive(Default)]
pub struct HistoryState {
    /// Show only turns that fired nothing. The "why didn't it work" filter,
    /// which is the reason anyone opens this.
    pub misses_only: bool,
}

/// What the reader did this frame.
#[derive(Default)]
pub struct HistoryResponse {
    /// A turn's author was clicked — the caller may want to filter to them.
    pub author_clicked: Option<String>,
}

/// The whole conversation, newest first.
///
/// Newest first is not a preference: the thing you just typed is the thing
/// you are looking for, and a feed that puts it at the bottom makes every
/// reader scroll to find out whether their own test worked.
///
/// `now_ms` is passed in rather than read from a clock so a story can pin it
/// and render deterministically.
/// Header and list together. A caller that wants the header to stay put
/// while the list scrolls calls [`conversation_header`] and
/// [`conversation_list`] itself — putting the filter toggle inside the
/// scrolled region means it scrolls away exactly when someone reaches for it.
pub fn conversation_history(
    ui: &mut Ui,
    entries: &[RecentActivity],
    state: &mut HistoryState,
    now_ms: f64,
) -> HistoryResponse {
    conversation_header(ui, entries, state);
    conversation_list(ui, entries, state, now_ms)
}

/// The count line and the misses-only toggle.
pub fn conversation_header(ui: &mut Ui, entries: &[RecentActivity], state: &mut HistoryState) {
    let misses = entries
        .iter()
        .filter(|e| e.matched_binding.is_none())
        .count();

    ui.horizontal(|ui| {
        ui.strong("Conversation");
        ui.colored_label(
            theme::TEXT_MUTED,
            match entries.len() {
                0 => "nothing yet".to_string(),
                n => format!("{n} messages · {misses} fired nothing"),
            },
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.checkbox(&mut state.misses_only, "misses only")
                .on_hover_text(
                    "Only messages that triggered nothing — the \"why didn't it work\" view",
                );
        });
    });
    ui.add_space(4.0);
}

/// The turns themselves, newest first.
pub fn conversation_list(
    ui: &mut Ui,
    entries: &[RecentActivity],
    state: &mut HistoryState,
    now_ms: f64,
) -> HistoryResponse {
    let mut response = HistoryResponse::default();

    let shown: Vec<&RecentActivity> = entries
        .iter()
        .rev()
        .filter(|e| !state.misses_only || e.matched_binding.is_none())
        .collect();

    if shown.is_empty() {
        ui.add_space(8.0);
        ui.colored_label(
            theme::TEXT_MUTED,
            if state.misses_only && !entries.is_empty() {
                "No misses — everything here matched a binding."
            } else {
                "Nothing yet. Post in a wired channel and it appears here."
            },
        );
        return response;
    }

    for entry in shown {
        if let Some(author) = conversation_turn(ui, entry, now_ms).author_clicked {
            response.author_clicked = Some(author);
        }
        ui.add_space(6.0);
    }

    response
}

/// One turn: the request, the working out, the answer.
pub fn conversation_turn(ui: &mut Ui, entry: &RecentActivity, now_ms: f64) -> HistoryResponse {
    // The family has to exist before a `FontId` can name it.
    crate::icons::install_phosphor_font(ui.ctx());
    let mut response = HistoryResponse::default();

    // Level 1 — what someone actually asked.
    ui.horizontal_wrapped(|ui| {
        // Filled for a hit, hollow for a miss: the whole feed is scannable on
        // this one column without reading a word of it.
        let (mark, color) = match &entry.matched_binding {
            Some(_) => ("●", theme::SUCCESS),
            None => ("○", theme::TEXT_MUTED),
        };
        ui.colored_label(color, mark);
        if ui
            .add(
                egui::Label::new(egui::RichText::new(&entry.author).color(theme::TEXT_MUTED))
                    .sense(egui::Sense::click()),
            )
            .on_hover_text("filter to this person")
            .clicked()
        {
            response.author_clicked = Some(entry.author.clone());
        }
        ui.label(egui::RichText::new(&entry.preview).strong());

        // Age, right-aligned. Absent from the original, and the reason "find
        // the message I sent 30 seconds ago" meant counting rows.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let delta_secs = ((now_ms - entry.at_ms) / 1000.0) as i64;
            ui.colored_label(theme::TEXT_MUTED, relative_label(delta_secs));
        });
    });

    // A miss explains itself and stops — there is no working out to show.
    if let Some(note) = &entry.note {
        indented(ui, |ui| {
            ui.colored_label(theme::ACCENT_YELLOW, format!("⊘  {note}"));
        });
        return response;
    }

    let Some(trace) = &entry.trace else {
        indented(ui, |ui| {
            if !is_awaiting_trace(entry, now_ms) {
                // The dropped-trace case. Saying "waiting…" forever promises
                // an answer that is never arriving.
                ui.colored_label(
                    theme::ACCENT_YELLOW,
                    match &entry.matched_binding {
                        Some(binding) => format!(
                            "⊘  dispatched to {binding}, but no trace came back — \
                             it may have been dropped"
                        ),
                        None => "⊘  no trace came back".to_string(),
                    },
                );
            } else {
                ui.colored_label(
                    theme::TEXT_MUTED,
                    match &entry.matched_binding {
                        // Dispatched, nothing back yet. Says so rather than
                        // looking like an answer that never came.
                        Some(binding) => format!("↻  dispatched to {binding}, waiting…"),
                        None => "↻  waiting…".to_string(),
                    },
                );
            }
        });
        return response;
    };

    // Level 2 — the working out.
    indented(ui, |ui| {
        for step in &trace.steps {
            trace_step(ui, step);
        }

        if let Some(note) = &trace.note {
            // `PhosphorIcon`, not U+26A0 — the default font renders that as
            // tofu, which is why this crate has a test for it.
            //
            // And `rich_text`, not `as_str`: Phosphor is its own FAMILY, so
            // the bare codepoint in a normal label is tofu of a second kind
            // that no test catches. The denylist checks which characters you
            // use; nothing checks which font you asked for.
            ui.horizontal(|ui| {
                ui.label(crate::PhosphorIcon::Warning.rich_text(13.0, theme::ACCENT_YELLOW));
                ui.colored_label(theme::ACCENT_YELLOW, note);
            });
        }

        // Level 3 — what it said back.
        if let Some(reply) = &trace.reply {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(theme::SUCCESS, "💬");
                ui.label(reply);
            });
        }

        if let Some(tokens) = trace.token_summary() {
            ui.colored_label(theme::TEXT_MUTED, tokens);
        }
    });

    response
}

/// One step of the working out.
pub fn trace_step(ui: &mut Ui, step: &TraceStep) {
    // The icon carries the step KIND so the eye can skip to the tool calls,
    // which are what you are usually looking for. Colour carries success.
    let icon = match step.kind {
        TraceKind::Tool => "🔧",
        TraceKind::Context => "◆",
        TraceKind::Note => "•",
    };
    let colour = if step.ok {
        theme::TEXT_MUTED
    } else {
        theme::ACCENT_RED
    };

    ui.horizontal_wrapped(|ui| {
        ui.colored_label(colour, icon);
        ui.colored_label(colour, &step.label);
    });

    // A tool's result sits under its call, indented again — the deepest thing
    // in the tree, because it is the thing you read last and least often.
    if let Some(detail) = &step.detail {
        indented(ui, |ui| {
            ui.colored_label(colour, detail);
        });
    }
}

/// Is this turn still expecting a trace? Pure, so "waiting" and "dropped" can
/// be reasoned about (and tested) without a frame.
pub fn is_awaiting_trace(entry: &RecentActivity, now_ms: f64) -> bool {
    entry.trace.is_none()
        && entry.note.is_none()
        && ((now_ms - entry.at_ms) / 1000.0) as i64 <= STALE_WAIT_SECS
}

/// Nest one level. egui has no tree primitive that keeps rows this compact,
/// and a real indent reads better here than a bullet prefix.
fn indented(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.add_space(18.0);
        ui.vertical(add);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(at_ms: f64) -> RecentActivity {
        RecentActivity {
            at_ms,
            message_id: "m".into(),
            guild_id: "g".into(),
            author: "someone".into(),
            preview: "ahoy".into(),
            matched_binding: Some("b".into()),
            note: None,
            trace: None,
        }
    }

    /// "Still coming" and "never coming" must not render the same, or a
    /// dropped trace reads as a slow one forever.
    #[test]
    fn waiting_becomes_dropped_once_it_is_too_late() {
        let now = 1_000_000.0;
        assert!(
            is_awaiting_trace(&turn(now - 5_000.0), now),
            "5s in is an ordinary wait"
        );
        assert!(
            !is_awaiting_trace(&turn(now - (STALE_WAIT_SECS as f64 + 1.0) * 1000.0), now),
            "past the threshold nothing is coming"
        );
    }

    /// A miss is explained by its note and never waits on a trace that was
    /// never dispatched.
    #[test]
    fn a_miss_is_not_awaiting_anything() {
        let now = 1_000_000.0;
        let mut missed = turn(now);
        missed.matched_binding = None;
        missed.note = Some("no binding matched".into());
        assert!(!is_awaiting_trace(&missed, now));
    }
}
