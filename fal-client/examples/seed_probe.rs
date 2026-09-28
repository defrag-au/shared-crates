//! Probe whether a fal model honours a `seed` (same seed twice -> same bytes).
//!
//! Requires `FAL_API_KEY` in the environment.
//!
//! ```sh
//! FAL_API_KEY=... cargo run -p fal-client --example seed_probe -- fal-ai/flux/dev
//! shasum .tmp/probe-a.png .tmp/probe-b.png
//! ```
//!
//! Fires the SAME prompt with the SAME seed twice and writes `.tmp/probe-a.png` /
//! `.tmp/probe-b.png`. Matching hashes = the seed is honoured and reproducible;
//! differing hashes = it is ignored or the endpoint is non-deterministic.

use fal_client::{FalClient, ImageOutput};
use serde::Serialize;

/// The minimal input accepted by both Nano Banana and the FLUX text-to-image models.
#[derive(Serialize)]
struct ProbeInput<'a> {
    prompt: &'a str,
    num_images: u32,
    sync_mode: bool,
    seed: u64,
}

const PROMPT: &str =
    "A simple flat emblem: one bold centred diamond on a dark teal field, minimal, no text.";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let model = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "fal-ai/flux/dev".to_string());
    let key = std::env::var("FAL_API_KEY").expect("FAL_API_KEY not set");

    let client = FalClient::new(&key);
    std::fs::create_dir_all(".tmp")?;
    println!("probing {model}");

    for name in ["probe-a", "probe-b"] {
        let out: ImageOutput = client
            .run(
                &model,
                &ProbeInput {
                    prompt: PROMPT,
                    num_images: 1,
                    sync_mode: true,
                    seed: 42,
                },
            )
            .await?;

        let bytes = out.first_bytes()?;
        std::fs::write(format!(".tmp/{name}.png"), &bytes)?;

        let seed = out.seed;
        let n = bytes.len();
        println!("wrote .tmp/{name}.png ({n} bytes), echoed seed={seed:?}");
    }

    Ok(())
}
