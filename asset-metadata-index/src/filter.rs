//! The v2 directory's per-candidate name filter.
//!
//! A Bloom filter over the names of one `(policy, chunk)`'s records, sized by
//! record count, hashed with FNV-1a 64 finalised through MurmurHash3's
//! `fmix64` — every constant below is the format, and the builder-side
//! derivation and the reader must agree byte for byte.
//!
//! A filter can produce false positives (costing a read) and never false
//! negatives: every inserted name's bits are set by construction, which is
//! what makes pruning reads safe.

/// Bits per record a filter is sized for. 12 rather than the classical 10:
/// at ~0.31% false positives even the widest policy measured walks inside one
/// invocation, where 10 bits (~1%) does not.
const BITS_PER_RECORD: usize = 12;

/// Probes per insert and per query.
const PROBES: usize = 8;

/// FNV-1a 64's offset basis.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

/// FNV-1a 64's prime.
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// The second seed's domain separation: the name is hashed as `0x01 ‖ name`,
/// folded in without allocating. The first seed hashes the name alone.
const SEED_SEPARATOR: u8 = 0x01;

/// The largest filter a candidate can carry — `filter_len` is a `u16`, so
/// this is also what [`len_for`] caps at.
pub(crate) const MAX_FILTER_LEN: usize = u16::MAX as usize;

/// The smallest filter a candidate carries. The eight probes of a 2-byte
/// filter (`m = 16`) land on a large fraction of its bits, so one-record
/// pairs leak far more than the sizing promises — and MEASURED across the
/// published shards, **63% of all candidate pairs are one or two records**
/// (616,288 of 977,755), with 1,637 wide policies made mostly of them. 8
/// bytes takes `m` to 64 bits and the leak with it; it costs ~4 MB over the
/// whole corpus.
const MIN_FILTER_LEN: usize = 8;

/// The filter length for a candidate whose `(policy, chunk)` holds `records`
/// record names: 12 bits each, rounded up to whole bytes, floored at
/// [`MIN_FILTER_LEN`] and capped at the length field's ceiling. A one-record
/// pair gets the 8-byte floor; the fattest measured slice (48,586 records)
/// gets the 65,535-byte cap, still ≥10.8 bits/record.
#[must_use]
pub fn len_for(records: usize) -> usize {
    let sized = records.saturating_mul(BITS_PER_RECORD).div_ceil(8);
    if sized == 0 {
        return 0;
    }
    sized.clamp(MIN_FILTER_LEN, MAX_FILTER_LEN)
}

/// FNV-1a 64 over `bytes`, continuing from `hash`.
fn fnv1a64(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// MurmurHash3's 64-bit finaliser.
///
/// Load-bearing, measured twice over. FNV-1a alone leaves too much structure
/// in the **low bits**, and the probe position is taken `mod m` with `m` as
/// small as 16 bits — so two names differ only in a suffix hashed to nearby
/// positions, and on the published corpus an absent name passed ~10% of one
/// wide policy's candidate filters where the sizing promised ~0.3%. The
/// finaliser avalanches every input bit through every output bit.
fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 33)).wrapping_mul(0xff51_afd7_ed55_8ccd);
    z = (z ^ (z >> 33)).wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    z ^ (z >> 33)
}

/// The two seed hashes for a name, in the format's domain separation.
///
/// Both hashes are finalised with [`mix64`], and the stride is forced
/// **odd**: `position()` walks `h1 + i × h2 mod m`, and a stride sharing a
/// large factor with a small `m` collapses the probes onto a shorter orbit
/// (the second measured incident: 14 of 137). An odd stride has order
/// `m/gcd ≥ 2^v₂(m) ≥ 8` for every byte-length filter, so the eight probes
/// are always distinct.
fn seeds(name: &[u8]) -> (u64, u64) {
    let h1 = mix64(fnv1a64(FNV_OFFSET, name));
    let h2 = mix64(fnv1a64(fnv1a64(FNV_OFFSET, &[SEED_SEPARATOR]), name)) | 1;
    (h1, h2)
}

/// One probe position, `(h1 + i × h2) mod m` in u64 wrapping arithmetic.
fn position(h1: u64, h2: u64, i: u64, bits: usize) -> usize {
    (h1.wrapping_add(i.wrapping_mul(h2)) % bits as u64) as usize
}

