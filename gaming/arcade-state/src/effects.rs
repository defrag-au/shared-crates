//! `Effects` — what is on something for a while, keyed by an enum: a
//! power-up's seconds left, a stun, a slow, a splatter. Each kind is on for
//! some ticks, or off; at most one of each, by construction.
//!
//! ```
//! use arcade_state::{Effects, Kind};
//!
//! #[derive(Clone, Copy, Debug, PartialEq, Eq)]
//! enum Buff { Fast, Stunned }
//!
//! impl Kind for Buff {
//!     const ALL: &'static [Self] = &[Buff::Fast, Buff::Stunned];
//! }
//!
//! let mut fx: Effects<Buff, 2> = Effects::new();
//! fx.apply(Buff::Stunned, 30);
//! assert!(fx.has(Buff::Stunned) && !fx.has(Buff::Fast));
//! fx.tick();
//! assert_eq!(fx.left(Buff::Stunned), 29);
//! ```

use std::marker::PhantomData;

use crate::{DecodeError, Kind};

/// Ticks left of each kind of effect; 0 is off.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Effects<K: Kind, const N: usize> {
    left: [u16; N],
    kind: PhantomData<K>,
}

impl<K: Kind, const N: usize> Default for Effects<K, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Kind, const N: usize> Effects<K, N> {
    /// None on.
    pub fn new() -> Self {
        crate::check_size::<K, N>();
        Self {
            left: [0; N],
            kind: PhantomData,
        }
    }

    /// Whether `kind` is on.
    pub fn has(&self, kind: K) -> bool {
        self.left(kind) > 0
    }

    /// Ticks left of `kind`; 0 when off.
    pub fn left(&self, kind: K) -> u16 {
        self.left[kind.index()]
    }

    /// Turn `kind` on for `ticks` (at most its cap), or, if it is on
    /// already, keep whichever lasts longer. Applying 0 does nothing.
    pub fn apply(&mut self, kind: K, ticks: u16) {
        let left = &mut self.left[kind.index()];
        *left = (*left).max(ticks.min(kind.cap()));
    }

    /// Turn `kind` off now.
    pub fn clear(&mut self, kind: K) {
        self.left[kind.index()] = 0;
    }

    /// One tick off everything on.
    pub fn tick(&mut self) {
        for left in &mut self.left {
            *left = left.saturating_sub(1);
        }
    }

    /// Whether nothing is on.
    pub fn is_empty(&self) -> bool {
        self.left.iter().all(|l| *l == 0)
    }

    /// Everything on, with its ticks left, in [`Kind::ALL`] order.
    pub fn active(&self) -> impl Iterator<Item = (K, u16)> + '_ {
        K::ALL
            .iter()
            .zip(self.left)
            .filter(|(_, left)| *left > 0)
            .map(|(k, left)| (*k, left))
    }

    /// Append what is on, in the crate's sparse encoding.
    pub fn encode(&self, out: &mut Vec<u8>) {
        crate::encode(&self.left, out);
    }

    /// Read what [`encode`](Self::encode) wrote, from `bytes` at `*at`,
    /// moving `*at` past it.
    pub fn decode(bytes: &[u8], at: &mut usize) -> Result<Self, DecodeError> {
        crate::check_size::<K, N>();
        Ok(Self {
            left: crate::decode::<K, N>(bytes, at)?,
            kind: PhantomData,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Fx {
        Wings,
        Dazed,
        Slow,
    }

    impl Kind for Fx {
        const ALL: &'static [Self] = &[Fx::Wings, Fx::Dazed, Fx::Slow];
    }

    type Three = Effects<Fx, 3>;

    #[test]
    fn apply_keeps_the_longer_and_tick_runs_them_down() {
        let mut fx = Three::new();
        fx.apply(Fx::Dazed, 10);
        fx.apply(Fx::Dazed, 4);
        assert_eq!(fx.left(Fx::Dazed), 10, "the longer stays");
        fx.apply(Fx::Dazed, 20);
        assert_eq!(fx.left(Fx::Dazed), 20);
        fx.apply(Fx::Wings, 1);
        fx.tick();
        assert!(!fx.has(Fx::Wings) && fx.has(Fx::Dazed));
        assert_eq!(fx.left(Fx::Dazed), 19);
        fx.clear(Fx::Dazed);
        assert!(fx.is_empty());
    }

    #[test]
    fn the_same_state_is_the_same_bytes_whatever_the_order() {
        let mut a = Three::new();
        a.apply(Fx::Slow, 5);
        a.apply(Fx::Wings, 9);
        let mut b = Three::new();
        b.apply(Fx::Wings, 9);
        b.apply(Fx::Slow, 5);
        assert_eq!(a, b);
        let (mut ea, mut eb) = (Vec::new(), Vec::new());
        a.encode(&mut ea);
        b.encode(&mut eb);
        assert_eq!(ea, eb);
        assert_eq!(
            a.active().collect::<Vec<_>>(),
            [(Fx::Wings, 9), (Fx::Slow, 5)],
            "kind order"
        );
        let mut at = 0;
        assert_eq!(Three::decode(&ea, &mut at), Ok(a));
    }

    #[test]
    #[should_panic(expected = "N = K::ALL.len()")]
    fn a_wrong_size_is_caught() {
        let _ = Effects::<Fx, 2>::new();
    }
}
