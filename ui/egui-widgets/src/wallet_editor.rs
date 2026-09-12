//! `WalletEditor` — the reader's own roster of wallets: add one by handle or
//! address, watch it resolve, drop it again.
//!
//! Backs the collection-ownership frontend's "My Wallets" sidebar. UI only — it
//! classifies what was typed and emits a [`WalletEditorAction`]; resolution,
//! persistence and fetching are the host's.
//!
//! ## Not [`wallet_list`](crate::wallet_list)
//!
//! That one is the **operator's** view of a client's wallets: grouped by role,
//! with archive actions, and explicitly no add form. This is the **reader's own**
//! set, which they build and prune themselves. Same noun, different owner, so
//! they stay separate.
//!
//! ## Two colour systems, on purpose
//!
//! A row says two independent things and they must not be conflated:
//!
//! - **[`WalletOrigin`]** — where the entry came from, which tints the name.
//!   Permanent.
//! - **[`WalletEntryStatus`]** — what the app is doing with it, which drives the
//!   leading indicator. Transient.
//!
//! The first version had the host compute a colour from a `bool` the widget
//! already held (`is_browser_wallet` in, `accent: Ink` also in), which is the
//! "override that is really a default" shape: every call site passed cyan for
//! browser and green for typed, so it was not a choice, it was a derivation
//! written out at the call site. One named enum replaces both fields.
//!
//! ## What the reader typed is classified here, once
//!
//! [`Submission::classify`] turns the input box into a named thing, so the host
//! matches on a variant instead of re-sniffing prefixes. It previously sniffed
//! in two places — the widget, and the caller's add handler across a repo
//! boundary — with two different sets of prefixes. That drift had teeth:
//!
//! - `stake_test1…` matched neither branch and was mangled into
//!   `$stake_test1…`, so every preprod address became a handle lookup for a
//!   handle that cannot exist.
//! - `addr1…` was passed through as though it were a stake address, so a
//!   payment address went silently to a stake-keyed endpoint.
//!
//! Both are now variants a host has to answer for, which is the point: a
//! payment address is a real thing a reader will paste, and "we do not support
//! that yet" is a better answer than a wrong lookup.

use egui::{Align, Layout, RichText, Sense, Ui, vec2};

use crate::icons::PhosphorIcon;
use crate::theme::{Ink, Radius, Space, SpaceExt, TextSize, ThemeExt, Token};
use crate::viewport::Breakpoint;

// ============================================================================
// Types
// ============================================================================

/// Where an entry came from. Tints the name; never changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WalletOrigin {
    /// Typed in by hand, as an address or a handle.
    #[default]
    Typed,
    /// Auto-connected from the browser wallet extension. Shown with a badge,
    /// because the reader did not put it there and may wonder why it is.
    Browser,
}

impl WalletOrigin {
    pub const ALL: [Self; 2] = [Self::Typed, Self::Browser];

    /// The name's tint. Derived, not configured — see the module header.
    pub const fn ink(self) -> Ink {
        match self {
            Self::Typed => Ink::Token(Token::AccentGreen),
            Self::Browser => Ink::Token(Token::AccentCyan),
        }
    }

    /// The trailing badge, if this origin needs explaining.
    pub const fn badge(self) -> Option<&'static str> {
        match self {
            Self::Typed => None,
            Self::Browser => Some("browser"),
        }
    }
}

/// What the app is currently doing with an entry. Drives the leading indicator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletEntryStatus {
    /// Handle lookup in progress.
    Resolving,
    /// Fetching bundle data.
    Loading,
    /// Bundle processed — data ready.
    Ready,
    /// Something went wrong. The message is shown under the row.
    Failed(String),
}

impl WalletEntryStatus {
    /// Whether the app is waiting on the network for this entry.
    pub const fn is_busy(&self) -> bool {
        matches!(self, Self::Resolving | Self::Loading)
    }
}

/// One wallet in the roster.
///
/// Carries the identity, not a rendered string. The widget decides how to show
/// it, which is what lets an address elide to the width actually available
/// rather than to a width the host guessed.
#[derive(Clone, Debug, PartialEq)]
pub struct WalletEditorEntry {
    /// The stake address — or the raw handle, while that is all we have.
    pub key: String,
    /// The resolved ADA Handle **without** the `$`, when there is one.
    pub handle: Option<String>,
    pub status: WalletEntryStatus,
    pub origin: WalletOrigin,
}