/// Whether `name` might be among the names the filter was built from.
///
/// An empty filter is **not** empty advice: it means *unfiltered — always a
/// candidate* (the v1 shape and v2's escape hatch), so it answers `true`.
#[must_use]
pub fn might_contain(filter: &[u8], name: &[u8]) -> bool {
    verdict(filter, name) != FilterVerdict::Rejected
}

/// What a candidate's filter says about a name — the readable form of
/// [`might_contain`], and the one the walk's report states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterVerdict {
    /// The filter admits the name — the candidate is read.
    Admitted,
    /// The filter rejects the name — a read saved. A Bloom has no false
    /// negatives, so a rejection is the build saying it does not hold the
    /// name.
    Rejected,
    /// No filter at all (a v1 shard, or the fail-safe escape hatch) — always
    /// read.
    Unfiltered,
}

/// The filter's verdict for a name. The predicate the reader prunes with
/// ([`crate::DirectoryView::candidates_for`]) and the verdict the walk's
/// report states are this one function, so they cannot drift.
#[must_use]
pub fn verdict(filter: &[u8], name: &[u8]) -> FilterVerdict {
    let bits = filter.len() * 8;
    if bits == 0 {
        return FilterVerdict::Unfiltered;
    }
    let (h1, h2) = seeds(name);
    let admits = (0..PROBES as u64).all(|i| {
        let at = position(h1, h2, i, bits);
        filter[at / 8] & (1 << (at % 8)) != 0
    });
    if admits {
        FilterVerdict::Admitted
    } else {
        FilterVerdict::Rejected
    }
}

/// Builds one candidate's filter.
pub struct FilterBuilder {
    bits: Vec<u8>,
}

impl FilterBuilder {
    /// Size a filter for `records` record names — the candidate's record
    /// count, known before any insert.
    #[must_use]
    pub fn new(records: usize) -> Self {
        Self {
            bits: vec![0; len_for(records)],
        }
    }

    /// Insert one record name, exactly as the record stores it.
    pub fn insert(&mut self, name: &[u8]) {
        let bits = self.bits.len() * 8;
        if bits == 0 {
            return;
        }
        let (h1, h2) = seeds(name);
        for i in 0..PROBES as u64 {
            let at = position(h1, h2, i, bits);
            self.bits[at / 8] |= 1 << (at % 8);
        }
    }

