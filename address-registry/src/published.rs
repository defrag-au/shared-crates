//! The stake-service table as data: [`STAKE_REGISTRY`] and
//! [`STAKE_EXCLUSIONS`], published as files a service fetches at run time.
//!
//! # Why it is published
//!
//! The registry answers two different questions. WHAT an address is (a
//! marketplace escrow, a burn sink, a DEX pool) is script identity: code
//! decodes datums by it, so a new entry ships with code, and it stays compiled
//! in. WHO runs an ordinary wallet (an exchange's hot wallet, a minting
//! provider, a staking programme) is a claim about people. It changes without
//! any code changing, and a wrong one should be fixable in minutes, not after
//! every consumer is rebuilt and redeployed. The Wayup entry that was really a
//! person's wallet waited on exactly that.
//!
//! # Code first
//!
//! The Rust tables stay the source of truth, so every change is reviewed,
//! compiled and tested like code. The files under `published/` are generated
//! from them:
//!
//! ```sh
//! cargo run -p address-registry --bin publish-stake-services
//! ```
//!
//! and `the_published_files_match_the_code` fails while either differs, so CI
//! (`cargo test`) is the sync check. The repository is public, so a merge to
//! `main` publishes: a worker fetches [`PUBLISHED_BIN_URL`] or
//! [`PUBLISHED_JSON_URL`], caches it, and keeps its last good copy when a
//! fetch fails or a file is refused.
//!
//! # Two files, two jobs
//!
//! - **The JSON is the whole record** ([`StakeServiceTable`]): every service
//!   with its source, and every exclusion with its reason. For reviewers,
//!   audits, and anything that wants the evidence.
//! - **The binary is the lookup** ([`StakeLabels`]): each service's stake key
//!   as its 29 raw bytes, its kind and its label, and nothing else. What a
//!   service answering "who is this wallet" needs. Exclusions are not in it:
//!   an excluded key simply has no label. [`StakeLabels::to_bytes`] documents
//!   the layout.
//!
//! Compressed, the two differed by under 7% when both carried the sources
//! (MEASURED, brotli -q 11: 1,205 against 1,128 bytes): the prose is the bulk
//! of the record, and costs the same in either format. Leaving it out is what
//! makes the binary small.

use serde::{Deserialize, Serialize};

use crate::registry::{STAKE_EXCLUSIONS, STAKE_REGISTRY, StakeServiceKind};

/// Both files' format. Bumped when either layout changes; a reader refuses a
/// format it does not know rather than guessing.
pub const FORMAT: u16 = 1;

/// The binary's first four bytes.
pub const MAGIC: &[u8; 4] = b"STKL";

/// Where `main`'s JSON is served.
pub const PUBLISHED_JSON_URL: &str = "https://raw.githubusercontent.com/defrag-au/shared-crates/main/address-registry/published/stake-services.json";

/// Where `main`'s binary is served.
pub const PUBLISHED_BIN_URL: &str = "https://raw.githubusercontent.com/defrag-au/shared-crates/main/address-registry/published/stake-services.bin";

/// The published files, relative to the crate.
pub const JSON_PATH: &str = "published/stake-services.json";
pub const BIN_PATH: &str = "published/stake-services.bin";

/// A stake address's bytes: a header (`0xe1` for a key credential, `0xf1`
/// for a script, mainnet) and the 28-byte credential.
pub type StakeBytes = [u8; 29];

/// Every stake service, and every credential deliberately not one: the
/// JSON file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StakeServiceTable {
    pub format: u16,
    /// Sorted by `stake`, one entry each.
    pub services: Vec<Service>,
    /// Sorted by `stake`, one entry each, none also a service.
    pub exclusions: Vec<Exclusion>,
}

/// One service run from a wallet, by its stake credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Service {
    /// The bech32 stake address, `stake1…`.
    pub stake: String,
    pub label: String,
    pub kind: StakeServiceKind,
    /// How the attribution was established.
    pub source: String,
}

/// A stake credential that is not one service's, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exclusion {
    pub stake: String,
    pub reason: String,
}

