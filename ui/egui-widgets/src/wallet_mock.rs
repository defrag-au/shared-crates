//! Pseudo-wallet profiles for LOCAL demo modes.
//!
//! A registry of demo user profiles, shared across all egui frontends, so an
//! app's demo mode can show how *different kinds of users* render without any
//! code edits — pass a profile name from the URL (e.g. `?demo&demo_profile=whale`).
//!
//! Each profile is a fabricated identity with a distinct wallet shape chosen
//! to exercise a different rendering case:
//!
//! | profile     | shape                                                        |
//! |-------------|--------------------------------------------------------------|
//! | `alice`     | default local user — modest ADA, 2 pirates, $handle, one FT  |
//! | `bob`       | default counterparty — modest ADA, 2 pirates, $handle        |
//! | `whale`     | 1.25M ADA, 40 pirates + 12 SpaceBudz — deep picker, scrolling |
//! | `minnow`    | 12.5 ADA, no assets, NO handle — empty states, stake display |
//! | `trader`    | FT-heavy — several fungible positions, quantity editing      |
//! | `hoarder`   | cluttered bag — real NFTs + several unknown policies         |
//!
//! Real mainnet policies (Black Flag `Pirate1`–`Pirate2000`, SpaceBudz) are
//! used so IIIF thumbnails and collection-name enrichment light up; fake
//! policies are obviously fake and resolve to nothing anywhere, by design.
//!
//! Nothing here talks to a wallet or the chain: identities are fabricated, and
//! [`connect`] deliberately does NOT write localStorage, so a demo run never
//! hijacks the real auto-reconnect.
//!
//! **Apps must gate activation behind `cfg!(debug_assertions)`** (or an
//! equivalent local-only check) so release builds cannot enter the mode.

use super::wallet::{ConnectionState, Network, WalletBalance, WalletConnector, WalletProvider};
use std::collections::HashMap;

/// Black Flag — a real mainnet policy (assets `Pirate1`–`Pirate2000`), so
/// IIIF thumbnails and collection-name enrichment light up in demos.
pub const DEMO_NFT_POLICY: &str = "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6";

/// SpaceBudz v1 — a second real policy (assets `SpaceBud0`–`SpaceBud9999`)
/// for profiles that should span multiple collections.
pub const DEMO_NFT_POLICY_2: &str = "d5e6bf0500378d4f0da4e8dde6becec7621cd8cbf5cbb9b87013d4cc";

/// Obviously-fake policy for a fungible "DEMO" token (exercises the
/// quantity-editing paths). Resolves to nothing anywhere, by design.
pub const DEMO_FT_POLICY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef01234567";

/// One policy's worth of holdings: `(policy_id, [(ascii_asset_name, qty)])`.
pub type Holding = (String, Vec<(String, u64)>);

/// A fabricated wallet identity + balance shape. The addresses are
/// bech32-shaped but not real — they truncate nicely for display and any
/// chain/API lookup on them returns nothing.
pub struct MockIdentity {
    /// Profile name this identity was built from (e.g. "whale").
    pub profile: String,
    /// ADA handle with the `$` — `None` exercises the truncated-stake path.
    pub handle: Option<String>,
    pub stake: String,
    pub address: String,
    /// Total lovelace.
    pub lovelace: u64,
    /// Asset names (ASCII) held under [`DEMO_NFT_POLICY`] — the "primary
    /// collection", used by trade demos as the tradable NFTs.
    pub nft_names: Vec<String>,
    /// Additional holdings beyond the primary collection and the handle.
    pub extra: Vec<Holding>,
}

/// Names of all registered profiles (for help text / pickers).
pub fn profile_names() -> &'static [&'static str] {
    &["alice", "bob", "whale", "minnow", "trader", "hoarder"]
}

/// Look up a profile by name. Unknown names fall back to `alice` so a typo'd
/// URL still demos something.
pub fn profile(name: &str) -> MockIdentity {
    match name {
        "bob" => bob(),
        "whale" => whale(),
        "minnow" => minnow(),
        "trader" => trader(),
        "hoarder" => hoarder(),
        _ => alice(),
    }
}

/// Default local user — modest, representative wallet.
pub fn alice() -> MockIdentity {
    MockIdentity {
        profile: "alice".into(),
        handle: Some("$alice".into()),
        stake: fake_stake("alice"),
        address: fake_address("alice"),
        lovelace: 2_450_000_000,
        nft_names: vec!["Pirate84".into(), "Pirate501".into()],
        extra: vec![(DEMO_FT_POLICY.into(), vec![("DEMO".into(), 454_140)])],
    }
}

/// Default counterparty.
pub fn bob() -> MockIdentity {
    MockIdentity {
        profile: "bob".into(),
        handle: Some("$bob".into()),
        stake: fake_stake("bob"),
        address: fake_address("bob"),
        lovelace: 1_800_000_000,
        nft_names: vec!["Pirate1903".into(), "Pirate77".into()],
        extra: Vec::new(),
    }
}

/// Deep wallet: large ADA balance, many NFTs across two collections.
/// Exercises picker scrolling, per-collection accordion depth, and how large
/// numbers format.
pub fn whale() -> MockIdentity {
    MockIdentity {
        profile: "whale".into(),
        handle: Some("$whale".into()),
        stake: fake_stake("whale"),
        address: fake_address("whale"),
        lovelace: 1_250_000_000_000,
        nft_names: (2..42).map(|n| format!("Pirate{n}")).collect(),
        extra: vec![(
            DEMO_NFT_POLICY_2.into(),
            (100..112).map(|n| (format!("SpaceBud{n}"), 1)).collect(),
        )],
    }
}

