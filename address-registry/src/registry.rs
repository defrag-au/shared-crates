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
    // The UNDELEGATED form of the V2 sale escrow: same payment script
    // c727443d77df6cff95dca383994f4c3024d03ff56b02ecc22b0f3f65 as the entry
    // above, with no staking credential. Same script means the same validator
    // and so the same datum, which is why it is `JpgStoreV2` and not a version
    // of its own — the two addresses differ in delegation, nothing else.
    //
    // It replaces a "V3 sale" row that was never an address: that string was
    // this script's address with the type character changed `x` → `w`, leaving
    // the bech32 checksum invalid, so no decoder could produce it and this
    // exact-match table could never hit it. The escrow itself is real and
    // active — it appears as an output address in recorded sale transactions
    // under pipeline/tx-classifier/resources/test — so for as long as the
    // corrupt row stood in for it, those sales resolved to `None` and went
    // unclassified.
    "addr1w8rjw3pawl0kelu4mj3c8x20fsczf5pl744s9mxz9v8n7eg0fcr8k" => AC::Script(SC::Marketplace { marketplace: MP::JpgStore, purpose: Purpose::Sale, kind: MarketplaceType::JpgStoreV2, fee_calculation: jpg_store_fee_calculation }),
    // A "V4 sale" row was removed from here for the same reason — it also
    // failed the bech32 checksum — but unlike the V3 string its payload is
    // corrupt beyond the checksum digits, so the intended address cannot be
    // recovered from it and has to come from the source. Until then JPG.store
    // V4 has a `MarketplaceType`, a datum parser and a fee rule, but no
    // address to trigger them.
    //
    // `every_registered_address_is_a_real_address` stops any of this recurring.
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
/// These are addr1z (script payment + key staking) addresses where the script
/// hash is constant but the staking credential varies. The prefix covers the
/// payment credential portion.
///
/// WHOSE stake varies matters enormously to consumers: for pool contracts it
/// is the venue's own per-pool credential, but for ORDER and LISTING contracts
/// it is the CUSTOMER's — Splash orders, Minswap orders and Wayup listings all
/// carry the ordinary wallet's staking credential on the script address so the
/// user keeps their delegation. A consumer that groups addresses by stake key
/// and then names the stake after a prefix hit will label every customer as
/// the venue. Use [`lookup_address_match`] to learn that a hit came from this
/// table and treat the stake credential as unidentified.
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
    // Splash DEX ORDER contract — the staking credential is the CUSTOMER's
    // (canonical constant: mitos-dex-decode `splash::ORDER_SCRIPT_ADDR_PREFIX`)
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
    // Minswap V2 ORDER contract — the staking credential is the CUSTOMER's
    // (script hash: a65ca58a4e9c755fa830173d2a5caed458ac0c73f97db7faae2e7e3b)
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

// ── Stake-credential registry ────────────────────────────────────────────────

/// What a stake-identified service does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StakeServiceKind {
    /// Runs mints on a project's behalf and takes a per-mint fee. Its wallet
    /// appears as a destination in EVERY mint transaction it serves, for every
    /// unrelated project — so a consumer that walks a frontier must record it
    /// and refuse to expand it.
    MintingProvider,
    /// An NFT marketplace's escrow. Assets and offers sit at its script
    /// addresses while listed, so it appears as a HOLDER of everything on sale
    /// — including every ADA Handle currently listed. A consumer that asks a
    /// handle service "who lives at this stake key" gets thousands of handles
    /// that belong to unrelated sellers, slowly. Name it and never ask.
    Marketplace,
    /// A token venue — DEX, aggregator, swap desk. Same holder problem as a
    /// marketplace: liquidity sits at its addresses, so it tops any holder
    /// list. Only ever registered by a credential that is provably the
    /// venue's; DEX staking scripts are frequently shared, which is why most
    /// of them cannot be listed here at all.
    Exchange,
}

/// A service run from an ordinary wallet, identified by its stake credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StakeService {
    pub label: &'static str,
    pub kind: StakeServiceKind,
    /// How the attribution was established. Registry entries are evidence, so
    /// this is required rather than optional — see the module header.
    pub source: &'static str,
}

