use phf::{phf_map, Map};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Supported marketplace contract versions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MarketplaceType {
    JpgStoreV1,
    JpgStoreV2,
    JpgStoreV3,
    /// JPG.store V4 — new contract with simplified datum (asset ID + seller credentials only, no price)
    JpgStoreV4,
    Wayup,
    Unknown,
}

/// Script reference UTxO for a marketplace contract (used in Plutus script spend TXs).
#[derive(Debug, Clone, Copy)]
pub struct ScriptReference {
    pub tx_hash: &'static str,
    pub output_index: u32,
    pub script_hash: &'static str,
}

/// Buy redeemer CBOR for a marketplace contract.
#[derive(Debug, Clone, Copy)]
pub struct BuyRedeemer {
    pub cbor_hex: &'static str,
}

/// Empty constructor redeemer: Constructor(0) [] — used by JPG.store V1/V2/V3
const BUY_REDEEMER_EMPTY_CONSTRUCTOR: BuyRedeemer = BuyRedeemer { cbor_hex: "d87980" };

impl MarketplaceType {
    /// Get the script reference UTxO for this marketplace version (if known).
    pub fn script_reference(&self) -> Option<ScriptReference> {
        match self {
            MarketplaceType::JpgStoreV2 => Some(ScriptReference {
                tx_hash: "9a32459bd4ef6bbafdeb8cf3b909d0e3e2ec806e4cc6268529280b0fc1d06f5b",
                output_index: 0,
                script_hash: "9068a7a3f008803edac87af1619860f2cdcde40c26987325ace138ad",
            }),
            // V1/V3/V4/Wayup script references can be added as discovered
            _ => None,
        }
    }

    /// Get the buy redeemer for this marketplace version.
    pub fn buy_redeemer(&self) -> Option<BuyRedeemer> {
        match self {
            MarketplaceType::JpgStoreV1
            | MarketplaceType::JpgStoreV2
            | MarketplaceType::JpgStoreV3 => Some(BUY_REDEEMER_EMPTY_CONSTRUCTOR),
            // V4 and Wayup redeemers can be added as discovered
            _ => None,
        }
    }
}

/// Fee calculation function type for marketplace transactions
/// Takes base price in lovelace and marketplace address, returns fee in lovelace
pub type FeeCalculationFn = fn(base_price_lovelace: u64, marketplace_address: &str) -> u64;

/// No-op fee calculation - returns 0 fees
pub fn no_fee_calculation(_base_price_lovelace: u64, _marketplace_address: &str) -> u64 {
    0
}

/// JPG.store fee calculation - 2% of base price with 1 ADA minimum
pub fn jpg_store_fee_calculation(base_price_lovelace: u64, _marketplace_address: &str) -> u64 {
    const JPG_STORE_FEE_RATE: f64 = 0.02; // 2% as per https://help.jpg.store/en/articles/10123076-jpg-store-fees-explained-platform-and-blockchain-costs
    const MIN_FEE_LOVELACE: u64 = 1_000_000; // 1 ADA minimum

    let calculated_fee =
        (base_price_lovelace as f64 / (1f64 - JPG_STORE_FEE_RATE)) as u64 - base_price_lovelace;
    calculated_fee.max(MIN_FEE_LOVELACE)
}

/// Wayup fee calculation - 2% of base price with 1 ADA minimum and 10 ADA maximum
/// Rounded up to nearest 0.1 ADA (100,000 lovelace)
pub fn wayup_fee_calculation(base_price_lovelace: u64, _marketplace_address: &str) -> u64 {
    const WAYUP_FEE_RATE: f64 = 0.02; // 2%
    const MIN_FEE_LOVELACE: u64 = 1_000_000; // 1 ADA minimum
    const MAX_FEE_LOVELACE: u64 = 10_000_000; // 10 ADA maximum
    const ROUNDING_INCREMENT: u64 = 100_000; // Round to nearest 0.1 ADA

    let calculated_fee = (base_price_lovelace as f64 * WAYUP_FEE_RATE) as u64;

    // Round up to nearest 0.1 ADA increment
    let rounded_fee = calculated_fee.div_ceil(ROUNDING_INCREMENT) * ROUNDING_INCREMENT;

    rounded_fee.clamp(MIN_FEE_LOVELACE, MAX_FEE_LOVELACE)
}
use AddressCategory as AC;
use Marketplace as MP;
use MarketplacePurpose as Purpose;
use ScriptCategory as SC;