/// Each service's label and kind by its stake key's bytes: the binary file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StakeLabels {
    /// Sorted by `stake`, one entry each.
    pub entries: Vec<StakeLabel>,
}

/// One service, as the lookup needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StakeLabel {
    pub stake: StakeBytes,
    pub kind: StakeServiceKind,
    pub label: String,
}

/// Why a published file was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableError {
    /// The JSON did not decode.
    Json(String),
    /// The binary did not decode.
    Binary(&'static str),
    /// It decoded, and failed its checks.
    Check(Vec<String>),
}

impl std::fmt::Display for TableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(e) => write!(f, "stake-service table: bad JSON: {e}"),
            Self::Binary(e) => write!(f, "stake-service table: bad binary: {e}"),
            Self::Check(problems) => {
                write!(f, "stake-service table: {}", problems.join("; "))
            }
        }
    }
}

impl std::error::Error for TableError {}

fn checked<T>(value: T, problems: Vec<String>) -> Result<T, TableError> {
    match problems.is_empty() {
        true => Ok(value),
        false => Err(TableError::Check(problems)),
    }
}

fn unknown_format(format: u16) -> String {
    format!("format {format} is not {FORMAT}, the one this reader knows")
}

impl StakeServiceTable {
    /// The table this build was compiled with.
    pub fn compiled() -> Self {
        let mut services: Vec<Service> = STAKE_REGISTRY
            .entries()
            .map(|(stake, s)| Service {
                stake: (*stake).to_string(),
                label: s.label.to_string(),
                kind: s.kind,
                source: s.source.to_string(),
            })
            .collect();
        services.sort_by(|a, b| a.stake.cmp(&b.stake));
        let mut exclusions: Vec<Exclusion> = STAKE_EXCLUSIONS
            .iter()
            .map(|e| Exclusion {
                stake: e.stake.to_string(),
                reason: e.reason.to_string(),
            })
            .collect();
        exclusions.sort_by(|a, b| a.stake.cmp(&b.stake));
        Self {
            format: FORMAT,
            services,
            exclusions,
        }
    }

    /// The service a stake credential belongs to, if any.
    pub fn lookup(&self, stake: &str) -> Option<&Service> {
        self.services
            .binary_search_by(|s| s.stake.as_str().cmp(stake))
            .ok()
            .map(|i| &self.services[i])
    }

    /// The lookup the binary carries. A service whose key does not decode is
    /// left out; [`Self::check`] is what refuses one.
    pub fn labels(&self) -> StakeLabels {
        let mut entries: Vec<StakeLabel> = self
            .services
            .iter()
            .filter_map(|s| {
                Some(StakeLabel {
                    stake: stake_bytes(&s.stake)?,
                    kind: s.kind,
                    label: s.label.clone(),
                })
            })
            .collect();
        entries.sort_by_key(|e| e.stake);
        StakeLabels { entries }
    }

    /// Everything wrong with the table, or nothing. What a worker runs on
    /// every fetch before it believes one: a known format; every key a real
    /// mainnet stake address (bech32 checksum and all); labels, sources and
    /// reasons present; each list sorted with no repeats; and no key both a
    /// service and an exclusion.
    pub fn check(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.format != FORMAT {
            problems.push(unknown_format(self.format));
        }
        let keys = |stakes: Vec<&str>, what: &str, problems: &mut Vec<String>| {
            for s in &stakes {
                if stake_bytes(s).is_none() {
                    problems.push(format!("{what} {s:?} is not a mainnet stake address"));
                }
            }
            for pair in stakes.windows(2) {
                if pair[0] >= pair[1] {
                    problems.push(format!(
                        "{what}s are not sorted with one entry each at {:?}",
                        pair[1]
                    ));
                }
            }
        };
        keys(
            self.services.iter().map(|s| s.stake.as_str()).collect(),
            "service",
            &mut problems,
        );
        keys(
            self.exclusions.iter().map(|e| e.stake.as_str()).collect(),
            "exclusion",
            &mut problems,
        );
        for s in &self.services {
            if s.label.trim().is_empty() {
                problems.push(format!("service {} has no label", s.stake));
            }
            if s.source.trim().is_empty() {
                problems.push(format!("service {} has no source", s.stake));
            }
        }
        for e in &self.exclusions {
            if e.reason.trim().is_empty() {
                problems.push(format!("exclusion {} has no reason", e.stake));
            }
            if self.lookup(&e.stake).is_some() {
                problems.push(format!("{} is both a service and an exclusion", e.stake));
            }
        }
        problems
    }

