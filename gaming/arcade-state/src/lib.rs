//! `arcade-state` — small, enum-keyed state for deterministic lockstep sims
//! (`arcade-room`): what is on something for a while ([`Effects`]), and what
//! it is carrying ([`Counts`]).
//!
//! Both are the same shape, chosen for a world every client and the room
//! step identically, clone every frame to predict, and hash to compare:
//!
//! - **Canonical.** A fixed array indexed by kind, so the state is *what*
//!   there is, never the order it came about. Equal states encode, and so
//!   hash, equally however they were reached.
//! - **No allocation.** A few bytes to copy, however often the world is
//!   cloned.
//! - **Their own wire format, no serde.** Only what is non-zero is written: a
//!   count byte, then each one's kind index (a byte) and value (a
//!   little-endian `u16`), in [`Kind::ALL`] order. So an empty one is one
//!   byte. Decoding refuses anything the encoder would not have written, so
//!   a decoded state re-encodes to the same bytes.
//!
//! A game supplies its kinds by implementing [`Kind`] on an enum. The array
//! size `N` has to be spelt out in the type (stable Rust cannot size an
//! array from a trait's constant); `new` checks it against [`Kind::ALL`].

mod counts;
mod effects;

pub use counts::Counts;
pub use effects::Effects;

/// A game's kinds of something (effects, materials): every one, in a fixed
/// order. A kind's index in [`ALL`](Kind::ALL) is its code on the wire, so
/// append new kinds at the end (and bump the game's sim version) rather than
/// reordering.
pub trait Kind: Copy + Eq + 'static {
    const ALL: &'static [Self];

    /// Its index in [`ALL`](Kind::ALL).
    fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|k| *k == self)
            .expect("every kind is in Kind::ALL")
    }

    /// The most of it there can be: a carrying limit for [`Counts`], the
    /// longest for [`Effects`]. No limit by default.
    fn cap(self) -> u16 {
        u16::MAX
    }
}

/// Why encoded state did not decode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    Truncated,
    /// A kind out of range, a kind twice or out of order, a zero, or a value
    /// over its kind's cap: nothing the encoder writes.
    Malformed,
}

/// The non-zero values, sparsely: see the crate docs.
fn encode<const N: usize>(values: &[u16; N], out: &mut Vec<u8>) {
    out.push(values.iter().filter(|v| **v > 0).count() as u8);
    for (i, v) in values.iter().enumerate().filter(|(_, v)| **v > 0) {
        out.push(i as u8);
        out.extend_from_slice(&v.to_le_bytes());
    }
}

/// What [`encode`] wrote, from `bytes` at `*at`, moving `*at` past it.
fn decode<K: Kind, const N: usize>(bytes: &[u8], at: &mut usize) -> Result<[u16; N], DecodeError> {
    let mut take = |n: usize| -> Result<&[u8], DecodeError> {
        let end = at.checked_add(n).ok_or(DecodeError::Truncated)?;
        let out = bytes.get(*at..end).ok_or(DecodeError::Truncated)?;
        *at = end;
        Ok(out)
    };
    let count = usize::from(take(1)?[0]);
    if count > N {
        return Err(DecodeError::Malformed);
    }
    let mut values = [0u16; N];
    let mut last: Option<usize> = None;
    for _ in 0..count {
        let b = take(3)?;
        let (i, v) = (usize::from(b[0]), u16::from_le_bytes([b[1], b[2]]));
        let ok = i < N && v > 0 && v <= K::ALL[i].cap() && last.is_none_or(|l| i > l);
        if !ok {
            return Err(DecodeError::Malformed);
        }
        values[i] = v;
        last = Some(i);
    }
    Ok(values)
}

/// `N` must be how many kinds there are.
fn check_size<K: Kind, const N: usize>() {
    assert_eq!(K::ALL.len(), N, "wants N = K::ALL.len()");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Thing {
        A,
        B,
        C,
    }

    impl Kind for Thing {
        const ALL: &'static [Self] = &[Thing::A, Thing::B, Thing::C];
        fn cap(self) -> u16 {
            if self == Thing::B { 10 } else { u16::MAX }
        }
    }

    fn read(b: &[u8]) -> Result<[u16; 3], DecodeError> {
        decode::<Thing, 3>(b, &mut 0)
    }

    #[test]
    fn the_encoding_round_trips_and_empty_is_a_byte() {
        let mut out = Vec::new();
        encode(&[0, 0, 0], &mut out);
        assert_eq!(out, [0]);
        let mut out = vec![0xAA];
        encode(&[300, 0, 2], &mut out);
        let mut at = 1;
        assert_eq!(decode::<Thing, 3>(&out, &mut at), Ok([300, 0, 2]));
        assert_eq!(at, out.len());
    }

    #[test]
    fn anything_the_encoder_would_not_write_is_refused() {
        assert_eq!(read(&[]), Err(DecodeError::Truncated));
        assert_eq!(read(&[1, 0, 5]), Err(DecodeError::Truncated));
        assert_eq!(
            read(&[1, 3, 1, 0]),
            Err(DecodeError::Malformed),
            "no such kind"
        );
        assert_eq!(read(&[1, 0, 0, 0]), Err(DecodeError::Malformed), "a zero");
        assert_eq!(
            read(&[2, 1, 1, 0, 0, 1, 0]),
            Err(DecodeError::Malformed),
            "order"
        );
        assert_eq!(
            read(&[1, 1, 11, 0]),
            Err(DecodeError::Malformed),
            "over the cap"
        );
        assert_eq!(
            read(&[4]),
            Err(DecodeError::Malformed),
            "more than there are"
        );
    }
}
