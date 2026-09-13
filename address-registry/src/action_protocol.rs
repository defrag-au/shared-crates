//! The on-chain action protocol's deployment, per network.
//!
//! **A deployment is ONE record, never bare constants scattered across
//! workers.** Every script hash below is derived from the others by an
//! `aiken blueprint apply` chain, so a hash pasted into a second place is a
//! hash that can disagree with the first. `tools/action-protocol-deploy`
//! prints this record; it is not transcribed by hand.
//!
//! ## Why the table can be empty
//!
//! The whole chain hangs off a **seed UTxO** the operator picks at deployment
//! time: the config token's one-shot policy consumes it, the policy's hash is
//! that token's policy id, and every validator that reads the config is
//! compiled with it. So none of these hashes exist until a deployment has
//! actually happened, and a network with no entry is not a bug — it is a
//! network nobody has deployed to yet. Callers get `None` and should say so
//! plainly rather than inventing an address.
//!
//! ## What is NOT here
//!
//! Anything that can change without redeploying a validator: `ada_per_credit`,
//! `posting_cost`, the cost table, `max_debit_per_day`, the admitted
//! currencies, the sinks, the fee credential, and the authorized spender and
//! updater keys. All of that lives in the **`ProtocolConfig` UTxO**, found by
//! the token named here — which is the entire reason that UTxO exists.

use crate::registry::RegistryNetwork;

/// Where a script's bytes are parked, so a transaction can reference them
/// instead of carrying ~1.5 KB of Plutus.
///
/// **This is the one mutable field of a deployment.** `script-depot` can
/// retire a reference script to reclaim its ADA and redeploy it elsewhere,
/// which moves the UTxO while the hash stays put for ever. So bind everything
/// semantic to the hash, treat this as a cache, and let a builder that finds
/// it missing re-read the depot rather than fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReferenceScript {
    pub tx_hash: &'static str,
    pub output_index: u32,
}

/// One compiled script of the protocol.
#[derive(Debug, Clone, Copy)]
pub struct ProtocolScript {
    /// The applied script hash. For a minting policy this **is** the policy
    /// id; for a validator it is the payment credential of its address.
    pub hash: &'static str,
    /// `None` until the reference script has been parked.
    pub reference: Option<ReferenceScript>,
}

/// The action protocol on one network.
///
/// Field order follows the parameter chain, because that is the order the
/// hashes have to be produced in and the order a reader should check them.
#[derive(Debug, Clone, Copy)]
pub struct ActionProtocolDeployment {
    pub network: RegistryNetwork,

    /// Cancel-only definition registry. Takes no parameters, so its hash is
    /// fixed the moment the contracts compile and is the root of the chain.
    pub registry: ProtocolScript,
    /// The one-shot policy that marks THE config UTxO. Its hash is the
    /// `config_token` policy id.
    pub config_token_policy: ProtocolScript,
    /// Hex. `config` unless there was a reason.
    pub config_token_name: &'static str,
    /// The config validator. The marked UTxO lives at its address.
    pub protocol_config: ProtocolScript,
    /// The tank validator. Every fuel tank lives at its address.
    pub fuel: ProtocolScript,
    /// The CIP-68 pair policy. Its hash is **the fuel policy id** — what
    /// `escrow.ak` uses to tell a real tank from a look-alike.
    pub fuel_pair_policy: ProtocolScript,
    /// The escrow validator.
    pub escrow: ProtocolScript,
    /// The claim-marker policy. `escrow.ak` deliberately does NOT name it —
    /// it matches markers by name under any policy, because naming the policy
    /// back would close the parameter chain into a circle. This is here for
    /// indexers and for reading the chain, not for the validator.
    pub claim_marker_policy: ProtocolScript,

    /// The UTxO the config token's policy consumed. Recorded because it is
    /// the only input to the whole chain that is not derived from another
    /// hash — without it, none of the above can be reproduced.
    pub seed: ReferenceScript,

    /// What an escrow releases per claim settled, in lovelace. Compiled into
    /// `escrow.ak`, so it is a deployment fact rather than a setting.
    pub release_reserve_per_claim: u64,
    /// How long after a definition closes before its owner may reclaim the
    /// prizes without closing it, in **POSIX milliseconds**.
    ///
    /// Milliseconds, not slots: a Plutus validator cannot see slots at all —
    /// its only clock is the transaction's validity range, which the ledger
    /// gives as POSIX time in ms. Compiled into `escrow.ak` as a parameter,
    /// deliberately, because a config field defaulting to zero would let an
    /// owner pull the prizes the instant a window shut.
    pub settlement_grace_ms: u64,
}

