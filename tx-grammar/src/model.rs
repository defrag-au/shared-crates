//! The vocabulary a transaction is described in.
//!
//! Every type here is renderer-free and policy-free. A field says what a
//! transaction *did*; it never says how to colour it, which icon to use, or how
//! long to wait before deepening a scan. Those are the renderers' business.

use std::fmt;
use std::str::FromStr;

use chains::{AssetType, ChainRef, TokenRef};
use serde::{Deserialize, Serialize};

/// Bytes in a [`Quantity`]. EVM token ids and balances are 256-bit; a `u64`
/// would silently truncate them.
pub const QUANTITY_LEN: usize = 32;

/// A 256-bit unsigned quantity, big-endian and decimal on the wire.
///
/// The grammar compares and formats; the producer does the arithmetic. Serde
/// writes the decimal form because that is how a token amount is written
/// everywhere a person reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Quantity([u8; QUANTITY_LEN]);

impl Quantity {
    pub const ZERO: Self = Self([0; QUANTITY_LEN]);
    pub const ONE: Self = Self::from_u64(1);

    pub const fn from_u64(value: u64) -> Self {
        let mut bytes = [0u8; QUANTITY_LEN];
        let source = value.to_be_bytes();
        let mut i = 0;
        while i < source.len() {
            bytes[QUANTITY_LEN - source.len() + i] = source[i];
            i += 1;
        }
        Self(bytes)
    }

    pub const fn from_u128(value: u128) -> Self {
        let mut bytes = [0u8; QUANTITY_LEN];
        let source = value.to_be_bytes();
        let mut i = 0;
        while i < source.len() {
            bytes[QUANTITY_LEN - source.len() + i] = source[i];
            i += 1;
        }
        Self(bytes)
    }

    pub fn is_zero(self) -> bool {
        self.0.iter().all(|byte| *byte == 0)
    }

    /// Sum, saturating at the 256-bit ceiling.
    ///
    /// Used to total a lot. Saturating rather than wrapping because a sum that
    /// overflows is not a smaller lot, and a renderer showing a wrapped figure
    /// would be lying about the one number it was asked for.
    pub fn saturating_add(self, other: Self) -> Self {
        let mut sum = [0u8; QUANTITY_LEN];
        let mut carry = 0u16;
        for i in (0..QUANTITY_LEN).rev() {
            let value = self.0[i] as u16 + other.0[i] as u16 + carry;
            sum[i] = (value & 0xff) as u8;
            carry = value >> 8;
        }
        if carry > 0 {
            return Self([0xff; QUANTITY_LEN]);
        }
        Self(sum)
    }

    /// The decimal form — the spelling an identified asset uses.
    pub fn to_decimal(self) -> String {
        let mut bytes = self.0;
        if bytes.iter().all(|byte| *byte == 0) {
            return "0".to_string();
        }
        let mut digits = Vec::with_capacity(78);
        while bytes.iter().any(|byte| *byte != 0) {
            let remainder = divide_small(&mut bytes, 10);
            digits.push(char::from(b'0' + remainder));
        }
        digits.iter().rev().collect()
    }

    /// Parses a decimal string. Leading zeros are accepted; a character that is
    /// not a digit is refused rather than skipped.
    pub fn from_decimal(raw: &str) -> Result<Self, QuantityError> {
        if raw.is_empty() {
            return Err(QuantityError::Empty);
        }
        let mut bytes = [0u8; QUANTITY_LEN];
        for character in raw.chars() {
            let digit = character
                .to_digit(10)
                .ok_or(QuantityError::NotADigit(character))? as u8;
            if !multiply_add(&mut bytes, 10, digit) {
                return Err(QuantityError::Overflow);
            }
        }
        Ok(Self(bytes))
    }
}

impl fmt::Display for Quantity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_decimal())
    }
}

impl TryFrom<String> for Quantity {
    type Error = QuantityError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_decimal(&value)
    }
}

impl From<Quantity> for String {
    fn from(value: Quantity) -> Self {
        value.to_decimal()
    }
}

impl FromStr for Quantity {
    type Err = QuantityError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::from_decimal(raw)
    }
}

impl From<u64> for Quantity {
    fn from(value: u64) -> Self {
        Self::from_u64(value)
    }
}

