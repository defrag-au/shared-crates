//! `gateway_log` — the listener's own log lines, in the surface that already
//! knows what they mean (feature `gateway`).
//!
//! The companion to [`conversation_history`], and deliberately its opposite.
//! That widget renders a *conversation*: what someone asked, what the agent
//! worked out, what it said back. This one renders a **log** — a flat,
//! monospaced, newest-first stream of everything else the listener did, which
//! until now existed only as `tracing` calls readable by being attached to a
//! `wrangler tail` at the moment they happened.
//!
//! The things it answers that nothing else does: which close code Discord
//! sent, whether the reconnect resumed or re-identified, whether the identify
//! budget was hit, why a dispatch to the executor failed, which admin saved
//! what.
//!
//! ## Filtering is the whole interaction
//!
//! Nobody reads a log; they search one. So the two controls are a **severity
//! floor** (the "just show me what broke" move) and a **substring filter**,
//! and both are in a header that stays put while the list scrolls — a control
//! inside the scrolled region scrolls away exactly when someone reaches for
//! it, which is the same lesson [`conversation_header`] already learned.
//!
//! The counts are on the header rather than implied by the list, because
//! "0 shown of 250" and "0 lines" are entirely different problems and look
//! identical once a filter has hidden everything.
//!
//! [`conversation_history`]: crate::conversation_history
//! [`conversation_header`]: crate::conversation_history::conversation_header

use egui::Ui;
use gateway_wiring::{GatewayLogEntry, LogLevel};

use crate::relative_time::relative_label;
use crate::theme;

/// Cross-frame state for the log pane.
pub struct LogState {
    /// Show this level and everything more severe. Defaults to `Info`: `Debug`
    /// is per-message chatter that buries the reconnect you came to find.
    pub min_level: LogLevel,
    /// Substring filter, matched case-insensitively against the message and
    /// the target.
    pub filter: String,
}

impl Default for LogState {
    fn default() -> Self {
        Self {
            min_level: LogLevel::Info,
            filter: String::new(),
        }
    }
}

impl LogState {
    /// Does this line survive the current filters?
    ///
    /// Pure, and public, so a caller can count matches (or export them)
    /// without rendering — and so the rule the header counts by is the same
    /// one the list draws by.
    pub fn shows(&self, entry: &GatewayLogEntry) -> bool {
        if entry.level < self.min_level {
            return false;
        }
        if self.filter.trim().is_empty() {
            return true;
        }
        let needle = self.filter.to_lowercase();
        entry.message.to_lowercase().contains(&needle)
            || entry.target.to_lowercase().contains(&needle)
    }
}

/// Header and list together, for a caller with nothing to put between them.
pub fn gateway_log(ui: &mut Ui, entries: &[GatewayLogEntry], state: &mut LogState, now_ms: f64) {
    gateway_log_header(ui, entries, state);
    gateway_log_list(ui, entries, state, now_ms);
}

/// The counts, the severity floor, and the substring filter.
pub fn gateway_log_header(ui: &mut Ui, entries: &[GatewayLogEntry], state: &mut LogState) {
    let shown = entries.iter().filter(|e| state.shows(e)).count();
    let problems = entries.iter().filter(|e| e.level >= LogLevel::Warn).count();

    ui.horizontal(|ui| {
        ui.strong("Listener log");
        ui.colored_label(
            theme::TEXT_MUTED,
            match entries.len() {
                0 => "nothing yet".to_string(),
                n if shown == n => format!("{n} lines"),
                n => format!("{shown} of {n} lines"),
            },
        );
        // Worth its own colour: it is the number that decides whether you
        // bother reading the rest.
        if problems > 0 {
            ui.colored_label(theme::ACCENT_YELLOW, format!("{problems} warn+"));
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut state.filter)
                    .hint_text("filter")
                    .desired_width(160.0),
            );
            level_selector(ui, &mut state.min_level);
        });
    });
    ui.add_space(4.0);
}

/// The severity floor, as one button per level.
///
/// A row of toggles rather than a dropdown: there are five of them, the whole
/// point is to flip between "everything" and "just the failures" in one click,
/// and a dropdown makes that two plus a read.
fn level_selector(ui: &mut Ui, min_level: &mut LogLevel) {
    // Most severe first, so the common choice (WARN) is nearest the filter box
    // rather than at the far end of the row.
    for level in LogLevel::all().into_iter().rev() {
        let selected = *min_level == level;
        let colour = level_colour(level);
        let label = egui::RichText::new(level.label())
            .monospace()
            .size(11.0)
            .color(if selected { colour } else { theme::TEXT_MUTED });
        if ui
            .selectable_label(selected, label)
            .on_hover_text(format!("{} and above", level.label()))
            .clicked()
        {
            *min_level = level;
        }
    }
}

