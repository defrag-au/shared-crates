//! `ImageStack` story — the tuning bench.
//!
//! The pile is the visual hook that makes a transaction card feel like the
//! social card rather than like a table row, and it is entirely a matter of
//! proportion: mount width, peek, tilt spread and shadow falloff decide whether
//! it reads as *prints dropped on a desk* or as *some overlapping squares*. The
//! first attempt looked distinctly weaker than the server-rendered version and
//! nobody could say why by reading the code, which is what this story is for.
//!
//! Every field of [`ImageStackStyle`] is a slider, and the count runs to the
//! widget's hard cap of five. Drag until it looks right,
//! then write the numbers into `ImageStackStyle::default()` — that is the point
//! of the bench, and the reason the style is fractions rather than pixels.
//!
//! ## What to look at
//!
//! - **The size ladder at the bottom.** The same pile at row / feature / poster
//!   scale, and beneath it the 30px pile fanned beside the same one unfanned.
//!   That pair is what settled whether the transaction card fans at row
//!   density: at the first-guess style it did not survive 30px, at the tuned
//!   style it does. Re-check it whenever the defaults move.
//! - **One image vs several.** A lone print is never tilted; the tilt exists to
//!   say "there are more behind", and with nothing behind it is just crooked.
//! - **The shadow.** `epaint` cannot blur a rotated polygon, so it is faked by
//!   stacking eight concentric quads. At low spread the banding shows; at high
//!   spread it turns into a smudge. It has a usable middle.
//! - **The dark backdrop toggle.** The server card sits on `#0b0b10`; a card in
//!   the app sits on `BG_SECONDARY`, which is markedly lighter. Paper white
//!   pops far harder against the former, and that difference — not the geometry
//!   — may be most of why the rendered version looked stronger.

use crate::{accent, muted};
use egui_widgets::image_loader::{iiif_asset_url, AssetImageSize};
use egui_widgets::image_stack::{ImageStack, ImageStackStyle, StackImage};
use egui_widgets::slider_group::{Fader, SliderGroup};

/// Real assets, so the treatment is judged against real artwork. A pile of grey
/// placeholder squares looks fine at any settings and tells you nothing.
const POLICY_ID: &str = "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6";

const PIRATES: &[(&str, &str)] = &[
    ("5069726174653834", "Pirate84"),
    ("506972617465323733", "Pirate273"),
    ("50697261746531303430", "Pirate1040"),
    ("506972617465333830", "Pirate380"),
    ("506972617465313432", "Pirate142"),
];

pub struct ImageStackState {
    style: ImageStackStyle,
    count: usize,
    size: f32,
    dark_backdrop: bool,
}

