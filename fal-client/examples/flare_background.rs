//! Generate an opaque full-frame background with GPT Image 2.5 Flare.
//!
//! Requires `FAL_API_KEY`. Writes `.tmp/background.png`.
//!
//! ```sh
//! FAL_API_KEY=... cargo run -p fal-client --example flare_background
//! ```

use fal_client::{FalClient, ImageOutput};
use serde::Serialize;

const FLARE: &str = "openai/gpt-image-2.5/flare/text-to-image";

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

const PROMPT: &str = "A dark alien cityscape at night seen from a distance: tall angular monolith spires, a glowing golden haze low on the horizon, faint stars, atmospheric depth, painterly sci-fi concept art, empty of characters and foreground objects, fills the whole frame edge to edge, no text, no border.";

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
                background: "opaque",
                image_size: Size {
                    width: 1024,
                    height: 1024,
                },
                quality: "high",
                output_format: "png",
                num_images: 1,
                sync_mode: true,
            },
        )
        .await?;

    let bytes = out.first_bytes()?;
    std::fs::write(".tmp/background.png", &bytes)?;
    let n = bytes.len();
    println!("wrote .tmp/background.png ({n} bytes)");

    Ok(())
}
