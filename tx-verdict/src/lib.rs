//! `tx-verdict` — one transaction, reduced to a VERDICT.
//!
//! # Why this exists
//!
//! A transaction gets rendered in several places — a feed row in the app, a
//! social card behind a shared link, a Discord notification — and each was
//! reading the raw row and reaching its own conclusion. The social card had
//! learned, one live mistake at a time, what a transaction *is*: that a
//! listing's price is what somebody wanted and not income; that a two-item
//! lot must not be named after one member; that on a collection-offer fill
//! the money says "to them" while the items say "bought from them", and the
//! items are right. The feed row, reading the same data independently, made
//! exactly those mistakes again.
//!
//! So the conclusion is a type, and the renderers render it. A verdict is
//! ANSWERS, not facts to be ranked later: the ranking is the point.
//!
//! # What is here, and what is not
//!
//! This crate is the SHAPE — [`TxVerdict`] and everything inside it — plus
//! the string rules that read no row type: how a lot is named
//! ([`common_stem`], [`lot_headline`]), how an on-chain name becomes text
//! ([`asset_name`]), how a wallet is displayed ([`display_target`]).
//!
//! It is NOT the projections. Building a verdict from a wallet-relative
//! `FlowRow` or a policy-relative `PolicyRow` lives beside those rows, because
//! the rows do; building one from a `TxInsight` can live beside that. Each
//! producer knows its own input; every consumer knows only this.
//!
//! # Viewpoint is the load-bearing field
//!
//! *A link must not pretend to a point of view it does not have.* A wallet's
//! own side can say "bought"; a policy watching a unit move between two
//! strangers has nobody to be "us", so it states the pair and no verb. That
//! is enforced by the type: [`Verb`] exists only inside [`Viewpoint::Wallet`].
//!
//! # Two absences, kept apart
//!
//! [`Party::BelowFloor`] resolves by walking deeper — offering to is true and
//! actionable. [`Party::Ambiguous`] never resolves, and offering to deepen for
//! it is a lie. They must not collapse into an `Option`.

use serde::{Deserialize, Serialize};

/// How firmly a party is named. `chain-ledger`'s enum, not a redeclaration:
/// a venue's `seller_stake` is `Derived` from a market event, a resolved
/// output is `Observed`, and a name typed in by a person is `Asserted`.
pub use chain_ledger::Basis;

// ============================================================================
// The verdict
// ============================================================================

/// Which of the five things a transaction is. Decided once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    /// Something changed hands for money. The price is the headline.
    Settlement,
    /// Listings, offers, cancellations — intentions, often several at once.
    /// No item represents it and no price describes it.
    VenueBatch,
    /// Assets came into existence. The COUNT is the headline; the ADA that
    /// rode with them is min-UTxO carrier, not income.
    Minted,
    Burned,
    /// No subject and no price. The net is the only figure there is.
    Movement,
}

/// What a figure argues. NOT a sign: `−7.63 ₳` on a purchase is negative and
/// completely ordinary, where a partial figure is unsigned and wants a
/// warning. Renderers map this to colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    Positive,
    Negative,
    Neutral,
    Caution,
}

/// The one figure that says what this was.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Headline {
    /// Preformatted — `10 ₳`, `2 × MachineHeadz`, `6 listed · 2 offers made`.
    /// The producer owns formatting because it owns the decimals and symbol.
    pub value: String,
    /// A word after the figure — `total`, `lot of 2`. Only ever for a LOT,
    /// where `10 ₳` beside `2 × MachineHeadz` reads as ten each about as
    /// readily as ten for the pair.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qualifier: Option<String>,
    pub tone: Tone,
}

/// Who was on one side — or which kind of nobody.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Party {
    /// A named party, and how firmly.
    Known { label: String, basis: Basis },
    /// The source output sits below the walk floor. Walking deeper finds it —
    /// render as a placeholder, and offer to reach further back.
    BelowFloor,
    /// Several parties on this side. Stated, never guessed, never offered a
    /// deepen: it does not resolve however far the walk goes.
    Ambiguous { count: usize },
}

/// What this wallet DID, when there is a wallet whose side we are on.
///
/// `Bought`/`Sold` come from which way the ITEMS went, never from the money.
/// `SentTo`/`ReceivedFrom` are the money-derived fallback for shapes with no
/// subject to ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verb {
    Bought,
    Sold,
    SentTo,
    ReceivedFrom,
}

