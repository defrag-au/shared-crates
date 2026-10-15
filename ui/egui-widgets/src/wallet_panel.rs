//! `WalletPanel` — the reader's wallets as a summary, with the whole management
//! flow behind a popup it owns.
//!
//! [`wallet_editor`](crate::wallet_editor) is the roster itself: add a wallet,
//! watch it resolve, link it, drop it again. That is the right widget and the
//! wrong thing to leave standing in a sidebar — fourteen rows of roster is most
//! of the column, and it is the same fourteen rows every frame. This widget is
//! the answer: **inline it draws only what the roster is** (how many wallets,
//! which chains, how many are proven, and a mark per wallet), and the roster —
//! plus the wallet connect strip — moves into a popup one click away.
//!
//! ## What it does and does not do
//!
//! It composes, it does not replace: the popup body IS
//! [`wallet_editor::show`], and the connect strip IS
//! [`WalletButton`](crate::wallet_button::WalletButton). Everything the reader
//! can do in the popup is therefore one of the actions below, emitted for the
//! host to perform — the widget never writes, spawns, or persists. That is the
//! crate's standing contract (see [`StakeSessionPanel`](crate::stake_session)),
//! and it is what lets a story drive the whole flow with a mock host.
//!
//! ## The summary
//!
//! One mark per wallet, then the counts. The marks are the two systems
//! [`WalletOrigin`] and [`WalletEntryStatus`] already encode — tint by origin,
//! filled while settled, hollow while it resolves, red if it failed — so the
//! strip cannot disagree with the rows it summarises. The counts name the chains
//! the roster actually reaches: an EVM account and a Cardano stake key are both
//! "a wallet" here, and the reader adding the second one wants to see the first.
//!
//! // Gated with `wallet_button`, whose picker it opens: the connect strip is
//! the wasm half. Verified by the storybook story, not by `cargo test` — like
//! [`wallet_mock`](crate::wallet_mock), this module never compiles natively.

use egui::{Align, Color32, Layout, RichText, Sense, Stroke, Ui, vec2};

use crate::commands::Command;
use crate::icons::PhosphorIcon;
use crate::theme::{InkExt, Radius, Space, SpaceExt, TextSize, ThemeExt};
use crate::wallet::{WalletConnector, WalletProvider};
use crate::wallet_button::{PickerChrome, WalletAction, WalletButton};
use crate::wallet_editor::{
    self, Submission, WalletEditorAction, WalletEditorConfig, WalletEditorEntry, WalletEditorState,
    WalletEntryStatus, WalletOrigin,
};

/// Width of the management popup, clamped to the viewport by the caller.
const POPUP_W: f32 = 420.0;

/// How many marks the summary draws before it stops and counts the rest.
///
/// Past this the strip stops being a glance and becomes a second list; the
/// number it hides is on the same line, and the popup has the rows themselves.
const PIP_CAP: usize = 16;

// ============================================================================
// Types
// ============================================================================

/// Persistent widget state: whether the popup is up, and the roster editor's own
/// buffer (the add field and its modal).
#[derive(Default)]
pub struct WalletPanelState {
    /// The management popup is open.
    pub open: bool,
    /// The roster editor's state, owned here because the editor only draws
    /// inside the popup. Public so a host that resets its page can clear a
    /// half-typed handle along with the rest of the page's transient state.
    pub editor: WalletEditorState,
}

/// What the reader did, for the host to perform.
#[derive(Debug, Clone, PartialEq)]
pub enum WalletPanelAction {
    /// Connect this provider — the CIP-30 flow, exactly as `WalletButton`
    /// reports it.
    Connect(WalletProvider),
    /// Drop the browser wallet connection. The roster row it seeded is the
    /// host's to remove, not this.
    Disconnect,
    /// A roster edit — the same actions [`wallet_editor`] emits, unchanged.
    Editor(WalletEditorAction),
}

#[derive(Default)]
pub struct WalletPanelResponse {
    pub action: Option<WalletPanelAction>,
}

