//! CBOR types shared by the mini-protocols: [`Point`] and [`Tip`].

use minicbor::{Decode, Decoder, Encode, Encoder};

/// A point on the chain: the origin, or a block identified by slot and hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Point {
    Origin,
    Specific { slot: u64, hash: [u8; 32] },
}

impl Point {
    pub fn slot(&self) -> Option<u64> {
        match self {
            Self::Origin => None,
            Self::Specific { slot, .. } => Some(*slot),
        }
    }
}

// Origin encodes as `[]`, a specific point as `[slot, hash]`.
impl<C> Encode<C> for Point {
    fn encode<W: minicbor::encode::Write>(
        &self,
        e: &mut Encoder<W>,
        _ctx: &mut C,
    ) -> Result<(), minicbor::encode::Error<W::Error>> {
        match self {
            Self::Origin => {
                e.array(0)?;
            }
            Self::Specific { slot, hash } => {
                e.array(2)?.u64(*slot)?.bytes(hash)?;
            }
        }
        Ok(())
    }
}

impl<'b, C> Decode<'b, C> for Point {
    fn decode(d: &mut Decoder<'b>, _ctx: &mut C) -> Result<Self, minicbor::decode::Error> {
        match d.array()? {
            Some(0) => Ok(Self::Origin),
            Some(2) => {
                let slot = d.u64()?;
                let hash = d
                    .bytes()?
                    .try_into()
                    .map_err(|_| minicbor::decode::Error::message("point hash is not 32 bytes"))?;
                Ok(Self::Specific { slot, hash })
            }
            other => Err(minicbor::decode::Error::message(format!(
                "unexpected point array length: {other:?}"
            ))),
        }
    }
}

/// The peer's chain tip: a point plus that block's height.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tip {
    pub point: Point,
    pub block_number: u64,
}

// `[point, block_number]`
impl<C> Encode<C> for Tip {
    fn encode<W: minicbor::encode::Write>(
        &self,
        e: &mut Encoder<W>,
        ctx: &mut C,
    ) -> Result<(), minicbor::encode::Error<W::Error>> {
        e.array(2)?;
        self.point.encode(e, ctx)?;
        e.u64(self.block_number)?;
        Ok(())
    }
}

impl<'b, C> Decode<'b, C> for Tip {
    fn decode(d: &mut Decoder<'b>, ctx: &mut C) -> Result<Self, minicbor::decode::Error> {
        if d.array()? != Some(2) {
            return Err(minicbor::decode::Error::message(
                "tip is not a 2-element array",
            ));
        }
        let point = Point::decode(d, ctx)?;
        let block_number = d.u64()?;
        Ok(Self {
            point,
            block_number,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_and_tips_round_trip() {
        let tip = Tip {
            point: Point::Specific {
                slot: 186_000_000,
                hash: [9; 32],
            },
            block_number: 13_358_656,
        };
        let bytes = minicbor::to_vec(&tip).unwrap();
        assert_eq!(minicbor::decode::<Tip>(&bytes).unwrap(), tip);

        let origin = minicbor::to_vec(Point::Origin).unwrap();
        assert_eq!(origin, vec![0x80]);
        assert_eq!(minicbor::decode::<Point>(&origin).unwrap(), Point::Origin);
    }

    #[test]
    fn a_short_hash_is_rejected() {
        let mut bytes = Vec::new();
        Encoder::new(&mut bytes)
            .array(2)
            .unwrap()
            .u64(1)
            .unwrap()
            .bytes(&[1, 2, 3])
            .unwrap();
        assert!(minicbor::decode::<Point>(&bytes).is_err());
    }
}
