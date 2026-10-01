//! Generate a transparent-background layer with GPT Image 2.5 Flare.
//!
//! Requires `FAL_API_KEY` in the environment. Calls `run()` because the crate has
//! no typed wrapper for the gpt-image endpoints.
//!
//! ```sh
//! FAL_API_KEY=... cargo run -p fal-client --example flare_layer
//! ```
//!
//! Writes `.tmp/layer-1.png` .. `.tmp/layer-N.png` (N = NUM_IMAGES), then measure
//! their registration with:
//!
//! ```sh
//! cargo run -p asset-icc --example alpha_bbox -- .tmp/layer-1.png .tmp/layer-2.png .tmp/layer-3.png
//! ```

use fal_client::{FalClient, ImageOutput};
use serde::Serialize;

const FLARE: &str = "openai/gpt-image-2.5/flare/text-to-image";

/// Explicit size object (the schema also accepts presets). Must be multiples of
/// 16, max edge 3840, total pixels 655,360..8,294,400.
#[derive(Serialize)]
struct Size {
    width: u32,
    height: u32,
}

#[derive(Serialize)]
struct FlareInput<'a> {
    prompt: &'a str,
    background: &'a str,
    image_size: Size,
    quality: &'a str,
    output_format: &'a str,
    num_images: u32,
    sync_mode: bool,
}

const NUM_IMAGES: u32 = 3;

/// The subject is asked for a specific placement so the measurement is
/// meaningful: dead-centre, middle ~60% of a square frame.
const PROMPT: &str = "A single alien explorer's head and shoulders, dead centre, occupying the middle 60 percent of the square frame with equal margins on all sides, facing the viewer, neutral symmetrical pose, crisp clean edges, isolated subject on a fully transparent background, no text, no border, no shadow.";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key = std::env::var("FAL_API_KEY").expect("FAL_API_KEY not set");
    let client = FalClient::new(&key);
    std::fs::create_dir_all(".tmp")?;

    let out: ImageOutput = client
        .run(
            FLARE,
            &FlareInput {
                prompt: PROMPT,
                background: "transparent",
                image_size: Size {
                    width: 1024,
                    height: 1024,
                },
                quality: "high",
                output_format: "png",
                num_images: NUM_IMAGES,
                sync_mode: true,
            },
        )
        .await?;

    for (i, img) in out.images.iter().enumerate() {
        let bytes = fal_client::decode_data_uri(&img.url)?;
        let path = format!(".tmp/layer-{}.png", i + 1);
        std::fs::write(&path, &bytes)?;

        let w = img.width.unwrap_or(0);
        let h = img.height.unwrap_or(0);
        let n = bytes.len();
        println!("wrote {path} ({n} bytes) {w}x{h}");
    }

    Ok(())
}
