//! `Rng` — the seeded random numbers a replay-verified game draws from: the
//! same stream on every target, so a client, a verifying Worker and a native
//! test agree on every spawn.
//!
//! # Why not `rand::rngs::SmallRng`
//!
//! `SmallRng` chooses its algorithm by pointer width: xoshiro256++ on a 64-bit
//! target, xoshiro128++ on a 32-bit one. wasm32 is 32-bit. So a native test,
//! a native tool and a wasm client or Worker given the same seed draw
//! *different* numbers, and a replay that verifies on one fails on the other.
//! `rand` also reserves the right to change `SmallRng` between releases.
//!
//! This is xoshiro256** seeded through SplitMix64 (the seeding its authors
//! recommend), written out here so the algorithm is fixed by this file, not
//! by a dependency's version or the target. It uses only 64-bit integer
//! arithmetic, and floats are built from its bits exactly. The golden tests
//! pin the stream: if one fails, every recording made before the change no
//! longer replays.
//!
//! # Streams that do not disturb each other
//!
//! [`Rng::fork`] derives an independent stream from the *seed* and a salt,
//! not from how much has been drawn. Give spawns, waves and cosmetic effects
//! their own forks, and adding a particle never changes where the next wave
//! comes from.

use serde::{Deserialize, Serialize};

/// A seeded, portable random number generator. Clone it to checkpoint a
/// stream; serialise it to put one in a snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rng {
    seed: u64,
    s: [u64; 4],
}

/// One step of SplitMix64: the next state and its output.
fn splitmix(state: u64) -> (u64, u64) {
    let state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (state, z ^ (z >> 31))
}

impl Rng {
    /// A stream from `seed`. Every seed, zero included, gives a good stream.
    pub fn new(seed: u64) -> Self {
        let mut st = seed;
        let mut s = [0u64; 4];
        for word in &mut s {
            let (next, v) = splitmix(st);
            st = next;
            *word = v;
        }
        Self { seed, s }
    }

    /// The seed this stream started from.
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// An independent stream named by `salt`, derived from this stream's
    /// seed alone: the same whatever has been drawn from this one.
    pub fn fork(&self, salt: u64) -> Self {
        Self::new(self.seed ^ splitmix(salt).1)
    }

    pub fn next_u64(&mut self) -> u64 {
        let [s0, s1, s2, s3] = &mut self.s;
        let out = s1.wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = *s1 << 17;
        *s2 ^= *s0;
        *s3 ^= *s1;
        *s1 ^= *s2;
        *s0 ^= *s3;
        *s2 ^= t;
        *s3 = s3.rotate_left(45);
        out
    }

    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// Uniform in `[0, 1)`, from 24 random bits: every value exact.
    pub fn f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 * (1.0 / (1u32 << 24) as f32)
    }

    /// Uniform in `[0, 1)`, from 53 random bits.
    pub fn f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Uniform in `[0, n)`; 0 when `n` is 0.
    pub fn below(&mut self, n: u64) -> u64 {
        ((u128::from(self.next_u64()) * u128::from(n)) >> 64) as u64
    }

    /// Uniform in `[lo, hi)`; `lo` when the range is empty.
    pub fn range_i64(&mut self, lo: i64, hi: i64) -> i64 {
        if hi <= lo {
            return lo;
        }
        lo.wrapping_add(self.below(hi.abs_diff(lo)) as i64)
    }

    /// Uniform in `[lo, hi)`.
    pub fn range_f32(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }

    /// True with probability `p`.
    pub fn chance(&mut self, p: f32) -> bool {
        self.f32() < p
    }

    /// A uniformly chosen element; `None` for an empty slice.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            None
        } else {
            items.get(self.below(items.len() as u64) as usize)
        }
    }

    /// Shuffle in place (Fisher–Yates).
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i as u64 + 1) as usize;
            items.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitmix_matches_the_published_reference() {
        assert_eq!(splitmix(0).1, 0xe220_a839_7b1d_cdaf);
    }

    /// The stream itself. Computed by an independent implementation of the
    /// reference algorithms; if this fails, old recordings no longer replay.
    #[test]
    fn the_stream_is_pinned() {
        let mut r = Rng::new(42);
        assert_eq!(
            [r.next_u64(), r.next_u64(), r.next_u64()],
            [
                0x1578_0b2e_0c2e_c716,
                0x6104_d986_6d11_3a7e,
                0xae17_5332_39e4_99a1
            ]
        );
        let mut z = Rng::new(0);
        assert_eq!(
            [z.next_u64(), z.next_u64()],
            [0x99ec_5f36_cb75_f2b4, 0xbf6e_1f78_4956_452a]
        );
        // Read back from a wasm32 build (a cdylib run under Node): a fork and
        // a float come out bit-for-bit the same there as here.
        assert_eq!(Rng::new(42).fork(5).f32().to_bits(), 1_056_899_066);
    }

    #[test]
    fn floats_and_ranges_stay_in_bounds() {
        let mut r = Rng::new(7);
        for _ in 0..10_000 {
            let f = r.f32();
            assert!((0.0..1.0).contains(&f));
            let d = r.f64();
            assert!((0.0..1.0).contains(&d));
            assert!(r.below(6) < 6);
            assert!((-3..4).contains(&r.range_i64(-3, 4)));
            let x = r.range_f32(2.0, 5.0);
            assert!((2.0..5.0).contains(&x));
        }
        assert_eq!(r.below(0), 0);
        assert_eq!(r.range_i64(5, 5), 5);
        assert!(r.range_i64(i64::MIN, i64::MAX) < i64::MAX, "no overflow");
        assert_eq!(r.pick::<u8>(&[]), None);
    }

    #[test]
    fn a_fork_depends_on_the_seed_not_on_what_was_drawn() {
        let a = Rng::new(9);
        let mut b = Rng::new(9);
        for _ in 0..100 {
            b.next_u64();
        }
        assert_eq!(a.fork(1), b.fork(1));
        assert_ne!(a.fork(1), a.fork(2));
        assert_ne!(a.fork(1).next_u64(), Rng::new(9).next_u64());
    }

    #[test]
    fn shuffle_is_a_permutation_and_repeatable() {
        let mut a: Vec<u32> = (0..20).collect();
        let mut b = a.clone();
        Rng::new(3).shuffle(&mut a);
        Rng::new(3).shuffle(&mut b);
        assert_eq!(a, b);
        assert_ne!(a, (0..20).collect::<Vec<_>>());
        a.sort_unstable();
        assert_eq!(a, (0..20).collect::<Vec<_>>());
    }

    #[test]
    fn a_checkpoint_resumes_the_same_stream() {
        let mut r = Rng::new(11);
        r.next_u64();
        let saved = r.clone();
        let next = r.next_u64();
        let mut restored = saved;
        assert_eq!(restored.next_u64(), next);
    }
}
