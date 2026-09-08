//! `ListingGrid` story — a price is not a promise that you can buy it.
//!
//! Fixtures are real residual jpg.store listings, measured 2026-09-07/08:
//! MarsBirds "Martian Spawn" (65 listings at the V1 sale script, 19–25 ADA,
//! datums resolvable), a Clay Nation listing at V2 whose datum preimage is not
//! recoverable from chain *or* the indexer, and a SpaceBudz bundle member.
//!
//! **The thing to look at:** every card carries a price, and three of them
//! cannot be acted on. Before [`Buyability`] existed the grid rendered those
//! three identically to the rest — the reader would pick the cheapest, click
//! buy, and get a failure from the node several seconds later with a message
//! about datums. The blocked treatment moves that discovery to before the
//! click, and puts the reason where the eye already learned to look for the
//! add-to-cart affordance.
//!
//! Second read: the blocked reasons are not interchangeable, which is why
//! [`BlockedReason`] is an enum rather than a `bool` on the card. "No datum" is
//! permanent — nothing can recover a hash preimage that was never published.
//! "Unsupported" is a registry gap we can close. "Bundle" is neither: the
//! listing is perfectly buyable, just not on its own. Collapsing them to
//! "unavailable" would have us chasing the wrong fix.
//!
//! Third read: the cheapest card in the grid is blocked. Sort a book by price
//! ascending, hide unbuyable rows, and the floor you quote is wrong; show them
//! without marking them and the first thing anyone clicks fails. Neither is
//! acceptable, so they stay, ranked honestly, and say why.
//!
//! Images are deliberately absent. The story is about the state treatment, and
//! a grid backed by live IPFS gateways would make every screenshot a different
//! picture — including the ones taken to review this.

use egui_widgets::{BlockedReason, Buyability, ListingCard, ListingGrid, ListingGridConfig};

pub struct ListingGridState {
    cards: Vec<ListingCard>,
    /// Cards the reader has added, so the in-cart ring is reachable by
    /// clicking rather than only present in the fixture.
    cart: Vec<String>,
    card_width: f32,
    last_action: Option<String>,
}

impl Default for ListingGridState {
    fn default() -> Self {
        Self {
            cards: fixture(),
            cart: vec!["MartianSpawn0319".to_string()],
            card_width: 84.0,
            last_action: None,
        }
    }
}

/// A listing as it comes off the ask book.
fn card(name: &str, price_ada: f64, buyability: Buyability) -> ListingCard {
    ListingCard {
        name: name.to_string(),
        price_lovelace: (price_ada * 1_000_000.0) as u64,
        marketplace: "jpg.store".to_string(),
        unit: name.to_string(),
        buyability,
        ..Default::default()
    }
}

/// Ordered by price ascending, as a floor-first book would be — which is what
/// puts a blocked listing at the head of the grid.
fn fixture() -> Vec<ListingCard> {
    let mut cards = vec![
        // Cheapest in the book, and unbuyable. This ordering is the point.
        card(
            "ClayNation2501",
            17.5,
            Buyability::Blocked(BlockedReason::DatumUnavailable),
        ),
        card("MartianSpawn1265", 19.0, Buyability::Buyable),
        card("MartianSpawn2131", 19.0, Buyability::Buyable),
        card("MartianSpawn0262", 21.5, Buyability::Buyable),
        card("MartianSpawn0319", 22.0, Buyability::InCart),
        card("MartianSpawn0411", 23.0, Buyability::Buyable),
        card(
            "ClayNation7445",
            24.0,
            Buyability::Blocked(BlockedReason::DatumUnavailable),
        ),
        card("MartianSpawn0544", 25.0, Buyability::Buyable),
        card("MartianSpawn0590", 25.0, Buyability::Buyable),
    ];

    // A bundle member: priced for the whole bundle, spendable only as a unit.
    let mut bundle = card(
        "SpaceBud1224",
        5000.0,
        Buyability::Blocked(BlockedReason::BundleMember),
    );
    bundle.bundle_size = Some(5);
    cards.push(bundle);

    // A V4 listing: the contract exists, we simply cannot drive it.
    cards.push(card(
        "SpaceBud5991",
        9999.0,
        Buyability::Blocked(BlockedReason::UnsupportedContract),
    ));

    cards
}

pub fn show(ui: &mut egui::Ui, state: &mut ListingGridState) {
    ui.horizontal(|ui| {
        ui.label("Card width");
        ui.add(egui::Slider::new(&mut state.card_width, 60.0..=140.0).suffix(" px"));
        if ui.button("Reset cart").clicked() {
            state.cart.clear();
            state.cards = fixture();
            state.last_action = None;
        }
    });
    ui.add_space(4.0);

    // Counting what is actually purchasable, which is the number a sweep
    // planner cares about and is NOT the row count.
    let buyable = state
        .cards
        .iter()
        .filter(|c| matches!(c.buyability, Buyability::Buyable))
        .count();
    let blocked = state
        .cards
        .iter()
        .filter(|c| matches!(c.buyability, Buyability::Blocked(_)))
        .count();
    ui.label(
        egui::RichText::new(format!(
            "{} listings — {buyable} buyable, {blocked} blocked, {} in cart",
            state.cards.len(),
            state.cart.len()
        ))
        .size(11.0)
        .color(egui_widgets::theme::TEXT_MUTED),
    );
    ui.add_space(6.0);

    let grid = ListingGrid::with_config(ListingGridConfig {
        card_width: state.card_width,
        thumbnail_size: state.card_width,
        ..Default::default()
    });
    let resp = grid.show(ui, &state.cards);

    if let Some(idx) = resp.add_to_cart {
        if let Some(c) = state.cards.get_mut(idx) {
            c.buyability = Buyability::InCart;
            state.cart.push(c.name.clone());
            state.last_action = Some(format!("Added {} to cart", c.name));
        }
    }
    if let Some(idx) = resp.clicked {
        if let Some(c) = state.cards.get(idx) {
            state.last_action = Some(format!("Opened {}", c.name));
        }
    }

    ui.add_space(8.0);
    if let Some(ref action) = state.last_action {
        ui.label(
            egui::RichText::new(action)
                .size(11.0)
                .color(egui_widgets::theme::ACCENT_GREEN),
        );
    } else {
        ui.label(
            egui::RichText::new("Hover a card — buyable ones offer a +; blocked ones say why")
                .size(11.0)
                .color(egui_widgets::theme::TEXT_MUTED),
        );
    }
}