/// Registry of known regular addresses (wallets, exchanges, etc.) and their purposes
/// This registry should be manually curated for accuracy
pub static ADDRESS_REGISTRY: Map<&'static str, AddressCategory> = phf_map! {
    "addr1xxgx3far7qygq0k6epa0zcvcvrevmn0ypsnfsue94nsn3tfvjel5h55fgjcxgchp830r7h2l5msrlpt8262r3nvr8eks2utwdd" => AC::Script(SC::Marketplace { marketplace: MP::JpgStore, purpose: Purpose::Offer, kind: MarketplaceType::JpgStoreV1, fee_calculation: jpg_store_fee_calculation }),
    "addr1x8rjw3pawl0kelu4mj3c8x20fsczf5pl744s9mxz9v8n7efvjel5h55fgjcxgchp830r7h2l5msrlpt8262r3nvr8ekstg4qrx" => AC::Script(SC::Marketplace { marketplace: MP::JpgStore, purpose: Purpose::Sale, kind: MarketplaceType::JpgStoreV2, fee_calculation: jpg_store_fee_calculation }),
    "addr1zxgx3far7qygq0k6epa0zcvcvrevmn0ypsnfsue94nsn3tvpw288a4x0xf8pxgcntelxmyclq83s0ykeehchz2wtspks905plm" => AC::Script(SC::Marketplace { marketplace: MP::JpgStore, purpose: Purpose::Sale, kind: MarketplaceType::JpgStoreV1, fee_calculation: jpg_store_fee_calculation }),
    "addr1xxzvcf02fs5e282qk3pmjkau2emtcsj5wrukxak3np90n2evjel5h55fgjcxgchp830r7h2l5msrlpt8262r3nvr8eksg6pw3p" => AC::Script(SC::Marketplace { marketplace: MP::JpgStore, purpose: Purpose::Fee, kind: MarketplaceType::JpgStoreV3, fee_calculation: no_fee_calculation }),
    "addr1w8rjw3pawl0kelu4mj3c8x20fsczf5pl744s9mxz9v8n7efvjel5h55fgjcxgchp830r7h2l5msrlpt8262r3nvr8ekstg4qrx" => AC::Script(SC::Marketplace { marketplace: MP::JpgStore, purpose: Purpose::Sale, kind: MarketplaceType::JpgStoreV3, fee_calculation: jpg_store_fee_calculation }),
    // JPG.store V4 — new simplified contract (asset ID + seller credentials, no price in datum)
    "addr1w999n67e47he8y0v36hjtzluargwu25zw94f6lqnm82aqqsg4xkcp" => AC::Script(SC::Marketplace { marketplace: MP::JpgStore, purpose: Purpose::Sale, kind: MarketplaceType::JpgStoreV4, fee_calculation: jpg_store_fee_calculation }),
    "addr1zxnk7racqx3f7kg7npc4weggmpdskheu8pm57egr9av0mtvasazx8r5xwqtnfjsfrnat3h6yrycd2hfm9qpg7d0hf50s7x4y79" => AC::Script(SC::Marketplace { marketplace: MP::Wayup, purpose: Purpose::Sale, kind: MarketplaceType::Wayup, fee_calculation: wayup_fee_calculation }),
    "addr1v87m5srrtx52s8jdragjl8wle0eq57dzv2n62nxh3nx65dq0edwwu" => AC::Script(SC::Marketplace { marketplace: MP::Wayup, purpose: Purpose::Sale, kind: MarketplaceType::Wayup, fee_calculation: wayup_fee_calculation }),
    "addr1xx2l3rxnj5cuvj58fxnztewnlxneejzayqqakg7c2xkkt0gejuwlk348lfs3mh65tm5ym27hg9z5cjphv6w7sv3dwxqsk9as6l" => AC::Script(SC::Minter(Minter::JpgStore)),
    "addr1z98ps3vxeewk94rwp5dtxvzlr4aczync78p8am9l9w4vcn04fr9rh39dpgmzl234njvxfpnah654jxuwzlgnqejnnkwq2zuf48" => AC::Script(SC::Staking { label: "The Vault", project: "CNFT Tools" }),
    // dexes — Splash pool contracts (type 6: script payment + script staking, per-pool credentials)
    "addr1x89ksjnfu7ys02tedvslc9g2wk90tu5qte0dt4dge60hdudj764lvrxdayh2ux30fl0ktuh27csgmpevdu89jlxppvrsg0g63z" => AC::Script(SC::Exchange { label: "Splash" }),
    // DexHunter aggregator contract
    "addr1xxg94wrfjcdsjncmsxtj0r87zk69e0jfl28n934sznu95tdj764lvrxdayh2ux30fl0ktuh27csgmpevdu89jlxppvrs2993lw" => AC::Script(SC::Exchange { label: "DexHunter" }),
    // Minswap batcher contract (type 7: script payment, no staking)
    "addr1w8p79rpkcdz8x9d6tft0x0dx5mwuzac2sa4gm8cvkw5hcnqst2ctf" => AC::Script(SC::Exchange { label: "Minswap" }),

    "addr1zyd0sj57d9lpu7cy9g9qdurpazqc9l4eaxk6j59nd2gkh4275jq4yvpskgayj55xegdp30g5rfynax66r8vgn9fldndsqzf5tn" => AC::Script(SC::Exchange { label: "SaturnSwap" }),
};

