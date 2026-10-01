//! Framing lab — three ways to hold a composition across a variation set.
//!
//! Requires `FAL_API_KEY`. Everything lands in `.tmp/framing-lab/`.
//!
//! ```sh
//! FAL_API_KEY=... cargo run -p fal-client --example framing_lab -- hero
//! FAL_API_KEY=... cargo run -p fal-client --example framing_lab -- variations 01-hero-a.png
//! FAL_API_KEY=... cargo run -p fal-client --example framing_lab -- lineart    01-hero-a.png
//! FAL_API_KEY=... cargo run -p fal-client --example framing_lab -- lock       03-lineart.png [control_strength]
//! FAL_API_KEY=... cargo run -p fal-client --example framing_lab -- variant
//! FAL_API_KEY=... cargo run -p fal-client --example framing_lab -- palettes   03-lineart.png [control_strength]
//! ```
//!
//! `hero` is text-to-image, and is the only stage where framing is *invented*.
//! Every later stage is conditioned on that frame, which is where framing is
//! *kept*:
//!
//! * `variations` — Nano Banana 2 edit, hero as the sole reference, `auto` ratio.
//! * `lineart`    — Nano Banana 2 edit, hero flattened to black-on-white linework.
//! * `lock`       — FLUX canny text-to-image into those edges: identical geometry,
//!   colour driven by the prompt.
//! * `variant`    — swap ONE trait slot on the hero, then flatten again: the test
//!   for whether a generated variant stays registered to the base.
//! * `palettes`   — one control, several colour assignments: the test for whether
//!   a prompt can bind colour to the right region, which is what a fixed trait
//!   set needs.

use fal_client::{FalClient, FluxCannyGenRequest, ImageOutput, decode_data_uri, png_data_uri};
use std::path::Path;

const DIR: &str = ".tmp/framing-lab";

/// The frozen composition clause. Identical for every subject — only the
/// subject appended below it changes. This is the text half of "consistent
/// framing"; the image half is the reference passed to the edit stages.
const FRAME: &str = "Square 1:1 illustration, full-bleed, flat single-colour background. \
Medium shot, waist-up, eye level. The subject is three-quarter facing the viewer, head in the \
upper right of the frame, with one large object held upright and filling the left third. Bold \
cel-shaded comic-book rendering: heavy black outlines, hard flat shadow shapes, a limited \
three-colour palette, no gradients. No text, no lettering, no logo, no border or frame.";

const HERO_SUBJECT: &str = "A grizzled deep-sea salvage diver in a patched orange survival suit \
and a brass diving helmet with a cracked faceplate, holding a heavy rusted harpoon gun.";

/// Flatten to linework — the pose, with nothing else on it.
const LINEART_PROMPT: &str = "Redraw this as clean black line art on plain white: \
uniform-weight pure black outlines, no shading, no colour, no hatching, no grey, no \
background. Keep the pose, silhouette and composition identical.";

/// Swap exactly one trait slot, holding the base frame registered — the move a
/// generative collection needs once per trait variant.
const SLOT_SWAP: &str = "Keep this exact image — same composition, camera, crop, pose, body, \
arms, background and rendering — but replace ONLY the diving helmet with a wide-brimmed \
sailor's hat. Change nothing else.";

/// Style half of the canny prompt: no colours named, so [`PALETTES`] does all the
/// binding on its own.
const CANNY_STYLE: &str = "Square 1:1 illustration, full-bleed, flat single-colour background. \
Bold cel-shaded comic-book rendering: heavy black outlines, hard flat shadow shapes, \
no gradients, no text, no lettering, no border. Waist-up, three-quarter facing the viewer, \
head in the upper right of the frame, one large object held upright filling the left third.";

/// `(slug, palette)` — the same control, with colour assigned to named regions.
/// If a palette lands on the wrong region, canny cannot render a trait set.
const PALETTES: &[(&str, &str)] = &[
    (
        "warm",
        "a polished BRASS diving helmet, an ORANGE rubber suit, BLACK rubber gloves, \
a rusted iron harpoon, on a flat TEAL background",
    ),
    (
        "cold",
        "a brushed STEEL-GREY diving helmet, a CRIMSON canvas suit, WHITE cotton gloves, \
a polished SILVER harpoon, on a flat NAVY background",
    ),
    (
        "earthy",
        "an aged COPPER diving helmet, a MUSTARD-YELLOW suit, BROWN leather gloves, \
a dark bronze harpoon, on a flat PLUM background",
    ),
];

/// `(slug, subject)` — swapped into the frozen frame at every stage.
const VARIATIONS: &[(&str, &str)] = &[
    (
        "a",
        "A young desert scavenger in cracked goggles and a patched canvas flight jacket, \
holding a battered bolt-action rifle.",
    ),
    (
        "b",
        "An elderly lighthouse keeper in a cable-knit sweater under a heavy storm coat, \
holding an antique brass lantern raised to shoulder height.",
    ),
];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stage = std::env::args().nth(1).unwrap_or_else(|| "hero".into());
    let input = std::env::args().nth(2);
    let control_strength: f32 = std::env::args()
        .nth(3)
        .map(|s| s.parse().expect("control strength must be a float in 0..1"))
        .unwrap_or(0.85);
    let key = std::env::var("FAL_API_KEY").expect("FAL_API_KEY not set");

    let client = FalClient::new(&key);
    std::fs::create_dir_all(DIR)?;

    match stage.as_str() {
        "hero" => hero(&client).await?,
        "variations" => variations(&client, input.as_deref()).await?,
        "lineart" => lineart(&client, input.as_deref()).await?,
        "lock" => lock(&client, input.as_deref(), control_strength).await?,
        "variant" => variant(&client, input.as_deref()).await?,
        "palettes" => palettes(&client, input.as_deref(), control_strength).await?,
        other => eprintln!("unknown stage `{other}`; expected hero|variations|lineart|lock"),
    }
    Ok(())
}

