//! Generate the three offworld-claimant token icons via fal's Nano Banana 2.
//!
//! Requires `FAL_API_KEY` in the environment.
//!
//! ```sh
//! FAL_API_KEY=... cargo run -p fal-client --example claimant_icons
//! ```
//!
//! Writes `.tmp/vell.png`, `.tmp/kryll.png`, `.tmp/threx.png` - square 1K PNGs,
//! shaped as wallet icons (bold silhouette, tight crop) rather than illustrations.
//!
//! Pass a name (`vell`, `kryll`, `threx`) to generate just one.

use fal_client::FalClient;

/// Prepended to every subject so the three read as one set.
const STYLE: &str = "Square 1:1 token icon, tight crop - the subject fills the frame with almost no margin. Bold, simple, high-contrast silhouette. A deep murky coloured ground, clearly not pure black, with a subtle vignette and strong rim lighting so the silhouette separates from the ground. Semi-realistic painterly sci-fi in a Lovecraftian steampunk register; a warm golden glow is the brightest accent. No text, no letters, no insignia, no border or frame. Must stay legible when shrunk to a tiny 24-pixel wallet icon.";

/// `(slug, subject)` - the subject is appended to [`STYLE`].
const CLAIMANTS: &[(&str, &str)] = &[
    (
        "vell",
        "A crystalline mineral entity with no human anatomy - a tall, sharply angular obelisk of pale frost-white and icy blue crystal, built from a few large flat facets. Bold dark facet lines and deep blue shadow give it hard internal structure; bright icy rim light traces every edge. Its head is a long smooth eyeless mask of ice. Through its lower core runs a large, brilliant, glowing golden shard of ichor. Crisp hard edges, high contrast, simple bold silhouette: an angular crystal shard.",
    ),
    (
        "kryll",
        "A swift insectile offworld drifter wrapped in a translucent, faintly luminous iridescent membrane - teal, violet and pale blue. Segmented and angular, built for speed, mid-swoop, trailing ribbons of burning golden ichor like a comet's exhaust. Silhouette: a sharp diagonal streak, hooked like a dart.",
    ),
    (
        "threx",
        "A coiled chitinous offworld brood-mother - a reptilian hive lineage. Overlapping armoured plates in sickly acid green and bone, many small glowing golden eyes clustered along its length, and a clutch of pale luminous eggs pulsing with golden ichor at its centre. Silhouette: a bold spiral.",
    ),
];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let only = std::env::args().nth(1);
    let key = std::env::var("FAL_API_KEY").expect("FAL_API_KEY not set");

    let client = FalClient::new(&key);
    std::fs::create_dir_all(".tmp")?;

    for (slug, subject) in CLAIMANTS {
        if only.as_deref().is_some_and(|want| want != *slug) {
            continue;
        }

        let prompt = format!("{STYLE} {subject}");
        let out = client.nano_banana(&prompt, 1, "1:1", "1K").await?;
        let bytes = out.first_bytes()?;

        let path = format!(".tmp/{slug}.png");
        std::fs::write(&path, &bytes)?;

        let seed = out.seed;
        let n = bytes.len();
        println!("wrote {path} ({n} bytes), seed={seed:?}");
    }

    Ok(())
}