/// Address prefixes for scripts that use variable staking credentials.
/// These are type 4 (addr1z) addresses where the script hash is constant but the
/// staking credential varies (per-seller for marketplaces, per-pool for DEXes).
/// The prefix covers the payment credential portion.
static ADDRESS_PREFIX_REGISTRY: &[(&str, AddressCategory)] = &[
    // Wayup marketplace — per-seller staking credential variants
    (
        "addr1zxnk7racqx3f7kg7npc4weggmpdskheu8pm57egr9av0mt",
        AC::Script(SC::Marketplace {
            marketplace: MP::Wayup,
            purpose: Purpose::Sale,
            kind: MarketplaceType::Wayup,
            fee_calculation: wayup_fee_calculation,
        }),
    ),
    // Splash DEX — per-pool staking credential variants (addr1z type 4)
    (
        "addr1z9ryamhgnuz6lau86sqytte2gz5rlktv2yce05e0h3207q",
        AC::Script(SC::Exchange { label: "Splash" }),
    ),
    // Minswap DEX — per-pool staking credential variants (addr1z type 4)
    (
        "addr1z84q0denmyep98ph3tmzwsmw0j7zau9ljmsqx6a4rvaau6",
        AC::Script(SC::Exchange { label: "Minswap" }),
    ),
    // Minswap V2 pool contract (script hash: e1317b152faac13426e6a83e06ff88a4d62cce3c1634ab0a5ec13309)
    (
        "addr1z8snz7c4974vzdpxu65ruphl3zjdvtxw8strf2c2tmqnxz",
        AC::Script(SC::Exchange { label: "Minswap" }),
    ),
    // Minswap V2 order contract (script hash: a65ca58a4e9c755fa830173d2a5caed458ac0c73f97db7faae2e7e3b)
    (
        "addr1zxn9efv2f6w82hagxqtn62ju4m293tqvw0uhmdl64ch8uw",
        AC::Script(SC::Exchange { label: "Minswap" }),
    ),
    // CSWAP (CardanoSwaps) — per-pool staking credential variants (addr1z type 4)
    (
        "addr1z8ke0c9p89rjfwmuh98jpt8ky74uy5mffjft3zlcld9h7m",
        AC::Script(SC::Exchange { label: "CSWAP" }),
    ),
    // CSWAP batcher — per-pool staking credential variants (addr1z type 4)
    (
        "addr1z8d9k3aw6w24eyfjacy809h68dv2rwnpw0arrfau98jk6nh",
        AC::Script(SC::Exchange { label: "CSWAP" }),
    ),
    // CrowdLock vesting contract — per-user staking credential variants (addr1z type 4)
    // Canonical constant: token_holders::CROWDLOCK_ADDRESS_PREFIX
    (
        "addr1zyupekdkyr8f6lrnm4zulcs8juwv080hjfgsqvgkp98kkd",
        AC::Script(SC::Vesting { label: "CrowdLock" }),
    ),
];

// ── Testnet / Preprod registries ─────────────────────────────────────────────

/// Registry of known testnet/preprod addresses.
/// Addresses here use `addr_test1` prefix and are separate from mainnet.
/// Note: App-specific testnet addresses (Asset Hire, Levvy V2, etc.) live in
/// the `address-config` crate within cnft.dev-workers.
pub static TESTNET_ADDRESS_REGISTRY: Map<&'static str, AddressCategory> = phf_map! {};