impl Verb {
    /// The clause between the two parties: `$boef` **bought from** `$them`.
    pub fn phrase(self) -> &'static str {
        match self {
            Verb::Bought => "bought from",
            Verb::Sold => "sold to",
            Verb::SentTo => "sent to",
            Verb::ReceivedFrom => "received from",
        }
    }

    /// One word, for a facet chip.
    pub fn word(self) -> &'static str {
        match self {
            Verb::Bought => "bought",
            Verb::Sold => "sold",
            Verb::SentTo => "sent",
            Verb::ReceivedFrom => "received",
        }
    }
}

/// Whose point of view this verdict is from — and therefore what it may say.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Viewpoint {
    /// One wallet's own side. A verb is available.
    Wallet {
        who: String,
        verb: Verb,
        other: Party,
    },
    /// No side: a policy watching a unit move between two strangers. The pair
    /// is stated symmetrically, with no verb of ownership.
    Pair { from: Party, to: Party },
    /// One actor and no other side — a mint, a burn, a wallet with no
    /// counterparty recorded.
    Sole { who: String },
}

/// One item: enough to ask for its artwork and read its name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub policy: String,
    pub name_hex: String,
}

/// What the verdict leads with, visually.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Art {
    /// The traded or minted items, front-first, capped for display by the
    /// producer. A two-item sale drawn as ONE picture reads as a one-item
    /// sale — the picture is what a reader takes in first.
    Prints {
        items: Vec<Item>,
    },
    /// A venue's mark — for a batch no single item represents.
    Mark {
        venue: String,
    },
    None,
}

/// What a facet is, so a renderer can pick a chip style without parsing text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TagKind {
    Venue,
    Shape,
    Verb,
}

/// A filterable facet. Navigation, not narration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tag {
    pub label: String,
    pub kind: TagKind,
}

/// One transaction, already reduced to answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxVerdict {
    pub shape: Shape,
    /// Tx hash, hex.
    pub tx: String,
    /// What kind of thing this was — `collection offer accepted · wayup`.
    /// Labels the headline; must not compete with it.
    pub kicker: String,
    pub headline: Headline,
    /// What was traded — `2 × MachineHeadz`. Never one member's name for a
    /// lot; see [`common_stem`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    pub view: Viewpoint,
    pub when_unix: i64,
    pub art: Art,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<Tag>,
    /// The thing that changes the reading — "2 items in this transaction",
    /// "813.4 ₳ locked into contracts". Short: on a row it sits beside chips.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caution: Option<String>,
    /// The reconciliation line — `wallet net −7.6295 ₳`. True, and not the
    /// point. `None` for a policy, which has no lovelace to reconcile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footnote: Option<String>,
}

// ============================================================================
// String rules that read no row type
// ============================================================================