impl From<u128> for Quantity {
    fn from(value: u128) -> Self {
        Self::from_u128(value)
    }
}

/// Why a string was not a 256-bit quantity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuantityError {
    Empty,
    NotADigit(char),
    Overflow,
}

impl fmt::Display for QuantityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("no digits"),
            Self::NotADigit(character) => {
                write!(formatter, "not a decimal digit: {character:?}")
            }
            Self::Overflow => formatter.write_str("more than 256 bits"),
        }
    }
}

impl std::error::Error for QuantityError {}

fn multiply_add(bytes: &mut [u8; QUANTITY_LEN], multiply: u8, add: u8) -> bool {
    let mut carry = add as u16;
    for byte in bytes.iter_mut().rev() {
        let value = (*byte as u16) * (multiply as u16) + carry;
        *byte = (value & 0xff) as u8;
        carry = value >> 8;
    }
    carry == 0
}

fn divide_small(bytes: &mut [u8; QUANTITY_LEN], divisor: u8) -> u8 {
    let mut remainder = 0u16;
    for byte in bytes.iter_mut() {
        let value = (remainder << 8) | (*byte as u16);
        *byte = (value / divisor as u16) as u8;
        remainder = value % divisor as u16;
    }
    remainder as u8
}

/// How firmly a fact is known.
///
/// `Observed` is the ledger itself; `Derived` is a trace, a venue's record or a
/// decoded datum; `Asserted` is a human or a registry; `Inferred` is a shape
/// heuristic; `Proven` is a proof whose statement holds even when the parties
/// are hidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Basis {
    #[default]
    Observed,
    Derived,
    Asserted,
    Inferred,
    Proven,
}

/// Whether a transaction took effect, and how its step ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// The producer could not establish it. Never default to success.
    #[default]
    Unknown,
    Succeeded,
    Reverted,
    Failed,
    Pending,
    Dropped,
}

impl Outcome {
    /// A step that was included but did not take effect.
    pub fn is_failure(self) -> bool {
        matches!(self, Self::Reverted | Self::Failed | Self::Dropped)
    }
}

/// Whether the fact is on the public ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    #[default]
    Public,
    Shielded,
    Unknown,
}

impl Visibility {
    /// The more private of the two, for a cell built from several primitives.
    pub fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::Shielded, _) | (_, Self::Shielded) => Self::Shielded,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            _ => Self::Public,
        }
    }
}

/// Where an object sits after an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Custody {
    #[default]
    InHand,
    /// A marketplace's escrow: the seller still owns it, but the UTxO has moved.
    Escrow,
    /// A script that is not a marketplace — a vesting lock, a bridge, an order.
    Script,
    /// A staking delegation or a protocol position.
    Staked,
}

/// Which way value moved for the viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    In,
    Out,
    Both,
    /// A custody change or a claim: nothing left the viewer's reach.
    InPlace,
    /// A claim or a permission: nothing moved at all.
    #[default]
    Unchanged,
}

/// What part a party played in the transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// The viewer's own wallet or policy. The text renderer says "you".
    Own,
    Peer,
    Venue,
    Contract,
    Pool,
    Network,
    #[default]
    Unknown,
}

/// What kind of identity a [`PartyKey`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartyKind {
    #[default]
    Account,
    Stake,
    Contract,
    Script,
    Pool,
    Shielded,
}

/// A chain-qualified identity: the key a click navigates to.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PartyKey {
    pub chain: ChainRef,
    pub address: String,
    #[serde(default)]
    pub kind: PartyKind,
}

impl PartyKey {
    pub fn new(chain: ChainRef, address: impl Into<String>) -> Self {
        Self {
            chain,
            address: address.into(),
            kind: PartyKind::Account,
        }
    }

    pub fn with_kind(mut self, kind: PartyKind) -> Self {
        self.kind = kind;
        self
    }
}

/// Who was on one side — or which kind of nobody.
///
/// The absences are deliberately separate. `BelowFloor` resolves by walking
/// deeper; `Ambiguous` never does; `Untraced` is a transfer the index did not
/// follow; `Shielded` is hidden by design. Collapsing them into `None` loses
/// the only distinction a reader can act on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Party {
    Known {
        label: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<PartyKey>,
        basis: Basis,
    },
    BelowFloor,
    Untraced,
    Shielded {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        commitment: Option<String>,
    },
    Ambiguous {
        count: usize,
    },
    Unknown,
}