/// Services keyed by STAKE credential rather than by payment address.
///
/// ## Why this table exists separately
///
/// [`ADDRESS_REGISTRY`] keys on payment addresses, which is correct for
/// scripts: a contract IS its address. Two things are not reachable that way.
///
/// A service run from an ORDINARY WALLET spends from many payment addresses
/// under a single staking credential, and enumerating them is both endless and
/// pointless. The stake key is the stable identity.
///
/// A SCRIPT is reachable by address — but consumers routinely hold only the
/// stake key, because deriving one from an address is a local decode while
/// keeping the address is not always possible. Anything that groups holders by
/// stake key (a holder snapshot, a handle batch) has thrown the payment
/// address away by the time it wants a name.
///
/// ## The trap this does NOT fall into
///
/// [`ADDRESS_PREFIX_REGISTRY`] carries a warning: order and listing contracts
/// keep the CUSTOMER's staking credential, so naming a stake key after a
/// script-prefix hit labels every customer as the venue. That is exactly the
/// failure this table could reintroduce, so the bar for an entry is:
///
/// **Only add a stake credential that genuinely belongs to the service.** For a
/// wallet-run service that means its own staking key. For a script it means the
/// credential is FIXED across the venue's own addresses rather than carried in
/// from whoever built the transaction — check that the same credential appears
/// in two or more of the venue's registered addresses, and that it is not the
/// seller's. Never derive an entry from a prefix-matched address.
pub static STAKE_REGISTRY: Map<&'static str, StakeService> = phf_map! {
    // Anvil — Cardano minting API. Takes a flat per-mint fee (1.15 ADA at time
    // of writing) in the mint transaction itself, alongside the project's own
    // payment. Observed across four unrelated collections.
    "stake1uy50zl7a9k9c74v66c0gn833at5sh83qnjldk8hg4rrv05g3mmskr" => StakeService {
        label: "Anvil",
        kind: StakeServiceKind::MintingProvider,
        source: "observed 2026-08-22: constant 1.15 ADA mint-tx fee across policies \
                 55bd0ac4 (KAT Pack, 333), 6b42eca9 (chadano_citizen, 1501), \
                 e26a8565 (perps_into_the_factions, 242), 812197d5 (Biddy_DeGoat, 127); \
                 1,009 unspent ~1 ADA UTxOs from unrelated projects",
    },

    // JPG.store — SCRIPT stake credential 2c967f4b…833e6d, shared by the V1
    // offer escrow (addr1xxgx3far…) and the V2 sale escrow (addr1x8rjw3paw…).
    // Fixed across both, so it is the venue's own credential and not a
    // seller's — see the bar for entry above.
    "stake17ykfvl6t62y5fvryvtsnch3lt406dcpls4n4d9pcekpnumg6v83tq" => StakeService {
        label: "JPG.store",
        kind: StakeServiceKind::Marketplace,
        source: "delegation part of the registered JPG.store escrow addresses \
                 addr1xxgx3far… (offer, V1) and addr1x8rjw3paw… (sale, V2); \
                 both are addr1x (script payment + script stake) and carry the \
                 identical script credential 2c967f4bd28944b06462e13c5e3f5d5f\
                 a6e03f8567569438cd833e6d",
    },
    // JPG.store — KEY stake credential 81728e7e…cb806d, the delegation part of
    // the V1 sale escrow (addr1zxgx3far…). A different credential from the one
    // above and reached from one registered address only, so it is listed
    // explicitly rather than inferred.
    "stake1uxqh9rn76n8nynsnyvf4ulndjv0srcc8jtvumut3989cqmgjt49h6" => StakeService {
        label: "JPG.store",
        kind: StakeServiceKind::Marketplace,
        source: "delegation part of the registered JPG.store V1 sale escrow \
                 addr1zxgx3far… (script payment + key stake); credential \
                 81728e7ed4cf324e1323135e7e6d931f01e30792d9cdf17129cb806d",
    },
    // JPG.store's MINTER, a different contract from the escrows above and so a
    // different credential. One payment script, no other label reaches it.
    "stake17yvew80mg6nl5cgama29a6zd40t5z32vfqmkd80gxgkhrqgy48g8s" => StakeService {
        label: "JPG.store",
        kind: StakeServiceKind::Marketplace,
        source: "delegation part of the registered JPG.store minter address; \
                 reached by exactly one payment script and no other registered \
                 label",
    },
    // Wayup — reached by two of its own sale scripts and nothing else.
    //
    // Covers only the Wayup addresses carrying THIS credential. Wayup also has
    // a sale address delegating to the shared credential noted in the
    // exclusions below, which is deliberately not registered; that address
    // stays unnamed rather than being named wrongly.
    "stake1uxwcw3rr36r8q9e5egy3e74cmazpjvx4t5ajsq50xhm568celda4g" => StakeService {
        label: "Wayup",
        kind: StakeServiceKind::Marketplace,
        source: "delegation shared by two registered Wayup sale scripts and \
                 carrying no other registered label",
    },
    // SaturnSwap — three of its own scripts, sole label.
    "stake1u902fq2jxqctywjf22rv5xsch52p5jf7nddpnkyfj5lkekcnnhvtv" => StakeService {
        label: "SaturnSwap",
        kind: StakeServiceKind::Exchange,
        source: "delegation shared by three registered SaturnSwap scripts and \
                 carrying no other registered label",
    },

    // ── Deliberately NOT registered ─────────────────────────────────────────
    //
    // Two credentials look like obvious additions and are not, because more
    // than one entity's contracts delegate to them. `a_registered_credential_
    // is_claimed_by_exactly_one_entity` enforces this; the note is here so the
    // absence reads as a decision rather than an oversight.
    //
    //   stake17xe0d2lkpnx7jt4wrgh5lhm97t40vgydsukx7rje0nqskpc5zugc3
    //     Fourteen distinct payment scripts across the corpus, registered
    //     under BOTH "Splash" and "DexHunter". It is a shared staking script,
    //     not an identity. Naming it after either would mislabel the other —
    //     the same failure as the 168 Mekka counterparties wrongly labelled
    //     "Splash", noted on ADDRESS_PREFIX_REGISTRY.
    //
    //   stake1u8653j3mcjks5d304g6eexryse7ma22erw8p05fsvefem8qklu7w7
    //     Three scripts, registered under both "The Vault" staking and "Wayup"
    //     sale. Same problem, smaller blast radius.
};