    /// The JSON encoding: pretty, with a trailing newline, byte-stable.
    pub fn to_json(&self) -> String {
        let mut out =
            serde_json::to_string_pretty(self).expect("a table of strings always serialises");
        out.push('\n');
        out
    }

    /// A table from its JSON, checked.
    pub fn from_json(json: &str) -> Result<Self, TableError> {
        let table =
            serde_json::from_str::<Self>(json).map_err(|e| TableError::Json(e.to_string()))?;
        let problems = table.check();
        checked(table, problems)
    }
}

impl StakeLabels {
    /// The service a stake address belongs to, if any. A string that is not
    /// a mainnet stake address has none.
    pub fn lookup(&self, stake: &str) -> Option<&StakeLabel> {
        self.lookup_bytes(&stake_bytes(stake)?)
    }

    /// The service a stake key's bytes belong to, if any.
    pub fn lookup_bytes(&self, stake: &StakeBytes) -> Option<&StakeLabel> {
        self.entries
            .binary_search_by(|e| e.stake.cmp(stake))
            .ok()
            .map(|i| &self.entries[i])
    }

    /// Everything wrong with the lookup, or nothing: every key a mainnet
    /// stake header, sorted with no repeats, and every label present.
    pub fn check(&self) -> Vec<String> {
        let mut problems = Vec::new();
        for e in &self.entries {
            if !is_stake_header(e.stake[0]) {
                problems.push(format!(
                    "header {:#04x} is not a mainnet stake address",
                    e.stake[0]
                ));
            }
            if e.label.trim().is_empty() {
                problems.push(format!("{} has no label", e.stake_address()));
            }
        }
        for pair in self.entries.windows(2) {
            if pair[0].stake >= pair[1].stake {
                problems.push(format!(
                    "not sorted with one entry each at {}",
                    pair[1].stake_address()
                ));
            }
        }
        problems
    }

    /// The binary encoding. `format` is little-endian; `n` is an unsigned
    /// LEB128 varint; `label` is its byte length as `n`, then UTF-8.
    ///
    /// ```text
    /// "STKL"  format:u16
    /// n  entries: stake:29 bytes  kind:u8  label
    /// ```
    ///
    /// `kind` is [`kind_code`]: explicit codes, so reordering the enum never
    /// changes a published file. Nothing may follow the last entry.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&FORMAT.to_le_bytes());
        put_len(&mut out, self.entries.len());
        for e in &self.entries {
            out.extend_from_slice(&e.stake);
            out.push(kind_code(e.kind));
            put_len(&mut out, e.label.len());
            out.extend_from_slice(e.label.as_bytes());
        }
        out
    }

    /// A lookup from its binary, checked.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TableError> {
        let mut r = Reader { bytes, at: 0 };
        if r.take(4)? != MAGIC {
            return Err(TableError::Binary("not a stake-label table"));
        }
        let format = u16::from_le_bytes([r.byte()?, r.byte()?]);
        if format != FORMAT {
            // The layout after the header is the format's own: read no further.
            return Err(TableError::Check(vec![unknown_format(format)]));
        }
        let mut entries = Vec::new();
        for _ in 0..r.len()? {
            let stake: StakeBytes = r.take(29)?.try_into().expect("took 29");
            let kind = kind_from_code(r.byte()?).ok_or(TableError::Binary(
                "a service kind this reader does not know",
            ))?;
            let n = r.len()?;
            let label = String::from_utf8(r.take(n)?.to_vec())
                .map_err(|_| TableError::Binary("a label not UTF-8"))?;
            entries.push(StakeLabel { stake, kind, label });
        }
        if r.at != bytes.len() {
            return Err(TableError::Binary("bytes after the last entry"));
        }
        let labels = Self { entries };
        let problems = labels.check();
        checked(labels, problems)
    }
}

