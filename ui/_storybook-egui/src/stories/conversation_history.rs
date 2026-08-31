//! `conversation_history` story — every shape a turn can take.
//!
//! The states below are the whole point. In a deployment each one needs a
//! real Discord server, a running listener, and often a *failure* you cannot
//! summon on demand — a dropped trace, a tool that 404s, a model that answers
//! with nothing. So they were never looked at side by side, and the flat list
//! in the portal pane shipped without anyone noticing it could not express
//! most of them.
//!
//! Time is pinned rather than read from a clock, so "18s ago" and "4m ago"
//! render the same on every frame and the stale-trace threshold can actually
//! be crossed on demand.

use egui_widgets::conversation_history::{
    conversation_history, HistoryState, STALE_WAIT_SECS,
};
use egui_widgets::theme;
use gateway_wiring::{ActionTrace, RecentActivity, TraceKind, TraceStep};

/// Pinned "now". Fixtures are offsets from this, so the feed reads the same
/// every frame.
const NOW_MS: f64 = 1_780_000_000_000.0;

/// Which hosting context to render in — see the `wiring_editor` story for why
/// this toggle exists (a `SidePanel` renders nothing in the portal's scrolled
/// column, and that shipped).
#[derive(Clone, Copy, PartialEq)]
pub enum HostedIn {
    Bare,
    PortalShell,
}

pub struct ConversationHistoryStory {
    pub entries: Vec<RecentActivity>,
    pub state: HistoryState,
    pub hosted_in: HostedIn,
    /// Wind the clock forward, so a "waiting…" turn crosses the stale
    /// threshold and admits its trace is never arriving.
    pub clock_offset_secs: f64,
    pub last: String,
}

impl Default for ConversationHistoryStory {
    fn default() -> Self {
        Self {
            entries: fixtures(),
            state: HistoryState::default(),
            hosted_in: HostedIn::Bare,
            clock_offset_secs: 0.0,
            last: String::new(),
        }
    }
}

fn step(kind: TraceKind, label: &str, detail: Option<&str>, ok: bool) -> TraceStep {
    TraceStep {
        kind,
        label: label.to_string(),
        detail: detail.map(str::to_string),
        ok,
    }
}

fn turn(
    secs_ago: f64,
    author: &str,
    preview: &str,
    binding: Option<&str>,
    note: Option<&str>,
    trace: Option<ActionTrace>,
) -> RecentActivity {
    RecentActivity {
        at_ms: NOW_MS - secs_ago * 1000.0,
        message_id: format!("m-{secs_ago}"),
        guild_id: "g".into(),
        author: author.to_string(),
        preview: preview.to_string(),
        matched_binding: binding.map(str::to_string),
        note: note.map(str::to_string),
        trace,
    }
}