/// Look up a service by its stake credential (bech32 `stake1…`).
pub fn lookup_stake(stake: &str) -> Option<&'static StakeService> {
    STAKE_REGISTRY.get(stake)
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

/// How a [`lookup_address_match`] hit was found — and therefore how much of
/// the address the registry actually identified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchKind {
    /// A full-address entry: the whole address, staking credential included,
    /// belongs to the registered entity.
    Exact,
    /// A payment-credential prefix from [`ADDRESS_PREFIX_REGISTRY`]: only the
    /// SCRIPT is identified. The staking credential riding on the address may
    /// be the venue's (pool contracts) or an ordinary customer's (order and
    /// listing contracts keep the user's delegation) — the registry cannot
    /// tell you which, so an entity keyed by that stake credential must NOT
    /// inherit the venue's name from this hit alone.
    VariableStakePrefix,
}

/// [`lookup_address_for_network`], but reporting whether the hit identified
/// the full address or only its payment-credential script.
pub fn lookup_address_match(
    address: &str,
    network: RegistryNetwork,
) -> Option<(&'static AddressCategory, MatchKind)> {
    let (registry, prefixes) = match network {
        RegistryNetwork::Mainnet => (&ADDRESS_REGISTRY, ADDRESS_PREFIX_REGISTRY),
        RegistryNetwork::Testnet => (&TESTNET_ADDRESS_REGISTRY, TESTNET_ADDRESS_PREFIX_REGISTRY),
    };
    if let Some(cat) = registry.get(address) {
        return Some((cat, MatchKind::Exact));
    }
    prefixes
        .iter()
        .find(|(prefix, _)| address.starts_with(prefix))
        .map(|(_, category)| (category, MatchKind::VariableStakePrefix))
}