/// An asset's on-chain name, if it is legible.
///
/// # The CIP-67 label comes off first
///
/// A CIP-68 asset name begins with a four-byte binary label — `000de140` for
/// an NFT, `000643b0` for its reference token — which is emphatically not
/// text. Decoding without stripping it yields something unprintable, this
/// returns `None`, and the renderer silently loses the item's name. That is
/// exactly what happened on the first live social card: artwork and price, no
/// name, on a collection that is entirely CIP-68.
///
/// `Asset::strip_any_cip67_prefix` is the shared definition of which labels
/// exist, and it passes a bare CIP-25 name through untouched — so this one
/// call covers both standards without a branch here that could disagree with
/// the registry elsewhere.
///
/// This is the ON-CHAIN name, NOT a registry display name: for an NFT there is
/// usually no registry entry to consult, which is why anything still
/// unprintable after the strip is dropped rather than escaped into a card.
pub fn asset_name(name_hex: &str) -> Option<String> {
    let bare = cardano_assets::Asset::strip_any_cip67_prefix(name_hex);
    let bytes = hex_decode(&bare)?;
    let text = String::from_utf8(bytes).ok()?;
    let legible = text
        .chars()
        .all(|c| !c.is_control() && (c.is_ascii_graphic() || c == ' '));
    (legible && !text.trim().is_empty()).then(|| text.trim().to_string())
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// `11 × Spanner`, `11 items`, or the one name — the headline for a set of
/// assets with no price to lead with.
///
/// One item is named. SEVERAL ARE NOT, because naming a lot after one of its
/// members is a claim about which item the sale was — "KWIC Playable #2057"
/// above "2 items in this transaction" reads as one sale with an extra thrown
/// in, when it was one lot of two. Where the members share a stem, that stem
/// is the honest thing to say; where they do not, a bare count is.
pub fn lot_headline(names: &[String], count: u32) -> String {
    match (count, common_stem(names)) {
        (1, _) => names
            .first()
            .cloned()
            .unwrap_or_else(|| "1 item".to_string()),
        (n, Some(stem)) => format!("{n} × {stem}"),
        (n, None) => format!("{n} items"),
    }
}

/// The shared leading words of a set of item names, if there is a real one.
///
/// `["KWIC Playable #2057", "KWIC Playable #1899"]` → `KWIC Playable`.
///
/// # The prefix has to be cut back off the identifier
///
/// A raw common prefix stops wherever the names happen to diverge, which is
/// usually PART WAY THROUGH the number that distinguishes them. `KWIC Playable
/// #2057` and `KWIC Playable #2109` share `KWIC Playable #2` — and `2 × KWIC
/// Playable #2` reads as two copies of item 2, which is the same lie as naming
/// the lot after one member, told with a different string. That shipped.
///
/// So trailing DIGITS come off before trailing punctuation: it is the digits
/// that are a fragment of an identifier, and stripping them turns
/// `KWIC Playable #2` into `KWIC Playable #` into `KWIC Playable`. It also
/// rescues the unseparated case — `HouseOfTitans1232` and `HouseOfTitans1233`
/// share `HouseOfTitans123`, which becomes `HouseOfTitans` rather than being
/// thrown away.
///
/// A digit that is genuinely part of a name is safe, because it will not be in
/// trailing position: `Series2 Alpha` and `Series2 Beta` diverge after the
/// separator, so the prefix is `Series2 ` and only the space comes off. A
/// digit is stripped exactly when the names diverge INSIDE it, which is when
/// it is a fragment.
///
/// REFUSED when the result is shorter than `MIN_STEM` or when there is only
/// one name to draw it from. Two characters shared by coincidence are not a
/// collection name, and printing them as one is a confident invention.
pub fn common_stem(names: &[String]) -> Option<String> {
    /// Below this a shared prefix is more likely coincidence than a name.
    const MIN_STEM: usize = 3;

    let (first, rest) = names.split_first()?;
    if rest.is_empty() {
        return None;
    }
    let mut len = first.chars().count();
    for other in rest {
        len = len.min(
            first
                .chars()
                .zip(other.chars())
                .take_while(|(a, b)| a == b)
                .count(),
        );
    }
    let stem: String = first.chars().take(len).collect();
    let stem = stem
        .trim_end_matches(|c: char| c.is_ascii_digit())
        .trim_end_matches(|c: char| !c.is_alphanumeric())
        .to_string();
    (stem.chars().count() >= MIN_STEM).then_some(stem)
}

/// `$boef` stays; a raw key is middle-elided. One rule, so a card's title and
/// its image — or a card and the app — cannot disagree about the wallet.
pub fn display_target(target: &str) -> String {
    if target.starts_with('$') || target.chars().count() <= 20 {
        return target.to_string();
    }
    let chars: Vec<char> = target.chars().collect();
    let head: String = chars.iter().take(12).collect();
    let tail: String = chars[chars.len() - 6..].iter().collect();
    format!("{head}…{tail}")
}

/// `offer_accepted` → `offer accepted`. Case is left alone: a kicker sits in
/// muted small text above the figure, where a capital would compete with the
/// headline it labels.
pub fn despug(s: &str) -> String {
    s.replace('_', " ")
}

/// A venue slug a logo bucket is keyed by.
///
/// Lower-cased and stripped of anything a `logo://` scheme will not accept,
/// so a venue the market ledger names in some other shape still resolves
/// rather than failing to parse at a renderer.
pub fn venue_slug(venue: &str) -> String {
    venue
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(
            |c| match c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                true => c,
                false => '-',
            },
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(text: &str) -> String {
        text.bytes().map(|b| format!("{b:02x}")).collect()
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    // The `common_stem` cases below are the social card's own — each one
    // shipped wrong once. They are the spec.

    #[test]
    fn an_unrelated_pair_gets_no_invented_name() {
        assert_eq!(common_stem(&names(&["Alpha One", "Alberto"])), None);
    }

    #[test]
    fn a_numbered_collection_yields_its_stem() {
        assert_eq!(
            common_stem(&names(&["KWIC Playable #2057", "KWIC Playable #1899"])),
            Some("KWIC Playable".to_string())
        );
    }

    #[test]
    fn one_name_is_not_a_stem() {
        assert_eq!(common_stem(&names(&["KWIC Playable #2057"])), None);
    }

    #[test]
    fn a_prefix_that_stops_mid_number_is_cut_back_to_the_name() {
        // Shared `KWIC Playable #2` — and `2 × KWIC Playable #2` would read as
        // two copies of item 2.
        assert_eq!(
            common_stem(&names(&["KWIC Playable #2057", "KWIC Playable #2109"])),
            Some("KWIC Playable".to_string())
        );
    }

    #[test]
    fn an_unseparated_name_still_yields_its_stem() {
        assert_eq!(
            common_stem(&names(&["HouseOfTitans1232", "HouseOfTitans1233"])),
            Some("HouseOfTitans".to_string())
        );
    }

    #[test]
    fn a_digit_that_is_part_of_the_name_survives() {
        assert_eq!(
            common_stem(&names(&["Series2 Alpha", "Series2 Beta"])),
            Some("Series2".to_string())
        );
    }

    #[test]
    fn lot_headline_names_one_and_counts_many() {
        assert_eq!(lot_headline(&names(&["Spanner #1"]), 1), "Spanner #1");
        assert_eq!(lot_headline(&[], 1), "1 item");
        assert_eq!(
            lot_headline(&names(&["Spanner #1", "Spanner #2"]), 11),
            "11 × Spanner"
        );
        assert_eq!(lot_headline(&names(&["Ab", "Cd"]), 2), "2 items");
    }

    #[test]
    fn asset_name_strips_a_cip67_label_and_refuses_binary() {
        assert_eq!(
            asset_name(&hex("MachineHeadz527")).as_deref(),
            Some("MachineHeadz527")
        );
        // A CIP-68 user token: label first, then the name.
        assert_eq!(
            asset_name(&format!("000de140{}", hex("MachineHeadz527"))).as_deref(),
            Some("MachineHeadz527")
        );
        // Control bytes are not a name.
        assert_eq!(asset_name("0001"), None);
        assert_eq!(asset_name(""), None);
        assert_eq!(asset_name("abc"), None, "odd length is not hex");
    }

    #[test]
    fn display_target_keeps_handles_and_elides_keys() {
        assert_eq!(display_target("$boef"), "$boef");
        let key = "addr1q8k6zq3v9m2n5p8s1t4w7y0b3e6h9k2n5q8t1w4y7h6nqg";
        let shown = display_target(key);
        assert!(shown.starts_with("addr1q8k6zq"), "{shown}");
        assert!(shown.ends_with("7h6nqg"), "{shown}");
        assert!(shown.contains('…'));
    }

    #[test]
    fn venue_slug_is_bucket_safe() {
        assert_eq!(venue_slug(" JPG Store "), "jpg-store");
        assert_eq!(venue_slug("wayup"), "wayup");
    }

    /// The verdict is a wire type for the bots and the notifier as well as an
    /// in-memory one for the app: it must round-trip, and a `Pair` must not
    /// acquire a verb on the way through.
    #[test]
    fn a_verdict_round_trips_and_a_pair_has_no_verb() {
        let v = TxVerdict {
            shape: Shape::Settlement,
            tx: "ab".repeat(32),
            kicker: "collection offer accepted · wayup".into(),
            headline: Headline {
                value: "10 ₳".into(),
                qualifier: Some("lot of 2".into()),
                tone: Tone::Positive,
            },
            subject: Some("2 × MachineHeadz".into()),
            view: Viewpoint::Pair {
                from: Party::Known {
                    label: "$elchapojr".into(),
                    basis: Basis::Derived,
                },
                to: Party::BelowFloor,
            },
            when_unix: 1_788_408_449,
            art: Art::Prints { items: vec![] },
            tags: vec![],
            caution: None,
            footnote: None,
        };
        let json = serde_json::to_string(&v).unwrap();
        assert!(!json.contains("verb"), "{json}");
        assert!(json.contains("below_floor"), "{json}");
        let back: TxVerdict = serde_json::from_str(&json).unwrap();
        assert_eq!(back, v);
    }
}