/// Invent a frame. Two candidates, so there is something to choose between.
async fn hero(client: &FalClient) -> Result<(), Box<dyn std::error::Error>> {
    let prompt = format!("{FRAME} {HERO_SUBJECT}");
    let out = client.nano_banana(&prompt, 2, "1:1", "1K").await?;
    save_all(&out, "01-hero")
}

/// Condition on the hero: same frame, different subject.
async fn variations(
    client: &FalClient,
    hero: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let hero_uri = png_data_uri(&std::fs::read(
        Path::new(DIR).join(hero.unwrap_or("01-hero-a.png")),
    )?);

    for (slug, subject) in VARIATIONS {
        let prompt = format!(
            "Keep the composition, camera angle, crop, pose, background colour and rendering \
style exactly as they are. Replace only the subject with: {subject} The held object stays in \
the same place. Do not reframe."
        );
        let out = client
            .nano_banana_edit(&[&hero_uri], &prompt, 1, "auto", "1K")
            .await?;
        save_all(&out, &format!("02-variation-{slug}"))?;
    }
    Ok(())
}

/// Flatten the hero to linework — the pose, with nothing else on it.
async fn lineart(client: &FalClient, hero: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let hero_uri = png_data_uri(&std::fs::read(
        Path::new(DIR).join(hero.unwrap_or("01-hero-a.png")),
    )?);

    let out = client
        .nano_banana_edit(&[&hero_uri], LINEART_PROMPT, 1, "auto", "1K")
        .await?;
    save_all(&out, "03-lineart")
}

/// Generate fresh characters into the linework's edges. Same geometry every
/// time; only the prompt changes.
async fn lock(
    client: &FalClient,
    lineart: Option<&str>,
    control_strength: f32,
) -> Result<(), Box<dyn std::error::Error>> {
    let control = png_data_uri(&std::fs::read(
        Path::new(DIR).join(lineart.unwrap_or("03-lineart.png")),
    )?);

    for (slug, subject) in VARIATIONS {
        let prompt = format!("{FRAME} {subject}");
        let out = client
            .flux_canny_generate(&FluxCannyGenRequest {
                prompt: &prompt,
                control: &control,
                control_strength,
                ..Default::default()
            })
            .await?;
        save_all(&out, &format!("04-locked-{slug}-{control_strength}"))?;
    }
    Ok(())
}

/// Write every image in `out` into [`DIR`], lettering the names when there is
/// more than one.
fn save_all(out: &ImageOutput, stem: &str) -> Result<(), Box<dyn std::error::Error>> {
    let many = out.images.len() > 1;
    for (i, image) in out.images.iter().enumerate() {
        let suffix = if many {
            format!("-{}", (b'a' + i as u8) as char)
        } else {
            String::new()
        };
        write(
            &format!("{stem}{suffix}.png"),
            &decode_data_uri(&image.url)?,
        )?;
    }
    println!("seed={:?}", out.seed);
    Ok(())
}

fn write(name: &str, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new(DIR).join(name);
    std::fs::write(&path, bytes)?;
    println!("wrote {} ({} bytes)", path.display(), bytes.len());
    Ok(())
}

/// Swap one trait slot on the hero, then flatten the result. If the body
/// outline survives the swap, variants stay registered to the base frame.
async fn variant(client: &FalClient, hero: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let hero_uri = png_data_uri(&std::fs::read(
        Path::new(DIR).join(hero.unwrap_or("01-hero-a.png")),
    )?);

    let swapped = client
        .nano_banana_edit(&[&hero_uri], SLOT_SWAP, 1, "auto", "1K")
        .await?;
    save_all(&swapped, "05-slot-hat")?;

    let swapped_uri = png_data_uri(&swapped.first_bytes()?);
    let out = client
        .nano_banana_edit(&[&swapped_uri], LINEART_PROMPT, 1, "auto", "1K")
        .await?;
    save_all(&out, "05-slot-hat-lineart")
}

/// One control, several colour assignments. Every region must take its own
/// colour for canny to render a fixed trait set.
async fn palettes(
    client: &FalClient,
    lineart: Option<&str>,
    control_strength: f32,
) -> Result<(), Box<dyn std::error::Error>> {
    let control = png_data_uri(&std::fs::read(
        Path::new(DIR).join(lineart.unwrap_or("03-lineart.png")),
    )?);

    for (slug, palette) in PALETTES {
        let prompt = format!("{CANNY_STYLE} Depict: {palette}.");
        let out = client
            .flux_canny_generate(&FluxCannyGenRequest {
                prompt: &prompt,
                control: &control,
                control_strength,
                ..Default::default()
            })
            .await?;
        save_all(&out, &format!("06-palette-{slug}"))?;
    }
    Ok(())
}