impl WalletEditorEntry {
    /// A typed-in entry, still resolving.
    pub fn resolving(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            handle: None,
            status: WalletEntryStatus::Resolving,
            origin: WalletOrigin::Typed,
        }
    }

    pub fn status(mut self, status: WalletEntryStatus) -> Self {
        self.status = status;
        self
    }

    pub fn origin(mut self, origin: WalletOrigin) -> Self {
        self.origin = origin;
        self
    }

    pub fn handle(mut self, handle: impl Into<String>) -> Self {
        self.handle = Some(handle.into());
        self
    }

    /// What the row shows: the handle if it has one, else the elided key.
    pub fn display(&self) -> String {
        match &self.handle {
            Some(h) => format!("${h}"),
            None => crate::truncate_hex(&self.key, 10, 6),
        }
    }
}

/// What the reader typed, named.
///
/// The host matches on this instead of sniffing prefixes itself — see the
/// module header for the two bugs that came of sniffing in two places.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Submission {
    /// An ADA Handle, **without** the leading `$` whether or not one was typed.
    Handle(String),
    /// A stake address, mainnet or test.
    StakeAddress(String),
    /// A payment address, mainnet or test. A distinct variant because it is NOT
    /// a stake address and cannot be used as one — a host that cannot convert it
    /// should say so rather than look it up and get nothing.
    PaymentAddress(String),
}

impl Submission {
    /// Classify raw input. `None` for anything blank.
    ///
    /// Anything without a recognised address prefix is taken as a handle, which
    /// is how a reader typing a bare `boef` gets what they meant.
    pub fn classify(raw: &str) -> Option<Self> {
        let t = raw.trim();
        if t.is_empty() {
            return None;
        }
        if let Some(handle) = t.strip_prefix('$') {
            let handle = handle.trim();
            return (!handle.is_empty()).then(|| Self::Handle(handle.to_string()));
        }
        // `stake_test1` before `stake1` would be wrong the other way round too —
        // neither is a prefix of the other — but both must be present, which is
        // exactly what the single `starts_with("stake1")` test used to miss.
        if t.starts_with("stake1") || t.starts_with("stake_test1") {
            return Some(Self::StakeAddress(t.to_string()));
        }
        if t.starts_with("addr1") || t.starts_with("addr_test1") {
            return Some(Self::PaymentAddress(t.to_string()));
        }
        Some(Self::Handle(t.to_string()))
    }

    /// The text a host would store or display for this submission.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Handle(s) | Self::StakeAddress(s) | Self::PaymentAddress(s) => s,
        }
    }
}

/// Appearance and copy.
pub struct WalletEditorConfig<'a> {
    pub heading: &'a str,
    pub subtitle: Option<&'a str>,
    pub placeholder: &'a str,
    /// Shown where the list would be when there is nothing in it. A roster that
    /// renders nothing reads as broken rather than as empty.
    pub empty_text: &'a str,
}

impl Default for WalletEditorConfig<'_> {
    fn default() -> Self {
        Self {
            heading: "My Wallets",
            subtitle: None,
            // Address first, matching the holder-lookup field elsewhere in the
            // same sidebar — two inputs that take the same things should not
            // describe them in two different orders.
            placeholder: "stake1... or $handle",
            empty_text: "No wallets yet",
        }
    }
}

