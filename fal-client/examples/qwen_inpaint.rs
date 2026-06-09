//! Fire a real Qwen inpaint against fal.
//!
//! Requires `FAL_API_KEY` in the environment.
//!
//! ```sh
//! FAL_API_KEY=... cargo run -p fal-client --example qwen_inpaint -- \
//!     base.png mask.png "a backwards snapback cap, flat vector"
//! ```
//! Writes the result to `fal-out.png`.

use fal_client::{png_data_uri, FalClient, InpaintRequest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let base = args.next().expect("usage: <base.png> <mask.png> <prompt>");
    let mask = args.next().expect("missing mask.png");
    let prompt = args.next().expect("missing prompt");
    let key = std::env::var("FAL_API_KEY").expect("FAL_API_KEY not set");

    let image = png_data_uri(&std::fs::read(&base)?);
    let mask = png_data_uri(&std::fs::read(&mask)?);

    let client = FalClient::new(&key);
    let out = client
        .qwen_inpaint(&InpaintRequest {
            prompt: &prompt,
            image: &image,
            mask: &mask,
            ..Default::default()
        })
        .await?;

    let bytes = out.first_bytes()?;
    std::fs::write("fal-out.png", &bytes)?;
    println!(
        "wrote fal-out.png ({} bytes), seed={:?}",
        bytes.len(),
        out.seed
    );
    Ok(())
}
