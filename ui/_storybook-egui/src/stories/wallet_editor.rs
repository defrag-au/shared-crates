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
    /// Rows mid-link: `(index, frames remaining)`. The host's real flow waits
    /// on a wallet dialog, which takes as long as the reader takes.
    linking: Vec<(usize, u32)>,
    pub sidebar_w: f32,
}

impl Default for WalletEditorStoryState {
    fn default() -> Self {
        Self {
            editor: WalletEditorState::default(),
            entries: mock_entries(),
            last_action: String::new(),
            pending: Vec::new(),
            linking: Vec::new(),
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
        // origin rather than chosen at the call site. The host can prove this
        // one — it is the connected browser wallet — so it carries the shield.
        WalletEditorEntry::resolving("stake1u8perplord")
            .handle("perplord")
            .status(WalletEntryStatus::Ready)
            .origin(WalletOrigin::Browser)
            .linkable(true),
        // Proven: a signature the server holds, so the row is green with a
        // `linked` badge and its button says unlink rather than remove.
        WalletEditorEntry::resolving("stake1u8hodler")
            .handle("hodler")
            .status(WalletEntryStatus::Ready)
            .origin(WalletOrigin::Linked),
        WalletEditorEntry::resolving("curiousfutures"),
        WalletEditorEntry::resolving("stake1q8xkk4m9vhs2n7wlq3zzr5td0pmy6g4cnxj")
            .status(WalletEntryStatus::Loading),
        // A long name with no handle AND an error — the row that used to push
        // the remove button out of its column.
        WalletEditorEntry::resolving("stake1qy2ffk39dj2mmz8tt5lq0wgc3xn7v4hp9k9fp")
            .status(WalletEntryStatus::Failed("no such handle".into())),
        // An EVM account: the same roster, an owner string from another chain.
        // No handle to resolve, so the row elides the account instead — and
        // the host can prove it, because the injected provider holds it.
        WalletEditorEntry::resolving("0x7980aa64093853cb78c927e05b88fed96e945f81")
            .status(WalletEntryStatus::Ready)
            .linkable(true),
    ]
}

pub fn show(ui: &mut egui::Ui, state: &mut WalletEditorStoryState) {
    crate::heading(ui, "WalletEditor");
    crate::caption(
        ui,
        "A reader's own roster: add by handle, address or EVM account, watch it \
         resolve, drop it again. Sibling to `wallet_list`, which is the OPERATOR's \
         view of a client's wallets — same noun, different owner.",
    );
    crate::caption(
        ui,
        "Three origins, a ladder of trust rather than three flavours of one \
         thing: blue is typed (the reader's note), cyan is the browser wallet \
         the app found, green is LINKED — a signature the server holds. A \
         surface that must not guess reads the linked set, never the roster. \
         The shield links a row the host can prove; the `×` unlinks a proven \
         one. Both are emitted as actions for the host to answer, not done by \
         the widget.",
    );
    crate::caption(
        ui,
        "The status column is a separate system, deliberately: the leading \
         mark is what the app is doing with the row (transient), the name's \
         tint and badge are where it came from (permanent).",
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
            if e.handle.is_none() && !e.key.starts_with("stake1") && !e.key.starts_with("0x") {
                let key = e.key.clone();
                e.handle = Some(key);
            }
        }
    }

    // Tick the mock links — a wallet dialog's worth of waiting, then the proof.
    let mut linked: Vec<usize> = Vec::new();
    for (idx, frames) in &mut state.linking {
        *frames = frames.saturating_sub(1);
        if *frames == 0 {
            linked.push(*idx);
        }
    }
    if !state.linking.is_empty() {
        ui.ctx().request_repaint();
    }
    state.linking.retain(|(_, f)| *f > 0);
    for idx in linked {
        if let Some(e) = state.entries.get_mut(idx) {
            e.origin = WalletOrigin::Linked;
            e.status = WalletEntryStatus::Ready;
            e.linkable = false;
        }
    }

    crate::caption(
        ui,
        "The sidebar it lives in is narrow. Drag this to see what the name column does when it runs out of room.",
    );
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
                                Submission::Account(a) => format!("Add EVM account {a}"),
                            };
                            let entry = match &sub {
                                Submission::Handle(h) => WalletEditorEntry::resolving(h.clone()),
                                Submission::StakeAddress(a) | Submission::PaymentAddress(a) => {
                                    WalletEditorEntry::resolving(a.clone())
                                        .status(WalletEntryStatus::Loading)
                                }
                                Submission::Account(a) => WalletEditorEntry::resolving(a.clone())
                                    .status(WalletEntryStatus::Loading),
                            };
                            state.entries.push(entry);
                            state.pending.push((state.entries.len() - 1, 90));
                        }
                        wallet_editor::WalletEditorAction::Unlink(idx) => {
                            state.last_action = format!(
                                "Unlink [{idx}] {} — a real host calls the server here, \
                                 because a local drop would leave the link standing",
                                state.entries.get(idx).map_or("?".into(), |e| e.display())
                            );
                            if let Some(e) = state.entries.get_mut(idx) {
                                // Back to a local note: the wallet stays in the
                                // roster, the proof is gone.
                                e.origin = WalletOrigin::Typed;
                                e.linkable = true;
                            }
                        }
                        wallet_editor::WalletEditorAction::Link(idx) => {
                            state.last_action = format!(
                                "Link [{idx}] {} — the host runs challenge → sign → verify",
                                state.entries.get(idx).map_or("?".into(), |e| e.display())
                            );
                            if let Some(e) = state.entries.get_mut(idx) {
                                e.status = WalletEntryStatus::Loading;
                            }
                            state.linking.push((idx, 90));
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
         · The shield on a row the host can prove — it goes busy, then green \
         with a `linked` badge, and its `×` starts saying unlink.\n\
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