impl Party {
    pub fn observed(label: impl Into<String>) -> Self {
        Self::Known {
            label: label.into(),
            key: None,
            basis: Basis::Observed,
        }
    }

    pub fn derived(label: impl Into<String>) -> Self {
        Self::Known {
            label: label.into(),
            key: None,
            basis: Basis::Derived,
        }
    }

    pub fn asserted(label: impl Into<String>) -> Self {
        Self::Known {
            label: label.into(),
            key: None,
            basis: Basis::Asserted,
        }
    }

    pub fn inferred(label: impl Into<String>) -> Self {
        Self::Known {
            label: label.into(),
            key: None,
            basis: Basis::Inferred,
        }
    }

    /// Attach the navigation key. A no-op on an absence, which has nowhere to go.
    pub fn keyed(mut self, key: PartyKey) -> Self {
        if let Self::Known { key: slot, .. } = &mut self {
            *slot = Some(key);
        }
        self
    }

    /// Where a click on this party goes, if anywhere.
    pub fn walk_target(&self) -> Option<&PartyKey> {
        match self {
            Self::Known { key, .. } => key.as_ref(),
            _ => None,
        }
    }

    pub fn is_absent(&self) -> bool {
        !matches!(self, Self::Known { .. })
    }
}

/// A party plus the part it played. The role is separate from the identity so
/// the same address can be a venue in one action and a peer in another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actor {
    pub role: Role,
    pub party: Party,
}

impl Actor {
    pub fn own(party: Party) -> Self {
        Self {
            role: Role::Own,
            party,
        }
    }

    pub fn peer(party: Party) -> Self {
        Self {
            role: Role::Peer,
            party,
        }
    }

    pub fn venue(party: Party) -> Self {
        Self {
            role: Role::Venue,
            party,
        }
    }

    pub fn contract(party: Party) -> Self {
        Self {
            role: Role::Contract,
            party,
        }
    }

    pub fn pool(party: Party) -> Self {
        Self {
            role: Role::Pool,
            party,
        }
    }

    pub fn network(chain: ChainRef) -> Self {
        Self {
            role: Role::Network,
            party: Party::observed(chain.as_caip2()),
        }
    }

    pub fn unknown() -> Self {
        Self {
            role: Role::Unknown,
            party: Party::Unknown,
        }
    }

    pub fn with_role(mut self, role: Role) -> Self {
        self.role = role;
        self
    }

    pub fn is_own(&self) -> bool {
        self.role == Role::Own
    }
}

/// One chain-qualified asset and how much of it.
///
/// `decimals` is display metadata a producer resolves; the grammar never
/// guesses it. `label` is the resolved symbol or name — absent means the
/// renderer falls back to the token reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    pub token: TokenRef,
    pub quantity: Quantity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decimals: Option<u8>,
}

impl Asset {
    pub fn new(token: TokenRef, quantity: Quantity) -> Self {
        Self {
            token,
            quantity,
            label: None,
            decimals: None,
        }
    }

    pub fn one(token: TokenRef) -> Self {
        Self::new(token, Quantity::ONE)
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn with_decimals(mut self, decimals: u8) -> Self {
        self.decimals = Some(decimals);
        self
    }

    pub fn chain(&self) -> ChainRef {
        self.token.chain()
    }

    pub fn namespace(&self) -> &str {
        self.token.asset_type().namespace()
    }

    pub fn token_id(&self) -> Option<&str> {
        self.token.token_id()
    }

    /// A currency is what a price is paid in: native ADA/ETH/dust, or an
    /// ERC-20. Everything else is a token being traded.
    pub fn is_currency(&self) -> bool {
        matches!(self.namespace(), "slip44" | "erc20")
    }
}

/// What an action was about.
///
/// A collection is its own variant because a collection offer names a policy or
/// a contract with no token id, and drawing a fake artwork for it is the
/// failure this avoids. `Hidden` is the Midnight case: the object is real, the
/// ledger does not reveal it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Object {
    Asset(Asset),
    Lot {
        items: Vec<Asset>,
        distinct: u32,
        total: Quantity,
    },
    Collection(AssetType),
    Hidden {
        visibility: Visibility,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
}

impl Object {
    pub fn asset(asset: Asset) -> Self {
        Self::Asset(asset)
    }

