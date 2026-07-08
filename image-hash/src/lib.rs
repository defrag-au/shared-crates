//! Perceptual hashing for visual asset search.
//!
//! Difference hash (dHash) at two scales — 8x8 (64-bit) and 16x16
//! (256-bit) — over Lanczos-downscaled grayscale. Parameters are FROZEN:
//! they were validated empirically against live collection art
//! (2026-07-07, Toolheads OG identification): true matches from the same
//! source art score a combined distance ≤ 8 across a 500px-PNG vs
//! 64px-JPEG source gap, while the nearest false match scored ≥ 33.
//! 256px source thumbnails tightened true matches further (d=3 vs d=6).
//! The fixture tests pin exactly that behaviour — do not change the
//! resize filter, grayscale conversion, or bit order without
//! re-validating against real collections.
//!
//! Banding: the 256-bit hash splits into eight 32-bit bands for indexed
//! candidate lookup (SQL `WHERE b0=? OR … OR b7=?`). By pigeonhole, any
//! match within Hamming distance ≤ 7 on the 256-bit hash shares at least
//! one exact band, so banded lookup has guaranteed recall in the range
//! that matters and near-zero false candidates (~500k corpus vs 2^32
//! band space).

use image::imageops::FilterType;

/// Combined-distance threshold under which two hashes are the same
/// source art (empirical: true ≤ 8, nearest false ≥ 33; midpoint with
/// margin toward precision).
pub const MATCH_THRESHOLD: u32 = 12;

/// Above MATCH but below this, surface as "possible" — between the
/// validated clusters.
pub const POSSIBLE_THRESHOLD: u32 = 24;

/// The two dHash scales of one image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisualHash {
    /// 8x8 dHash (9x8 luma grid, row-major, MSB-first).
    pub dhash8: u64,
    /// 16x16 dHash (17x16 luma grid) as four u64 words, MSB-first.
    pub dhash16: [u64; 4],
}

impl VisualHash {
    /// Hash raw encoded image bytes (JPEG/PNG/WebP).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, HashError> {
        let img = image::load_from_memory(bytes).map_err(|e| HashError::Decode(e.to_string()))?;
        Ok(Self::from_image(&img))
    }

    /// Hash a decoded image.
    pub fn from_image(img: &image::DynamicImage) -> Self {
        Self {
            dhash8: dhash_bits(img, 8)[0],
            dhash16: {
                let words = dhash_bits(img, 16);
                [words[0], words[1], words[2], words[3]]
            },
        }
    }

    /// Combined Hamming distance across both scales — the number the
    /// thresholds are calibrated against.
    pub fn distance(&self, other: &Self) -> u32 {
        let mut d = (self.dhash8 ^ other.dhash8).count_ones();
        for (a, b) in self.dhash16.iter().zip(other.dhash16.iter()) {
            d += (a ^ b).count_ones();
        }
        d
    }

    /// The eight 32-bit bands of the 256-bit hash, for indexed lookup.
    pub fn bands(&self) -> [u32; 8] {
        let mut out = [0u32; 8];
        for (i, word) in self.dhash16.iter().enumerate() {
            out[i * 2] = (word >> 32) as u32;
            out[i * 2 + 1] = *word as u32;
        }
        out
    }

    /// Reassemble from stored columns (dhash8 + 32-byte dhash16 blob).
    pub fn from_stored(dhash8: u64, dhash16_bytes: &[u8; 32]) -> Self {
        let mut words = [0u64; 4];
        for (i, chunk) in dhash16_bytes.chunks_exact(8).enumerate() {
            words[i] = u64::from_be_bytes(chunk.try_into().expect("8-byte chunk"));
        }
        Self {
            dhash8,
            dhash16: words,
        }
    }

    /// Full hash as 80 hex chars: dhash8 (16) + dhash16 (64), big-endian.
    /// The single-column storage encoding — text sidesteps both D1
    /// binding traps (i64→BigInt rejection, byte-array rejection) and
    /// SQLite affinity coercion.
    pub fn to_hex(&self) -> String {
        let mut out = String::with_capacity(80);
        out.push_str(&format!("{:016x}", self.dhash8));
        for word in &self.dhash16 {
            out.push_str(&format!("{word:016x}"));
        }
        out
    }

    /// Parse the 80-hex-char storage encoding.
    pub fn from_hex(s: &str) -> Option<Self> {
        if s.len() != 80 {
            return None;
        }
        let dhash8 = u64::from_str_radix(&s[0..16], 16).ok()?;
        let mut dhash16 = [0u64; 4];
        for (i, word) in dhash16.iter_mut().enumerate() {
            *word = u64::from_str_radix(&s[16 + i * 16..32 + i * 16], 16).ok()?;
        }
        Some(Self { dhash8, dhash16 })
    }

    /// The 256-bit hash as bytes for BLOB storage (big-endian words).
    pub fn dhash16_bytes(&self) -> [u8; 32] {
        let mut out = [0u8; 32];
        for (i, word) in self.dhash16.iter().enumerate() {
            out[i * 8..(i + 1) * 8].copy_from_slice(&word.to_be_bytes());
        }
        out
    }
}

