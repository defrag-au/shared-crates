//! `Counts` — how many of each kind something holds, keyed by an enum:
//! materials in a pouch, keys, gems. Each kind has a cap ([`Kind::cap`]),
//! so a count never overflows its carrying limit.
//!
//! Costs are whole arrays in [`Kind::ALL`] order, so a recipe's price is
//! checked and paid in one call ([`Counts::spend`]), never half-taken.
//!
//! ```
//! use arcade_state::{Counts, Kind};
//!
//! #[derive(Clone, Copy, Debug, PartialEq, Eq)]
//! enum Ore { Iron, Gold }
//!
//! impl Kind for Ore {
//!     const ALL: &'static [Self] = &[Ore::Iron, Ore::Gold];
//!     fn cap(self) -> u16 { 99 }
//! }
//!
//! let mut bag: Counts<Ore, 2> = Counts::new();
//! bag.add(Ore::Iron, 5);
//! assert!(!bag.spend(&[3, 1]), "no gold: nothing taken");
//! assert_eq!(bag.count(Ore::Iron), 5);
//! bag.add(Ore::Gold, 1);
//! assert!(bag.spend(&[3, 1]));
//! assert_eq!((bag.count(Ore::Iron), bag.count(Ore::Gold)), (2, 0));
//! ```

use std::marker::PhantomData;

use crate::{DecodeError, Kind};

/// How many of each kind; never more than its cap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Counts<K: Kind, const N: usize> {
    held: [u16; N],
    kind: PhantomData<K>,
}

impl<K: Kind, const N: usize> Default for Counts<K, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Kind, const N: usize> Counts<K, N> {
    /// Nothing held.
    pub fn new() -> Self {
        crate::check_size::<K, N>();
        Self {
            held: [0; N],
            kind: PhantomData,
        }
    }

    pub fn count(&self, kind: K) -> u16 {
        self.held[kind.index()]
    }

    /// Add `n` of `kind`, up to its cap; returns how many went in.
    pub fn add(&mut self, kind: K, n: u16) -> u16 {
        let held = &mut self.held[kind.index()];
        let room = kind.cap().saturating_sub(*held);
        let added = n.min(room);
        *held += added;
        added
    }

    /// Take `n` of `kind` if there are that many; nothing taken otherwise.
    pub fn take(&mut self, kind: K, n: u16) -> bool {
        let held = &mut self.held[kind.index()];
        if *held < n {
            return false;
        }
        *held -= n;
        true
    }

    /// Whether `cost` (by [`Kind::ALL`] order) can be paid.
    pub fn afford(&self, cost: &[u16; N]) -> bool {
        self.held.iter().zip(cost).all(|(have, need)| have >= need)
    }

    /// Pay `cost` if it can be paid, all of it; nothing taken otherwise.
    pub fn spend(&mut self, cost: &[u16; N]) -> bool {
        if !self.afford(cost) {
            return false;
        }
        for (have, need) in self.held.iter_mut().zip(cost) {
            *have -= need;
        }
        true
    }

    /// Whether nothing is held.
    pub fn is_empty(&self) -> bool {
        self.held.iter().all(|n| *n == 0)
    }

    /// Every kind held, with its count, in [`Kind::ALL`] order.
    pub fn held(&self) -> impl Iterator<Item = (K, u16)> + '_ {
        K::ALL
            .iter()
            .zip(self.held)
            .filter(|(_, n)| *n > 0)
            .map(|(k, n)| (*k, n))
    }

    /// Append what is held, in the crate's sparse encoding.
    pub fn encode(&self, out: &mut Vec<u8>) {
        crate::encode(&self.held, out);
    }

    /// Read what [`encode`](Self::encode) wrote, from `bytes` at `*at`,
    /// moving `*at` past it. A count over its kind's cap is refused.
    pub fn decode(bytes: &[u8], at: &mut usize) -> Result<Self, DecodeError> {
        crate::check_size::<K, N>();
        Ok(Self {
            held: crate::decode::<K, N>(bytes, at)?,
            kind: PhantomData,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Mat {
        Bolt,
        Cell,
    }

    impl Kind for Mat {
        const ALL: &'static [Self] = &[Mat::Bolt, Mat::Cell];
        fn cap(self) -> u16 {
            match self {
                Mat::Bolt => 99,
                Mat::Cell => 5,
            }
        }
    }

    type Pouch = Counts<Mat, 2>;

    #[test]
    fn adding_stops_at_the_cap() {
        let mut p = Pouch::new();
        assert_eq!(p.add(Mat::Cell, 3), 3);
        assert_eq!(p.add(Mat::Cell, 3), 2, "only room for two");
        assert_eq!(p.count(Mat::Cell), 5);
        assert!(p.take(Mat::Cell, 5) && p.is_empty());
        assert!(!p.take(Mat::Bolt, 1), "none to take");
    }

    #[test]
    fn a_cost_is_paid_whole_or_not_at_all() {
        let mut p = Pouch::new();
        p.add(Mat::Bolt, 4);
        assert!(!p.afford(&[2, 1]));
        assert!(!p.spend(&[2, 1]));
        assert_eq!(p.count(Mat::Bolt), 4, "nothing half-taken");
        p.add(Mat::Cell, 1);
        assert!(p.spend(&[2, 1]));
        assert_eq!(p.held().collect::<Vec<_>>(), [(Mat::Bolt, 2)]);
    }

    #[test]
    fn it_round_trips_and_refuses_a_count_over_the_cap() {
        let mut p = Pouch::new();
        p.add(Mat::Bolt, 40);
        p.add(Mat::Cell, 2);
        let mut bytes = Vec::new();
        p.encode(&mut bytes);
        assert_eq!(Pouch::decode(&bytes, &mut 0), Ok(p));
        // Six cells, over the cap of five.
        assert_eq!(
            Pouch::decode(&[1, 1, 6, 0], &mut 0),
            Err(DecodeError::Malformed)
        );
    }
}
