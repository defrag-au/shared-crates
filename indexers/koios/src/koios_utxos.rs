//! Response models for Koios's extended UTxO + account-asset endpoints
//! (`/address_utxos`, `/credential_utxos`, `/account_utxos`,
//! `/account_assets`).
//!
//! All of these are POST endpoints that accept an *array* of inputs and
//! return the rows tagged with the owning address / stake address, so a
//! single request can serve many lookups at once — prefer the `_batch`
//! methods on [`crate::KoiosApi`] when resolving more than one entity.

use serde::Deserialize;

/// A complete UTxO as returned by the extended (`_extended=true`) UTxO
/// endpoints. Mirrors Koios's `utxo_infos` schema. `asset_list` and
/// `inline_datum` are only populated when `_extended` is set.
#[derive(Debug, Clone, Deserialize)]
pub struct KoiosUtxo {
    pub tx_hash: String,
    pub tx_index: u32,
    pub address: String,
    /// Lovelace value (Koios returns this as a string).
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub value: u64,
    #[serde(default)]
    pub stake_address: Option<String>,
    #[serde(default)]
    pub payment_cred: Option<String>,
    #[serde(default)]
    pub datum_hash: Option<String>,
    #[serde(default)]
    pub inline_datum: Option<KoiosInlineDatum>,
    #[serde(default)]
    pub reference_script: Option<serde_json::Value>,
    /// Native assets on the UTxO. Koios returns `null` (not `[]`) for
    /// ADA-only UTxOs, hence `Option`.
    #[serde(default)]
    pub asset_list: Option<Vec<KoiosUtxoAsset>>,
    #[serde(default)]
    pub is_spent: Option<bool>,
}

/// Inline datum: the raw CBOR (`bytes`) plus its Plutus-JSON representation
/// (`value`, detailed-schema).
#[derive(Debug, Clone, Deserialize)]
pub struct KoiosInlineDatum {
    pub bytes: String,
    pub value: serde_json::Value,
}

/// A single native asset entry on a UTxO.
#[derive(Debug, Clone, Deserialize)]
pub struct KoiosUtxoAsset {
    pub policy_id: String,
    /// Asset name in hex (may be empty).
    pub asset_name: String,
    #[serde(default)]
    pub fingerprint: Option<String>,
    #[serde(default)]
    pub decimals: Option<u8>,
    /// Quantity (Koios returns this as a string).
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub quantity: u64,
}

impl KoiosUtxoAsset {
    /// Concatenated `policy_id || asset_name_hex`, matching the
    /// Maestro/`cardano-assets` "unit" convention.
    #[must_use]
    pub fn unit(&self) -> String {
        format!("{}{}", self.policy_id, self.asset_name)
    }
}

/// A `(unit, amount)` pair. Lovelace is represented with the unit
/// `"lovelace"`, matching Maestro's `AddressUtxo::assets` shape so call
/// sites can migrate with minimal churn.
#[derive(Debug, Clone)]
pub struct UtxoAmount {
    pub unit: String,
    pub amount: u64,
}

impl KoiosUtxo {
    /// Lovelace plus every native asset as `(unit, amount)` pairs
    /// (lovelace unit == `"lovelace"`).
    #[must_use]
    pub fn amounts(&self) -> Vec<UtxoAmount> {
        let assets = self.asset_list.as_deref().unwrap_or(&[]);
        let mut out = Vec::with_capacity(assets.len() + 1);
        out.push(UtxoAmount {
            unit: "lovelace".to_string(),
            amount: self.value,
        });
        for a in assets {
            out.push(UtxoAmount {
                unit: a.unit(),
                amount: a.quantity,
            });
        }
        out
    }

    /// Amount of a specific unit (`"lovelace"` or `policy_id||asset_name`).
    #[must_use]
    pub fn amount_of(&self, unit: &str) -> u64 {
        if unit == "lovelace" {
            return self.value;
        }
        self.asset_list
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .find(|a| a.unit() == unit)
            .map_or(0, |a| a.quantity)
    }