/// Oldest first — the widget reverses, because the message you just sent is
/// the one you are looking for.
fn fixtures() -> Vec<RecentActivity> {
    vec![
        // A miss whose cause is OUR fault, not the wiring's. This and the
        // plain "no binding matched" look identical from the channel and
        // mean entirely different things — the reason `note` exists.
        turn(
            600.0,
            "kestrel",
            "@augie what's the floor on toolheads?",
            None,
            Some("no bot id yet — the listener resumed without a READY, so mentions match nothing"),
            None,
        ),
        // The ordinary miss.
        turn(430.0, "marlowe", "gm everyone", None, Some("no binding matched"), None),
        // A react: fires, reports, never calls a model — so no token line.
        turn(
            300.0,
            "quill",
            "ahoy",
            Some("b-ahoy"),
            None,
            Some(ActionTrace {
                steps: vec![step(TraceKind::Note, "reacted ⚓", None, true)],
                ..ActionTrace::default()
            }),
        ),
        // The full agent turn: context, tools, a reply, and the billed shape.
        turn(
            180.0,
            "kestrel",
            "@augie which of my ships is fastest?",
            Some("b-mention"),
            None,
            Some(ActionTrace {
                steps: vec![
                    step(TraceKind::Context, "3 plugins · 11 tools", None, true),
                    step(
                        TraceKind::Tool,
                        "list_owned(policy=guild default)",
                        Some("7 assets"),
                        true,
                    ),
                    step(
                        TraceKind::Tool,
                        "asset_traits(Sloop #221)",
                        Some("Speed 8 · Hull 3 · Crew 2"),
                        true,
                    ),
                ],
                reply: Some(
                    "Sloop #221 — Speed 8, the quickest of your seven. \
                     Brigantine #12 is close behind at 7."
                        .into(),
                ),
                prompt_tokens: 13_431,
                cached_prompt_tokens: 12_000,
                completion_tokens: 69,
                reasoning_tokens: 801,
                ..ActionTrace::default()
            }),
        ),
        // A FAILED step. The most interesting line in any trace, and the one
        // a flat list could never show — it fired, so the feed says "hit",
        // and the channel just shows a bot that answered oddly.
        turn(
            95.0,
            "marlowe",
            "@augie show me my rarest",
            Some("b-mention"),
            None,
            Some(ActionTrace {
                steps: vec![
                    step(TraceKind::Context, "3 plugins · 11 tools", None, true),
                    step(
                        TraceKind::Tool,
                        "asset_rarity(policy=8f2c…, asset=Toolhead #1104)",
                        Some("upstream 502 — cnft.tools unreachable"),
                        false,
                    ),
                ],
                note: Some("answered without rarity data".into()),
                reply: Some("I couldn't reach the rarity service just now.".into()),
                prompt_tokens: 4_120,
                completion_tokens: 34,
                ..ActionTrace::default()
            }),
        ),
        // Dispatched, nothing back yet — the ordinary few seconds between a
        // message and its trace, which arrive out of order.
        turn(8.0, "quill", "@augie hello?", Some("b-mention"), None, None),
    ]
}

pub fn show(ui: &mut egui::Ui, state: &mut ConversationHistoryStory) {
    ui.label(
        egui::RichText::new("Conversation History")
            .color(theme::ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "What was said, what the bot worked out, what it answered. Three levels, \
             because a flat list tells you a binding fired and nothing about why the \
             reply was wrong.",
        )
        .small()
        .color(theme::TEXT_MUTED),
    );
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        ui.label("hosted in:");
        ui.selectable_value(&mut state.hosted_in, HostedIn::Bare, "bare Ui");
        ui.selectable_value(
            &mut state.hosted_in,
            HostedIn::PortalShell,
            "portal shell (ScrollArea)",
        );
        if ui.button("reset").clicked() {
            *state = ConversationHistoryStory::default();
        }
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label("clock:");
        ui.add(
            egui::Slider::new(&mut state.clock_offset_secs, 0.0..=300.0)
                .suffix("s later")
                .step_by(5.0),
        )
        .on_hover_text(format!(
            "Wind forward past {STALE_WAIT_SECS}s and the pending turn stops \
             promising an answer that isn't coming"
        ));
    });
    if !state.last.is_empty() {
        ui.colored_label(theme::TEXT_MUTED, &state.last);
    }
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    match state.hosted_in {
        HostedIn::Bare => body(ui, state),
        HostedIn::PortalShell => {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(520.0)
                .show(ui, |ui| {
                    let inner_width = 900.0_f32.min(ui.available_width());
                    let pad = ((ui.available_width() - inner_width).max(0.0)) / 2.0;
                    ui.horizontal(|ui| {
                        ui.add_space(pad);
                        ui.vertical(|ui| {
                            ui.set_max_width(inner_width);
                            body(ui, state);
                        });
                    });
                });
        }
    }
}

fn body(ui: &mut egui::Ui, state: &mut ConversationHistoryStory) {
    let now = NOW_MS + state.clock_offset_secs * 1000.0;
    let resp = conversation_history(ui, &state.entries, &mut state.state, now);
    if let Some(author) = resp.author_clicked {
        state.last = format!("author clicked: {author}");
    }
}
