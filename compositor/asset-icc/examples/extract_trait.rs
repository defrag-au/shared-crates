//! Anchor a generated trait into a declared slot and extract it as an item.
//!
//! ```sh
//! cargo run -p asset-icc --example extract_trait -- trait.png 320 120 384 300 item.png
//! ```
//!
//! Args: `<in.png> <slot_x> <slot_y> <slot_w> <slot_h> <out.png>`.
//!
//! The subject's alpha bounding box is scaled (`contain`, so its proportions are
//! preserved) and centred within the slot, then written out. The step is a pure
//! function of `(source, slot)`, so re-running it on the same source yields the
//! same bytes — printed as a stable FNV-1a content hash — which is what makes the
//! item *reproducible* even though generation is not.

use asset_icc::open_rgba;
use image::{RgbaImage, imageops};
use std::path::Path;

/// Alpha at or below this is treated as background.
const ALPHA_FLOOR: u8 = 8;

/// Pixels of slack around the bbox so anti-aliased edges are not clipped.
const PAD: u32 = 2;

/// Opaque bounding box, `(x0, y0, x1, y1)` inclusive.
fn bbox(img: &RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let (w, h) = img.dimensions();
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    let mut found = false;
    for (x, y, px) in img.enumerate_pixels() {
        if px.0[3] > ALPHA_FLOOR {
            found = true;
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    found.then_some((x0, y0, x1, y1))
}

/// Stable 64-bit FNV-1a — a content address for the item, no crypto dependency.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let src = args
        .next()
        .expect("usage: <in.png> <slot_x> <slot_y> <slot_w> <slot_h> <out.png>");
    let sx: i64 = args.next().expect("missing slot_x").parse()?;
    let sy: i64 = args.next().expect("missing slot_y").parse()?;
    let sw: u32 = args.next().expect("missing slot_w").parse()?;
    let sh: u32 = args.next().expect("missing slot_h").parse()?;
    let out_path = args.next().expect("missing out.png");

    let img = open_rgba(Path::new(&src))?;
    let (w, h) = img.dimensions();

    let Some((x0, y0, x1, y1)) = bbox(&img) else {
        anyhow::bail!("{src}: fully transparent, nothing to extract");
    };

    // Padded crop, then contain-fit into the slot.
    let cx0 = x0.saturating_sub(PAD);
    let cy0 = y0.saturating_sub(PAD);
    let cx1 = (x1 + PAD).min(w - 1);
    let cy1 = (y1 + PAD).min(h - 1);
    let (cw, ch) = (cx1 - cx0 + 1, cy1 - cy0 + 1);
    let subject = imageops::crop_imm(&img, cx0, cy0, cw, ch).to_image();

    let scale = f32::min(sw as f32 / cw as f32, sh as f32 / ch as f32);
    let nw = ((cw as f32 * scale).round() as u32).max(1);
    let nh = ((ch as f32 * scale).round() as u32).max(1);
    let resized = imageops::resize(&subject, nw, nh, imageops::FilterType::Lanczos3);

    // Slot on a canvas the size of the source, so items share coordinates and can
    // be layered directly.
    let mut canvas = RgbaImage::new(w, h);
    let ox = sx + ((sw - nw) / 2) as i64;
    let oy = sy + ((sh - nh) / 2) as i64;
    imageops::overlay(&mut canvas, &resized, ox, oy);
    canvas.save(&out_path)?;

    let Some((rx0, ry0, rx1, ry1)) = bbox(&canvas) else {
        anyhow::bail!("{out_path}: empty after extraction — bug");
    };

    let hash = fnv1a(&canvas);
    let (rw, rh) = (rx1 - rx0 + 1, ry1 - ry0 + 1);
    println!(
        "{src} -> {out_path}\n  slot ({sx},{sy}) {sw}x{sh}  source bbox {cw}x{ch}  scale {scale:.3}\n  item bbox {rw}x{rh} at ({rx0},{ry0})  centre ({}, {})  hash {hash:016x}",
        (rx0 + rx1) / 2,
        (ry0 + ry1) / 2
    );

    Ok(())
}