/// Testnet address prefix registry (variable staking credentials).
static TESTNET_ADDRESS_PREFIX_REGISTRY: &[(&str, AddressCategory)] = &[];

// ── Network enum ─────────────────────────────────────────────────────────────

/// Which network's address registry to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RegistryNetwork {
    #[default]
    Mainnet,
    Testnet,
}

// ── Lookup functions ─────────────────────────────────────────────────────────

/// Look up an address in the mainnet registry (default, backward-compatible).
pub fn lookup_address(address: &str) -> Option<&'static AddressCategory> {
    lookup_address_for_network(address, RegistryNetwork::Mainnet)
}

/// Look up an address in the registry for the specified network.
pub fn lookup_address_for_network(
    address: &str,
    network: RegistryNetwork,
) -> Option<&'static AddressCategory> {
    let (registry, prefixes) = match network {
        RegistryNetwork::Mainnet => (&ADDRESS_REGISTRY, ADDRESS_PREFIX_REGISTRY),
        RegistryNetwork::Testnet => (&TESTNET_ADDRESS_REGISTRY, TESTNET_ADDRESS_PREFIX_REGISTRY),
    };

    // Fast exact match first
    if let Some(cat) = registry.get(address) {
        return Some(cat);
    }

    // Prefix-based fallback for per-seller script addresses
    for (prefix, category) in prefixes {
        if address.starts_with(prefix) {
            return Some(category);
        }
    }

    None
}

/// Registry of known script addresses (smart contracts) and their purposes
/// This registry should be manually curated for accuracy
pub static SCRIPT_REGISTRY: Map<&'static str, ContractInfo> = phf_map! {
    "d3b3a8d77b6dfb28c76e1ab11c0b569bfe531fbf6f08d72d89c931aff4aea85f" => ContractInfo {
        category: ScriptCategory::Marketplace {
            marketplace: Marketplace::JpgStore,
            kind: MarketplaceType::JpgStoreV1,
            purpose: MarketplacePurpose::Sale,
            fee_calculation: jpg_store_fee_calculation,
        }
    }
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Minter {
    #[default]
    Unknown,
    JpgStore,
}

impl fmt::Display for Minter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Minter::Unknown => write!(f, "Unknown"),
            Minter::JpgStore => write!(f, "JPG.store"),
        }
    }
}

/// Marketplace platforms
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Marketplace {
    #[default]
    Unknown,
    JpgStore,
    Wayup,
}

impl fmt::Display for Marketplace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Marketplace::Unknown => write!(f, "Unknown"),
            Marketplace::JpgStore => write!(f, "JPG.store"),
            Marketplace::Wayup => write!(f, "Wayup"),
        }
    }
}

impl Marketplace {
    pub fn from_address(address: &str) -> Option<Self> {
        match lookup_address(address) {
            Some(AddressCategory::Marketplace(marketplace)) => Some(*marketplace),
            Some(AddressCategory::Script(ScriptCategory::Marketplace { marketplace, .. })) => {
                Some(*marketplace)
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum MarketplacePurpose {
    #[default]
    Unknown,
    Offer,
    Sale,
    Fee,
}

impl fmt::Display for MarketplacePurpose {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MarketplacePurpose::Unknown => write!(f, "Unknown"),
            MarketplacePurpose::Offer => write!(f, "Offer"),
            MarketplacePurpose::Sale => write!(f, "Sale"),
            MarketplacePurpose::Fee => write!(f, "Fee"),
        }
    }
}

impl MarketplacePurpose {
    pub fn from_address(address: &str) -> Option<Self> {
        match lookup_address(address) {
            Some(AddressCategory::Script(ScriptCategory::Marketplace { purpose, .. })) => {
                Some(*purpose)
            }
            _ => None,
        }
    }
}

/// Information about a known regular address (wallet, exchange, etc.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressInfo {
    /// Human-readable description of the address
    pub description: String,
    /// Type/category of the address
    pub category: AddressCategory,
}

/// Categories of regular addresses
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AddressCategory {
    #[default]
    Unknown,
    Marketplace(Marketplace),
    Script(ScriptCategory),
}

impl fmt::Display for AddressCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AddressCategory::Unknown => write!(f, "Unknown"),
            AddressCategory::Marketplace(marketplace) => write!(f, "{marketplace}"),
            AddressCategory::Script(script_category) => write!(f, "{script_category}"),
        }
    }
}