impl Default for ImageStackState {
    fn default() -> Self {
        Self {
            style: ImageStackStyle::default(),
            count: 3,
            size: 120.0,
            dark_backdrop: false,
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut ImageStackState) {
    ui.label(
        egui::RichText::new("Image Stack")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Several images as a fanned pile of mounted prints — so a lot of many reads as a lot \
             of many. Every proportion is a slider; drag until it looks right, then write the \
             numbers into ImageStackStyle::default().",
        )
        .color(muted(ui))
        .small(),
    );
    ui.add_space(12.0);

    let urls: Vec<String> = PIRATES
        .iter()
        .map(|(hex, _)| iiif_asset_url(POLICY_ID, hex, AssetImageSize::Thumbnail))
        .collect();
    let images: Vec<StackImage<'_>> = PIRATES
        .iter()
        .zip(urls.iter())
        .take(state.count)
        .map(|((_, name), url)| StackImage::new(name).image(url))
        .collect();

    ui.horizontal_top(|ui| {
        // ── The bench ────────────────────────────────────────────────────
        let backdrop = match state.dark_backdrop {
            // The server card's own background, for comparing like with like.
            true => egui::Color32::from_rgb(11, 11, 16),
            false => crate::tok(ui, egui_widgets::theme::Token::BgSecondary),
        };
        // A FIXED RECT, PAINTED AND CLIPPED — never a frame sized by its
        // content. The pile's allocation grows with spacing, lift and shadow
        // spread, so a frame wrapped around it changed size on every slider
        // drag and shoved the slider column sideways while you were holding
        // it. A bench that moves the controls under your cursor is unusable.
        // Now the bench is a constant box; a pile too big for it is clipped,
        // which is the honest thing to show anyway.
        let (bench, _) = ui.allocate_exact_size(egui::vec2(360.0, 300.0), egui::Sense::hover());
        ui.painter()
            .rect_filled(bench, egui::CornerRadius::same(8), backdrop);
        let inner = bench.shrink(16.0);
        let mut pile = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(
            egui::Layout::centered_and_justified(egui::Direction::TopDown),
        ));
        pile.set_clip_rect(inner);
        ImageStack::new(&images)
            .size(state.size)
            .style(state.style)
            .show(&mut pile);

        ui.add_space(20.0);

        // ── The desk ─────────────────────────────────────────────────────
        ui.vertical(|ui| {
            let s = &mut state.style;
            // One bank, not three separated ones. `SliderGroup` measures its
            // label column across the rows it is given, so three groups would
            // measure three different widths and the spine would kink twice —
            // and the spine is the thing that makes nine channels readable.
            //
            // This also deletes the hand-patching that used to live here:
            // `slider_width = 180`, `interact_size.x = 64` and a `fixed_decimals`
            // on every row, all of it fighting a value box that sized itself to
            // its text and so jittered the labels after it as digits changed.
            // The bank's readout column is sized to the widest value the RANGE
            // can print, which is the same fix made structural.
            crate::controls(ui, |ui| {
                SliderGroup::new()
                    .fader(Fader::new("size", &mut state.size, 24.0..=240.0).suffix("px"))
                    .slider("images", &mut state.count, 1..=5)
                    .fader(Fader::new("mount", &mut s.mount, 0.0..=0.20).decimals(3))
                    // Past 1.0 the prints stop overlapping and spread out with a gap.
                    .fader(Fader::new("spacing", &mut s.spacing, 0.0..=1.30).decimals(3))
                    .fader(Fader::new("lift", &mut s.lift, 0.0..=0.20).decimals(3))
                    .fader(
                        Fader::new("tilt spread", &mut s.tilt_deg, 0.0..=20.0)
                            .decimals(1)
                            .suffix("°"),
                    )
                    .fader(
                        Fader::new("shadow offset", &mut s.shadow_offset, 0.0..=0.20).decimals(3),
                    )
                    .fader(
                        Fader::new("shadow spread", &mut s.shadow_spread, 0.0..=0.30).decimals(3),
                    )
                    .slider("shadow alpha", &mut s.shadow_alpha, 0..=255)
                    .show(ui);
            });

            ui.add_space(6.0);
            ui.checkbox(&mut state.dark_backdrop, "server-card backdrop (#0b0b10)");

            ui.add_space(8.0);
            if ui.button("reset to default").clicked() {
                *s = ImageStackStyle::default();
            }
            ui.add_space(6.0);
            // The numbers, in the shape they get pasted back as. A bench you
            // have to transcribe from by eye is a bench nobody uses twice.
            ui.label(
                egui::RichText::new(format!(
                    "mount: {:.3}, spacing: {:.3}, lift: {:.3},\ntilt_deg: {:.1}, \
                     shadow_offset: {:.3},\nshadow_spread: {:.3}, shadow_alpha: {}",
                    s.mount,
                    s.spacing,
                    s.lift,
                    s.tilt_deg,
                    s.shadow_offset,
                    s.shadow_spread,
                    s.shadow_alpha
                ))
                .monospace()
                .small()
                .color(muted(ui)),
            );
        });
    });

    ui.add_space(20.0);
    ui.separator();
    ui.add_space(10.0);

    // ── The size ladder ──────────────────────────────────────────────────
    // Where the fan stops working is a fact about pixels, not about taste, and
    // it is the number that decides whether a feed row gets a pile or a single
    // thumbnail.
    ui.label(
        egui::RichText::new("The same pile at the three card densities")
            .color(accent(ui))
            .small()
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Row / Feature / Poster, at the current style. All three fan — the row below is the \
             evidence that 30px survives it at the tuned defaults.",
        )
        .color(muted(ui))
        .small(),
    );
    ui.add_space(10.0);

    // FIXED-HEIGHT ROWS, for the same reason as the bench: the ladder's
    // height is the tallest pile in it, which changes with every slider, and
    // everything below it walked up and down the page as you dragged.
    let row_width = ui.available_width();
    ui.allocate_ui_with_layout(
        egui::vec2(row_width, 230.0),
        egui::Layout::left_to_right(egui::Align::TOP),
        |ui| {
            for (label, size, fan) in [
                ("Row (30)", 30.0, true),
                ("Feature (56)", 56.0, true),
                ("Poster (130)", 130.0, true),
            ] {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(label).color(muted(ui)).small());
                    ui.add_space(4.0);
                    ImageStack::new(&images)
                        .size(size)
                        .style(state.style)
                        .fan(fan)
                        .show(ui);
                });
                ui.add_space(28.0);
            }
        },
    );

    ui.add_space(16.0);
    // A row shows the front image ALONE — the comparison that decides whether
    // `fan(false)` is the right call at that scale, rather than assuming it.
    ui.label(
        egui::RichText::new("Row scale: fanned vs single, side by side")
            .color(accent(ui))
            .small()
            .strong(),
    );
    ui.add_space(8.0);
    ui.allocate_ui_with_layout(
        egui::vec2(row_width, 80.0),
        egui::Layout::left_to_right(egui::Align::TOP),
        |ui| {
            for (label, fan) in [("fanned", true), ("single", false)] {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(label).color(muted(ui)).small());
                    ui.add_space(4.0);
                    ImageStack::new(&images)
                        .size(30.0)
                        .style(state.style)
                        .fan(fan)
                        .show(ui);
                });
                ui.add_space(28.0);
            }
        },
    );
}
