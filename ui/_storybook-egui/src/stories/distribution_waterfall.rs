//! Storybook demo for the DistributionWaterfall widget.
//!
//! Shows how a buyer's payment flows down to each party's wallet, across the
//! three lifecycle modes and from either the manager's or a party's point of
//! view. The "thin price" toggle reproduces the classic confusion — a 2 ADA
//! mint where the platform-fee floor eats the whole sale and the artist's
//! "50%" is 0.

use egui_widgets::{DistributionWaterfall, WaterfallMode, WaterfallParty};

use crate::{ACCENT, BG_MAIN, TEXT_MUTED};

pub struct DistributionWaterfallStoryState {
    /// 0 = Projected, 1 = Live, 2 = Final.
    pub mode_idx: usize,
    /// Highlight the Artist line (a party-dashboard view).
    pub view_as_artist: bool,
    /// 2 ADA sale — the platform-fee floor dominates and founders get 0.
    pub thin_price: bool,
}

impl Default for DistributionWaterfallStoryState {
    fn default() -> Self {
        Self {
            mode_idx: 0,
            view_as_artist: true,
            thin_price: false,
        }
    }
}

fn party(name: &str, bps: u32, lovelace: u64) -> WaterfallParty {
    WaterfallParty {
        name: name.to_string(),
        share_bps: bps,
        lovelace,
    }
}

/// Build a 50/50 waterfall for `sales` sales at `price` lovelace each.
fn build(
    price: u64,
    sales: u64,
    mode: WaterfallMode,
    basis: String,
    highlight: Option<String>,
) -> DistributionWaterfall {
    let gross = price * sales;
    let delivery = 1_500_000 * sales;
    let network = 400_000 * sales;
    // 5% of (paid − refund), min 2 ADA, per sale.
    let platform_per = ((price as u128 * 500 / 10_000) as u64).max(2_000_000);
    let platform = platform_per * sales;
    let distributable = gross
        .saturating_sub(delivery)
        .saturating_sub(network)
        .saturating_sub(platform);
    let founder = distributable / 2;
    let artist = distributable - founder;
    DistributionWaterfall {
        mode,
        basis,
        gross_lovelace: gross,
        delivery_lovelace: delivery,
        refund_lovelace: 0,
        network_fee_lovelace: network,
        platform_fee_lovelace: platform,
        platform_fee_label: Some("5%, min 2 ADA".to_string()),
        distributable_lovelace: distributable,
        parties: vec![
            party("Founder", 5000, founder),
            party("Artist", 5000, artist),
        ],
        highlight,
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut DistributionWaterfallStoryState) {
    ui.label(
        egui::RichText::new("DistributionWaterfall Widget")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "How a buyer's payment flows down to each party. The artist's \"50%\" is \
             50% of the distributable — after NFT min-ADA, network fee, and the \
             platform fee come off the top. Same shape Projected → Live → Final.",
        )
        .color(TEXT_MUTED)
        .size(11.0),
    );
    ui.add_space(12.0);

    // Controls
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Mode:").size(11.0));
        ui.selectable_value(&mut state.mode_idx, 0, "Projected");
        ui.selectable_value(&mut state.mode_idx, 1, "Live");
        ui.selectable_value(&mut state.mode_idx, 2, "Final");
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut state.view_as_artist, "View as Artist (highlight)");
        ui.add_space(12.0);
        ui.checkbox(&mut state.thin_price, "Thin price (2 ADA — floor eats it)");
    });
    ui.add_space(12.0);

    let price = if state.thin_price {
        2_000_000
    } else {
        70_000_000
    };
    let (mode, sales, basis) = match state.mode_idx {
        1 => (
            WaterfallMode::Live,
            142,
            "across 142 sales so far".to_string(),
        ),
        2 => (WaterfallMode::Final, 500, "across 500 sales".to_string()),
        _ => (
            WaterfallMode::Projected,
            1,
            format!("per {} ADA sale", price / 1_000_000),
        ),
    };
    let highlight = state.view_as_artist.then(|| "Artist".to_string());
    let wf = build(price, sales, mode, basis, highlight);

    ui.allocate_ui(egui::vec2(380.0, ui.available_height()), |ui| {
        egui::Frame::new()
            .fill(BG_MAIN)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .stroke(egui::Stroke::new(1.0, egui_widgets::theme::BG_HIGHLIGHT))
            .show(ui, |ui| {
                wf.show(ui);
            });
    });

    ui.add_space(12.0);
    if ui.button("Reset").clicked() {
        *state = DistributionWaterfallStoryState::default();
    }
}
