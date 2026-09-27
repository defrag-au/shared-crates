//! Restyle icons to match a reference via Nano Banana 2 edit.
//!
//! Requires `FAL_API_KEY` in the environment.
//!
//! ```sh
//! FAL_API_KEY=... cargo run -p fal-client --example restyle_icons -- \
//!     .tmp/threx.png .tmp/vell.png .tmp/kryll.png
//! ```
//!
//! The first argument is the style reference; every later argument is a subject
//! re-rendered to match it. Writes `.tmp/restyled-<name>.png`, leaving the
//! originals untouched.

use fal_client::{FalClient, png_data_uri};

/// Two references, in order: the subject to keep, then the style to copy.
const PROMPT: &str = "Restyle the subject in the first image to exactly match \
the art style of the second image: the same painterly rendering, the same deep \
desaturated teal-green background and vignette, the same rim lighting, and the \
same value range and level of detail. Keep the first image's subject and its \
composition unchanged; change only the rendering style, the lighting and the \
background to match the second image.";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let reference = args.next().expect("usage: <reference.png> <subject.png>...");
    let key = std::env::var("FAL_API_KEY").expect("FAL_API_KEY not set");

    let client = FalClient::new(&key);
    let reference_uri = png_data_uri(&std::fs::read(&reference)?);
    std::fs::create_dir_all(".tmp")?;

    for target in args {
        let subject_uri = png_data_uri(&std::fs::read(&target)?);
        let out = client
            .nano_banana_edit(
                &[subject_uri.as_str(), reference_uri.as_str()],
                PROMPT,
                1,
                "auto",
                "1K",
            )
            .await?;
        let bytes = out.first_bytes()?;

        let stem = std::path::Path::new(&target)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("icon");
        let path = format!(".tmp/restyled-{stem}.png");
        std::fs::write(&path, &bytes)?;

        let n = bytes.len();
        println!("wrote {path} ({n} bytes)");
    }

    Ok(())
}
