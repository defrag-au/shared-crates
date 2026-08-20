//! PartyFinder — hunt down a wallet by ANY of its names, then watch it.
//!
//! People arrive at a project view wanting to find *one* wallet and follow it
//! through a time interval. But a wallet carries several identifiers — a stake
//! key (the party key), any number of payment addresses, and zero or more
//! handles — and which one someone has in their clipboard is anyone's guess.
//! So search resolves **any** of them:
//!
//! - handle, with or without the leading `$` (`whale`, `$whale`)
//! - stake key, exact / prefix / substring
//! - payment address, prefix / **suffix** / substring — people paste tails
//! - the operator's label
//!
//! Choosing a result **pins** the party in the shared [`Selection`], so every
//! face emphasises it; the spine's brush is the interval. Find → pin → watch.
//!
//! [`AliasIndex`] is a plain, I/O-free struct the app builds from its data
//! (the project ledger records addresses and handles for tracked wallets); the
//! widget only reads it. [`PartyFinder`] renders through the crate's
//! presentational [`TypeaheadSearch`], so it looks like every other picker.

use egui::{Color32, Ui};

use crate::selection::Selection;
use crate::typeahead_search::{TypeaheadOption, TypeaheadSearch};

/// Everything a wallet may be called.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WalletIdentity {
    /// The party key (stake address, or the payment address for stakeless).
    pub key: String,
    /// Operator label, if any. Never invented.
    pub label: Option<String>,
    /// Payment addresses seen for this party.
    pub addresses: Vec<String>,
    /// ADA Handles held, WITHOUT the `$`.
    pub handles: Vec<String>,
}

impl WalletIdentity {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            ..Default::default()
        }
    }

    pub fn label(mut self, l: impl Into<String>) -> Self {
        self.label = Some(l.into());
        self
    }

    pub fn address(mut self, a: impl Into<String>) -> Self {
        self.addresses.push(a.into());
        self
    }

    pub fn handle(mut self, h: impl Into<String>) -> Self {
        let h = h.into();
        self.handles.push(h.trim_start_matches('$').to_string());
        self
    }

    /// The best display name: label, else first handle (with `$`), else the
    /// elided key. Never a guess.
    pub fn display(&self) -> String {
        if let Some(l) = &self.label
            && !l.is_empty()
        {
            return l.clone();
        }
        if let Some(h) = self.handles.first() {
            return format!("${h}");
        }
        elide(&self.key)
    }
}

/// A searchable set of wallet identities.
#[derive(Debug, Clone, Default)]
pub struct AliasIndex {
    entries: Vec<WalletIdentity>,
}

/// How strongly a query matched — lower is better. Exposed so callers can
/// explain a match if they want to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchTier {
    HandleExact,
    HandlePrefix,
    StakeExact,
    LabelExact,
    LabelPrefix,
    StakePrefix,
    AddressPrefix,
    AddressSuffix,
    HandleSubstring,
    LabelSubstring,
    StakeSubstring,
    AddressSubstring,
}

impl AliasIndex {
    pub fn new(entries: Vec<WalletIdentity>) -> Self {
        Self { entries }
    }

    pub fn push(&mut self, id: WalletIdentity) {
        self.entries.push(id);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, key: &str) -> Option<&WalletIdentity> {
        self.entries.iter().find(|e| e.key == key)
    }

    pub fn iter(&self) -> impl Iterator<Item = &WalletIdentity> {
        self.entries.iter()
    }

    /// Rank every IDENTIFIER against `query` — one hit per matching name, not
    /// one per wallet.
    ///
    /// Flat on purpose. A wallet with handles `$shuffler` and `$curiousfutures`
    /// produces two rows, each titled by its own handle; folding them under a
    /// "primary" alias hid `$curiousfutures` behind `$shuffler` and made the
    /// list unreadable. Ties keep index order (first appearance in the ledger).
    pub fn find(&self, query: &str, limit: usize) -> Vec<AliasHit<'_>> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let q_handle = q.trim_start_matches('$');
        let mut hits: Vec<(MatchTier, usize, String, &WalletIdentity)> = Vec::new();
        for (i, e) in self.entries.iter().enumerate() {
            hits_for(e, &q, q_handle, |tier, matched| {
                hits.push((tier, i, matched, e))
            });
        }
        hits.sort_by(|a, b| (a.0, a.1, &a.2).cmp(&(b.0, b.1, &b.2)));
        hits.into_iter()
            .take(limit)
            .map(|(tier, _, matched, identity)| AliasHit {
                identity,
                matched,
                tier,
            })
            .collect()
    }
}