/// Every deployment there is.
///
/// **Empty until a real deployment lands.** See the module note: the chain
/// hangs off a seed UTxO chosen at deploy time, so these hashes cannot be
/// written in advance. Run `tools/action-protocol-deploy`, park the scripts
/// with script-depot, and paste the record it prints.
pub static ACTION_PROTOCOL_DEPLOYMENTS: &[ActionProtocolDeployment] = &[
    // Preprod, applied 2026-09-13 from the seed below by
    // `tools/action-protocol-deploy`. Every hash except `registry`'s is
    // DERIVED from that seed, so this record and that UTxO are one fact:
    // re-run against a different seed and all six change.
    //
    // `reference` is `None` on every script — nothing is parked yet. The
    // config is minted BEFORE the scripts are deployed, because the mint
    // needs no reference script (the one-shot policy rides inline) and
    // parking first would put the seed in reach of the deployment's own coin
    // selection.
    ActionProtocolDeployment {
        network: RegistryNetwork::Testnet,
        // No parameters, so this hash is fixed by the contracts alone and is
        // the same on every network. A useful cross-check: if it ever differs
        // from a fresh `aiken build`, the contracts changed.
        registry: ProtocolScript {
            hash: "78bbd2b4a99afc6500df8c9f38300001835bf2bc7211ecb1dffecfea",
            reference: None,
        },
        config_token_policy: ProtocolScript {
            hash: "69d92aa98650b90e38efec8e765ff97da805744ba50fc77159139ba6",
            reference: None,
        },
        // `config`, hex. The name is part of what every validator is
        // compiled with, so it is not free to change.
        config_token_name: "636f6e666967",
        protocol_config: ProtocolScript {
            hash: "6554a62acfed422f457a76082dc925ea6db14e4755125ee0544a9968",
            reference: None,
        },
        fuel: ProtocolScript {
            hash: "fbd337c669878736373f4a9563659820a3248a1e045c6ab875afb106",
            reference: None,
        },
        fuel_pair_policy: ProtocolScript {
            hash: "e2d151f2421feca65f3b913aa7a4c18f086d8747168b96ef3951b4d3",
            reference: None,
        },
        escrow: ProtocolScript {
            hash: "37c22ecd2ecec492fdcb2db1de5b566b51450da248b5818fc9dc6428",
            reference: None,
        },
        claim_marker_policy: ProtocolScript {
            hash: "515f938fe3a792a3243c8213ea24b9d2b61ce33db480757d9fbd02f2",
            reference: None,
        },
        // Parked at the depot, out of reach of the wallet's coin selection.
        // Recorded because it is the only input to the chain above that is
        // not derived from another hash — without it none of this can be
        // reproduced.
        seed: ReferenceScript {
            tx_hash: "366518e2b4cc0ed1802e29f7cb0291310fd07ac50ac5d73e870327f51cd82566",
            output_index: 0,
        },
        release_reserve_per_claim: 2_500_000,
        settlement_grace_ms: 86_400_000,
    },
];

/// The protocol on a network, or `None` where it has not been deployed.
pub fn lookup_action_protocol(
    network: RegistryNetwork,
) -> Option<&'static ActionProtocolDeployment> {
    ACTION_PROTOCOL_DEPLOYMENTS
        .iter()
        .find(|d| d.network == network)
}

impl ActionProtocolDeployment {
    /// Every script, named, in parameter-chain order — for a surface that
    /// lists a deployment rather than reaching for one field.
    pub fn scripts(&self) -> [(&'static str, ProtocolScript, ScriptRole); 7] {
        use ScriptRole::{MintingPolicy, Validator};
        [
            ("registry", self.registry, Validator),
            ("config_token", self.config_token_policy, MintingPolicy),
            ("protocol_config", self.protocol_config, Validator),
            ("fuel", self.fuel, Validator),
            ("fuel_pair", self.fuel_pair_policy, MintingPolicy),
            ("escrow", self.escrow, Validator),
            ("claim_marker", self.claim_marker_policy, MintingPolicy),
        ]
    }

    /// The config token as a concatenated asset id, which is how every chain
    /// API names one.
    pub fn config_asset_id(&self) -> String {
        format!(
            "{}{}",
            self.config_token_policy.hash, self.config_token_name
        )
    }
}

/// What a script hash means — an address to send to, or a policy id.
///
/// The distinction matters because a minting policy has no address of its
/// own, and a surface that renders one for it is inviting somebody to send
/// assets somewhere unspendable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptRole {
    Validator,
    MintingPolicy,
}

