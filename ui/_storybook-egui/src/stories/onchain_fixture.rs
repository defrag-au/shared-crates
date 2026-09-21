//! A real Eternal Chaos piece, for the on-chain-art stories.
//!
//! `440606b6…` ("Eternal Chaos", 512 assets) is the collection this whole path
//! was built for, so the stories show *it* rather than a piece from some other
//! project that merely happens to be on-chain art. What makes it the right
//! fixture is that it differs from the rest of the corpus in two ways that
//! matter:
//!
//! - **The document is base64.** The BlockGen authority tokens in
//!   `cardano-assets`' own corpus ship `data:text/html;utf8,<html>…`, percent
//!   encoded. This collection ships `data:text/html;base64,PGh0bWw+…`. Both are
//!   live art and the extraction has to see through either encoding.
//! - **It has a cover.** A real IPFS still sits in `image`, so the collection's
//!   grid thumbnails work today — which is exactly the case the viewer's
//!   cover-vs-piece caption exists for. The thumbnail is a still captured at
//!   mint; the piece animates.
//!
//! The fixture is the raw Koios `asset_info` response, not a hand-extracted
//! inner object, so it is verifiably what the read path receives.

use cardano_assets::{AssetEnvelope, LiveArt};

/// One `asset_info` row for `EternalChaos000`, verbatim.
const ASSET_INFO: &str =
    include_str!("../../../../cardano-assets/resources/test/eternal-chaos-asset-info.json");

/// The collection's policy id.
pub const POLICY_ID: &str = "440606b6a9de303b6886bf7a9e9944f528cabf99096668000665918b";

/// The asset's name in hex — `EternalChaos000`.
pub const ASSET_NAME_HEX: &str = "457465726e616c4368616f73303030";

/// The asset's display name, as the chain carries it. Note this is *not* the
/// hex-decoded asset name: the piece is called "The Excession".
pub const DISPLAY_NAME: &str = "The Excession";

/// The piece, parsed exactly as a front end would parse it.
///
/// `None` only if the fixture stops parsing, which the story says out loud
/// rather than showing an empty frame.
#[must_use]
pub fn live_art() -> Option<LiveArt> {
    let rows: serde_json::Value = serde_json::from_str(ASSET_INFO).ok()?;
    let row = rows.get(0)?;
    let policy = row.get("policy_id")?.as_str()?;
    let name = row.get("asset_name_ascii")?.as_str()?;
    let inner = row
        .get("minting_tx_metadata")?
        .get("721")?
        .get(policy)?
        .get(name)?;
    let envelope: AssetEnvelope = serde_json::from_value(inner.clone()).ok()?;
    envelope.live_art()
}

/// The cover as the app would request it — the IIIF service at `Full`
/// (1686px), which is the largest size it keeps warm.
#[must_use]
pub fn cover_url() -> String {
    format!(
        "https://iiif.hodlcroft.com/iiif/3/{POLICY_ID}:{ASSET_NAME_HEX}/full/1686,/0/default.jpg"
    )
}

/// The piece's real traits, as the bundle returns them.
#[must_use]
pub fn traits() -> Vec<(String, String)> {
    vec![
        ("artist".into(), "Charles Machin".into()),
        ("medium".into(), "Fully On-Chain Generative CNFT".into()),
        ("formation".into(), "Arche Formation".into()),
        ("style".into(), "Warba".into()),
        ("Warp".into(), "Excession".into()),
        ("palette".into(), "Ultramarine".into()),
    ]
}

/// What the collection is, for a subtitle.
pub const COLLECTION: &str = "Eternal Chaos";
