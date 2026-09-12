//! `WalletEditor` story — the reader's own wallet roster.
//!
//! The list is live: what you type is classified and added, and the mock
//! "resolution" runs on a timer so the busy → ready (and busy → failed) paths
//! are something you watch rather than something you read about in a caption.

use egui_widgets::wallet_editor::{
    self, Submission, WalletEditorConfig, WalletEditorEntry, WalletEditorState, WalletEntryStatus,
    WalletOrigin,
};

use crate::{controls, muted};

pub struct WalletEditorStoryState {
    pub editor: WalletEditorState,
    pub entries: Vec<WalletEditorEntry>,
    pub last_action: String,
    /// Rows still "resolving", as `(index, frames remaining)`. Stands in for the
    /// async handle lookup the real host runs.
    pending: Vec<(usize, u32)>,
    pub sidebar_w: f32,
}

impl Default for WalletEditorStoryState {
    fn default() -> Self {
        Self {
            editor: WalletEditorState::default(),
            entries: mock_entries(),
            last_action: String::new(),
            pending: Vec::new(),
            sidebar_w: 320.0,
        }
    }
}

fn mock_entries() -> Vec<WalletEditorEntry> {
    vec![
        WalletEditorEntry::resolving("stake1u8boef")
            .handle("boef")
            .status(WalletEntryStatus::Ready),
        WalletEditorEntry::resolving("stake1u8djo")
            .handle("djo")
            .status(WalletEntryStatus::Ready),
        // The one the reader did not add. Cyan + a badge, both derived from the
        // origin rather than chosen at the call site.
        WalletEditorEntry::resolving("stake1u8perplord")
            .handle("perplord")
            .status(WalletEntryStatus::Ready)
            .origin(WalletOrigin::Browser),
        WalletEditorEntry::resolving("curiousfutures"),
        WalletEditorEntry::resolving("stake1q8xkk4m9vhs2n7wlq3zzr5td0pmy6g4cnxj")
            .status(WalletEntryStatus::Loading),
        // A long name with no handle AND an error — the row that used to push
        // the remove button out of its column.
        WalletEditorEntry::resolving("stake1qy2ffk39dj2mmz8tt5lq0wgc3xn7v4hp9k9fp")
            .status(WalletEntryStatus::Failed("no such handle".into())),
    ]
}