    /// The filter's bytes.
    #[must_use]
    pub fn finish(self) -> Vec<u8> {
        self.bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn len_for_rounds_sizes_and_caps_at_the_field() {
        assert_eq!(len_for(0), 0);
        assert_eq!(len_for(1), 8); // the floor
        assert_eq!(len_for(2), 8); // still the floor
        assert_eq!(len_for(8), 12); // 96 bits
        assert_eq!(len_for(48_586), MAX_FILTER_LEN); // capped, ≥10.8 bits/record
    }

    #[test]
    fn every_inserted_name_passes() {
        let names: Vec<Vec<u8>> = (0..64).map(|i| format!("asset-{i}").into_bytes()).collect();
        let mut builder = FilterBuilder::new(names.len());
        for name in &names {
            builder.insert(name);
        }
        let filter = builder.finish();
        for name in &names {
            assert!(might_contain(&filter, name), "{name:?} must pass");
        }
    }

    #[test]
    fn an_empty_filter_is_always_a_candidate() {
        assert!(might_contain(&[], b"anything"));
    }

    #[test]
    fn the_same_names_make_the_same_bytes() {
        let build = || {
            let mut builder = FilterBuilder::new(3);
            builder.insert(b"one");
            builder.insert(b"two");
            builder.insert(b"three");
            builder.finish()
        };
        assert_eq!(build(), build());
    }

    /// The name bytes are a domain of their own: an inserted empty name is
    /// still found, and a filter built for no records never claims it holds
    /// anything (its length is zero — the unfiltered shape).
    #[test]
    fn the_zero_record_filter_is_unfiltered() {
        let builder = FilterBuilder::new(0);
        assert!(builder.finish().is_empty());
    }

    /// The stride's job: whatever the filter's width, the eight probes of one
    /// name land on eight distinct bits. (The pre-fix hashing collapsed them
    /// for small filters — the measured 10% false-positive incident.)
    #[test]
    fn probes_never_collapse() {
        for bits in [16, 24, 40, 72, 128, 1024, 8192] {
            for name in [
                &b"nope"[..],
                b"UnbotheredLycan1031",
                b"UnbotheredLycan1032",
                b"",
                &[0xff; 32],
            ] {
                let (h1, h2) = seeds(name);
                let mut positions: Vec<usize> = (0..PROBES as u64)
                    .map(|i| position(h1, h2, i, bits))
                    .collect();
                positions.sort_unstable();
                let distinct = positions.len();
                positions.dedup();
                assert_eq!(
                    positions.len(),
                    distinct,
                    "{name:?} probes collapse on a {bits}-bit filter"
                );
            }
        }
    }

    /// Sequential editions — batch mints — are the adversarial case for a
    /// filter whose hash leaves string structure in place: names in one pair
    /// share long prefixes and differ in the last bytes, and if the probes of
    /// different names stay aligned, false positives run an order of
    /// magnitude above the sizing. MEASURED on the full build before the
    /// finaliser: an absent name passed ~10% of one wide policy's candidate
    /// filters (14 of 137). This pins the rate near the designed ~0.3%.
    #[test]
    fn similar_names_do_not_inflate_the_false_positive_rate() {
        let held_out: Vec<Vec<u8>> = (0..128)
            .map(|i| format!("UnbotheredLycan{}", 50_000 + i).into_bytes())
            .collect();
        let mut false_positives = 0usize;
        for t in 0..16u32 {
            let mut builder = FilterBuilder::new(128);
            for i in 0..128u32 {
                builder.insert(format!("UnbotheredLycan{}", 1_000 + t * 200 + i).as_bytes());
            }
            let filter = builder.finish();
            for name in &held_out {
                if might_contain(&filter, name) {
                    false_positives += 1;
                }
            }
        }
        let queries = 16 * held_out.len();
        // The sizing's rate is ~0.31%; 2% is a wide regression bound that
        // structure-leaking hashing blows through and a sound one clears by
        // an order of magnitude.
        assert!(
            false_positives * 100 < queries * 2,
            "{false_positives}/{queries} false positives — the hash is leaking structure"
        );
    }

    /// The corpus's dominant shape: 63% of candidate pairs hold one or two
    /// records (MEASURED across the published shards), and 1,637 wide
    /// policies are mostly such pairs — the class an unfloored 2-byte filter
    /// leaked ~2% per query on. The floor is what keeps a wide policy's
    /// expected reads near one.
    ///
    /// The two shapes are counted apart because their leaks differ in kind.
    /// A one-record filter has no fill to speak of: double hashing makes both
    /// the inserted name and the query an 8-term arithmetic progression mod
    /// `m`, so a query matches exactly when it reproduces the inserted
    /// progression as a set — 2 of the `m·(m/2)` hash pairs, i.e.
    /// `2/(64·32) ≈ 0.10%` per query at the 8-byte floor (`m = 64`). The same
    /// argument on the 2-byte shape (`m = 16`) gives `1/64 ≈ 1.6%` — the leak
    /// the floor removes. MEASURED: 2/6000 at the floor, 89/6000 at `m = 16`.
    /// The 30-record filters carry a real fill and sit at the sizing's ~0.3%.
    #[test]
    fn one_record_pairs_do_not_leak() {
        let queries = 50usize;
        let (mut tiny_fp, mut tiny_trials) = (0usize, 0usize);
        let (mut wide_fp, mut wide_trials) = (0usize, 0usize);
        for t in 0..137u32 {
            let names: Vec<Vec<u8>> = if t < 120 {
                vec![format!("UnbotheredLycan{}", 1_000 + t).into_bytes()]
            } else {
                (0..30)
                    .map(|i| format!("UnbotheredLycan{}-{i}", 1_000 + t).into_bytes())
                    .collect()
            };
            let mut builder = FilterBuilder::new(names.len());
            for name in &names {
                builder.insert(name);
            }
            let filter = builder.finish();
            let (fp, trials) = if t < 120 {
                (&mut tiny_fp, &mut tiny_trials)
            } else {
                (&mut wide_fp, &mut wide_trials)
            };
            for q in 0..queries {
                let query = format!("UnbotheredWolves{t}-{q}").into_bytes();
                *trials += 1;
                if might_contain(&filter, &query) {
                    *fp += 1;
                }
            }
        }
        // 0.4% is ~4× the structural expectation and a quarter of the
        // pre-floor 1.6% class.
        assert!(
            tiny_fp * 1000 <= tiny_trials * 4,
            "{tiny_fp}/{tiny_trials} false positives on the one-record shape"
        );
        // 1.5% is ~4× the fill sizing's ~0.3%.
        assert!(
            wide_fp * 1000 <= wide_trials * 15,
            "{wide_fp}/{wide_trials} false positives on the 30-record shape"
        );
    }
}
