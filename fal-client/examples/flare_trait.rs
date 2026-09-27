//! Generate a trait layer (crown/crest) with GPT Image 2.5 Flare.
//!
//! Requires `FAL_API_KEY`. Writes `.tmp/trait-crown.png` — transparent, subject
//! only, no head, so it can be anchored into a slot afterwards.
//!
//! ```sh
//! FAL_API_KEY=... cargo run -p fal-client --example flare_trait
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

const PROMPT: &str = "A single alien crystal crown, orbiting crest of tall faceted shards, symmetrical, viewed straight on from the front, isolated object on a fully transparent background, no head, no wearer, no person, no text, no border, fills the middle of the frame.";

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
                num_images: 1,
                sync_mode: true,
            },
        )
        .await?;

    let bytes = out.first_bytes()?;
    std::fs::write(".tmp/trait-crown.png", &bytes)?;
    let n = bytes.len();
    println!("wrote .tmp/trait-crown.png ({n} bytes)");

    Ok(())
}