pub fn show(ui: &mut egui::Ui, state: &mut WalletEditorStoryState) {
    crate::heading(ui, "WalletEditor");
    crate::caption(
        ui,
        "A reader's own roster: add by handle or address, watch it resolve, drop \
         it again. Sibling to `wallet_list`, which is the OPERATOR's view of a \
         client's wallets — same noun, different owner.",
    );
    crate::caption(
        ui,
        "Two colour systems, deliberately: the name's tint is where the entry \
         came from (permanent), the leading mark is what the app is doing with \
         it (transient). Both are derived — the origin enum decides the tint and \
         the badge, so a call site cannot pick a colour that disagrees with the \
         badge beside it.",
    );
    ui.add_space(10.0);

    // Tick the mock resolutions.
    let mut finished: Vec<usize> = Vec::new();
    for (idx, frames) in &mut state.pending {
        *frames = frames.saturating_sub(1);
        if *frames == 0 {
            finished.push(*idx);
        }
    }
    if !state.pending.is_empty() {
        ui.ctx().request_repaint();
    }
    state.pending.retain(|(_, f)| *f > 0);
    for idx in finished {
        if let Some(e) = state.entries.get_mut(idx) {
            // Every third one fails, so the error row is reachable by using the
            // widget rather than only by reading the mock data.
            e.status = match idx % 3 == 2 {
                true => WalletEntryStatus::Failed("no such handle".into()),
                false => WalletEntryStatus::Ready,
            };
            if e.handle.is_none() && !e.key.starts_with("stake1") {
                let key = e.key.clone();
                e.handle = Some(key);
            }
        }
    }

    crate::caption(ui, "The sidebar it lives in is narrow. Drag this to see what the name column does when it runs out of room.");
    controls(ui, |ui| {
        egui_widgets::slider_group::SliderGroup::new()
            .fader(
                egui_widgets::slider_group::Fader::new(
                    "sidebar",
                    &mut state.sidebar_w,
                    160.0..=520.0,
                )
                .suffix("px"),
            )
            .show(ui);
    });
    ui.add_space(8.0);

    let width = state.sidebar_w;
    ui.allocate_ui(egui::vec2(width, ui.available_height()), |ui| {
        egui::Frame::new()
            .fill(crate::bg(ui))
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui_widgets::theme::hairline(crate::highlight(ui)))
            .show(ui, |ui| {
                ui.set_width(width - 24.0);
                let config = WalletEditorConfig::default();
                let resp = wallet_editor::show(ui, &mut state.editor, &state.entries, &config);

                if let Some(action) = resp.action {
                    match action {
                        wallet_editor::WalletEditorAction::Add(sub) => {
                            // The host matches on a NAMED thing. It no longer
                            // re-sniffs prefixes, which is what let a testnet
                            // address and a payment address both go wrong.
                            state.last_action = match &sub {
                                Submission::Handle(h) => format!("Add handle ${h}"),
                                Submission::StakeAddress(a) => {
                                    format!("Add stake address {a}")
                                }
                                Submission::PaymentAddress(a) => format!(
                                    "Add PAYMENT address {a} — a real host would \
                                     refuse this or convert it, not look it up"
                                ),
                            };
                            let entry = match &sub {
                                Submission::Handle(h) => WalletEditorEntry::resolving(h.clone()),
                                Submission::StakeAddress(a) | Submission::PaymentAddress(a) => {
                                    WalletEditorEntry::resolving(a.clone())
                                        .status(WalletEntryStatus::Loading)
                                }
                            };
                            state.entries.push(entry);
                            state.pending.push((state.entries.len() - 1, 90));
                        }
                        wallet_editor::WalletEditorAction::Remove(idx) => {
                            state.last_action = format!(
                                "Remove [{idx}] {}",
                                state.entries.get(idx).map_or("?".into(), |e| e.display())
                            );
                            if idx < state.entries.len() {
                                state.entries.remove(idx);
                                // Indices shift, and a stale one would resolve
                                // the wrong row.
                                state.pending.clear();
                            }
                        }
                    }
                }
            });
    });

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);

    // ── The command the roster offers ───────────────────────────────────────
    crate::heading(ui, "What this widget told the app it can do");
    crate::caption(
        ui,
        "The roster above `offer`s a command every pass it draws. Nothing in \
         this story registered it — the widget did, from where it lives. The \
         `+` in its corner does not open the form directly; it INVOKES that \
         command, which is exactly what the palette does, so the two are one \
         path rather than two implementations that drift.",
    );
    ui.add_space(6.0);
    let offered = egui_widgets::commands::offered(ui.ctx());
    for c in &offered {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(&c.id)
                    .color(crate::tok(ui, egui_widgets::theme::Token::AccentCyan))
                    .monospace()
                    .small(),
            );
            ui.label(
                egui::RichText::new(&c.title)
                    .color(crate::secondary(ui))
                    .small(),
            );
            if let Some(g) = &c.group {
                ui.label(egui::RichText::new(g).color(muted(ui)).small());
            }
            // Firing it from here proves the point: this is a third caller,
            // and it needs to know nothing but the id.
            if ui.small_button("invoke").clicked() {
                egui_widgets::commands::invoke(ui.ctx(), c.id.clone());
            }
        });
    }
    if offered.is_empty() {
        crate::caption(ui, "nothing offered — scroll the roster back into view");
    }

    ui.add_space(6.0);
    crate::caption(
        ui,
        "Scroll the roster off screen and the entry disappears by itself: an \
         offer is stamped with the pass it happened in, and anything older than \
         one pass is dropped. A palette entry that cannot reach its widget is \
         worse than a missing one — it looks like it works and does nothing.",
    );

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);

    crate::heading(ui, "Try");
    crate::caption(
        ui,
        "· `boef` — a bare word is a handle, which is what someone typing one means.\n\
         · `stake_test1uq…` — a testnet address. Used to be mangled into `$stake_test1…`.\n\
         · `addr1q9…` — a payment address. Used to go silently to a stake-keyed lookup.\n\
         · Remove every row to see the empty state.",
    );

    ui.add_space(8.0);
    if state.last_action.is_empty() {
        ui.label(
            egui::RichText::new("no actions yet")
                .color(muted(ui))
                .small(),
        );
    } else {
        ui.label(
            egui::RichText::new(&state.last_action)
                .color(crate::secondary(ui))
                .small(),
        );
    }

    ui.add_space(10.0);
    if ui.button("Reset").clicked() {
        *state = WalletEditorStoryState::default();
    }
}