    pub fn lot(items: Vec<Asset>, total: Quantity) -> Self {
        Self::Lot {
            distinct: u32::try_from(items.len()).unwrap_or(u32::MAX),
            items,
            total,
        }
    }

    /// A lot whose `items` are a sample of `distinct` real things.
    ///
    /// Producers cap what they carry: a sweep of forty NFTs is forty facts and
    /// five pictures. `lot` would then report five, which understates the
    /// sweep; this says both, so the count is the truth and the sample is what
    /// a renderer can draw.
    pub fn lot_sampled(items: Vec<Asset>, distinct: u32, total: Quantity) -> Self {
        Self::Lot {
            distinct: distinct.max(u32::try_from(items.len()).unwrap_or(u32::MAX)),
            items,
            total,
        }
    }

    pub fn collection(asset_type: AssetType) -> Self {
        Self::Collection(asset_type)
    }

    pub fn hidden(visibility: Visibility) -> Self {
        Self::Hidden {
            visibility,
            label: None,
        }
    }

    /// The units the object carries. A lot of five NFTs has `5`; one ERC-1155
    /// id at quantity five has `5` on its [`Asset`].
    pub fn quantity(&self) -> Quantity {
        match self {
            Self::Asset(asset) => asset.quantity,
            Self::Lot { total, .. } => *total,
            Self::Collection(_) | Self::Hidden { .. } => Quantity::ZERO,
        }
    }

    /// How many distinct things the object names.
    pub fn distinct(&self) -> u32 {
        match self {
            Self::Asset(_) | Self::Collection(_) => 1,
            Self::Lot { distinct, .. } => *distinct,
            Self::Hidden { .. } => 0,
        }
    }

    pub fn items(&self) -> &[Asset] {
        match self {
            Self::Asset(asset) => std::slice::from_ref(asset),
            Self::Lot { items, .. } => items,
            Self::Collection(_) | Self::Hidden { .. } => &[],
        }
    }

    pub fn as_asset(&self) -> Option<&Asset> {
        match self {
            Self::Asset(asset) => Some(asset),
            _ => None,
        }
    }

    /// Whether every item is a currency. An empty lot is not.
    pub fn is_currency(&self) -> bool {
        !self.items().is_empty() && self.items().iter().all(Asset::is_currency)
    }

    pub fn is_hidden(&self) -> bool {
        matches!(self, Self::Hidden { .. })
    }
}

/// What sort of claim an intention is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimKind {
    /// A seller's asking price.
    Ask,
    /// A buyer's bid on one thing.
    Bid,
    /// A buyer's bid on a whole collection.
    CollectionBid,
    /// A timed listing.
    Auction,
    /// The source did not say which. Never guessed.
    #[default]
    Unknown,
}

/// An intention about an object: an ask, a bid, an auction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    pub object: Object,
    pub actor: Actor,
    pub kind: ClaimKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub venue: Option<String>,
}

impl Claim {
    pub fn ask(object: Object, actor: Actor) -> Self {
        Self::new(object, actor, ClaimKind::Ask)
    }

    pub fn bid(object: Object, actor: Actor) -> Self {
        Self::new(object, actor, ClaimKind::Bid)
    }

    pub fn collection_bid(object: Object, actor: Actor) -> Self {
        Self::new(object, actor, ClaimKind::CollectionBid)
    }

    pub fn auction(object: Object, actor: Actor) -> Self {
        Self::new(object, actor, ClaimKind::Auction)
    }

    pub fn unknown(object: Object, actor: Actor) -> Self {
        Self::new(object, actor, ClaimKind::Unknown)
    }

    pub fn on(mut self, venue: impl Into<String>) -> Self {
        self.venue = Some(venue.into());
        self
    }

    fn new(object: Object, actor: Actor, kind: ClaimKind) -> Self {
        Self {
            object,
            actor,
            kind,
            venue: None,
        }
    }
}

/// What a figure argues. Not a sign: an asking price and a paid price are both
/// positive numbers and mean different things.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FigureKind {
    Paid,
    Ask,
    Bid,
    Fee,
    Refund,
    PriceChange,
}

impl FigureKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Paid => "paid",
            Self::Ask => "ask",
            Self::Bid => "bid",
            Self::Fee => "fee",
            Self::Refund => "refund",
            Self::PriceChange => "price",
        }
    }
}

