//! Story: `TierLadder` — the access ladder as a modal, from the two vantage
//! points that matter: an anonymous reader on the free tier, and a holder
//! partway up it.

use egui_widgets::tier_ladder::{Standing, TierLadder, TierRoute, TierRung};

use crate::{ACCENT, TEXT_MUTED};

/// Anonymous: on the floor, every rung above needs a wallet first.
fn anonymous_rungs() -> Vec<TierRung<'static>> {
    vec![
        TierRung::new("Free", "30 days of history")
            .free(true)
            .standing(Standing::Current),
        TierRung::new("90 days", "90 days of history, plus excavation")
            .route(TierRoute::new("$Aliens", "250,000")),
        TierRung::new("6 months", "182 days of history")
            .route(TierRoute::new("$Aliens", "1,000,000")),
        TierRung::new("12 months", "365 days of history")
            .route(TierRoute::new("$Aliens", "2,000,000"))
            .route(TierRoute::new("$PERP", "500")),
        TierRung::new("Full chain", "every transaction, no window")
            .route(TierRoute::new("$Aliens", "25,000,000")),
    ]
}

/// A holder on 90 days: the rung below is covered, the next one shows how far
/// short they are on each route to it.
fn holder_rungs() -> Vec<TierRung<'static>> {
    vec![
        TierRung::new("Free", "30 days of history")
            .free(true)
            .standing(Standing::Held),
        TierRung::new("90 days", "90 days of history, plus excavation")
            .route(TierRoute::new("$Aliens", "250,000"))
            .standing(Standing::Current),
        TierRung::new("6 months", "182 days of history")
            .route(TierRoute::new("$Aliens", "1,000,000").have("410,000")),
        TierRung::new("12 months", "365 days of history")
            .route(TierRoute::new("$Aliens", "2,000,000").have("410,000"))
            .route(TierRoute::new("$PERP", "500").have("0")),
        TierRung::new("Full chain", "every transaction, no window")
            .route(TierRoute::new("$Aliens", "25,000,000")),
    ]
}

pub fn show(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Tier Ladder").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "The whole access ladder as a modal — what each rung gives, every way to \
             reach it, and where the reader stands. Opened by clicking the tier chip; \
             it is the answer to \"why can I only see 30 days?\".",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    // One bool per modal is the whole caller-side state.
    let id = ui.id();
    let mut anon_open = ui
        .data_mut(|d| d.get_temp::<bool>(id.with("anon")))
        .unwrap_or(false);
    let mut holder_open = ui
        .data_mut(|d| d.get_temp::<bool>(id.with("holder")))
        .unwrap_or(false);

    ui.horizontal(|ui| {
        if ui.button("Open as anonymous").clicked() {
            anon_open = true;
        }
        if ui.button("Open as a 90-day holder").clicked() {
            holder_open = true;
        }
    });

    let anon = anonymous_rungs();
    let _ = TierLadder::new(&anon)
        .anonymous(true)
        .intro("Anyone can read recent activity. Holding $Aliens opens up history.")
        .show(ui, &mut anon_open);

    let holder = holder_rungs();
    let _ = TierLadder::new(&holder)
        .intro("Your allocation is checked when you sign in.")
        .show(ui, &mut holder_open);

    ui.data_mut(|d| {
        d.insert_temp(id.with("anon"), anon_open);
        d.insert_temp(id.with("holder"), holder_open);
    });
}
