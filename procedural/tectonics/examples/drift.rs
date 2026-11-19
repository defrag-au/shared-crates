//! Run a planet and write its frames as sheets the globe viewer plays as a
//! timeline: from a supercontinent for a few hundred million years, or with
//! `--primordial` from a hot, newly crusted planet for billions.
//!
//! ```sh
//! cargo run --release -p tectonics --example drift -- --out .tmp/drift
//! cargo run --release -p tectonics --example drift -- --primordial --out .tmp/deep
//! cargo run --release -p globe-viewer -- --timeline .tmp/drift
//! ```
//!
//! Flags: `--primordial` (start hot and young: [`Params::primordial`]; runs
//! 4000 My, a frame every 250 My, unless told otherwise), `--seed` (1),
//! `--samples` (100000), `--plates` (10, or 24 primordial), `--until` (My,
//! 240), `--every` (My between frames written, 20), `--width` (sheet width,
//! 1600; height is half), `--out` (directory).
//!
//! Colour is a plain relief key, not the cel style: sea floor by depth (new
//! crust at the ridges stands shallow and pale, old basins deep), land by
//! height, plate boundaries drawn faintly. The height sheet keeps the sea
//! flat at its surface and raises the land, for the viewer's relief light.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use glam::Vec3;
use image::{ImageBuffer, Luma, Rgba, RgbaImage};
use tectonics::sphere::{Index, spacing};
use tectonics::{Frame, Params, Sim};

struct Args {
    params: Params,
    until: f32,
    every: f32,
    width: u32,
    out: PathBuf,
}

fn parse() -> Result<Args> {
    // `--primordial` picks the preset the other flags then adjust.
    let primordial = std::env::args().any(|a| a == "--primordial");
    let mut args = if primordial {
        Args {
            params: Params::primordial(1),
            until: 4000.0,
            every: 250.0,
            width: 1600,
            out: PathBuf::from(".tmp/drift"),
        }
    } else {
        Args {
            params: Params::earth_like(1),
            until: 240.0,
            every: 20.0,
            width: 1600,
            out: PathBuf::from(".tmp/drift"),
        }
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        if flag == "--primordial" {
            continue;
        }
        let mut value = || it.next().with_context(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--seed" => args.params.seed = value()?.parse()?,
            "--samples" => args.params.samples = value()?.parse()?,
            "--plates" => args.params.plates = value()?.parse()?,
            "--until" => args.until = value()?.parse()?,
            "--every" => args.every = value()?.parse()?,
            "--width" => args.width = value()?.parse()?,
            "--out" => args.out = PathBuf::from(value()?),
            other => bail!("unknown flag {other}"),
        }
    }
    Ok(args)
}

fn main() -> Result<()> {
    let args = parse()?;
    std::fs::create_dir_all(&args.out)?;
    let started = std::time::Instant::now();
    let mut sim = Sim::new(args.params.clone());
    println!(
        "{} samples ({:.0} km apart) · {} plates · until {} My",
        args.params.samples,
        spacing(args.params.samples) * args.params.radius_km,
        args.params.plates,
        args.until
    );

    // The pixel → grid point lookup is the same for every frame.
    let (w, h) = (args.width, args.width / 2);
    let lookup = pixel_lookup(sim.points(), w, h, args.params.samples);
    let mut written = 0;
    let mut failure = None;
    let mut next = 0.0;
    sim.run(args.until, |frame| {
        // Frames come every resample; write one every `--every` My.
        if frame.time + 1e-3 < next {
            return;
        }
        next += args.every;
        let s = frame.stats;
        // How the land's height is spread: what holds continents up.
        let mut land: Vec<(f32, f32)> = frame
            .crust
            .iter()
            .filter(|c| c.kind == tectonics::Kind::Continental)
            .map(|c| (c.elevation, c.age))
            .collect();
        if !land.is_empty() {
            land.sort_by(|a, b| a.0.total_cmp(&b.0));
            let median = land[land.len() / 2].0;
            let high = land.iter().filter(|l| l.0 > 1.5).count() as f32 / land.len() as f32;
            let young = land.iter().filter(|l| l.1 < 50.0).count() as f32 / land.len() as f32;
            // Mean uplift per land sample since the last resample, km.
            let per = |total: f32| total / land.len() as f32;
            println!(
                "        land: median {median:.2} km · above 1.5 km {:.0}% · built in last 50 My {:.0}% · raised by subduction {:.3} km · by collision {:.3} km",
                high * 100.0,
                young * 100.0,
                per(s.raised_by_subduction),
                per(s.raised_by_collision)
            );
        }
        println!(
            "{:>5.0} My · heat {:.2}× · continent {:>4.1}% · dry land {:>4.1}% · highest {:>4.1} km · opened {:>5} · subducted {:>5} · collisions {} · rifts {} · plates {} · {:.1}s",
            frame.time,
            frame.heat,
            s.continental * 100.0,
            frame.surface.iter().filter(|&&z| z > 0.0).count() as f32 / frame.surface.len() as f32
                * 100.0,
            frame
                .crust
                .iter()
                .map(|c| c.elevation)
                .fold(f32::MIN, f32::max),
            s.opened,
            s.subducted,
            s.collisions,
            s.rifts,
            frame.plates.len(),
            started.elapsed().as_secs_f32()
        );
        if failure.is_none()
            && let Err(e) = write_sheets(frame, &lookup, w, h, &args.out, written) {
                failure = Some(e);
            }
        written += 1;
    });
    if let Some(e) = failure {
        return Err(e);
    }
    println!("wrote {written} frames to {}", args.out.display());
    Ok(())
}