/// One matched identifier — the flat unit of a search result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasHit<'a> {
    pub identity: &'a WalletIdentity,
    /// The identifier that matched, ready to show: `$handle`, the label, the
    /// stake key, or an address.
    pub matched: String,
    pub tier: MatchTier,
}

impl AliasHit<'_> {
    /// Party key of the wallet this identifier belongs to.
    pub fn key(&self) -> &str {
        &self.identity.key
    }
}

/// Emit every identifier of `e` that matches, with its tier.
fn hits_for(e: &WalletIdentity, q: &str, q_handle: &str, mut emit: impl FnMut(MatchTier, String)) {
    if !q_handle.is_empty() {
        for h in &e.handles {
            let hl = h.to_lowercase();
            let tier = if hl == q_handle {
                MatchTier::HandleExact
            } else if hl.starts_with(q_handle) {
                MatchTier::HandlePrefix
            } else if hl.contains(q_handle) {
                MatchTier::HandleSubstring
            } else {
                continue;
            };
            emit(tier, format!("${h}"));
        }
    }
    let kl = e.key.to_lowercase();
    let stake_tier = if kl == q {
        Some(MatchTier::StakeExact)
    } else if kl.starts_with(q) {
        Some(MatchTier::StakePrefix)
    } else if kl.contains(q) {
        Some(MatchTier::StakeSubstring)
    } else {
        None
    };
    if let Some(t) = stake_tier {
        emit(t, e.key.clone());
    }
    if let Some(l) = &e.label {
        let ll = l.to_lowercase();
        let tier = if ll == q {
            Some(MatchTier::LabelExact)
        } else if ll.starts_with(q) {
            Some(MatchTier::LabelPrefix)
        } else if ll.contains(q) {
            Some(MatchTier::LabelSubstring)
        } else {
            None
        };
        if let Some(t) = tier {
            emit(t, l.clone());
        }
    }
    for a in &e.addresses {
        let al = a.to_lowercase();
        let tier = if al.starts_with(q) {
            MatchTier::AddressPrefix
        } else if al.ends_with(q) {
            MatchTier::AddressSuffix
        } else if al.contains(q) {
            MatchTier::AddressSubstring
        } else {
            continue;
        };
        emit(tier, a.clone());
    }
}

/// Caller-owned state for the finder (query text + dropdown highlight).
#[derive(Debug, Default, Clone)]
pub struct PartyFinderState {
    pub query: String,
    pub highlight: usize,
    /// `(party key, the identifier it was chosen BY)` — so the pinned chip says
    /// `$curiousfutures` when that is what you searched for, not the wallet's
    /// other handle. Cleared when the pin no longer matches.
    pub chosen_as: Option<(String, String)>,
}

pub struct PartyFinderResponse {
    /// A party was chosen (and pinned) this frame.
    pub chosen: Option<String>,
    /// The pin was cleared this frame.
    pub cleared: bool,
}

/// The finder: a search box that resolves any wallet identifier and pins the
/// result in the shared selection, plus a chip for what's currently pinned.
pub struct PartyFinder<'a> {
    id_salt: &'a str,
    index: &'a AliasIndex,
    state: &'a mut PartyFinderState,
    selection: &'a mut Selection,
    placeholder: &'a str,
    accent: Option<Color32>,
    limit: usize,
}

impl<'a> PartyFinder<'a> {
    pub fn new(
        id_salt: &'a str,
        index: &'a AliasIndex,
        state: &'a mut PartyFinderState,
        selection: &'a mut Selection,
    ) -> Self {
        Self {
            id_salt,
            index,
            state,
            selection,
            placeholder: "Find a wallet: $handle, stake1…, addr1…, or a label",
            accent: None,
            limit: 12,
        }
    }

    pub fn placeholder(mut self, p: &'a str) -> Self {
        self.placeholder = p;
        self
    }

    pub fn accent(mut self, c: Color32) -> Self {
        self.accent = Some(c);
        self
    }

    pub fn limit(mut self, n: usize) -> Self {
        self.limit = n.max(1);
        self
    }