/// Whether a bech32 Shelley address pays to a SCRIPT rather than a key.
///
/// Purely textual: the first data character after the `addr1`/`addr_test1`
/// separator encodes the CIP-19 header's address type (first five bits are
/// the four type bits plus the network nibble's high bit, and the bech32
/// charset maps that to `type * 2`). Script-payment types 1/3/5/7 land on
/// `z`/`x`/`2`/`w`; key-payment types 0/2/4/6 land on `q`/`y`/`g`/`v`.
/// Byron/stake addresses and malformed strings return `false`.
pub fn payment_credential_is_script(address: &str) -> bool {
    let data = address
        .strip_prefix("addr1")
        .or_else(|| address.strip_prefix("addr_test1"));
    matches!(
        data.and_then(|d| d.chars().next()),
        Some('z' | 'x' | '2' | 'w')
    )
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

    /// Check if the address belongs to a known vesting contract (Shield or CrowdLock).
    fn is_any_vesting_address(&self, address: &str) -> bool {
        matches!(
            self.lookup(address),
            Some(AddressCategory::Script(ScriptCategory::Vesting { .. }))
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

/// Returns all known exact addresses and prefix patterns from the mainnet registry.
///
/// Used by tooling (e.g., block-tap-mcp) to expand `address_category` conditions
/// into concrete address lists at bake time.
pub fn all_known_addresses() -> Vec<&'static str> {
    let mut addrs: Vec<&'static str> = ADDRESS_REGISTRY.keys().copied().collect();
    for (prefix, _) in ADDRESS_PREFIX_REGISTRY {
        addrs.push(prefix);
    }
    addrs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anvil_is_found_by_stake_credential() {
        let s = lookup_stake("stake1uy50zl7a9k9c74v66c0gn833at5sh83qnjldk8hg4rrv05g3mmskr")
            .expect("Anvil is registered");
        assert_eq!(s.label, "Anvil");
        assert_eq!(s.kind, StakeServiceKind::MintingProvider);
        assert!(!s.source.is_empty(), "an entry must carry its evidence");
    }

    #[test]
    fn an_unregistered_stake_key_is_not_named() {
        assert!(
            lookup_stake("stake1u98f5mr0mn8tv2kqndk5cwen4uasc7cewlzdklz6y664zacl9lvjz").is_none()
        );
    }

    /// Both jpg.store escrow credentials resolve to the same venue. A consumer
    /// that only has a stake key — a holder snapshot, a handle batch — must be
    /// able to name the marketplace without the payment address.
    #[test]
    fn jpg_store_escrow_is_found_by_either_stake_credential() {
        for stake in [
            "stake17ykfvl6t62y5fvryvtsnch3lt406dcpls4n4d9pcekpnumg6v83tq",
            "stake1uxqh9rn76n8nynsnyvf4ulndjv0srcc8jtvumut3989cqmgjt49h6",
        ] {
            let s = lookup_stake(stake).unwrap_or_else(|| panic!("{stake} is registered"));
            assert_eq!(s.label, "JPG.store");
            assert_eq!(s.kind, StakeServiceKind::Marketplace);
            assert!(!s.source.is_empty(), "an entry must carry its evidence");
        }
    }

    /// The script credential is shared by the V1 offer and V2 sale escrows.
    /// Those two addresses are the evidence the entry rests on, so if either
    /// leaves [`ADDRESS_REGISTRY`] the stake entry has lost its justification.
    #[test]
    fn the_shared_jpg_store_credential_still_has_two_witnesses() {
        for addr in [
            "addr1xxgx3far7qygq0k6epa0zcvcvrevmn0ypsnfsue94nsn3tfvjel5h55fgjcxgchp830r7h2l5msrlpt8262r3nvr8eks2utwdd",
            "addr1x8rjw3pawl0kelu4mj3c8x20fsczf5pl744s9mxz9v8n7efvjel5h55fgjcxgchp830r7h2l5msrlpt8262r3nvr8ekstg4qrx",
        ] {
            assert!(
                matches!(
                    lookup_address(addr),
                    Some(AddressCategory::Script(ScriptCategory::Marketplace {
                        marketplace: Marketplace::JpgStore,
                        ..
                    }))
                ),
                "{addr} is a witness for the shared jpg.store stake credential"
            );
        }
    }

    /// The V2 escrow is registered twice: once delegated to JPG.store's own
    /// stake credential, once undelegated. Same payment script, so the same
    /// validator and the same datum — which is why both carry `JpgStoreV2`.
    /// Splitting them across versions would route byte-identical datums to two
    /// different schemas, and the undelegated one is the address that actually
    /// shows up in recorded sales.
    #[test]
    fn both_forms_of_the_v2_escrow_are_the_same_contract() {
        use pallas_addresses::Address;

        const DELEGATED: &str = "addr1x8rjw3pawl0kelu4mj3c8x20fsczf5pl744s9mxz9v8n7efvjel5h55fgjcxgchp830r7h2l5msrlpt8262r3nvr8ekstg4qrx";
        const UNDELEGATED: &str = "addr1w8rjw3pawl0kelu4mj3c8x20fsczf5pl744s9mxz9v8n7eg0fcr8k";

        let script_of = |a: &str| match Address::from_bech32(a) {
            Ok(Address::Shelley(sh)) => sh.payment().to_hex(),
            _ => panic!("{a} must decode"),
        };
        assert_eq!(
            script_of(DELEGATED),
            script_of(UNDELEGATED),
            "the two forms must share a payment script, or they are not one contract"
        );

        let kind_of = |a: &str| match lookup_address(a) {
            Some(AddressCategory::Script(ScriptCategory::Marketplace { kind, .. })) => *kind,
            other => panic!("{a} should be a registered marketplace, got {other:?}"),
        };
        assert_eq!(kind_of(DELEGATED), MarketplaceType::JpgStoreV2);
        assert_eq!(
            kind_of(UNDELEGATED),
            kind_of(DELEGATED),
            "one script, one datum schema"
        );
    }

    /// The ENTITY a category attributes an address to, ignoring the role it
    /// plays. "JPG.store Offer" and "JPG.store Sale" are one entity; "Splash"
    /// and "DexHunter" are two. Stake ownership is a claim about the entity,
    /// so this is the granularity the invariant below has to compare at.
    fn owner_of(category: &AddressCategory) -> Option<String> {
        Some(match category {
            AddressCategory::Unknown => return None,
            AddressCategory::Marketplace(m) => m.to_string(),
            AddressCategory::Script(s) => match s {
                ScriptCategory::Unknown => return None,
                ScriptCategory::Marketplace { marketplace, .. } => marketplace.to_string(),
                ScriptCategory::Exchange { label } => label.to_string(),
                ScriptCategory::DeFi { protocol, .. } => protocol.to_string(),
                ScriptCategory::Minter(m) => m.to_string(),
                ScriptCategory::Staking { label, .. } => label.to_string(),
                ScriptCategory::Vesting { label } => label.to_string(),
            },
        })
    }

    /// The invariant that makes [`STAKE_REGISTRY`] safe to add to.
    ///
    /// A staking credential may only be named after an entity if every
    /// registered address delegating to it belongs to that entity. DEX and
    /// aggregator contracts routinely SHARE a staking script — fourteen
    /// distinct payment scripts across Splash and DexHunter delegate to one
    /// credential — and an order or listing contract carries the CUSTOMER's
    /// delegation. Either way, naming such a credential labels somebody else's
    /// wallet as the venue, which is the failure mode this whole table is
    /// fenced against.
    ///
    /// Checking it here means the judgement is enforced rather than
    /// remembered: add a credential shared by two entities and this fails with
    /// both names, which is exactly the prompt needed.
    #[test]
    fn a_registered_credential_is_claimed_by_exactly_one_entity() {
        use std::collections::{BTreeMap, BTreeSet};

        let mut owners: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (address, category) in ADDRESS_REGISTRY.entries() {
            let (Some(stake), Some(owner)) = (stake_of(address), owner_of(category)) else {
                continue;
            };
            if STAKE_REGISTRY.contains_key(stake.as_str()) {
                owners.entry(stake).or_default().insert(owner);
            }
        }

        for (stake, claimants) in owners {
            let registered = lookup_stake(&stake).expect("filtered on membership").label;
            assert_eq!(
                claimants.len(),
                1,
                "{stake} is registered as {registered:?} but registered addresses \
                 for {claimants:?} delegate to it — a credential claimed by more \
                 than one entity cannot name any of them"
            );
            let claimant = claimants.iter().next().expect("exactly one");
            assert_eq!(
                claimant, registered,
                "{stake} is registered as {registered:?} but its addresses belong \
                 to {claimant:?}"
            );
        }
    }

    /// Decode a registered address's delegation part, or `None` if it has one
    /// of the forms that carries no stake credential.
    fn stake_of(address: &str) -> Option<String> {
        use pallas_addresses::{Address, StakeAddress};
        let Ok(Address::Shelley(sh)) = Address::from_bech32(address) else {
            return None;
        };
        StakeAddress::try_from(sh).ok()?.to_bech32().ok()
    }

    /// Nothing in this crate parses the addresses it stores — lookups are
    /// string comparisons, and `payment_credential_is_script` reads one
    /// character. So a typo produces a row that is silently unreachable rather
    /// than a build error, and the table quietly stops covering what it claims
    /// to. That is not hypothetical: a "JPG.store V3 sale" entry sat here for
    /// a long time holding the V2 address with `x` changed to `w`, which fails
    /// the bech32 checksum and could never have matched anything.
    #[test]
    fn every_registered_address_is_a_real_address() {
        use pallas_addresses::Address;
        for address in ADDRESS_REGISTRY
            .keys()
            .chain(TESTNET_ADDRESS_REGISTRY.keys())
        {
            assert!(
                Address::from_bech32(address).is_ok(),
                "{address} is registered but is not a decodable address — \
                 an exact-match table can never hit it"
            );
        }
    }

    /// The two tables have to agree about who owns a staking credential.
    ///
    /// Every fully-registered JPG.store address delegates to a credential that
    /// [`STAKE_REGISTRY`] must also name JPG.store. This is what makes the
    /// stake entries self-maintaining: add a venue address carrying a
    /// credential nobody registered and this fails, which is the prompt to
    /// decide whether the credential is really the venue's — the one judgement
    /// the stake table's doc comment insists on.
    #[test]
    fn a_registered_venue_address_delegates_to_a_registered_credential() {
        let mut checked = 0;
        for (address, category) in ADDRESS_REGISTRY.entries() {
            if !matches!(
                category,
                AddressCategory::Script(ScriptCategory::Marketplace {
                    marketplace: Marketplace::JpgStore,
                    ..
                })
            ) {
                continue;
            }
            // Enterprise addresses (V4) delegate to nothing — no claim to check.
            let Some(stake) = stake_of(address) else {
                continue;
            };
            let named = lookup_stake(&stake).map(|s| s.label);
            assert_eq!(
                named,
                Some("JPG.store"),
                "{address} is a registered JPG.store address delegating to \
                 {stake}, which the stake table does not name as JPG.store"
            );
            checked += 1;
        }
        assert!(
            checked >= 3,
            "expected several JPG.store addresses to carry a stake credential, \
             checked only {checked} — has the registry been gutted?"
        );
    }

    /// The stake table takes bech32 STAKE keys. A payment address must never
    /// hit it, or a consumer could name a wallet after a service it merely
    /// paid.
    #[test]
    fn a_payment_address_never_hits_the_stake_table() {
        assert!(
            lookup_stake(
                "addr1qx68zqqmcfhy2jvj7ugffvrx0utykjeq9k2vfzy5lyn3defg79la6tvt3a2e44s73x0rr6hfpw0zp897mv0w32xxclgsle8hll"
            )
            .is_none(),
            "Anvil's own payment address must be looked up by its STAKE key, not directly"
        );
    }

    /// A Splash ORDER address carries the customer's staking credential — the
    /// registry identifies the script, and must SAY it identified only the
    /// script, or the customer's stake key gets named after the DEX. This is
    /// not hypothetical: 168 Mekka counterparties were labelled "Splash".
    #[test]
    fn a_prefix_hit_declares_it_only_identified_the_script() {
        let customer_order =
            "addr1z9ryamhgnuz6lau86sqytte2gz5rlktv2yce05e0h3207qdhc9k425ezp5cw8a3ssg7swp6fjdmnp3y8vcuka3fjr7mgqw6ke7p";
        let (cat, kind) = lookup_address_match(customer_order, RegistryNetwork::Mainnet).unwrap();
        assert!(matches!(
            cat,
            AddressCategory::Script(ScriptCategory::Exchange { label: "Splash" })
        ));
        assert_eq!(kind, MatchKind::VariableStakePrefix);
    }

    #[test]
    fn a_full_address_entry_is_an_exact_match() {
        let minswap_batcher = "addr1w8p79rpkcdz8x9d6tft0x0dx5mwuzac2sa4gm8cvkw5hcnqst2ctf";
        let (_, kind) = lookup_address_match(minswap_batcher, RegistryNetwork::Mainnet).unwrap();
        assert_eq!(kind, MatchKind::Exact);
    }

    #[test]
    fn script_payment_detection_reads_the_type_character() {
        // script payment: types 1 (z), 3 (x), 7 (w)
        assert!(payment_credential_is_script("addr1z9ryamhgnuz6lau86sq"));
        assert!(payment_credential_is_script("addr1xxgx3far7qygq0k6epa"));
        assert!(payment_credential_is_script("addr1w8p79rpkcdz8x9d6tft"));
        // key payment: types 0 (q), 6 (v)
        assert!(!payment_credential_is_script("addr1qy9mg28evkzcfghlrg8"));
        assert!(!payment_credential_is_script("addr1v877gsrkj9t2j64yc06"));
        // not a payment address at all
        assert!(!payment_credential_is_script("stake1uxmaqke42j9q6v83lv"));
        assert!(!payment_credential_is_script(""));
    }
}
