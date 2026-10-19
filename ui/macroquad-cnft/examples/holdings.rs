//! A player's live holdings of one collection, with their art.
//!
//! The end-to-end check for this crate: real requests to the ownership
//! service, real art from IIIF, decoded and uploaded as textures, drawn
//! rarest first.
//!
//! ```sh
//! cargo run -p macroquad-cnft --example holdings -- --stake stake1… [--policy <56 hex>]
//! cargo run -p macroquad-cnft --example holdings -- --stake stake1… --shot .tmp/holdings.png
//! ```
//!
//! `--policy` defaults to Toolheads. `--shot` waits for the holdings and
//! every visible texture, then writes the PNG and its text report.

use macroquad::prelude::*;
use macroquad_cnft::{
    ArtLoader, ArtTile, HoldingsLoader, ImageSize, LoadState, OWNERSHIP_BASE, PolicyId, Source,
    TileStyle, art_url,
};
use macroquad_gamekit::harness::flag;
use macroquad_gamekit::{
    Align, Backdrop, Frame, Game, MeasureAt, RichText, TextStyle, app, load_font, stroke_rounded,
};

const FONT: &[u8] = include_bytes!("../../macroquad-storybook/assets/NotoSans-Bold.ttf");
const TOOLHEADS: &str = "285c0b8e91ba323da4ca083c9db837e111dafbf3143ece4d03eba8f4";
const DESIGN: Vec2 = Vec2::new(720.0, 560.0);
const COLS: usize = 4;
const SHOWN: usize = 8;

const BG: Color = Color::new(0.10, 0.11, 0.15, 1.0);
const TEXT: Color = Color::new(0.86, 0.88, 0.95, 1.0);
const MUTED: Color = Color::new(0.55, 0.58, 0.70, 1.0);
const ACCENT: Color = Color::new(0.48, 0.64, 0.97, 1.0);

struct Screen {
    font: Option<Font>,
    holdings: HoldingsLoader,
    art: ArtLoader,
    tile: ArtTile,
}

impl Screen {
    fn text(&self, s: &str, size: f32, color: Color, at: Vec2, px: f32) {
        let mut t = RichText::new();
        let id = t.style(TextStyle::new(self.font.clone(), size, color));
        t.text(s, id);
        let l = t.layout(&MeasureAt(px), f32::INFINITY, Align::Start, 0.0);
        l.draw(&t, at, px, 1.0, |_, _| {});
    }

    /// The URLs of the assets on screen.
    fn visible_urls(&self) -> Vec<String> {
        self.holdings
            .holdings()
            .assets()
            .iter()
            .take(SHOWN)
            .map(|a| art_url(a, ImageSize::Thumb))
            .collect()
    }
}

impl Game for Screen {
    fn update(&mut self, frame: &Frame) {
        self.holdings.update();
        for (i, url) in self.visible_urls().iter().enumerate() {
            self.art.request(url, i as i32);
        }
        self.art.update(frame.clock);
    }

    fn draw(&mut self, frame: &Frame) {
        clear_background(BG);
        if !frame.text {
            return;
        }
        let px = frame.px;
        let h = self.holdings.holdings();
        let status = match self.holdings.state() {
            LoadState::Loading => "loading…".to_string(),
            LoadState::Ready => format!("{} held, rarest first", h.len()),
            LoadState::Failed(e) => format!("failed: {e}"),
        };
        self.text(status.as_str(), 18.0, TEXT, vec2(24.0, 18.0), px);
        self.text(h.policy_id().as_str(), 11.0, MUTED, vec2(24.0, 44.0), px);

        let (gap, top) = (16.0, 72.0);
        let w = (DESIGN.x - 48.0 - gap * (COLS as f32 - 1.0)) / COLS as f32;
        let assets: Vec<_> = h.assets().iter().take(SHOWN).cloned().collect();
        for (i, asset) in assets.iter().enumerate() {
            let (col, row) = ((i % COLS) as f32, (i / COLS) as f32);
            let x = 24.0 + col * (w + gap);
            let y = top + row * (w + 56.0);
            let r = Rect::new(x, y, w, w);
            let url = art_url(asset, ImageSize::Thumb);
            self.tile.draw(&mut self.art, &url, r, 10.0);
            stroke_rounded(r, 10.0, 1.5, ACCENT);
            self.text(&asset.name, 14.0, TEXT, vec2(x, y + w + 6.0), px);
            let rank = asset
                .rarity_rank
                .map_or_else(|| "unranked".to_string(), |r| format!("rank {r}"));
            let role = asset.trait_value("role").unwrap_or("");
            self.text(
                &format!("{rank}  {role}"),
                11.0,
                MUTED,
                vec2(x, y + w + 26.0),
                px,
            );
        }
    }

    fn backdrop(&self) -> Backdrop {
        Backdrop {
            top: BG,
            bottom: BG,
            accent: ACCENT,
        }
    }

    /// Ready once the holdings are in and every visible texture has landed or
    /// failed for good, so a capture shows the art rather than placeholders.
    fn ready(&self) -> bool {
        !self.holdings.is_loading() && !self.art.loading()
    }
}

fn window_conf() -> Conf {
    Conf {
        window_title: "macroquad-cnft holdings".to_owned(),
        window_width: DESIGN.x as i32,
        window_height: DESIGN.y as i32,
        high_dpi: true,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let Some(stake) = flag("--stake") else {
        eprintln!("usage: holdings --stake stake1… [--policy <56 hex>] [--shot out.png]");
        std::process::exit(2);
    };
    let policy = flag("--policy").unwrap_or_else(|| TOOLHEADS.to_string());
    let policy = PolicyId::new(policy).expect("--policy is a 56-character hex policy id");
    let source = Source::stakes(OWNERSHIP_BASE, [stake]).expect("--stake is a stake address");

    let screen = Screen {
        font: load_font(FONT).ok(),
        holdings: HoldingsLoader::new(source, policy),
        art: ArtLoader::default(),
        tile: ArtTile::new(TileStyle::DARK),
    };
    app::run(DESIGN, screen).await;
}