impl StakeLabel {
    /// The bech32 stake address, `stake1…`.
    pub fn stake_address(&self) -> String {
        stake_address(&self.stake)
    }
}

/// A kind's code in the binary encoding. EXHAUSTIVE on purpose: a new kind
/// does not compile until it has a code, and a code is never reused.
pub const fn kind_code(kind: StakeServiceKind) -> u8 {
    match kind {
        StakeServiceKind::MintingProvider => 0,
        StakeServiceKind::Marketplace => 1,
        StakeServiceKind::Exchange => 2,
        StakeServiceKind::Staking => 3,
    }
}

/// The kind a code names, or `None` for one this build does not know. A
/// reader then refuses the whole file and keeps its last good one: a new
/// kind needs a release that knows what to do with it.
pub const fn kind_from_code(code: u8) -> Option<StakeServiceKind> {
    match code {
        0 => Some(StakeServiceKind::MintingProvider),
        1 => Some(StakeServiceKind::Marketplace),
        2 => Some(StakeServiceKind::Exchange),
        3 => Some(StakeServiceKind::Staking),
        _ => None,
    }
}

fn put_len(out: &mut Vec<u8>, mut n: usize) {
    loop {
        let low = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            out.push(low);
            return;
        }
        out.push(low | 0x80);
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], TableError> {
        let end = self
            .at
            .checked_add(n)
            .filter(|end| *end <= self.bytes.len())
            .ok_or(TableError::Binary("ends early"))?;
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }

    fn byte(&mut self) -> Result<u8, TableError> {
        Ok(self.take(1)?[0])
    }

    fn len(&mut self) -> Result<usize, TableError> {
        let mut n: usize = 0;
        // Four groups is 28 bits: far past any table, and short of overflow.
        for shift in (0..28).step_by(7) {
            let b = self.byte()?;
            n |= ((b & 0x7f) as usize) << shift;
            if b & 0x80 == 0 {
                return Ok(n);
            }
        }
        Err(TableError::Binary("a length too long"))
    }
}

// ── bech32, for mainnet stake addresses only ────────────────────────────────
//
// Decoded here rather than with an address library: the crate stays
// dependency-light, and a stake address is one fixed shape.

const CHARSET: &[u8; 32] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
const HRP: &[u8] = b"stake";

/// A mainnet stake address header: a key (`0xe1`) or script (`0xf1`)
/// credential.
const fn is_stake_header(h: u8) -> bool {
    matches!(h, 0xe1 | 0xf1)
}

/// A mainnet stake address's bytes, or `None` for anything else: the wrong
/// prefix or length, a character outside bech32, a bad checksum, or a header
/// that is not a mainnet stake address's.
pub fn stake_bytes(s: &str) -> Option<StakeBytes> {
    let data = s.strip_prefix("stake1")?;
    // 29 bytes are 47 groups of five bits, then a six-group checksum.
    if data.len() != 53 {
        return None;
    }
    let groups: Vec<u8> = data
        .bytes()
        .map(|c| CHARSET.iter().position(|x| *x == c).map(|v| v as u8))
        .collect::<Option<_>>()?;
    let mut values = hrp_expand();
    values.extend_from_slice(&groups);
    if polymod(&values) != 1 {
        return None;
    }
    let mut out = [0u8; 29];
    let (mut acc, mut bits, mut at) = (0u32, 0u32, 0usize);
    for g in &groups[..47] {
        acc = (acc << 5) | *g as u32;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out[at] = (acc >> bits) as u8;
            at += 1;
        }
    }
    // 235 bits for 232: the three left over are padding, and must be zero.
    if acc & ((1 << bits) - 1) != 0 || !is_stake_header(out[0]) {
        return None;
    }
    Some(out)
}

/// The bech32 stake address for a key's bytes.
pub fn stake_address(stake: &StakeBytes) -> String {
    let mut groups: Vec<u8> = Vec::with_capacity(53);
    let (mut acc, mut bits) = (0u32, 0u32);
    for b in stake {
        acc = (acc << 8) | *b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            groups.push(((acc >> bits) & 31) as u8);
        }
    }
    if bits > 0 {
        groups.push(((acc << (5 - bits)) & 31) as u8);
    }
    let mut values = hrp_expand();
    values.extend_from_slice(&groups);
    values.extend_from_slice(&[0; 6]);
    let sum = polymod(&values) ^ 1;
    groups.extend((0..6).map(|i| ((sum >> (5 * (5 - i))) & 31) as u8));
    let mut out = String::from("stake1");
    out.extend(groups.iter().map(|g| CHARSET[*g as usize] as char));
    out
}

