//! Generate the three offworld-claimant sigils via fal's Nano Banana 2.
//!
//! Requires `FAL_API_KEY` in the environment.
//!
//! ```sh
//! FAL_API_KEY=... cargo run -p fal-client --example race_sigils
//! ```
//!
//! Writes `.tmp/sigil-vell.png`, `.tmp/sigil-kryll.png`, `.tmp/sigil-threx.png` -
//! square 1K emblems, rendered as physical relics cut from otherworldly material
//! rather than flat logos, and shaped to read at a 24-pixel wallet icon.
//!
//! Pass a name (`vell`, `kryll`, `threx`) to generate just one.

use fal_client::FalClient;

/// Prepended to every glyph so the three read as one set of relics.
const STYLE: &str = "Square 1:1 faction emblem rendered as a physical relic, not a flat logo. The glyph is a real object carved or forged from an otherworldly material, with genuine depth, weight and surface texture: bevelled and carved relief edges, a surface that catches light with specular highlights, subtle wear, and an inner golden glow. Dramatic cinematic lighting. Deep desaturated teal-green background with a soft vignette so the object reads. Precious and alien, like a medallion from an ancient starfaring race. No text, no letters, no border or frame.";

/// `(slug, glyph)` - the glyph is appended to [`STYLE`].
const SIGILS: &[(&str, &str)] = &[
    (
        "vell",
        "The glyph is a nested diamond - three concentric rhombus outlines joined at their points - carved from translucent glacial-blue crystal like deep ice or a cut gemstone, with frosted bevelled edges and a suspended shard of molten gold glowing at its centre, internal refractions, a cold mineral surface.",
    ),
    (
        "kryll",
        "The glyph is a diagonal chevron-arrow - a bold barbs-and-arrowhead shape - forged from dark iridescent alien metal whose channels run with molten gold as if still cooling, a comet trail of fused gold spilling from its tail, pitted burnished metal, ember-warm.",
    ),
    (
        "threx",
        "The glyph is a trefoil - three thick rings joined around a central node - grown from fossilised resin and bone like ancient amber or chitin, each ring set with a glowing golden eye at its centre and gold veins running through the surface, organic and ancient.",
    ),
];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let only = std::env::args().nth(1);
    let key = std::env::var("FAL_API_KEY").expect("FAL_API_KEY not set");

    let client = FalClient::new(&key);
    std::fs::create_dir_all(".tmp")?;

    for (slug, glyph) in SIGILS {
        if only.as_deref().is_some_and(|want| want != *slug) {
            continue;
        }

        let prompt = format!("{STYLE} {glyph}");
        let out = client.nano_banana(&prompt, 1, "1:1", "1K").await?;
        let bytes = out.first_bytes()?;

        let path = format!(".tmp/sigil-{slug}.png");
        std::fs::write(&path, &bytes)?;

        let seed = out.seed;
        let n = bytes.len();
        println!("wrote {path} ({n} bytes), seed={seed:?}");
    }

    Ok(())
}
