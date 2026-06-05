//! Minimal dependency-free base64 (standard alphabet), matching the WASM-safe
//! approach used elsewhere in the workspace. Used to pass images to fal as data
//! URIs and to decode `sync_mode` results.

const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    let mut i = 0;
    while i + 2 < input.len() {
        let triple = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8) | input[i + 2] as u32;
        out.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        out.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        out.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        out.push(CHARS[(triple & 0x3F) as usize] as char);
        i += 3;
    }
    match input.len() - i {
        1 => {
            let b0 = input[i] as u32;
            out.push(CHARS[((b0 >> 2) & 0x3F) as usize] as char);
            out.push(CHARS[((b0 << 4) & 0x3F) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let pair = ((input[i] as u32) << 8) | input[i + 1] as u32;
            out.push(CHARS[((pair >> 10) & 0x3F) as usize] as char);
            out.push(CHARS[((pair >> 4) & 0x3F) as usize] as char);
            out.push(CHARS[((pair << 2) & 0x3F) as usize] as char);
            out.push('=');
        }
        _ => {}
    }
    out
}

/// Decode standard base64; returns `None` on any invalid character. Whitespace
/// and `=` padding are ignored.
pub fn decode(input: &str) -> Option<Vec<u8>> {
    let mut table = [255u8; 256];
    for (i, &c) in CHARS.iter().enumerate() {
        table[c as usize] = i as u8;
    }

    let mut vals = Vec::with_capacity(input.len());
    for b in input.bytes() {
        match b {
            b'=' | b'\n' | b'\r' | b' ' => continue,
            _ => {
                let v = table[b as usize];
                if v == 255 {
                    return None;
                }
                vals.push(v);
            }
        }
    }

    let mut out = Vec::with_capacity(vals.len() * 3 / 4);
    let mut i = 0;
    while i + 4 <= vals.len() {
        let n = ((vals[i] as u32) << 18)
            | ((vals[i + 1] as u32) << 12)
            | ((vals[i + 2] as u32) << 6)
            | vals[i + 3] as u32;
        out.push((n >> 16) as u8);
        out.push((n >> 8) as u8);
        out.push(n as u8);
        i += 4;
    }
    match vals.len() - i {
        2 => {
            let n = ((vals[i] as u32) << 18) | ((vals[i + 1] as u32) << 12);
            out.push((n >> 16) as u8);
        }
        3 => {
            let n = ((vals[i] as u32) << 18)
                | ((vals[i + 1] as u32) << 12)
                | ((vals[i + 2] as u32) << 6);
            out.push((n >> 16) as u8);
            out.push((n >> 8) as u8);
        }
        _ => {}
    }
    Some(out)
}
