//! Register a generated layer's subject onto a target rectangle.
//!
//! The generated subject lands wherever the model felt like it — centred
//! horizontally, but with a scale that drifts ~12% between rolls. This maps its
//! alpha bounding box onto a fixed slot so every layer composes predictably.
//!
//! ```sh
//! cargo run -p asset-icc --example register_layer -- 0.60 .tmp/layer-1.png .tmp/layer-2.png
//! ```
//!
//! Args: `<target_fraction> <in.png>...`. The slot is a centred square of
//! `target_fraction * canvas` (0.60 => the middle 60% of a 1024px canvas).
//! Writes `.tmp/registered-<name>.png` and prints the before/after bbox.

use asset_icc::open_rgba;
use image::{RgbaImage, imageops};
use std::path::Path;

/// Alpha at or below this is treated as background (matches `alpha_bbox`).
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

fn describe(tag: &str, (x0, y0, x1, y1): (u32, u32, u32, u32)) {
    let (bw, bh) = (x1 - x0 + 1, y1 - y0 + 1);
    println!(
        "  {tag}: bbox {bw}x{bh} at ({x0},{y0})  centre ({}, {})",
        (x0 + x1) / 2,
        (y0 + y1) / 2
    );
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let fraction: f32 = args.next().unwrap_or_else(|| "0.60".into()).parse()?;

    for arg in args {
        let img = open_rgba(Path::new(&arg))?;
        let (w, h) = img.dimensions();

        let Some((x0, y0, x1, y1)) = bbox(&img) else {
            println!("{arg}: fully transparent, nothing to register");
            continue;
        };
        println!("{arg} ({w}x{h}):");
        describe("before", (x0, y0, x1, y1));

        // Padded crop around the subject.
        let cx0 = x0.saturating_sub(PAD);
        let cy0 = y0.saturating_sub(PAD);
        let cx1 = (x1 + PAD).min(w - 1);
        let cy1 = (y1 + PAD).min(h - 1);
        let (cw, ch) = (cx1 - cx0 + 1, cy1 - cy0 + 1);
        let subject = imageops::crop_imm(&img, cx0, cy0, cw, ch).to_image();

        // Slot: a centred square of `fraction` of the canvas. Contain, so the
        // subject keeps its own proportions and only its scale is normalised.
        let slot = (w.min(h) as f32 * fraction).round() as u32;
        let scale = f32::min(slot as f32 / cw as f32, slot as f32 / ch as f32);
        let nw = ((cw as f32 * scale).round() as u32).max(1);
        let nh = ((ch as f32 * scale).round() as u32).max(1);

        let resized = imageops::resize(&subject, nw, nh, imageops::FilterType::Lanczos3);

        let mut out = RgbaImage::new(w, h);
        let ox = ((w - nw) / 2) as i64;
        let oy = ((h - nh) / 2) as i64;
        imageops::overlay(&mut out, &resized, ox, oy);

        let Some((rx0, ry0, rx1, ry1)) = bbox(&out) else {
            println!("  registered image is empty — bug");
            continue;
        };
        describe("after", (rx0, ry0, rx1, ry1));

        let stem = Path::new(&arg)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("layer");
        let path = format!(".tmp/registered-{stem}.png");
        out.save(&path)?;
        println!("  slot {slot}x{slot}  scale {scale:.3}  -> {path}");
    }

    Ok(())
}
