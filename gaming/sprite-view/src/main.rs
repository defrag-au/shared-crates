//! `sprite-view` — review sprite files with no game built.
//!
//! ```sh
//! sprite-view sprites/                                  # the viewer, on every .sprite under sprites/
//! sprite-view sprites/whale.sprite                      # the viewer, on one
//! sprite-view sprites/ --check                          # every file read, its motion problems listed; no window
//! sprite-view sprites/ --sprites sheet.png [--only whale]               # the review sheet and its report
//! sprite-view sprites/ --sprites film.png --only whale --strip 24 --every 3   # one sprite alone, played
//! sprite-view sprites/ --view whale --shot v.png --size 1280x800        # capture the viewer
//! ```
//!
//! The files are macroquad-gamekit's sprite format (`sprite_file` module):
//! rows, palette, gait, scale and body motion. The viewer reloads a file
//! when it is saved, and `W` in it writes tuned values back. Everything drawn
//! is gamekit's; see its `ANIMATION.md` for what to look for.

use macroquad::prelude::*;
use macroquad_gamekit::harness::{Args, SheetSprite};
use macroquad_gamekit::motion::Cycle;
use macroquad_gamekit::{Backdrop, Frame, Game, SpriteFile, app};

/// Flags whose next argument is their value, not a sprite path.
const VALUED: &[&str] = &[
    "--sprites",
    "--only",
    "--strip",
    "--every",
    "--shot",
    "--at",
    "--size",
    "--px",
    "--crop",
    "--view",
    "--keys",
    "--plan",
    "--seconds",
];

/// The `.sprite` files the arguments name: files as given, directories
/// searched, sorted by path.
fn paths(argv: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (i, a) in argv.iter().enumerate().skip(1) {
        if a.starts_with("--") || VALUED.contains(&argv[i - 1].as_str()) {
            continue;
        }
        let path = std::path::Path::new(a);
        if path.is_dir() {
            collect(path, &mut out);
        } else {
            out.push(a.clone());
        }
    }
    out.sort();
    out.dedup();
    out
}

fn collect(dir: &std::path::Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "sprite") {
            out.push(path.to_string_lossy().into_owned());
        }
    }
}

/// Read every file, or say which did not read and stop.
fn load(paths: &[String]) -> Vec<SpriteFile> {
    let mut files = Vec::new();
    let mut bad = 0;
    for p in paths {
        match SpriteFile::load(p) {
            Ok(f) => files.push(f),
            Err(e) => {
                eprintln!("{e}");
                bad += 1;
            }
        }
    }
    if bad > 0 {
        std::process::exit(1);
    }
    if files.is_empty() {
        eprintln!("sprite-view: no .sprite files given (a file, or a directory to search)");
        std::process::exit(2);
    }
    files
}

/// `--check`: every file read, and each moving sprite's motion report.
/// Exits 1 only when a file does not read; motion problems are for reading.
fn check(files: &[SpriteFile]) {
    for f in files {
        let path = f.path.as_deref().unwrap_or(&f.name);
        let grids = f.grids().expect("checked when loaded");
        let (w, h) = (grids[0].w, grids[0].h);
        println!("## {} ({path}, {w}x{h}, {} frames)", f.name, grids.len());
        if grids.len() > 1 {
            println!("{}", Cycle::play(&grids, &f.gait(), f.scale).text());
        } else {
            println!("still\n");
        }
    }
}

/// The files as a game, so gamekit's loop serves `--sprites` (and its
/// `--only`, `--strip`, `--every`) on them.
struct Library {
    sprites: Vec<SheetSprite>,
    backdrop: Backdrop,
}

impl Game for Library {
    fn update(&mut self, _frame: &Frame) {}

    fn draw(&mut self, _frame: &Frame) {
        clear_background(self.backdrop.top);
    }

    fn backdrop(&self) -> Backdrop {
        self.backdrop
    }

    fn sprites(&self) -> Vec<SheetSprite> {
        self.sprites.clone()
    }
}

fn backdrop(files: &[SpriteFile]) -> Backdrop {
    let rgb = files.iter().find_map(|f| f.backdrop).unwrap_or(0x0d1b2a);
    let c = Color::from_rgba((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255);
    Backdrop {
        top: c,
        bottom: c,
        accent: WHITE,
    }
}

fn window_conf() -> Conf {
    let args = Args::read();
    Conf {
        window_title: args.window_title("sprite-view"),
        window_width: 1280,
        window_height: 800,
        high_dpi: true,
        ..Default::default()
    }
}

async fn run(files: Vec<SpriteFile>) {
    let sprites: Vec<SheetSprite> = files.iter().map(SheetSprite::from_file).collect();
    let backdrop = backdrop(&files);
    let flag = macroquad_gamekit::harness::flag;
    if flag("--sprites").is_some() {
        app::run(
            macroquad_gamekit::viewer::SIZE,
            Library { sprites, backdrop },
        )
        .await;
    } else {
        let start = flag("--view")
            .or_else(|| flag("--only"))
            .filter(|v| !v.starts_with("--"));
        app::view(sprites, backdrop, start.as_deref()).await;
    }
}

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let files = load(&paths(&argv));
    if argv.iter().any(|a| a == "--check") {
        check(&files);
        return;
    }
    macroquad::Window::from_config(window_conf(), run(files));
}