/// Information about a known smart contract
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractInfo {
    pub category: ScriptCategory,
}

#[derive(Debug, Clone, Default)]
pub enum ScriptCategory {
    #[default]
    Unknown,
    Marketplace {
        marketplace: Marketplace,
        kind: MarketplaceType,
        purpose: MarketplacePurpose,
        fee_calculation: FeeCalculationFn,
    },
    Exchange {
        label: &'static str,
    },
    DeFi {
        label: &'static str,
        protocol: &'static str,
    },
    Minter(Minter),
    Staking {
        label: &'static str,
        project: &'static str,
    },
    Vesting {
        label: &'static str,
    },
}

impl PartialEq for ScriptCategory {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ScriptCategory::Unknown, ScriptCategory::Unknown) => true,
            (
                ScriptCategory::Marketplace {
                    marketplace: m1,
                    kind: k1,
                    purpose: p1,
                    fee_calculation: _,
                },
                ScriptCategory::Marketplace {
                    marketplace: m2,
                    kind: k2,
                    purpose: p2,
                    fee_calculation: _,
                },
            ) => m1 == m2 && k1 == k2 && p1 == p2, // Exclude fee_calculation from comparison
            (ScriptCategory::Exchange { label: l1 }, ScriptCategory::Exchange { label: l2 }) => {
                l1 == l2
            }
            (
                ScriptCategory::DeFi {
                    label: l1,
                    protocol: p1,
                },
                ScriptCategory::DeFi {
                    label: l2,
                    protocol: p2,
                },
            ) => l1 == l2 && p1 == p2,
            (ScriptCategory::Minter(m1), ScriptCategory::Minter(m2)) => m1 == m2,
            (
                ScriptCategory::Staking {
                    label: l1,
                    project: p1,
                },
                ScriptCategory::Staking {
                    label: l2,
                    project: p2,
                },
            ) => l1 == l2 && p1 == p2,
            (ScriptCategory::Vesting { label: l1 }, ScriptCategory::Vesting { label: l2 }) => {
                l1 == l2
            }
            _ => false,
        }
    }
}

impl Eq for ScriptCategory {}

impl fmt::Display for ScriptCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScriptCategory::Unknown => write!(f, "Unknown"),
            ScriptCategory::Marketplace {
                marketplace,
                purpose,
                ..
            } => {
                write!(f, "{marketplace} {purpose}")
            }
            ScriptCategory::Exchange { label } => {
                write!(f, "{label} exchange")
            }
            ScriptCategory::DeFi { label, .. } => write!(f, "{label} DeFi"),
            ScriptCategory::Minter(minter) => write!(f, "{minter} Minter"),
            ScriptCategory::Staking { label, project } => {
                write!(f, "{label} staking for {project}")
            }
            ScriptCategory::Vesting { label } => {
                write!(f, "{label} vesting")
            }
        }
    }
}

// ── AddressLookup trait ──────────────────────────────────────────────────────

/// Trait for address registry implementations.
///
/// Consumers accept `Box<dyn AddressLookup>` instead of a concrete registry type.
/// This enables composing multiple registries (e.g. ecosystem + app-specific).
pub trait AddressLookup: Send + Sync {
    /// Look up an address category (exact + prefix match).
    fn lookup(&self, address: &str) -> Option<&AddressCategory>;

    /// Look up contract info by script hash.
    fn get_contract_info(&self, script_hash: &str) -> Option<&ContractInfo>;

    // ── Default convenience methods ──────────────────────────────────────

    /// Look up address category (alias for `lookup`).
    fn get_address_category(&self, address: &str) -> Option<&AddressCategory> {
        self.lookup(address)
    }

    /// Get marketplace info from an address.
    fn get_marketplace_info(
        &self,
        address: &str,
    ) -> Option<(Marketplace, Option<MarketplacePurpose>)> {
        match self.lookup(address) {
            Some(AddressCategory::Script(ScriptCategory::Marketplace {
                marketplace,
                purpose,
                ..
            })) => Some((*marketplace, Some(*purpose))),
            Some(AddressCategory::Marketplace(marketplace)) => Some((*marketplace, None)),
            _ => None,
        }
    }

    /// Check if an address is a known script address.
    fn is_known_script(&self, address: &str) -> bool {
        self.get_contract_info(address).is_some()
    }