    /// True if the UTxO holds any quantity of the given unit.
    #[must_use]
    pub fn holds_unit(&self, unit: &str) -> bool {
        self.asset_list
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .any(|a| a.unit() == unit)
    }

    /// The inline datum's Plutus-JSON value, if present.
    #[must_use]
    pub fn datum_value(&self) -> Option<&serde_json::Value> {
        self.inline_datum.as_ref().map(|d| &d.value)
    }

    /// The inline datum's raw CBOR hex, if present.
    #[must_use]
    pub fn datum_bytes(&self) -> Option<&str> {
        self.inline_datum.as_ref().map(|d| d.bytes.as_str())
    }

    /// True if this UTxO sits at a script payment address (marketplace
    /// "franken" address, plutus-locked, etc.). Mirrors the bech32 prefix
    /// check jpg-store-mirror uses.
    #[must_use]
    fn is_script_address(&self) -> bool {
        self.address.starts_with("addr1w")
            || self.address.starts_with("addr1x")
            || self.address.starts_with("addr1z")
            || self.address.starts_with("addr_test1w")
            || self.address.starts_with("addr_test1x")
            || self.address.starts_with("addr_test1z")
    }
}

/// Convert a Koios extended UTxO into the shared `cardano_assets::UtxoApi`
/// used across the tx-building workers. This is the Koios counterpart to
/// maestro's `From<AddressUtxo> for UtxoApi`, so call sites migrating off
/// Maestro keep the same `.into()` ergonomics.
///
/// Native assets whose unit fails to parse into an `AssetId` are skipped
/// (matching the `filter_map(|a| a.unit.parse().ok())` convention elsewhere).
/// `tags` are derived from the UTxO's on-chain shape so downstream selection
/// (collateral, change) can exclude datum-/script-bearing UTxOs.
impl From<&KoiosUtxo> for cardano_assets::UtxoApi {
    fn from(u: &KoiosUtxo) -> Self {
        let assets = u
            .asset_list
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .filter_map(|a| {
                a.unit()
                    .parse::<cardano_assets::AssetId>()
                    .ok()
                    .map(|asset_id| cardano_assets::AssetQuantity {
                        asset_id,
                        quantity: a.quantity,
                    })
            })
            .collect();

        let mut tags = Vec::new();
        if u.datum_hash.is_some() || u.inline_datum.is_some() {
            tags.push(cardano_assets::UtxoTag::HasDatum);
        }
        if u.reference_script.is_some() {
            tags.push(cardano_assets::UtxoTag::HasScriptRef);
        }
        if u.is_script_address() {
            tags.push(cardano_assets::UtxoTag::ScriptAddress);
        }

        cardano_assets::UtxoApi {
            tx_hash: u.tx_hash.clone(),
            output_index: u.tx_index,
            lovelace: u.value,
            assets,
            tags,
        }
    }
}

impl From<KoiosUtxo> for cardano_assets::UtxoApi {
    fn from(u: KoiosUtxo) -> Self {
        (&u).into()
    }
}

/// A single asset holding from `/account_assets`, tagged with its owning
/// stake address (batch requests return rows for every requested stake).
#[derive(Debug, Clone, Deserialize)]
pub struct KoiosAccountAsset {
    #[serde(default)]
    pub stake_address: Option<String>,
    pub policy_id: String,
    /// Asset name in hex (may be empty).
    pub asset_name: String,
    #[serde(default)]
    pub fingerprint: Option<String>,
    #[serde(default)]
    pub decimals: Option<u8>,
    /// Balance (Koios returns this as a string).
    #[serde(with = "wasm_safe_serde::u64_required")]
    pub quantity: u64,
}

impl KoiosAccountAsset {
    /// Concatenated `policy_id || asset_name_hex`, matching the
    /// Maestro/`cardano-assets` "unit" convention.
    #[must_use]
    pub fn unit(&self) -> String {
        format!("{}{}", self.policy_id, self.asset_name)
    }
}
