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
pub static ACTION_PROTOCOL_DEPLOYMENTS: &[ActionProtocolDeployment] = &[];

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

    /// Until a deployment lands this is empty, and every caller must handle
    /// that. Asserted so the day it stops being empty is a day somebody
    /// looked at this test and changed it deliberately.
    #[test]
    fn no_network_is_deployed_yet() {
        assert!(ACTION_PROTOCOL_DEPLOYMENTS.is_empty());
        for network in RegistryNetwork::ALL {
            assert!(lookup_action_protocol(network).is_none());
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