#[derive(Debug)]
pub enum HashError {
    Decode(String),
}

impl std::fmt::Display for HashError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Decode(e) => write!(f, "image decode failed: {e}"),
        }
    }
}

impl std::error::Error for HashError {}

/// Compute a `size`x`size`-bit dHash: downscale to (size+1)x`size` luma
/// with Lanczos3, then each bit = left pixel brighter than its right
/// neighbour, row-major, MSB-first, packed into u64 words.
fn dhash_bits(img: &image::DynamicImage, size: u32) -> [u64; 4] {
    let g = img
        .resize_exact(size + 1, size, FilterType::Lanczos3)
        .to_luma8();
    let mut words = [0u64; 4];
    let mut bit_index = 0usize;
    for row in 0..size {
        for col in 0..size {
            let left = g.get_pixel(col, row)[0];
            let right = g.get_pixel(col + 1, row)[0];
            if left > right {
                words[bit_index / 64] |= 1 << (63 - (bit_index % 64));
            }
            bit_index += 1;
        }
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> VisualHash {
        let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        VisualHash::from_bytes(&std::fs::read(path).unwrap()).unwrap()
    }

    /// The empirical regression pair from the Toolheads OG work: the
    /// toolheads.io "Blade" role image and Toolhead5045's IIIF thumb are
    /// the same source art; Toolhead0001 is a different asset.
    #[test]
    fn blade_pair_matches_and_stranger_does_not() {
        let role = fixture("blade_role.jpg");
        let same = fixture("toolhead5045_thumb.jpg");
        let other = fixture("toolhead0001_thumb.jpg");

        let d_same = role.distance(&same);
        let d_other = role.distance(&other);
        assert!(
            d_same <= MATCH_THRESHOLD,
            "same-art distance {d_same} exceeded threshold"
        );
        assert!(
            d_other > POSSIBLE_THRESHOLD,
            "different-art distance {d_other} implausibly low"
        );
        assert!(d_other > d_same + 15, "separation collapsed");
    }

    #[test]
    fn identical_bytes_hash_identically() {
        let a = fixture("toolhead5045_thumb.jpg");
        let b = fixture("toolhead5045_thumb.jpg");
        assert_eq!(a, b);
        assert_eq!(a.distance(&b), 0);
    }

    #[test]
    fn hex_round_trips() {
        let h = fixture("blade_role.jpg");
        let hex = h.to_hex();
        assert_eq!(hex.len(), 80);
        assert_eq!(VisualHash::from_hex(&hex).unwrap(), h);
        assert!(VisualHash::from_hex("abc").is_none());
    }

    #[test]
    fn banding_round_trips_and_shares_a_band_on_match() {
        let role = fixture("blade_role.jpg");
        let same = fixture("toolhead5045_thumb.jpg");

        // Storage round-trip.
        let restored = VisualHash::from_stored(role.dhash8, &role.dhash16_bytes());
        assert_eq!(restored, role);

        // Pigeonhole in practice: a true match shares >= 1 exact band.
        let shared = role
            .bands()
            .iter()
            .zip(same.bands().iter())
            .filter(|(a, b)| a == b)
            .count();
        assert!(shared >= 1, "true match shared no bands");
    }
}