/// Nearly-empty wallet, no handle. Exercises empty states, the
/// truncated-stake identity display, and insufficient-funds paths.
pub fn minnow() -> MockIdentity {
    MockIdentity {
        profile: "minnow".into(),
        handle: None,
        stake: fake_stake("minnow"),
        address: fake_address("minnow"),
        lovelace: 12_500_000,
        nft_names: Vec::new(),
        extra: Vec::new(),
    }
}

/// Fungible-token heavy wallet. Exercises quantity badges, quantity editing,
/// and FT group labelling.
pub fn trader() -> MockIdentity {
    MockIdentity {
        profile: "trader".into(),
        handle: Some("$trader".into()),
        stake: fake_stake("trader"),
        address: fake_address("trader"),
        lovelace: 5_200_000_000,
        nft_names: vec!["Pirate1200".into()],
        extra: vec![
            (DEMO_FT_POLICY.into(), vec![("DEMO".into(), 454_140)]),
            (
                fake_policy("beef"),
                vec![("MOON".into(), 12_000_000), ("STAR".into(), 250)],
            ),
            (fake_policy("cafe"), vec![("GOLD".into(), 1_000_000_000)]),
        ],
    }
}

/// Cluttered "bag" wallet: real NFTs mixed with several unknown policies.
/// Exercises unverified-group display, raw-hex fallbacks, and long lists of
/// junk — the wallet shape that motivated Defrackit.
pub fn hoarder() -> MockIdentity {
    let junk = (0..6)
        .map(|i| {
            (
                fake_policy(&format!("dead{i:02}")),
                vec![(format!("JUNK{i}"), 1 + i as u64 * 37)],
            )
        })
        .collect::<Vec<_>>();
    let mut extra = vec![(
        DEMO_NFT_POLICY_2.into(),
        vec![("SpaceBud777".into(), 1), ("SpaceBud778".into(), 1)],
    )];
    extra.extend(junk);
    MockIdentity {
        profile: "hoarder".into(),
        handle: Some("$hoarder".into()),
        stake: fake_stake("hoarder"),
        address: fake_address("hoarder"),
        lovelace: 350_000_000,
        nft_names: vec!["Pirate666".into(), "Pirate1337".into()],
        extra,
    }
}

/// ASCII → hex asset name (computed, never hand-typed).
pub fn ascii_hex(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02x}")).collect()
}

/// The profile's full balance: ADA, primary-collection NFTs, the ADA handle
/// (when the profile has one, so the top-bar `$handle` resolves), and any
/// extra holdings.
pub fn balance(id: &MockIdentity) -> WalletBalance {
    let mut assets: HashMap<String, HashMap<String, u64>> = HashMap::new();

    if !id.nft_names.is_empty() {
        let nfts: HashMap<String, u64> = id
            .nft_names
            .iter()
            .map(|name| (ascii_hex(name), 1))
            .collect();
        assets.insert(DEMO_NFT_POLICY.to_string(), nfts);
    }

    if let Some(ref handle) = id.handle {
        let handle_name = handle.trim_start_matches('$');
        assets.insert(
            wallet_pallas::ADA_HANDLE_POLICY.to_string(),
            HashMap::from([(ascii_hex(handle_name), 1)]),
        );
    }

    for (policy, holdings) in &id.extra {
        let entry = assets.entry(policy.clone()).or_default();
        for (name, qty) in holdings {
            entry.insert(ascii_hex(name), *qty);
        }
    }

    WalletBalance {
        lovelace: id.lovelace,
        assets,
    }
}

/// Mark the connector connected as this identity — WITHOUT writing
/// localStorage (unlike `apply_connect_result`), so the next real page
/// load doesn't try to auto-reconnect a wallet that doesn't exist.
pub fn connect(connector: &mut WalletConnector, id: &MockIdentity) {
    connector.connection_state = ConnectionState::Connected {
        provider: WalletProvider::Eternl,
        address: id.address.clone(),
        network: Network::Mainnet,
    };
    connector.address_hex = None;
    connector.stake_address = Some(id.stake.clone());
    connector.handle = id.handle.clone();
}

/// A bech32-shaped fake stake address, derived from the profile name so every
/// profile's identity is distinct and stable.
fn fake_stake(name: &str) -> String {
    pad_fake(&format!("stake1u9demo{name}"), 54)
}

/// A bech32-shaped fake payment address.
fn fake_address(name: &str) -> String {
    pad_fake(&format!("addr1q9demo{name}"), 72)
}

/// A fake 56-hex policy id from a short hex-ish seed.
fn fake_policy(seed: &str) -> String {
    let mut s = String::with_capacity(56);
    while s.len() < 56 {
        s.push_str(seed);
    }
    s.truncate(56);
    s
}

fn pad_fake(prefix: &str, len: usize) -> String {
    let mut s = prefix.to_string();
    while s.len() < len {
        s.push_str("demo");
    }
    s.truncate(len);
    s
}

// NOTE: no #[cfg(test)] suite here — this module (like `wallet`) only builds
// for wasm32 + the `cardano` feature, so native `cargo test` never compiles
// it. Verification is the demo screenshot loop.