    pub fn show(self, ui: &mut Ui) -> PartyFinderResponse {
        let Self {
            id_salt,
            index,
            state,
            selection,
            placeholder,
            accent,
            limit,
        } = self;
        let mut chosen = None;
        let cleared = false;

        // A pin dropped elsewhere invalidates the "chosen as" name.
        if state
            .chosen_as
            .as_ref()
            .is_some_and(|(k, _)| !selection.is_pinned(k))
        {
            state.chosen_as = None;
        }

        ui.horizontal(|ui| {
            // NO chip here any more. Pins are a SET now, and a row of them
            // belongs where it can be full width and stay put — the host draws
            // it. A finder that also owned the chips forced them to share a
            // narrow sidebar with the search box, and both were unusable.

            // The search box. Options are re-ranked every frame from the query;
            // the index is small (a project's tracked wallets), so that's cheap.
            // One row per matched IDENTIFIER: the name that matched is the
            // title, the stake key is the subtitle (unless the stake key IS the
            // match). Nothing else — a wallet's other handles are its own rows.
            let hits = index.find(&state.query, limit);
            let options: Vec<TypeaheadOption> = hits
                .iter()
                .map(|h| {
                    let key = h.key();
                    let title = if h.matched == key {
                        elide(key)
                    } else if h.matched.starts_with("addr1") {
                        elide(&h.matched)
                    } else {
                        h.matched.clone()
                    };
                    // The row id carries BOTH the party and the name it was
                    // found by, so the pin can be labelled by what you typed.
                    let mut opt = TypeaheadOption::new(row_id(key, &h.matched), title);
                    if h.matched != key {
                        opt = opt.subtitle(elide(key));
                    }
                    opt
                })
                .collect();
            let mut ta =
                TypeaheadSearch::new(id_salt, &mut state.query, &options, &mut state.highlight)
                    .placeholder(placeholder)
                    .empty_text("no wallet matches — try a handle, stake key, address, or label");
            if let Some(c) = accent {
                ta = ta.accent(c);
            }
            let resp = ta.show(ui);
            if let Some(id) = resp.chosen {
                let (key, matched) = split_row_id(&id);
                // ADD to the pinned set — finding a second wallet must not
                // dismiss the first, since comparing two is the normal case.
                selection.pin(key);
                state.chosen_as = Some((key.to_string(), matched.to_string()));
                state.query.clear();
                state.highlight = 0;
                chosen = Some(key.to_string());
            }
        });

        PartyFinderResponse { chosen, cleared }
    }
}

/// Row id = `key \t matched`. Tab cannot occur in a bech32 key, a handle, an
/// address or a sane label, so the split is unambiguous.
fn row_id(key: &str, matched: &str) -> String {
    format!("{key}\t{matched}")
}

fn split_row_id(id: &str) -> (&str, &str) {
    id.split_once('\t').unwrap_or((id, id))
}