/// Persistent widget state — the input buffer, and nothing else.
#[derive(Default, Clone, Debug)]
pub struct WalletEditorState {
    pub input: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WalletEditorAction {
    /// The reader submitted something, already classified.
    Add(Submission),
    /// Remove the entry at this index.
    Remove(usize),
}

pub struct WalletEditorResponse {
    pub action: Option<WalletEditorAction>,
}

// ============================================================================
// Widget
// ============================================================================

/// The column geometry every row shares.
struct Spine {
    /// Leading indicator column — dot, spinner or error mark.
    mark: f32,
    /// Trailing remove column.
    remove: f32,
    gap: f32,
    row_h: f32,
    icon: f32,
}

impl Spine {
    fn measure(ui: &Ui) -> Self {
        let gap = ui.space(Space::Sm);
        let icon = ui.text_size(TextSize::Sm);
        let text = ui.text_size(TextSize::Base);
        Self {
            mark: icon + gap,
            remove: icon + gap * 2.0,
            gap,
            // Finger-sized where a finger is what is available. A roster is a
            // list of tap targets, and 20px of it is not one.
            row_h: (text + ui.space(Space::Base) * 2.0).max(Breakpoint::from_ui(ui).min_touch()),
            icon,
        }
    }
}

/// Render the roster.
pub fn show(
    ui: &mut Ui,
    state: &mut WalletEditorState,
    entries: &[WalletEditorEntry],
    config: &WalletEditorConfig<'_>,
) -> WalletEditorResponse {
    crate::icons::ensure_fonts(ui);
    let colors = ui.tokens().color;
    let spine = Spine::measure(ui);
    let mut action: Option<WalletEditorAction> = None;

    ui.label(
        RichText::new(config.heading)
            .color(colors.text_secondary)
            .size(ui.text_size(TextSize::Md)),
    );
    if let Some(subtitle) = config.subtitle {
        ui.gap(Space::Xs);
        ui.label(
            RichText::new(subtitle)
                .color(colors.text_muted)
                .size(ui.text_size(TextSize::Sm)),
        );
    }
    ui.gap(Space::Base);

    // ── Add ─────────────────────────────────────────────────────────────────
    let mut submitted = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = spine.gap;
        // The field takes what the `+` does not, measured rather than guessed —
        // this was `available_width() - 32.0`, and 32 was neither the button's
        // width nor anything derivable from it.
        let button_w = spine.icon + spine.gap * 2.0;
        let field = ui.add(
            egui::TextEdit::singleline(&mut state.input)
                .hint_text(config.placeholder)
                .desired_width((ui.available_width() - button_w - spine.gap).max(40.0))
                .font(egui::FontId::monospace(ui.text_size(TextSize::Base))),
        );
        if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            submitted = true;
        }
        let (rect, resp) = ui.allocate_exact_size(vec2(button_w, spine.row_h), Sense::click());
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        PhosphorIcon::Plus.paint(
            ui.painter(),
            rect.center(),
            egui::Align2::CENTER_CENTER,
            spine.icon,
            match resp.hovered() {
                true => colors.accent,
                false => colors.accent_cyan,
            },
        );
        if resp.on_hover_text("Add wallet").clicked() {
            submitted = true;
        }
    });

    if submitted && let Some(sub) = Submission::classify(&state.input) {
        state.input.clear();
        action = Some(WalletEditorAction::Add(sub));
    }

    ui.gap(Space::Sm);

    // ── The roster ──────────────────────────────────────────────────────────
    if entries.is_empty() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_space(spine.mark);
            ui.label(
                RichText::new(config.empty_text)
                    .color(colors.text_muted)
                    .size(ui.text_size(TextSize::Sm)),
            );
        });
    }

    for (i, entry) in entries.iter().enumerate() {
        let row_h = match &entry.status {
            // An error gets its own line rather than being squeezed in beside
            // the name, where it pushed the remove button out of the column.
            WalletEntryStatus::Failed(_) => spine.row_h + ui.text_size(TextSize::Xs),
            _ => spine.row_h,
        };
        let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::hover());
        let hovered = resp.hovered();
        if hovered {
            ui.painter()
                .rect_filled(row, ui.tokens().corner(Radius::Sm), colors.bg_highlight);
        }

        let mut cell = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(row)
                .layout(Layout::left_to_right(Align::Min)),
        );
        cell.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;

            // Leading mark, in a fixed column so every name starts at one x.
            let (mark, _) = ui.allocate_exact_size(vec2(spine.mark, spine.row_h), Sense::hover());
            match &entry.status {
                WalletEntryStatus::Resolving | WalletEntryStatus::Loading => {
                    let mut m = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(mark)
                            .layout(Layout::centered_and_justified(egui::Direction::TopDown)),
                    );
                    m.spinner();
                }
                WalletEntryStatus::Ready => {
                    ui.painter().circle_filled(
                        mark.center() - vec2(spine.gap * 0.5, 0.0),
                        spine.icon * 0.22,
                        colors.accent_green,
                    );
                }
                WalletEntryStatus::Failed(_) => {
                    PhosphorIcon::Warning.paint(
                        ui.painter(),
                        mark.center() - vec2(spine.gap * 0.5, 0.0),
                        egui::Align2::CENTER_CENTER,
                        spine.icon,
                        colors.error,
                    );
                }
            }

            // Name, truncating — the one flexible column.
            let badge_w = match entry.origin.badge() {
                Some(b) => text_width(ui, b, TextSize::Xs) + spine.gap * 2.0,
                None => 0.0,
            };
            let name_w = (row.width() - spine.mark - badge_w - spine.remove).max(20.0);
            let mut name = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(egui::Rect::from_min_size(
                        egui::pos2(row.left() + spine.mark, row.top()),
                        vec2(name_w, spine.row_h),
                    ))
                    .layout(Layout::left_to_right(Align::Center)),
            );
            name.add(
                egui::Label::new(
                    RichText::new(entry.display())
                        .color(entry.origin.ink().of(ui))
                        .size(ui.text_size(TextSize::Base)),
                )
                .truncate(),
            );
            ui.add_space(name_w);

            if let Some(badge) = entry.origin.badge() {
                let (b, _) = ui.allocate_exact_size(vec2(badge_w, spine.row_h), Sense::hover());
                let mut bc = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(b)
                        .layout(Layout::centered_and_justified(egui::Direction::TopDown)),
                );
                bc.label(
                    RichText::new(badge)
                        .color(colors.text_muted)
                        .size(bc.text_size(TextSize::Xs)),
                );
            }

            // Remove, in the trailing column — one axis down the list.
            let (x_rect, x_resp) =
                ui.allocate_exact_size(vec2(spine.remove, spine.row_h), Sense::click());
            if x_resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            PhosphorIcon::X.paint(
                ui.painter(),
                x_rect.center(),
                egui::Align2::CENTER_CENTER,
                spine.icon,
                match (x_resp.hovered(), hovered) {
                    (true, _) => colors.error,
                    (false, true) => colors.text_secondary,
                    (false, false) => colors.text_muted,
                },
            );
            if x_resp
                .on_hover_text(format!("Remove {}", entry.display()))
                .clicked()
            {
                action = Some(WalletEditorAction::Remove(i));
            }
        });

        // The failure, under the name it belongs to.
        if let WalletEntryStatus::Failed(msg) = &entry.status {
            ui.painter().text(
                egui::pos2(row.left() + spine.mark, row.top() + spine.row_h),
                egui::Align2::LEFT_TOP,
                msg,
                egui::FontId::proportional(ui.text_size(TextSize::Xs)),
                colors.error,
            );
        }
    }

    WalletEditorResponse { action }
}

