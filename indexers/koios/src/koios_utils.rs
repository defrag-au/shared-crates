use serde::Serialize;

use crate::{koios_assets::KoiosAsset, koios_transaction::KoiosTransaction};
use std::collections::HashMap;

#[derive(Serialize, Debug, Clone)]
pub struct MovedAsset {
    pub policy_id: String,
    pub asset_name: String,
    pub quantity: f64,
    pub from_address: String,
    pub to_address: String,
}

type AssetKey = (String, String);

impl From<KoiosAsset> for AssetKey {
    fn from(value: KoiosAsset) -> Self {
        (value.policy_id.clone(), value.asset_name.clone())
    }
}

#[must_use]
pub fn trace_asset_movements(tx: &KoiosTransaction) -> Vec<MovedAsset> {
    let mut input_totals: HashMap<AssetKey, HashMap<String, f64>> = HashMap::new(); // fingerprint -> from_address -> quantity
    let mut output_totals: HashMap<AssetKey, Vec<(String, f64)>> = HashMap::new(); // fingerprint -> Vec<(to_address, quantity)>

    // Collect input quantities
    for input in &tx.inputs {
        let addr = &input.payment_addr.bech32;
        for asset in &input.asset_list {
            let qty = asset.quantity;
            input_totals
                .entry(asset.into())
                .or_default()
                .entry(addr.clone())
                .and_modify(|q| *q += qty)
                .or_insert(qty);
        }
    }

    // Collect output quantities
    for output in &tx.outputs {
        let addr = &output.payment_addr.bech32;
        for asset in &output.asset_list {
            let qty = asset.quantity;
            output_totals
                .entry(asset.into())
                .or_default()
                .push((addr.clone(), qty));
        }
    }

    let mut movements = vec![];

    // For each fingerprint, match inputs and distribute output quantities
    for (asset_key, from_map) in input_totals {
        if let Some(mut remaining_outputs) = output_totals.remove(&asset_key) {
            for (from_addr, mut remaining_input_qty) in from_map {
                for (to_addr, out_qty_ref) in &mut remaining_outputs {
                    if from_addr == *to_addr || remaining_input_qty == 0.0 {
                        continue;
                    }

                    let moved_qty = remaining_input_qty.min(*out_qty_ref);
                    if moved_qty > 0.0 {
                        movements.push(MovedAsset {
                            policy_id: asset_key.0.clone(),
                            asset_name: asset_key.1.clone(),
                            quantity: moved_qty,
                            from_address: from_addr.clone(),
                            to_address: to_addr.clone(),
                        });

                        remaining_input_qty -= moved_qty;
                        *out_qty_ref -= moved_qty;
                    }
                }
            }
        }
    }

    movements
}