/// The panel. Borrows the roster the host already has: entries from its own
/// state, the connector from its own session.
pub struct WalletPanel<'a> {
    entries: &'a [WalletEditorEntry],
    connector: &'a WalletConnector,
    config: &'a WalletEditorConfig<'a>,
}

// ============================================================================
// Widget
// ============================================================================

impl<'a> WalletPanel<'a> {
    pub fn new(
        entries: &'a [WalletEditorEntry],
        connector: &'a WalletConnector,
        config: &'a WalletEditorConfig<'a>,
    ) -> Self {
        Self {
            entries,
            connector,
            config,
        }
    }

    /// Draw the summary, and the popup when it is open.
    pub fn show(self, ui: &mut Ui, state: &mut WalletPanelState) -> WalletPanelResponse {
        crate::icons::ensure_fonts(ui);
        let colours = ui.tokens().color;
        let mut response = WalletPanelResponse::default();

        // ── Heading: the roster's name, and the way in ──────────────────────
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(self.config.heading)
                    .color(colours.text_secondary)
                    .size(ui.text_size(TextSize::Md)),
            );
            // Pushed right rather than laid out after the heading, so the
            // affordance sits in the row's corner whatever the heading says.
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = crate::icons::phosphor_label(ui, PhosphorIcon::Gear, "Manage");
                if ui
                    .button(label)
                    .on_hover_text("Connect, add, link or remove wallets")
                    .clicked()
                {
                    state.open = true;
                }
            });
        });

        // Offered every pass the summary draws, so the palette reaches the
        // manager from any page that shows one — and loses it with the page,
        // like every other offer. The id is DERIVED from the editor's, because
        // two panels in one app already need different `command_id`s and this
        // keeps that property without a second knob.
        if Command::new(
            format!("{}.manage", self.config.command_id),
            "Manage wallets",
        )
        .hint("connect, add, link or remove wallets")
        .group(self.config.heading)
        .icon(PhosphorIcon::Gear)
        .offer(ui)
        {
            state.open = true;
        }

        // ── The summary ─────────────────────────────────────────────────────
        ui.add_space(ui.space(Space::Sm));
        if self.entries.is_empty() {
            ui.label(
                RichText::new(self.config.empty_text)
                    .color(colours.text_muted)
                    .size(ui.text_size(TextSize::Sm)),
            );
        } else {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = ui.space(Space::Xs);
                let hidden = draw_marks(ui, self.entries);
                if hidden > 0 {
                    ui.label(
                        RichText::new(format!("+{hidden}"))
                            .color(colours.text_muted)
                            .size(ui.text_size(TextSize::Xs)),
                    );
                }
                ui.label(
                    RichText::new(counts_line(self.entries))
                        .color(colours.text_primary)
                        .size(ui.text_size(TextSize::Sm)),
                );
            });
        }

        if state.open {
            response.action = self.manager(ui, state);
        }
        response
    }

    /// The management popup: the connect strip over the roster.
    fn manager(self, ui: &mut Ui, state: &mut WalletPanelState) -> Option<WalletPanelAction> {
        let ctx = ui.ctx().clone();
        let colours = ui.tokens().color;
        let mut action = None;

        let id = egui::Id::new(("wallet-panel", self.config.command_id));
        // The first pass sizes an area against the style's default (600×400), and
        // a scroll area clamped to that reports the clamp back as its own size —
        // so the modal would measure itself at two rows and stay there. A
        // generous first pass is what lets it measure the roster it really has.
        let area = egui::Modal::default_area(id)
            .default_size(vec2(POPUP_W, ctx.content_rect().height() * 0.85));

        let modal = egui::Modal::new(id)
            .area(area)
            .frame(
                egui::Frame::new()
                    .fill(colours.bg_secondary)
                    .corner_radius(ui.tokens().corner(Radius::Lg))
                    .stroke(crate::theme::hairline(colours.border))
                    .inner_margin(ui.tokens().margin(Space::Xl)),
            )
            .show(&ctx, |ui| {
                ui.set_width(POPUP_W.min(ctx.content_rect().width() * 0.9));

                // Title row: what this is, and the way out. The roster draws
                // without a heading of its own — see `body_config`.
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(self.config.heading)
                            .color(colours.text_primary)
                            .size(ui.text_size(TextSize::Md)),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .small_button(PhosphorIcon::X.rich_text(12.0, colours.text_muted))
                            .on_hover_text("Close")
                            .clicked()
                        {
                            state.open = false;
                        }
                    });
                });
                ui.gap(Space::Base);

                // The browser wallet, and the picker that reaches one.
                let mut button = WalletButton::new().chrome(PickerChrome::Bare);
                match button.show(ui, self.connector) {
                    WalletAction::Connect(provider) => {
                        action = Some(WalletPanelAction::Connect(provider));
                    }
                    WalletAction::Disconnect => {
                        action = Some(WalletPanelAction::Disconnect);
                    }
                    WalletAction::None => {}
                }

                ui.gap(Space::Lg);
                // The roster's box is RESERVED, not discovered. A scroll view
                // sized from "whatever is left" inside a centred, content-sized
                // area reproduces its own clamp — the area measures the clamped
                // view, the view is then clamped to that — so a scrolling roster
                // settles at whatever the first frame allowed. Reserving
                // `min(what the roster wants, a share of the screen)` keeps the
                // popup tight around a short roster and scrollable for a long
                // one, on every frame.
                let budget = ctx.content_rect().height() * 0.6;
                let viewport = roster_viewport(ui, self.entries).min(budget);
                ui.allocate_ui_with_layout(
                    vec2(ui.available_width(), viewport),
                    Layout::top_down(Align::Min),
                    |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt(("wallet-panel-roster", self.config.command_id))
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                if let Some(editor) = wallet_editor::show(
                                    ui,
                                    &mut state.editor,
                                    self.entries,
                                    &self.body_config(),
                                )
                                .action
                                {
                                    action = Some(WalletPanelAction::Editor(editor));
                                }
                            });
                    },
                );
            });

        if modal.should_close() {
            state.open = false;
        }
        if !state.open {
            // Closing the manager takes the half-typed handle and the open add
            // form with it: both belong to the surface that just went away, and
            // reopening onto a stale form reads as state the reader left behind.
            state.editor.input.clear();
            state.editor.adding = false;
        }
        action
    }

    /// The editor's config with its heading suppressed: the popup's title row
    /// already names the roster, and two headings on one surface is one too many.
    fn body_config(&self) -> WalletEditorConfig<'a> {
        let c = self.config;
        WalletEditorConfig {
            heading: "",
            placeholder: c.placeholder,
            empty_text: c.empty_text,
            add_title: c.add_title,
            command_id: c.command_id,
        }
    }
}