fn hrp_expand() -> Vec<u8> {
    let mut v: Vec<u8> = HRP.iter().map(|c| c >> 5).collect();
    v.push(0);
    v.extend(HRP.iter().map(|c| c & 31));
    v
}

fn polymod(values: &[u8]) -> u32 {
    const GEN: [u32; 5] = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3];
    let mut chk: u32 = 1;
    for v in values {
        let top = chk >> 25;
        chk = ((chk & 0x1ffffff) << 5) ^ *v as u32;
        for (i, g) in GEN.iter().enumerate() {
            if (top >> i) & 1 == 1 {
                chk ^= g;
            }
        }
    }
    chk
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::lookup_stake;

    fn file(rel: &str) -> Vec<u8> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
        std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    /// THE SYNC CHECK. The published files are generated from the code, and
    /// a change to either table that was not republished fails here, so it
    /// cannot reach `main` with stale files beside it.
    #[test]
    fn the_published_files_match_the_code() {
        let table = StakeServiceTable::compiled();
        let fix = "regenerate with `cargo run -p address-registry --bin \
                   publish-stake-services` and commit the files with the change";
        assert!(
            file(JSON_PATH) == table.to_json().as_bytes(),
            "{JSON_PATH} is not what the code publishes: {fix}"
        );
        assert!(
            file(BIN_PATH) == table.labels().to_bytes(),
            "{BIN_PATH} is not what the code publishes: {fix}"
        );
    }

    #[test]
    fn the_compiled_table_passes_its_own_checks() {
        let table = StakeServiceTable::compiled();
        assert_eq!(table.check(), Vec::<String>::new());
        assert_eq!(table.services.len(), STAKE_REGISTRY.len());
        assert_eq!(table.exclusions.len(), STAKE_EXCLUSIONS.len());
        let labels = table.labels();
        assert_eq!(labels.check(), Vec::<String>::new());
        assert_eq!(labels.entries.len(), STAKE_REGISTRY.len());
    }

    /// Both files decode to what they came from, and answer as the compiled
    /// lookup does. Exclusions have no label.
    #[test]
    fn both_files_round_trip_and_answer_as_the_code_does() {
        let table = StakeServiceTable::compiled();
        let labels = table.labels();
        assert_eq!(
            StakeServiceTable::from_json(&table.to_json()),
            Ok(table.clone())
        );
        assert_eq!(
            StakeLabels::from_bytes(&labels.to_bytes()),
            Ok(labels.clone())
        );
        for (stake, s) in STAKE_REGISTRY.entries() {
            let found = table.lookup(stake).expect("in the record");
            assert_eq!((found.label.as_str(), found.kind), (s.label, s.kind));
            let found = labels.lookup(stake).expect("in the lookup");
            assert_eq!((found.label.as_str(), found.kind), (s.label, s.kind));
        }
        for e in STAKE_EXCLUSIONS {
            assert!(table.lookup(e.stake).is_none());
            assert!(labels.lookup(e.stake).is_none());
            assert!(lookup_stake(e.stake).is_none(), "{} is excluded", e.stake);
        }
    }

    /// The bytes are the address: every key in both tables decodes, and
    /// encodes back to exactly itself.
    #[test]
    fn a_stake_key_survives_bytes_and_back() {
        let keys = STAKE_REGISTRY
            .keys()
            .copied()
            .chain(STAKE_EXCLUSIONS.iter().map(|e| e.stake));
        for key in keys {
            let bytes = stake_bytes(key).unwrap_or_else(|| panic!("{key} decodes"));
            assert_eq!(stake_address(&bytes), key);
        }
        // A key credential and a script credential: both headers.
        assert_eq!(
            stake_bytes("stake1uy50zl7a9k9c74v66c0gn833at5sh83qnjldk8hg4rrv05g3mmskr")
                .map(|b| b[0]),
            Some(0xe1)
        );
        assert_eq!(
            stake_bytes("stake17ykfvl6t62y5fvryvtsnch3lt406dcpls4n4d9pcekpnumg6v83tq")
                .map(|b| b[0]),
            Some(0xf1)
        );
    }

    /// The binary is a lookup, not the record: a few hundred bytes.
    #[test]
    fn the_binary_is_small() {
        let table = StakeServiceTable::compiled();
        let bytes = table.labels().to_bytes();
        // Header, count, then per entry: 29 + 1 + a length byte + the label.
        let want = 6
            + 1
            + table
                .services
                .iter()
                .map(|s| 31 + s.label.len())
                .sum::<usize>();
        assert_eq!(bytes.len(), want);
    }

    #[test]
    fn every_kind_has_its_own_code() {
        for code in 0..=u8::MAX {
            if let Some(kind) = kind_from_code(code) {
                assert_eq!(kind_code(kind), code);
            }
        }
    }

    /// What a worker must refuse rather than serve.
    #[test]
    fn a_bad_table_is_refused_with_its_reasons() {
        let good = StakeServiceTable::compiled();
        let first = good.services[0].clone();

        let mut twice = good.clone();
        twice.services.insert(0, first.clone());
        assert!(StakeServiceTable::from_json(&twice.to_json()).is_err());

        let mut both = good.clone();
        both.exclusions.push(Exclusion {
            stake: first.stake.clone(),
            reason: "a test".to_string(),
        });
        both.exclusions.sort_by(|a, b| a.stake.cmp(&b.stake));
        let problems = both.check();
        assert!(
            problems
                .iter()
                .any(|p| p.contains("both a service and an exclusion")),
            "{problems:?}"
        );

        let mut unsourced = good.clone();
        unsourced.services[0].source = " ".to_string();
        assert!(unsourced.check().iter().any(|p| p.contains("no source")));

        // One character changed: the checksum catches it.
        let mut typo = good.clone();
        let mut key = first.stake.clone().into_bytes();
        let last = key.len() - 1;
        key[last] = if key[last] == b'q' { b'p' } else { b'q' };
        typo.services[0].stake = String::from_utf8(key).expect("ascii");
        assert!(
            typo.check()
                .iter()
                .any(|p| p.contains("not a mainnet stake address"))
        );

        let mut newer = good.clone();
        newer.format = FORMAT + 1;
        assert!(StakeServiceTable::from_json(&newer.to_json()).is_err());
    }

    #[test]
    fn a_damaged_binary_is_refused() {
        let labels = StakeServiceTable::compiled().labels();
        let bytes = labels.to_bytes();
        assert_eq!(
            StakeLabels::from_bytes(&bytes[..bytes.len() - 1]),
            Err(TableError::Binary("ends early"))
        );
        let mut longer = bytes.clone();
        longer.push(0);
        assert_eq!(
            StakeLabels::from_bytes(&longer),
            Err(TableError::Binary("bytes after the last entry"))
        );
        assert_eq!(
            StakeLabels::from_bytes(b"JSON{}"),
            Err(TableError::Binary("not a stake-label table"))
        );
        // A newer format: refused before its layout is read.
        let mut newer = bytes.clone();
        newer[4..6].copy_from_slice(&(FORMAT + 1).to_le_bytes());
        assert!(matches!(
            StakeLabels::from_bytes(&newer),
            Err(TableError::Check(_))
        ));
        // A kind this build does not know: the first entry's kind byte is
        // after the header (6), the count (1) and its key (29).
        let mut unknown = bytes.clone();
        unknown[6 + 1 + 29] = 200;
        assert_eq!(
            StakeLabels::from_bytes(&unknown),
            Err(TableError::Binary(
                "a service kind this reader does not know"
            ))
        );
        // Out of order.
        let mut swapped = labels.clone();
        swapped.entries.swap(0, 1);
        assert!(matches!(
            StakeLabels::from_bytes(&swapped.to_bytes()),
            Err(TableError::Check(_))
        ));
    }
}