/// Whether a figure is for one item, a lot, or the whole transaction.
///
/// This is what stops a bundle price being multiplied by its bundle size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Per {
    #[default]
    Item,
    Lot,
    Total,
}

/// A number with a meaning and the asset it is denominated in.
///
/// The amount carries its own token, so a sale settled in WETH is not rendered
/// as ADA and a USDC bid is not rendered as ETH.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Figure {
    pub kind: FigureKind,
    pub amount: Asset,
    pub per: Per,
    pub basis: Basis,
}

impl Figure {
    pub fn paid(amount: Asset) -> Self {
        Self::new(FigureKind::Paid, amount)
    }

    pub fn ask(amount: Asset) -> Self {
        Self::new(FigureKind::Ask, amount)
    }

    pub fn bid(amount: Asset) -> Self {
        Self::new(FigureKind::Bid, amount)
    }

    pub fn fee(amount: Asset) -> Self {
        Self::new(FigureKind::Fee, amount)
    }

    pub fn refund(amount: Asset) -> Self {
        Self::new(FigureKind::Refund, amount)
    }

    pub fn price_change(amount: Asset) -> Self {
        Self::new(FigureKind::PriceChange, amount)
    }

    pub fn per(mut self, per: Per) -> Self {
        self.per = per;
        self
    }

    pub fn basis(mut self, basis: Basis) -> Self {
        self.basis = basis;
        self
    }

    fn new(kind: FigureKind, amount: Asset) -> Self {
        Self {
            kind,
            amount,
            per: Per::Item,
            basis: Basis::Observed,
        }
    }
}

/// Why a fact is missing, when the missingness itself is the fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Hidden {
    Shielded,
    Private,
    NotIndexed,
}

/// A cell-level absence. Party-level absences live in [`Party`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Absence {
    Hidden { reason: Hidden },
    Unknown,
    Ambiguous { count: usize },
}

/// Whether a sale was publicly listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SaleKind {
    #[default]
    Public,
    Private,
}

/// How a listing was offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListingKind {
    #[default]
    FixedPrice,
    Auction,
    CollectionOffer,
    Unknown,
}

/// The mechanism an action used. A facet, never a primitive: a new marketplace
/// is a new venue label, not a new verb.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mechanism {
    Marketplace,
    Dex,
    Lending,
    Staking,
    Vesting,
    Bridge,
    Wrap,
    Contract,
    Unknown,
}

/// A typed fact about the transaction that is not one of the action axes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Facet {
    Chain { chain: ChainRef },
    Standard { namespace: String },
    Venue { venue: String },
    Mechanism { mechanism: Mechanism },
    SaleKind { sale: SaleKind },
    ListingKind { listing: ListingKind },
}

impl Facet {
    pub fn chain(chain: ChainRef) -> Self {
        Self::Chain { chain }
    }

    pub fn venue(venue: impl Into<String>) -> Self {
        Self::Venue {
            venue: venue.into(),
        }
    }

    pub fn mechanism(mechanism: Mechanism) -> Self {
        Self::Mechanism { mechanism }
    }

    pub fn standard(namespace: impl Into<String>) -> Self {
        Self::Standard {
            namespace: namespace.into(),
        }
    }

    pub fn sale_kind(sale: SaleKind) -> Self {
        Self::SaleKind { sale }
    }

    pub fn listing_kind(listing: ListingKind) -> Self {
        Self::ListingKind { listing }
    }
}

/// One asset's change for the viewer, after all the actions in a phrase.
///
/// A reconciliation is per asset, not one number: a transaction can net
/// `-0.05 ETH` and `+150 USDC`, and a single "net" would be meaningless.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BalanceDelta {
    pub asset: Asset,
    pub direction: Direction,
    pub basis: Basis,
}

impl BalanceDelta {
    pub fn credit(asset: Asset) -> Self {
        Self {
            asset,
            direction: Direction::In,
            basis: Basis::Observed,
        }
    }

    pub fn debit(asset: Asset) -> Self {
        Self {
            asset,
            direction: Direction::Out,
            basis: Basis::Observed,
        }
    }

    pub fn basis(mut self, basis: Basis) -> Self {
        self.basis = basis;
        self
    }
}
