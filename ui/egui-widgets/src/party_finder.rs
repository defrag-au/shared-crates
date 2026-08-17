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

use egui::{Color32, Ui, vec2};

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

    /// Rank every identity against `query`. Ties keep index order (stable),
    /// which for a project ledger is first-appearance order.
    pub fn find(&self, query: &str, limit: usize) -> Vec<(&WalletIdentity, MatchTier)> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let q_handle = q.trim_start_matches('$');
        let mut hits: Vec<(usize, MatchTier, &WalletIdentity)> = Vec::new();
        for (i, e) in self.entries.iter().enumerate() {
            if let Some(t) = tier_for(e, &q, q_handle) {
                hits.push((i, t, e));
            }
        }
        hits.sort_by_key(|(i, t, _)| (*t, *i));
        hits.into_iter()
            .take(limit)
            .map(|(_, t, e)| (e, t))
            .collect()
    }
}

fn tier_for(e: &WalletIdentity, q: &str, q_handle: &str) -> Option<MatchTier> {
    let mut best: Option<MatchTier> = None;
    let mut consider = |t: MatchTier| {
        if best.is_none_or(|b| t < b) {
            best = Some(t);
        }
    };
    if !q_handle.is_empty() {
        for h in &e.handles {
            let hl = h.to_lowercase();
            if hl == q_handle {
                consider(MatchTier::HandleExact);
            } else if hl.starts_with(q_handle) {
                consider(MatchTier::HandlePrefix);
            } else if hl.contains(q_handle) {
                consider(MatchTier::HandleSubstring);
            }
        }
    }
    let kl = e.key.to_lowercase();
    if kl == q {
        consider(MatchTier::StakeExact);
    } else if kl.starts_with(q) {
        consider(MatchTier::StakePrefix);
    } else if kl.contains(q) {
        consider(MatchTier::StakeSubstring);
    }
    if let Some(l) = &e.label {
        let ll = l.to_lowercase();
        if ll == q {
            consider(MatchTier::LabelExact);
        } else if ll.starts_with(q) {
            consider(MatchTier::LabelPrefix);
        } else if ll.contains(q) {
            consider(MatchTier::LabelSubstring);
        }
    }
    for a in &e.addresses {
        let al = a.to_lowercase();
        if al.starts_with(q) {
            consider(MatchTier::AddressPrefix);
        } else if al.ends_with(q) {
            consider(MatchTier::AddressSuffix);
        } else if al.contains(q) {
            consider(MatchTier::AddressSubstring);
        }
    }
    best
}

/// Caller-owned state for the finder (query text + dropdown highlight).
#[derive(Debug, Default, Clone)]
pub struct PartyFinderState {
    pub query: String,
    pub highlight: usize,
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
        let mut cleared = false;

        ui.horizontal(|ui| {
            // What's pinned, as a chip with a clear.
            if let Some(pinned) = selection.pinned.clone() {
                let name = index
                    .get(&pinned)
                    .map(|e| e.display())
                    .unwrap_or_else(|| elide(&pinned));
                let frame = egui::Frame::new()
                    .fill(ui.visuals().selection.bg_fill.linear_multiply(0.35))
                    .stroke(egui::Stroke::new(
                        1.0_f32,
                        ui.visuals().selection.stroke.color,
                    ))
                    .corner_radius(egui::CornerRadius::same(4))
                    .inner_margin(egui::Margin::symmetric(6, 3));
                frame.show(ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(6.0, 0.0);
                    ui.label(egui::RichText::new(name).small().strong());
                    if ui
                        .small_button("×")
                        .on_hover_text("stop watching")
                        .clicked()
                    {
                        selection.clear_pin();
                        cleared = true;
                    }
                });
            }

            // The search box. Options are re-ranked every frame from the query;
            // the index is small (a project's tracked wallets), so that's cheap.
            let hits = index.find(&state.query, limit);
            let options: Vec<TypeaheadOption> = hits
                .iter()
                .map(|(e, tier)| {
                    let mut sub = Vec::new();
                    for h in &e.handles {
                        sub.push(format!("${h}"));
                    }
                    if e.label.is_some() || !e.handles.is_empty() {
                        sub.push(elide(&e.key));
                    }
                    let mut opt = TypeaheadOption::new(e.key.clone(), e.display());
                    if !sub.is_empty() {
                        opt = opt.subtitle(sub.join(" · "));
                    }
                    if matches!(
                        tier,
                        MatchTier::AddressPrefix
                            | MatchTier::AddressSuffix
                            | MatchTier::AddressSubstring
                    ) {
                        opt = opt.badge("address", crate::ChipVariant::Muted);
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
            if let Some(key) = resp.chosen {
                selection.pinned = Some(key.clone());
                state.query.clear();
                state.highlight = 0;
                chosen = Some(key);
            }
        });

        PartyFinderResponse { chosen, cleared }
    }
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

    #[test]
    fn handle_with_or_without_dollar_wins() {
        let ix = index();
        let r = ix.find("$mekka", 5);
        assert_eq!(r[0].0.label.as_deref(), Some("S1 treasury"));
        assert_eq!(r[0].1, MatchTier::HandleExact);
        let r = ix.find("mekka", 5);
        assert_eq!(r[0].1, MatchTier::HandleExact);
        // prefix of a second handle on the same wallet
        let r = ix.find("mekkal", 5);
        assert_eq!(r[0].1, MatchTier::HandlePrefix);
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn stake_address_and_label_all_resolve() {
        let ix = index();
        assert_eq!(ix.find("stake1u98f5", 5)[0].1, MatchTier::StakePrefix);
        assert_eq!(ix.find("treasury", 5)[0].1, MatchTier::LabelSubstring);
        assert_eq!(ix.find("S1 Treasury", 5)[0].1, MatchTier::LabelExact);
        // address prefix AND a pasted tail
        assert_eq!(ix.find("addr1qya2ez", 5)[0].1, MatchTier::AddressPrefix);
        let tail = ix.find("tail99", 5);
        assert_eq!(tail[0].0.handles, vec!["whale"]);
        assert_eq!(tail[0].1, MatchTier::AddressSuffix);
    }

    #[test]
    fn ranking_prefers_handles_over_addresses_and_is_stable() {
        let mut ix = index();
        // A third wallet whose ADDRESS contains "whale" — must rank below the
        // wallet whose HANDLE is whale.
        ix.push(WalletIdentity::new("stake1uzzz").address("addr1whalezzz"));
        let r = ix.find("whale", 5);
        assert_eq!(r[0].0.handles, vec!["whale"]);
        assert_eq!(r[1].0.key, "stake1uzzz");
        assert!(r[0].1 < r[1].1);
    }

    #[test]
    fn display_never_invents_a_name() {
        let ix = index();
        let bare = ix
            .get("stake1uy0x3exj6jgh3nd3fkxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx")
            .unwrap();
        assert!(bare.display().starts_with("stake1uy0x3e"));
        assert!(bare.display().contains('…'));
        let whale = ix.find("whale", 1)[0].0;
        assert_eq!(whale.display(), "$whale");
        assert!(ix.find("", 5).is_empty());
        assert!(ix.find("nomatch-at-all", 5).is_empty());
    }
}