// ============================================================================
// The summary line
// ============================================================================

/// How tall the roster is, before any scrolling: every row at its own height,
/// the heading above them, and the failure lines under the rows that have one.
///
/// An estimate would do if it were generous — but a generous one leaves a gap
/// under the last row, and a tight one clips it. These are the numbers
/// [`wallet_editor::show`] lays out with, read from the same theme it reads
/// them from, so the two agree without the editor reporting a height back.
fn roster_viewport(ui: &Ui, entries: &[WalletEditorEntry]) -> f32 {
    let row_h = wallet_editor::row_height(ui);
    // One row for the heading (the `+` rides in it), and one for the empty
    // state's line when there is nothing to list.
    let rows = (entries.len() as f32).max(1.0);
    let failures = entries
        .iter()
        .filter(|e| matches!(e.status, WalletEntryStatus::Failed(_)))
        .count() as f32;
    row_h
        // The ambient spacing around the gap the editor draws under its heading,
        // and around the block of rows: the editor only tightens the spacing
        // BETWEEN rows, so these two come from whatever `Ui` it was handed.
        + ui.spacing().item_spacing.y * 2.0
        + ui.space(Space::Base)
        + rows * row_h
        + (rows - 1.0).max(0.0) * ui.space(Space::Xs)
        + failures * ui.text_size(TextSize::Xs)
}