    /// Check if an address is a known regular address.
    fn is_known_address(&self, address: &str) -> bool {
        self.lookup(address).is_some()
    }

    /// Check if address belongs to a specific marketplace.
    fn is_marketplace_address(&self, address: &str, marketplace: &Marketplace) -> bool {
        match self.get_marketplace_info(address) {
            Some((addr_marketplace, _)) => addr_marketplace == *marketplace,
            None => false,
        }
    }

    /// Check if address is ANY marketplace address.
    fn is_any_marketplace_address(&self, address: &str) -> bool {
        matches!(
            self.lookup(address),
            Some(AddressCategory::Script(ScriptCategory::Marketplace { .. }))
                | Some(AddressCategory::Marketplace(_))
        )
    }

    /// Get the fee calculation function for a marketplace address.
    fn get_marketplace_fee_calculation(&self, address: &str) -> Option<FeeCalculationFn> {
        match self.lookup(address) {
            Some(AddressCategory::Script(ScriptCategory::Marketplace {
                fee_calculation, ..
            })) => Some(*fee_calculation),
            _ => None,
        }
    }

    /// Calculate marketplace fee for a given address and base price.
    fn calculate_marketplace_fee(&self, address: &str, base_price_lovelace: u64) -> u64 {
        match self.get_marketplace_fee_calculation(address) {
            Some(fee_calc) => fee_calc(base_price_lovelace, address),
            None => 0,
        }
    }

    /// Get all known marketplaces involved in a transaction.
    fn get_transaction_marketplaces(
        &self,
        input_addresses: &[String],
        output_addresses: &[String],
    ) -> std::collections::HashSet<Marketplace> {
        let mut marketplaces = std::collections::HashSet::new();

        for address in input_addresses {
            if let Some((marketplace, _)) = self.get_marketplace_info(address) {
                marketplaces.insert(marketplace);
            }
        }

        for address in output_addresses {
            if let Some((marketplace, _)) = self.get_marketplace_info(address) {
                marketplaces.insert(marketplace);
            }
        }

        marketplaces
    }
}

// ── SmartContractRegistry (ecosystem addresses) ─────────────────────────────

/// Ecosystem address registry for identifying known contract addresses and their purposes.
///
/// Contains well-known ecosystem addresses (marketplaces, DEXes, etc.) from
/// the compile-time `ADDRESS_REGISTRY` and `SCRIPT_REGISTRY` maps.
/// App-specific addresses should be provided via a separate `AddressLookup`
/// implementation and composed using a composite registry.
#[derive(Debug, Clone)]
pub struct SmartContractRegistry {
    /// Which network's address registry to consult
    network: RegistryNetwork,
    /// Runtime additions for development/testing (not used in production lookups)
    runtime_contracts: std::collections::HashMap<String, ContractInfo>,
}

impl SmartContractRegistry {
    /// Create a new registry (defaults to Mainnet)
    pub fn new() -> Self {
        Self {
            network: RegistryNetwork::Mainnet,
            runtime_contracts: std::collections::HashMap::new(),
        }
    }

    /// Create a new registry for a specific network
    pub fn new_for_network(network: RegistryNetwork) -> Self {
        Self {
            network,
            runtime_contracts: std::collections::HashMap::new(),
        }
    }

    /// Add a contract to runtime registry (for development/testing only).
    /// Production contracts should be added to the SCRIPT_REGISTRY compile-time map.
    pub fn register_contract(&mut self, address: String, info: ContractInfo) {
        self.runtime_contracts.insert(address, info);
    }
}

impl AddressLookup for SmartContractRegistry {
    fn lookup(&self, address: &str) -> Option<&AddressCategory> {
        lookup_address_for_network(address, self.network)
    }

    fn get_contract_info(&self, address: &str) -> Option<&ContractInfo> {
        // First check compile-time registry (production contracts)
        if let Some(info) = SCRIPT_REGISTRY.get(address) {
            return Some(info);
        }

        // Fall back to runtime additions (development/testing)
        self.runtime_contracts.get(address)
    }

    fn is_known_script(&self, address: &str) -> bool {
        SCRIPT_REGISTRY.contains_key(address) || self.runtime_contracts.contains_key(address)
    }
}

impl Default for SmartContractRegistry {
    fn default() -> Self {
        Self::new()
    }
}