fn text_width(ui: &Ui, text: &str, size: TextSize) -> f32 {
    let font = egui::FontId::proportional(ui.text_size(size));
    ui.painter()
        .layout_no_wrap(text.to_owned(), font, egui::Color32::PLACEHOLDER)
        .size()
        .x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_testnet_stake_address_is_an_address_not_a_handle() {
        // The bug this replaced: only `stake1` was tested, so every preprod
        // address fell through to the handle branch and was mangled into
        // `$stake_test1…` — a lookup for a handle that cannot exist.
        assert_eq!(
            Submission::classify("stake_test1uqabc"),
            Some(Submission::StakeAddress("stake_test1uqabc".into()))
        );
        assert_eq!(
            Submission::classify("stake1u9xyz"),
            Some(Submission::StakeAddress("stake1u9xyz".into()))
        );
    }

    #[test]
    fn a_payment_address_is_its_own_thing() {
        // It used to fall through as a stake address and go silently to a
        // stake-keyed endpoint. A host cannot answer for what it cannot see.
        assert_eq!(
            Submission::classify("addr1q9abc"),
            Some(Submission::PaymentAddress("addr1q9abc".into()))
        );
        assert_eq!(
            Submission::classify("addr_test1q9abc"),
            Some(Submission::PaymentAddress("addr_test1q9abc".into()))
        );
    }

    #[test]
    fn a_handle_loses_its_dollar_exactly_once() {
        // The host stores and compares these, so `$boef` and `boef` arriving as
        // different strings is a duplicate in the roster.
        assert_eq!(
            Submission::classify("$boef"),
            Some(Submission::Handle("boef".into()))
        );
        assert_eq!(
            Submission::classify("boef"),
            Some(Submission::Handle("boef".into()))
        );
        assert_eq!(
            Submission::classify("  $boef  "),
            Some(Submission::Handle("boef".into()))
        );
    }

    #[test]
    fn nothing_typed_is_not_a_submission() {
        for blank in ["", "   ", "$", "$   "] {
            assert_eq!(Submission::classify(blank), None, "input {blank:?}");
        }
    }

    #[test]
    fn a_row_shows_its_handle_and_falls_back_to_an_elided_key() {
        let with = WalletEditorEntry::resolving("stake1uabcdefghijklmnop").handle("boef");
        assert_eq!(with.display(), "$boef");

        let without = WalletEditorEntry::resolving("stake1uabcdefghijklmnopqrstuv");
        let shown = without.display();
        assert!(shown.starts_with("stake1uabc"), "got {shown}");
        assert!(shown.contains('…') || shown.contains("..."), "got {shown}");
    }

    #[test]
    fn origin_decides_the_tint_rather_than_the_caller() {
        // Every call site computed cyan-for-browser / green-for-typed from a
        // bool the widget already had. That is a derivation, not a choice, and
        // writing it out per call site is how two of them drift.
        assert_ne!(
            WalletOrigin::Typed.ink(),
            WalletOrigin::Browser.ink(),
            "the two origins must remain distinguishable"
        );
        assert!(WalletOrigin::Typed.badge().is_none());
        assert!(WalletOrigin::Browser.badge().is_some());
    }

    #[test]
    fn only_a_finished_entry_stops_being_busy() {
        assert!(WalletEntryStatus::Resolving.is_busy());
        assert!(WalletEntryStatus::Loading.is_busy());
        assert!(!WalletEntryStatus::Ready.is_busy());
        assert!(!WalletEntryStatus::Failed("x".into()).is_busy());
    }
}
