//! Composite an overlay layer onto a base image (straight-alpha source-over).
//!
//! ```sh
//! cargo run -p asset-icc --example composite -- base.png overlay.png [out.png]
//! ```
//!
//! Uses `image::imageops::overlay`, which is the same source-over `surface-mesh`
//! exposes as `composite_over` — this example avoids a codec dependency on
//! `surface-mesh` (its `image` dep is deliberately codec-free).

use asset_icc::open_rgba;
use image::imageops;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let base_path = args.next().expect("usage: <base.png> <overlay.png> [out.png]");
    let over_path = args.next().expect("missing overlay.png");
    let out_path = args
        .next()
        .unwrap_or_else(|| ".tmp/composed.png".to_string());

    let mut base = open_rgba(Path::new(&base_path))?;
    let over = open_rgba(Path::new(&over_path))?;

    let (bw, bh) = base.dimensions();
    let (ow, oh) = over.dimensions();
    if (bw, bh) != (ow, oh) {
        println!("note: sizes differ ({bw}x{bh} base, {ow}x{oh} overlay); compositing top-left aligned");
    }

    imageops::overlay(&mut base, &over, 0, 0);
    base.save(&out_path)?;

    println!("{base_path} + {over_path} -> {out_path} ({bw}x{bh})");
    Ok(())
}