/// For each pixel of a `w × h` equirectangular sheet, the nearest grid point
/// (longitude from −180° at the left, north at the top; the convention every
/// sheet of a bake uses).
fn pixel_lookup(points: &[Vec3], w: u32, h: u32, samples: usize) -> Vec<u32> {
    let s = spacing(samples);
    let index = Index::new(points, s * 2.0);
    (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            let lon = (x as f32 + 0.5) / w as f32 * std::f32::consts::TAU - std::f32::consts::PI;
            let lat =
                std::f32::consts::FRAC_PI_2 - (y as f32 + 0.5) / h as f32 * std::f32::consts::PI;
            let d = Vec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin());
            index
                .nearest(points, d, s * 2.0)
                .or_else(|| index.nearest(points, d, s * 6.0))
                .expect("a grid point near every direction") as u32
        })
        .collect()
}

fn write_sheets(
    frame: &Frame,
    lookup: &[u32],
    w: u32,
    h: u32,
    out: &std::path::Path,
    n: usize,
) -> Result<()> {
    let at = |x: u32, y: u32| lookup[(y.min(h - 1) * w + x % w) as usize] as usize;
    let mut colour = RgbaImage::new(w, h);
    let mut height = ImageBuffer::<Luma<u16>, Vec<u16>>::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let i = at(x, y);
            // The surface: shelves on the sea floor, volcanoes breaking it.
            let z = frame.surface[i];
            let mut rgb = hot_world(frame, i, z);
            // Plate boundaries, faintly.
            let o = frame.owner[i];
            if frame.owner[at(x + 1, y)] != o || frame.owner[at(x, y + 1)] != o {
                rgb = rgb.map(|v| (v as f32 * 0.6) as u8);
            }
            colour.put_pixel(x, y, Rgba([rgb[0], rgb[1], rgb[2], 255]));
            // The sea's surface flat; land above it.
            let t = if z > 0.0 {
                0.3 + (z / 10.0) * 0.7
            } else {
                0.25
            };
            height.put_pixel(x, y, Luma([(t.clamp(0.0, 1.0) * 65_535.0) as u16]));
        }
    }
    colour.save(out.join(format!("epoch{n}-colour.png")))?;
    height.save(out.join(format!("epoch{n}-height.png")))?;
    Ok(())
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    [0, 1, 2].map(|k| (a[k] + (b[k] - a[k]) * t) as u8)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn blend(a: [u8; 3], b: [f32; 3], t: f32) -> [u8; 3] {
    mix(a.map(|v| v as f32), b, t)
}

/// A point's colour as the planet's heat leaves it, cooling into the plain
/// relief key ([`sea`], [`land`]) as the heat falls toward today's:
///
/// - **Land** is bare dark basalt and grey rock while the planet is hot, and
///   greens only as it cools. When life covers it is the world's lore, not
///   the tectonics': this ties it to the heat (below ~1.6×), as a Proposed
///   default.
/// - **Lava** glows where the crust is newest: sea floor a few My old at the
///   ridges, fresh hotspot volcanoes, and low land just raised by an arc —
///   in proportion to the heat above today's, so it fades out as the planet
///   cools.
/// - **The sea** is murky grey-green and steam-hazed while hot, clearing to
///   blue.
fn hot_world(frame: &Frame, i: usize, z: f32) -> [u8; 3] {
    let excess = ((frame.heat - 1.0) / 2.5).clamp(0.0, 1.0);
    let c = &frame.crust[i];
    let mut rgb = if z > 0.0 {
        let cool = land(z);
        // Bare rock: dark basalt low, grey-brown higher.
        let rock = if z < 1.5 {
            [62.0, 58.0, 56.0]
        } else {
            [118.0, 108.0, 100.0]
        };
        // Bare while hot, greening as the heat falls below ~1.6×.
        let bare = smoothstep(1.2, 1.6, frame.heat);
        blend(cool, rock, bare)
    } else {
        blend(sea(z), [70.0, 92.0, 84.0], excess * 0.8)
    };
    // Lava where crust is newest: hot ridges, fresh volcanoes, and land an
    // arc has only just raised from the sea (not whole cordilleras: uplift
    // keeps resetting their age). Only a hot planet's shows; today's ridges
    // are deep under the sea.
    let fresh = match c.kind {
        tectonics::Kind::Oceanic => {
            let ridge = 1.0 - smoothstep(0.0, 6.0, c.age);
            let volcano = smoothstep(2.0, 7.0, c.volcanic);
            ridge.max(volcano)
        }
        tectonics::Kind::Continental => {
            (1.0 - smoothstep(0.0, 3.0, c.age)) * (1.0 - smoothstep(0.5, 2.0, z))
        }
    };
    let glow = fresh * excess;
    if glow > 0.05 {
        rgb = blend(rgb, [255.0, 108.0, 32.0], glow * 0.85);
    }
    // Steam haze over everything on the hottest worlds.
    blend(rgb, [196.0, 186.0, 196.0], excess * 0.18)
}

/// New crust at a ridge (−2.5 km) pale, old abyssal plains (−6.4 km) deep.
fn sea(z: f32) -> [u8; 3] {
    mix([96.0, 170.0, 214.0], [22.0, 44.0, 108.0], (-z - 2.5) / 3.9)
}

/// Lowland green to upland ochre to bare grey peaks.
fn land(z: f32) -> [u8; 3] {
    if z < 1.5 {
        mix([104.0, 160.0, 72.0], [176.0, 150.0, 86.0], z / 1.5)
    } else {
        mix([176.0, 150.0, 86.0], [214.0, 210.0, 204.0], (z - 1.5) / 4.0)
    }
}
