//! Print the alpha bounding box of each PNG passed on the command line.
//!
//! ```sh
//! cargo run -p asset-icc --example alpha_bbox -- .tmp/layer-1.png .tmp/layer-2.png
//! ```
//!
//! Reports the canvas size, the subject's opaque bounding box, its centre and its
//! coverage — the numbers you need to judge whether a generated layer registered
//! to where the prompt asked for, and how far successive rolls drift.

use asset_icc::open_rgba;
use std::path::Path;

/// Alpha at or below this is treated as background.
const ALPHA_FLOOR: u8 = 8;

fn main() -> anyhow::Result<()> {
    for arg in std::env::args().skip(1) {
        let img = open_rgba(Path::new(&arg))?;
        let (w, h) = (img.width(), img.height());

        let mut min_x = w;
        let mut min_y = h;
        let mut max_x = 0u32;
        let mut max_y = 0u32;
        let mut opaque = 0u64;

        for (x, y, px) in img.enumerate_pixels() {
            if px.0[3] > ALPHA_FLOOR {
                opaque += 1;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }

        if opaque == 0 {
            println!("{arg}: canvas {w}x{h} - fully transparent");
            continue;
        }

        let (bw, bh) = (max_x - min_x + 1, max_y - min_y + 1);
        let cx = (min_x + max_x) / 2;
        let cy = (min_y + max_y) / 2;
        let coverage = opaque as f64 / (w as f64 * h as f64) * 100.0;
        let margins = format!("L{min_x} T{min_y} R{} B{}", w - 1 - max_x, h - 1 - max_y);

        println!(
            "{arg}: {w}x{h}  bbox {bw}x{bh} at ({min_x},{min_y})  centre ({cx},{cy})  margins {margins}  cover {coverage:.1}%"
        );
    }
    Ok(())
}