impl ScriptRole {
    pub const ALL: [ScriptRole; 2] = [ScriptRole::Validator, ScriptRole::MintingPolicy];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Validator => "validator",
            Self::MintingPolicy => "minting policy",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Preprod is deployed; mainnet is not.
    ///
    /// This test used to assert the table was EMPTY, which is why it is being
    /// read now — the day it stopped being empty had to be a day somebody
    /// looked at it and changed it on purpose. Mainnet stays absent until it
    /// has its own seed and its own chain; sharing preprod's would be sharing
    /// a spent UTxO, which mints nothing.
    #[test]
    fn preprod_is_deployed_and_mainnet_is_not() {
        assert!(lookup_action_protocol(RegistryNetwork::Testnet).is_some());
        assert!(lookup_action_protocol(RegistryNetwork::Mainnet).is_none());
    }

    /// `registry` takes no parameters, so its hash comes from the contracts
    /// alone — the same on every network, and unchanged by the seed.
    ///
    /// Pinned because it is the cheapest possible check that a pasted record
    /// came from the build it claims to: if this differs from a fresh
    /// `aiken build`, the contracts moved and every other hash here is stale.
    #[test]
    fn the_parameterless_root_is_the_hash_the_contracts_compile_to() {
        for deployment in ACTION_PROTOCOL_DEPLOYMENTS {
            assert_eq!(
                deployment.registry.hash,
                "78bbd2b4a99afc6500df8c9f38300001835bf2bc7211ecb1dffecfea",
                "{:?}: registry takes no parameters, so its hash cannot differ \
                 between networks — one of these records is from another build",
                deployment.network
            );
        }
    }

    /// One record per network, or a lookup silently returns whichever came
    /// first — the failure that puts mainnet hashes in front of preprod.
    #[test]
    fn a_network_is_never_listed_twice() {
        for network in RegistryNetwork::ALL {
            let count = ACTION_PROTOCOL_DEPLOYMENTS
                .iter()
                .filter(|d| d.network == network)
                .count();
            assert!(count <= 1, "{network:?} is listed {count} times");
        }
    }

    /// Every hash is 28 bytes of hex, and the token name is hex too. A
    /// bech32 address or a stray `0x` pasted in here would resolve to an
    /// address that exists and is not ours.
    #[test]
    fn every_hash_is_28_bytes_of_hex() {
        for deployment in ACTION_PROTOCOL_DEPLOYMENTS {
            for (name, script, _role) in deployment.scripts() {
                assert_eq!(
                    script.hash.len(),
                    56,
                    "{name}: a script hash is 28 bytes / 56 hex chars"
                );
                assert!(
                    script.hash.chars().all(|c| c.is_ascii_hexdigit()),
                    "{name}: not hex"
                );
            }
            assert!(
                deployment.config_token_name.len() % 2 == 0
                    && deployment
                        .config_token_name
                        .chars()
                        .all(|c| c.is_ascii_hexdigit()),
                "the config token name is hex, not text"
            );
            assert_eq!(deployment.seed.tx_hash.len(), 64, "a tx id is 32 bytes");
        }
    }

    /// Every script in the chain is distinct. Two equal hashes means a
    /// parameter was applied twice or a line was pasted twice — and the
    /// second is the one that happens.
    #[test]
    fn no_two_scripts_share_a_hash() {
        for deployment in ACTION_PROTOCOL_DEPLOYMENTS {
            let scripts = deployment.scripts();
            for (i, (name, script, _)) in scripts.iter().enumerate() {
                for (other_name, other, _) in scripts.iter().skip(i + 1) {
                    assert_ne!(
                        script.hash, other.hash,
                        "{name} and {other_name} have the same hash"
                    );
                }
            }
        }
    }
}