/// Which chain an entry's key reaches, or that nothing is known yet.
///
/// Derived through [`Submission::classify`] — the roster's own classifier, not a
/// second sniffer beside it. A handle whose lookup has not landed is not yet an
/// address, so it is counted nowhere until it resolves into one; its mark is
/// still drawn, which is where "2 resolving" comes from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Chain {
    Cardano,
    Evm,
    Unknown,
}

fn chain_of(key: &str) -> Chain {
    match Submission::classify(key) {
        Some(Submission::Account(_)) => Chain::Evm,
        // A payment address is Cardano's too — it is a row the app cannot use,
        // which the `failed` count says, not a chain it is not on.
        Some(_) => Chain::Cardano,
        None => Chain::Unknown,
    }
}

#[derive(Default)]
struct Tally {
    cardano: usize,
    evm: usize,
    linked: usize,
    busy: usize,
    failed: usize,
}

impl Tally {
    fn of(entries: &[WalletEditorEntry]) -> Self {
        let mut t = Self::default();
        for entry in entries {
            match chain_of(&entry.key) {
                Chain::Cardano => t.cardano += 1,
                Chain::Evm => t.evm += 1,
                Chain::Unknown => {}
            }
            if entry.origin == WalletOrigin::Linked {
                t.linked += 1;
            }
            if entry.status.is_busy() {
                t.busy += 1;
            }
            if matches!(entry.status, WalletEntryStatus::Failed(_)) {
                t.failed += 1;
            }
        }
        t
    }
}

/// The counts beside the marks, as one line.
///
/// Order is the reader's order of interest: what the roster reaches, what has
/// been proven, and only then what is still moving or broken.
fn counts_line(entries: &[WalletEditorEntry]) -> String {
    let t = Tally::of(entries);
    let mut parts: Vec<String> = Vec::new();
    if t.cardano > 0 {
        parts.push(format!("{} Cardano", t.cardano));
    }
    if t.evm > 0 {
        parts.push(format!("{} EVM", t.evm));
    }
    if t.linked > 0 {
        parts.push(format!("{} linked", t.linked));
    }
    if t.busy > 0 {
        parts.push(format!("{} resolving", t.busy));
    }
    if t.failed > 0 {
        parts.push(format!("{} failed", t.failed));
    }
    if parts.is_empty() {
        // Every entry is a handle with nothing happening to it. Rare, but an
        // empty line under a strip of marks reads as a bug.
        parts.push(format!("{} wallets", entries.len()));
    }
    parts.join(" · ")
}

/// One mark per wallet, in the two systems the rows use; returns how many were
/// left off the end.
fn draw_marks(ui: &mut Ui, entries: &[WalletEditorEntry]) -> usize {
    let colours = ui.tokens().color;
    let size = ui.text_size(TextSize::Xs);
    let radius = size * 0.32;
    let step = radius * 2.6;
    let shown = entries.len().min(PIP_CAP);
    let (strip, resp) = ui.allocate_exact_size(vec2(step * shown as f32, size), Sense::hover());
    resp.on_hover_text(
        "One mark per wallet — tinted by where it came from (typed, browser, linked), \
         hollow while it resolves, red if it failed",
    );

    let mark_ink = |entry: &WalletEditorEntry, ui: &Ui| entry.origin.ink().of(ui);
    let mut marks: Vec<(egui::Pos2, Option<Color32>)> = Vec::with_capacity(shown);
    for (i, entry) in entries.iter().take(shown).enumerate() {
        let at = egui::pos2(strip.left() + radius + step * i as f32, strip.center().y);
        let colour = match &entry.status {
            WalletEntryStatus::Failed(_) => Some(colours.error),
            status if status.is_busy() => None,
            _ => Some(mark_ink(entry, ui)),
        };
        marks.push((at, colour));
    }

    let painter = ui.painter();
    for (at, colour) in marks {
        match colour {
            Some(colour) => painter.circle_filled(at, radius, colour),
            None => painter.circle_stroke(at, radius, Stroke::new(1.0_f32, colours.text_muted)),
        };
    }
    entries.len() - shown
}
