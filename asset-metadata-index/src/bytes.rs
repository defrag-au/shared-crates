//! Bounds-checked reads and checked writes. Every accessor either returns a
//! value or a refusal — no indexing that can panic.

use crate::error::{DecodeError, EncodeError};

/// Read `n` bytes at `*at`, advancing `at`. The only reader in the crate.
pub(crate) fn take<'a>(b: &'a [u8], at: &mut usize, n: usize) -> Result<&'a [u8], DecodeError> {
    let end = at.checked_add(n).ok_or(DecodeError::BadLength)?;
    if end > b.len() {
        return Err(DecodeError::Truncated { at: *at, want: n });
    }
    let out = &b[*at..end];
    *at = end;
    Ok(out)
}

pub(crate) fn u8_at(b: &[u8], at: &mut usize) -> Result<u8, DecodeError> {
    Ok(take(b, at, 1)?[0])
}

pub(crate) fn u16_at(b: &[u8], at: &mut usize) -> Result<u16, DecodeError> {
    let arr: [u8; 2] = take(b, at, 2)?.try_into().expect("take returned 2 bytes");
    Ok(u16::from_le_bytes(arr))
}

pub(crate) fn u32_at(b: &[u8], at: &mut usize) -> Result<u32, DecodeError> {
    let arr: [u8; 4] = take(b, at, 4)?.try_into().expect("take returned 4 bytes");
    Ok(u32::from_le_bytes(arr))
}

pub(crate) fn array_at<const N: usize>(b: &[u8], at: &mut usize) -> Result<[u8; N], DecodeError> {
    Ok(take(b, at, N)?.try_into().expect("take returned N bytes"))
}

/// `len: u16` followed by the bytes, read as one unit.
pub(crate) fn take_u16_prefixed<'a>(b: &'a [u8], at: &mut usize) -> Result<&'a [u8], DecodeError> {
    let len = u16_at(b, at)? as usize;
    take(b, at, len)
}

/// `len: u32` followed by the bytes, read as one unit.
pub(crate) fn take_u32_prefixed<'a>(b: &'a [u8], at: &mut usize) -> Result<&'a [u8], DecodeError> {
    let len = u32_at(b, at)? as usize;
    take(b, at, len)
}

/// `len: u16` followed by the bytes. Refuses lengths the width cannot state.
pub(crate) fn put_u16_prefixed(
    out: &mut Vec<u8>,
    field: &'static str,
    bytes: &[u8],
) -> Result<(), EncodeError> {
    let len = u16::try_from(bytes.len()).map_err(|_| EncodeError::FieldTooLong {
        field,
        len: bytes.len(),
    })?;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

/// `len: u32` followed by the bytes.
pub(crate) fn put_u32_prefixed(
    out: &mut Vec<u8>,
    field: &'static str,
    bytes: &[u8],
) -> Result<(), EncodeError> {
    let len = u32::try_from(bytes.len()).map_err(|_| EncodeError::FieldTooLong {
        field,
        len: bytes.len(),
    })?;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}
