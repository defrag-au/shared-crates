//! Story: `AboutModal` — the BETA badge's answer to "what am I actually
//! looking at".
//!
//! The copy here is Flow Explorer's real beta statement, because the widget is
//! only as good as whether a caveat survives being written down: "performance
//! is not optimised" and "the thresholds will move" are both things a reader
//! would otherwise hit and misread — as broken, and as a bait-and-switch.
//!
//! The second variant is the same widget with no status word, which is what an
//! "about" looks like once a product has shipped.

use egui_widgets::{AboutModal, AboutPoint, PhosphorIcon};

use crate::{ACCENT, TEXT_MUTED};

pub fn show(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("About Modal").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "What a product is, what state it is in, and what to expect from it. \
             Opened from the BETA badge — a badge alone is decoration readers skip, \
             so the caveats it stands for never land.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    let id = ui.id();
    let mut beta_open = ui
        .data_mut(|d| d.get_temp::<bool>(id.with("beta")))
        .unwrap_or(false);
    let mut shipped_open = ui
        .data_mut(|d| d.get_temp::<bool>(id.with("shipped")))
        .unwrap_or(false);

    ui.horizontal(|ui| {
        if ui.button("Open the beta notice").clicked() {
            beta_open = true;
        }
        if ui.button("Open without a status word").clicked() {
            shipped_open = true;
        }
    });

    AboutModal::new("Flow Explorer")
        .status("BETA")
        .intro("Early access. Here is what that means in practice.")
        .point(AboutPoint::new(
            PhosphorIcon::Hourglass,
            "Performance is not optimised yet",
            "Big wallets take a while to load and the feed can stutter while a scan \
             is running. Making it fast is the next piece of work, not an accident.",
        ))
        .point(AboutPoint::new(
            PhosphorIcon::Coins,
            "The tiers are still being set",
            "Holding thresholds are provisional while we learn what depth of history \
             the service can actually sustain. Expect them to move in both directions.",
        ))
        .point(AboutPoint::new(
            PhosphorIcon::Clock,
            "History depth varies",
            "How far back a wallet reads depends on your tier and on how far the scan \
             has reached. Very large addresses are capped and say so on the wallet.",
        ))
        .show(ui, &mut beta_open);

    AboutModal::new("Flow Explorer")
        .intro("A wallet's money story, read straight off the chain.")
        .point(AboutPoint::new(
            PhosphorIcon::Eye,
            "Everything is on-chain",
            "No account, no tracking. Watch any stake address or handle.",
        ))
        .show(ui, &mut shipped_open);

    ui.data_mut(|d| {
        d.insert_temp(id.with("beta"), beta_open);
        d.insert_temp(id.with("shipped"), shipped_open);
    });
}