fn elide(key: &str) -> String {
    if key.len() <= 20 {
        return key.to_string();
    }
    format!("{}…{}", &key[..12], &key[key.len() - 6..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> AliasIndex {
        AliasIndex::new(vec![
            WalletIdentity::new("stake1u98f5mr0mn8tv2kqndk5cwen4uasc7cewlzdklz6y664zacl9lvjz")
                .label("S1 treasury")
                .address("addr1qya2ezfd8vp3rr8gfr5ew4sr2jtjh46q79p9dyt4wpma29dzpf84yzsel5235v8gkga829l8q5ugdfxyntz52gynyrdslxqgl8")
                .handle("$mekka")
                .handle("mekkalabs"),
            WalletIdentity::new("stake1uxf2xjgwe5drvzqa9pabcdefghijklmnopqrstuvwxyz0123456789")
                .handle("whale")
                .address("addr1qxwhale0000000000000000000000000000000000000000000000tail99"),
            WalletIdentity::new("stake1uy0x3exj6jgh3nd3fkxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"),
        ])
    }

    // ── end-to-end: does choosing actually PIN? ───────────────────────────
    // The unit tests above cover ranking. They do not cover the thing that was
    // broken in the app: a hit that is ranked, rendered, and then unreachable.

    use egui::{Event, PointerButton, Pos2, RawInput, Rect, pos2, vec2};

    /// egui 0.34 deprecates top-level `Panel::show(ctx, …)` in favour of
    /// `show_inside(ui)`, but exposes no root `Ui` to show them inside — its own
    /// crate docs still call `.show(ctx, …)`, and it silences the lint
    /// internally the same way. Revisit when upstream lands the replacement.
    #[expect(deprecated)]
    fn run_finder(
        ctx: &egui::Context,
        ix: &AliasIndex,
        state: &mut PartyFinderState,
        sel: &mut Selection,
        events: Vec<Event>,
    ) -> PartyFinderResponse {
        let mut out = PartyFinderResponse {
            chosen: None,
            cleared: false,
        };
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(700.0, 600.0))),
            events,
            ..Default::default()
        };
        ctx.begin_pass(raw);
        // The app hosts the finder in a TOP PANEL, above the faces. A panel
        // clips its contents, so the dropdown has to be reachable from there —
        // testing it in a bare CentralPanel would not prove that.
        egui::Panel::top("bar").show(ctx, |ui| {
            out = PartyFinder::new("f", ix, state, sel).show(ui);
        });
        egui::CentralPanel::default().show(ctx, |_ui| {});
        let _ = ctx.end_pass();
        out
    }

    fn click(pos: Pos2) -> Vec<Vec<Event>> {
        vec![
            vec![Event::PointerMoved(pos)],
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
            vec![Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        ]
    }

    fn key(k: egui::Key) -> Vec<Event> {
        vec![Event::Key {
            key: k,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }]
    }

    /// Clicking a result pins that wallet in the shared selection.
    #[test]
    fn clicking_a_result_pins_the_wallet() {
        let ctx = egui::Context::default();
        crate::icons::install_fonts(&ctx);
        let ix = index();
        let mut state = PartyFinderState {
            query: "whale".into(),
            ..Default::default()
        };
        let mut sel = Selection::default();
        run_finder(&ctx, &ix, &mut state, &mut sel, vec![]);
        // First row sits below the input row inside the panel.
        let row1 = pos2(200.0, 46.0 + 6.0 + 4.0 + 23.0);
        let mut resp = None;
        for ev in click(row1) {
            resp = Some(run_finder(&ctx, &ix, &mut state, &mut sel, ev));
        }
        let resp = resp.unwrap();
        assert!(resp.chosen.is_some(), "a click on a result must choose it");
        assert_eq!(
            sel.pinned,
            vec!["stake1uxf2xjgwe5drvzqa9pabcdefghijklmnopqrstuvwxyz0123456789".to_string()],
            "and the choice must land in the SHARED selection, not just the response"
        );
        assert!(state.query.is_empty(), "the box clears for the next search");
    }

    /// Searching for a second wallet ADDS it. Comparing two is the normal case
    /// in an investigation, and a finder that replaced the pin made you hold
    /// the first one in your head.
    #[test]
    fn finding_a_second_wallet_keeps_the_first() {
        let ctx = egui::Context::default();
        crate::icons::install_fonts(&ctx);
        let ix = index();
        let mut sel = Selection::default();

        let mut state = PartyFinderState {
            query: "mekka".into(),
            ..Default::default()
        };
        run_finder(&ctx, &ix, &mut state, &mut sel, vec![]);
        for ev in click(pos2(200.0, 20.0)) {
            run_finder(&ctx, &ix, &mut state, &mut sel, ev);
        }
        run_finder(&ctx, &ix, &mut state, &mut sel, key(egui::Key::Enter));
        assert_eq!(sel.pinned.len(), 1);

        state.query = "whale".into();
        run_finder(&ctx, &ix, &mut state, &mut sel, vec![]);
        for ev in click(pos2(200.0, 20.0)) {
            run_finder(&ctx, &ix, &mut state, &mut sel, ev);
        }
        run_finder(&ctx, &ix, &mut state, &mut sel, key(egui::Key::Enter));
        assert_eq!(sel.pinned.len(), 2, "both wallets are watched");
        assert!(
            sel.is_selected(&sel.pinned[0].clone()),
            "the first is still lit"
        );
    }

    /// Enter pins the highlighted result, and the chip is named by the
    /// identifier you searched BY.
    #[test]
    fn enter_pins_and_the_chip_remembers_what_you_typed() {
        let ctx = egui::Context::default();
        crate::icons::install_fonts(&ctx);
        let ix = AliasIndex::new(vec![
            WalletIdentity::new("stake1uxwcw3rr36r8q9e5eg")
                .handle("shuffler")
                .handle("curiousfutures"),
        ]);
        let mut state = PartyFinderState {
            query: "curiousfutures".into(),
            ..Default::default()
        };
        let mut sel = Selection::default();
        run_finder(&ctx, &ix, &mut state, &mut sel, vec![]);
        // Focus the input, then Enter.
        for ev in click(pos2(200.0, 20.0)) {
            run_finder(&ctx, &ix, &mut state, &mut sel, ev);
        }
        run_finder(&ctx, &ix, &mut state, &mut sel, key(egui::Key::Enter));
        assert!(sel.is_pinned("stake1uxwcw3rr36r8q9e5eg"));
        assert_eq!(
            state.chosen_as.as_ref().map(|(_, as_)| as_.as_str()),
            Some("$curiousfutures"),
            "not the wallet's first handle, $shuffler"
        );
    }

    #[test]
    fn handle_with_or_without_dollar_wins() {
        let ix = index();
        let r = ix.find("$mekka", 5);
        assert_eq!(r[0].identity.label.as_deref(), Some("S1 treasury"));
        assert_eq!(r[0].tier, MatchTier::HandleExact);
        assert_eq!(r[0].matched, "$mekka");
        let r = ix.find("mekka", 5);
        assert_eq!(r[0].tier, MatchTier::HandleExact);
        // "mekka" is also a PREFIX of "mekkalabs" on the same wallet — flat
        // means that is its own row, titled by its own handle.
        assert_eq!(r.len(), 2);
        assert_eq!(r[1].matched, "$mekkalabs");
        assert_eq!(r[1].tier, MatchTier::HandlePrefix);
    }

    /// The `$curiousfutures` case: a wallet whose FIRST handle is `$shuffler`
    /// must still produce a row titled `$curiousfutures` when that is what you
    /// searched for. Folding it under a "primary" alias hid it.
    #[test]
    fn every_handle_is_its_own_row() {
        let ix = AliasIndex::new(vec![
            WalletIdentity::new("stake1uxwcw3rr36r8q9e5eg")
                .handle("shuffler")
                .handle("curiousfutures"),
        ]);
        let r = ix.find("curiousfutures", 5);
        assert_eq!(r.len(), 1);
        assert_eq!(
            r[0].matched, "$curiousfutures",
            "titled by the match, not the first handle"
        );
        assert_eq!(r[0].key(), "stake1uxwcw3rr36r8q9e5eg");
        // And nothing about the other handle leaks into this row.
        assert!(!r[0].matched.contains("shuffler"));
    }

    #[test]
    fn stake_address_and_label_all_resolve() {
        let ix = index();
        assert_eq!(ix.find("stake1u98f5", 5)[0].tier, MatchTier::StakePrefix);
        assert_eq!(ix.find("treasury", 5)[0].tier, MatchTier::LabelSubstring);
        assert_eq!(ix.find("S1 Treasury", 5)[0].tier, MatchTier::LabelExact);
        // address prefix AND a pasted tail
        assert_eq!(ix.find("addr1qya2ez", 5)[0].tier, MatchTier::AddressPrefix);
        let tail = ix.find("tail99", 5);
        assert_eq!(tail[0].identity.handles, vec!["whale"]);
        assert_eq!(tail[0].tier, MatchTier::AddressSuffix);
        assert!(tail[0].matched.starts_with("addr1qxwhale"));
    }

    #[test]
    fn ranking_prefers_handles_over_addresses_and_is_stable() {
        let mut ix = index();
        // A third wallet whose ADDRESS contains "whale" — must rank below the
        // wallet whose HANDLE is whale.
        ix.push(WalletIdentity::new("stake1uzzz").address("addr1whalezzz"));
        let r = ix.find("whale", 5);
        // The handle row leads. Both address rows (the whale wallet's own
        // `addr1qxwhale…` and stake1uzzz's) come after it, in index order.
        assert_eq!(r[0].matched, "$whale");
        assert_eq!(r[0].tier, MatchTier::HandleExact);
        let address_rows: Vec<&str> = r[1..].iter().map(|h| h.key()).collect();
        assert!(address_rows.contains(&"stake1uzzz"));
        assert!(r[1..].iter().all(|h| h.tier > r[0].tier));
    }

    #[test]
    fn display_never_invents_a_name() {
        let ix = index();
        let bare = ix
            .get("stake1uy0x3exj6jgh3nd3fkxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx")
            .unwrap();
        assert!(bare.display().starts_with("stake1uy0x3e"));
        assert!(bare.display().contains('…'));
        assert_eq!(ix.find("whale", 1)[0].identity.display(), "$whale");
        assert!(ix.find("", 5).is_empty());
        assert!(ix.find("nomatch-at-all", 5).is_empty());
    }

    #[test]
    fn row_id_round_trips() {
        let id = row_id("stake1abc", "$curiousfutures");
        assert_eq!(split_row_id(&id), ("stake1abc", "$curiousfutures"));
        // A legacy/plain id degrades to (id, id) rather than panicking.
        assert_eq!(split_row_id("stake1abc"), ("stake1abc", "stake1abc"));
    }
}
