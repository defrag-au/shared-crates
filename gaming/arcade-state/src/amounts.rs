//! `Amounts` — how much of each kind something holds, keyed by an enum and
//! counted wide and signed: a colony's stores in milliunits, a station's
//! tanks, a treasury. Every value is an `i64`, and each kind has a cap *per
//! instance* rather than per kind, because what a store can hold is a
//! property of that store ([`Amounts::new`], [`Amounts::set_cap`]).
//!
//! Like [`Counts`](crate::Counts), the values are one canonical array in
//! [`Kind::ALL`] order, so equal states encode and hash equally however they
//! were reached, and the encoding is sparse: a count byte, then each non-zero
//! kind's index and its little-endian `i64`.
//!
//! ```
//! use arcade_state::{Amounts, Kind};
//!
//! #[derive(Clone, Copy, Debug, PartialEq, Eq)]
//! enum Store { Ore, Water }
//!
//! impl Kind for Store {
//!     const ALL: &'static [Self] = &[Store::Ore, Store::Water];
//! }
//!
//! let mut depot: Amounts<Store, 2> = Amounts::new([5_000, 900]);
//! depot.add(Store::Ore, 2_000);
//! assert_eq!(depot.amount(Store::Ore), 2_000);
//! depot.add(Store::Water, 1_000);
//! assert_eq!(depot.amount(Store::Water), 900, "stopped at its cap");
//! assert!(depot.take(Store::Ore, 1_500));
//! assert!(!depot.take(Store::Ore, 1_500), "nothing half-taken");
//! assert_eq!(depot.amount(Store::Ore), 500);
//! ```

use std::marker::PhantomData;

use crate::{DecodeError, Kind};

/// How much of each kind; never below zero, never above its cap once
/// something has been added.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Amounts<K: Kind, const N: usize> {
    held: [i64; N],
    cap: [i64; N],
    kind: PhantomData<K>,
}

impl<K: Kind, const N: usize> Amounts<K, N> {
    /// Nothing held, with room for up to `caps` of each kind.
    pub fn new(caps: [i64; N]) -> Self {
        crate::check_size::<K, N>();
        for (kind, cap) in K::ALL.iter().zip(caps) {
            assert!(cap >= 0, "the cap on kind {} is negative", kind.index());
        }
        Self {
            held: [0; N],
            cap: caps,
            kind: PhantomData,
        }
    }

    /// What is held of `kind`.
    pub fn amount(&self, kind: K) -> i64 {
        self.held[kind.index()]
    }

    /// Room for `kind`.
    pub fn cap(&self, kind: K) -> i64 {
        self.cap[kind.index()]
    }

    /// Every kind's cap, in [`Kind::ALL`] order.
    pub fn caps(&self) -> [i64; N] {
        self.cap
    }

    /// Change how much of `kind` can be held. Nothing held moves, so lowering
    /// a cap under an amount leaves that amount standing until the next
    /// [`add`](Self::add) brings it back inside.
    pub fn set_cap(&mut self, kind: K, cap: i64) {
        assert!(cap >= 0, "the cap on kind {} is negative", kind.index());
        self.cap[kind.index()] = cap;
    }

    /// Add `n` of `kind`, staying between none and the cap; returns the
    /// change actually applied, which is `n` unless the cap or zero got in
    /// the way.
    pub fn add(&mut self, kind: K, n: i64) -> i64 {
        let i = kind.index();
        let before = self.held[i];
        self.held[i] = before.saturating_add(n).clamp(0, self.cap[i]);
        self.held[i] - before
    }

    /// Take `n` of `kind` if there are that many; nothing taken otherwise. A
    /// negative `n` is refused.
    pub fn take(&mut self, kind: K, n: i64) -> bool {
        if n < 0 || self.amount(kind) < n {
            return false;
        }
        self.held[kind.index()] -= n;
        true
    }

    /// Whether nothing is held.
    pub fn is_empty(&self) -> bool {
        self.held.iter().all(|n| *n == 0)
    }