/// The lines themselves, newest first.
///
/// Newest first for the same reason the conversation is: the line you are
/// looking for is the one your last action just produced, and a log that puts
/// it at the bottom makes every reader scroll to find out what happened.
pub fn gateway_log_list(
    ui: &mut Ui,
    entries: &[GatewayLogEntry],
    state: &mut LogState,
    now_ms: f64,
) {
    let shown: Vec<&GatewayLogEntry> = entries.iter().rev().filter(|e| state.shows(e)).collect();

    if shown.is_empty() {
        ui.add_space(8.0);
        ui.colored_label(
            theme::TEXT_MUTED,
            if entries.is_empty() {
                "Nothing logged yet. Lines appear as the listener works."
            } else {
                // The filter hid everything — an entirely different situation
                // from an empty log, and one the reader can fix.
                "No lines match the current filter."
            },
        );
        return;
    }

    for entry in shown {
        gateway_log_line(ui, entry, now_ms);
    }
}

/// One line: age, level, target, message.
///
/// Monospaced and single-row-per-line by construction. A log's value is that
/// the eye can run down a column; wrapping every long message into a paragraph
/// destroys exactly that, so a long line is truncated on screen and the full
/// text is on hover.
pub fn gateway_log_line(ui: &mut Ui, entry: &GatewayLogEntry, now_ms: f64) {
    let colour = level_colour(entry.level);
    let delta_secs = ((now_ms - entry.at_ms) / 1000.0) as i64;

    ui.horizontal(|ui| {
        ui.add(
            egui::Label::new(
                egui::RichText::new(relative_label(delta_secs))
                    .monospace()
                    .size(11.0)
                    .color(theme::TEXT_MUTED),
            )
            .truncate(),
        );
        ui.add(
            egui::Label::new(
                egui::RichText::new(entry.level.label())
                    .monospace()
                    .size(11.0)
                    .color(colour),
            )
            .truncate(),
        );
        if !entry.target.is_empty() {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(&entry.target)
                        .monospace()
                        .size(11.0)
                        .color(theme::TEXT_MUTED),
                )
                .truncate(),
            );
        }
        // INFO and below stay muted so a scan lands on the colour; a warning
        // or an error is the message you are here for, so it is full strength.
        let message_colour = if entry.level >= LogLevel::Warn {
            colour
        } else {
            theme::TEXT_SECONDARY
        };
        ui.add(
            egui::Label::new(
                egui::RichText::new(&entry.message)
                    .monospace()
                    .size(11.0)
                    .color(message_colour),
            )
            .truncate(),
        )
        .on_hover_text(&entry.message);
    });
}

/// Severity to colour. One mapping, so the level chip and its message can
/// never disagree about how bad a line is.
pub fn level_colour(level: LogLevel) -> egui::Color32 {
    match level {
        LogLevel::Error => theme::ACCENT_RED,
        LogLevel::Warn => theme::ACCENT_YELLOW,
        LogLevel::Info => theme::ACCENT_BLUE,
        LogLevel::Debug => theme::TEXT_MUTED,
        LogLevel::Trace => theme::TEXT_MUTED,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(level: LogLevel, target: &str, message: &str) -> GatewayLogEntry {
        GatewayLogEntry {
            at_ms: 0.0,
            level,
            target: target.into(),
            message: message.into(),
        }
    }

    /// The severity floor is a floor, not an equality — "show me WARN" has to
    /// include the errors or the one view people actually use hides the worst
    /// lines in it.
    #[test]
    fn the_level_filter_is_a_floor() {
        let state = LogState {
            min_level: LogLevel::Warn,
            filter: String::new(),
        };
        assert!(state.shows(&line(LogLevel::Error, "do", "fatal close 4014")));
        assert!(state.shows(&line(LogLevel::Warn, "do", "connect failed")));
        assert!(!state.shows(&line(LogLevel::Info, "do", "gateway identifying")));
        assert!(!state.shows(&line(LogLevel::Debug, "do", "unhandled op 11")));
    }

    /// The filter matches the target as well as the message, because "show me
    /// only the DO" is a question people ask of a log that also carries the
    /// worker's lines.
    #[test]
    fn the_text_filter_matches_message_or_target() {
        let state = LogState {
            min_level: LogLevel::Trace,
            filter: "resume".into(),
        };
        assert!(state.shows(&line(LogLevel::Info, "do", "gateway session RESUMED")));
        assert!(state.shows(&line(LogLevel::Info, "resume_state", "checkpointed")));
        assert!(!state.shows(&line(LogLevel::Info, "do", "gateway identifying")));
    }

    /// A blank filter is not a filter. Whitespace counts as blank — a stray
    /// space should not empty the pane.
    #[test]
    fn a_blank_filter_hides_nothing() {
        let state = LogState {
            min_level: LogLevel::Trace,
            filter: "   ".into(),
        };
        assert!(state.shows(&line(LogLevel::Trace, "do", "anything at all")));
    }
}