    /// Every kind held, with its amount, in [`Kind::ALL`] order.
    pub fn held(&self) -> impl Iterator<Item = (K, i64)> + '_ {
        K::ALL
            .iter()
            .zip(self.held)
            .filter(|(_, n)| *n > 0)
            .map(|(k, n)| (*k, n))
    }

    /// Append what is held, in the crate's sparse encoding.
    pub fn encode(&self, out: &mut Vec<u8>) {
        crate::encode_i64(&self.held, out);
    }

    /// Read what [`encode`](Self::encode) wrote, from `bytes` at `*at`,
    /// moving `*at` past it, with room for `caps` of each kind.
    ///
    /// A negative amount is refused, since [`add`](Self::add) cannot produce
    /// one. An amount over its cap is allowed: [`set_cap`](Self::set_cap) can
    /// lower a cap under what is already held.
    pub fn decode(bytes: &[u8], at: &mut usize, caps: [i64; N]) -> Result<Self, DecodeError> {
        let mut amounts = Self::new(caps);
        amounts.held = crate::decode_i64::<N>(bytes, at)?;
        Ok(amounts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Store {
        Ore,
        Water,
        Parts,
    }

    impl Kind for Store {
        const ALL: &'static [Self] = &[Store::Ore, Store::Water, Store::Parts];
    }

    type Depot = Amounts<Store, 3>;

    fn depot() -> Depot {
        Depot::new([5_000, 900, 100])
    }

    #[test]
    fn adding_stays_between_none_and_the_instances_cap() {
        let mut d = depot();
        assert_eq!(d.add(Store::Ore, 2_000), 2_000);
        assert_eq!(d.add(Store::Water, 1_000), 900, "only room for 900");
        assert_eq!(d.amount(Store::Water), 900);
        assert_eq!(d.add(Store::Ore, -2_500), -2_000, "only what was there");
        assert_eq!(d.amount(Store::Ore), 0);
        assert_eq!(d.add(Store::Ore, -1), 0, "already empty");
        assert!(d.held().collect::<Vec<_>>() == [(Store::Water, 900)]);
    }

    #[test]
    fn a_take_is_all_or_nothing() {
        let mut d = depot();
        d.add(Store::Parts, 40);
        assert!(!d.take(Store::Parts, 41));
        assert_eq!(d.amount(Store::Parts), 40, "nothing half-taken");
        assert!(!d.take(Store::Parts, -1), "a negative take is refused");
        assert!(d.take(Store::Parts, 40) && d.is_empty());
    }

    #[test]
    fn a_lowered_cap_leaves_the_amount_until_the_next_add() {
        let mut d = depot();
        d.add(Store::Ore, 4_000);
        d.set_cap(Store::Ore, 1_000);
        assert_eq!(
            d.amount(Store::Ore),
            4_000,
            "lowering the cap moves nothing"
        );
        d.add(Store::Ore, -5);
        assert_eq!(d.amount(Store::Ore), 1_000, "the add brings it inside");
        assert_eq!(d.caps(), [1_000, 900, 100]);
    }

    #[test]
    fn the_encoding_round_trips_and_empty_is_a_byte() {
        let mut out = Vec::new();
        depot().encode(&mut out);
        assert_eq!(out, [0]);

        let mut d = depot();
        d.add(Store::Ore, 4_000_000_000);
        d.add(Store::Parts, 7);
        let mut out = vec![0xAA];
        d.encode(&mut out);
        let mut at = 1;
        assert_eq!(Depot::decode(&out, &mut at, d.caps()), Ok(d));
        assert_eq!(at, out.len());
    }

    #[test]
    fn anything_the_encoder_would_not_write_is_refused() {
        let caps = [5_000, 900, 100];
        let read = |b: &[u8]| Depot::decode(b, &mut 0, caps);

        assert_eq!(read(&[]), Err(DecodeError::Truncated));
        assert_eq!(read(&[1, 0, 5, 0]), Err(DecodeError::Truncated));
        assert_eq!(
            read(&[1, 3, 1, 0, 0, 0, 0, 0, 0, 0]),
            Err(DecodeError::Malformed),
            "no such kind"
        );
        assert_eq!(
            read(&[1, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            Err(DecodeError::Malformed),
            "a zero"
        );
        assert_eq!(
            read(&[2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]),
            Err(DecodeError::Malformed),
            "out of order"
        );
        assert_eq!(
            read(&[1, 0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]),
            Err(DecodeError::Malformed),
            "a negative amount"
        );
        assert_eq!(
            read(&[4]),
            Err(DecodeError::Malformed),
            "more than there are"
        );
        // Over its cap, but reachable: a cap can be lowered under an amount.
        assert!(read(&[1, 0, 9, 0, 0, 0, 0, 0, 0, 0]).is_ok());
    }

    #[test]
    #[should_panic(expected = "N = K::ALL.len()")]
    fn a_wrong_size_is_caught() {
        let _ = Amounts::<Store, 2>::new([1, 1]);
    }

    #[test]
    #[should_panic(expected = "negative")]
    fn a_negative_cap_is_caught() {
        let _ = Amounts::<Store, 3>::new([1, -1, 1]);
    }
}
