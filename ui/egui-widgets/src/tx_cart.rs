//! TX Cart widget — displays a list of pending chain actions with batch execution.
//!
//! Follows the standard 4-type pattern: Config, State, Action, show().
//! The widget is provider-agnostic — it renders items and manages the execution
//! flow, while the caller handles the actual TX building and signing.

use crate::theme;
use egui::{RichText, Ui};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Display configuration for the cart.
pub struct TxCartConfig {
    pub title: &'static str,
}

impl Default for TxCartConfig {
    fn default() -> Self {
        Self { title: "TX Cart" }
    }
}

/// A single item in the cart.
#[derive(Clone, Debug)]
pub struct TxCartItem {
    pub id: String,
    /// Collection/asset name (e.g., "Helmies")
    pub label: String,
    /// Policy ID (truncated for display)
    pub policy_id: String,
    /// Provider name (e.g., "jpg.store")
    pub provider: String,
    /// Action type for grouping display (e.g., "Created coll. offers", "Cancel coll. offers")
    pub action_label: String,
    /// Number of offers in this item
    pub quantity: u32,
    /// ADA amount per offer
    pub ada_per_item: f64,
    /// Optional hero image URL for the collection
    pub image_url: Option<String>,
    pub status: TxCartItemStatus,
}

/// Status of a cart item.
#[derive(Clone, Debug, PartialEq)]
pub enum TxCartItemStatus {
    Pending,
    Building,
    Signing,
    Submitted { tx_hash: String },
    Error { message: String },
}

impl TxCartItemStatus {
    pub fn label(&self) -> &str {
        match self {
            TxCartItemStatus::Pending => "Pending",
            TxCartItemStatus::Building => "Building...",
            TxCartItemStatus::Signing => "Signing...",
            TxCartItemStatus::Submitted { .. } => "Submitted",
            TxCartItemStatus::Error { .. } => "Error",
        }
    }

    pub fn color(&self) -> egui::Color32 {
        match self {
            TxCartItemStatus::Pending => theme::TEXT_MUTED,
            TxCartItemStatus::Building => theme::ACCENT_CYAN,
            TxCartItemStatus::Signing => theme::ACCENT_CYAN,
            TxCartItemStatus::Submitted { .. } => theme::ACCENT_GREEN,
            TxCartItemStatus::Error { .. } => theme::ACCENT_RED,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TxCartItemStatus::Submitted { .. } | TxCartItemStatus::Error { .. }
        )
    }
}

/// A planned transaction grouping cart items.
#[derive(Clone, Debug)]
pub struct TxCartPlannedTx {
    pub unsigned_tx_cbor: String,
    pub fee: u64,
    pub item_ids: Vec<String>,
    pub summary: String,
}

/// Cart execution phase.
#[derive(Clone, Debug, PartialEq)]
pub enum TxCartPhase {
    /// User is adding/removing items.
    Editing,
    /// Server is building TXs.
    Building,
    /// TXs built, showing preview.
    Preview,
    /// Signing and submitting TXs sequentially.
    Executing { total: usize, completed: usize },
    /// All done.
    Done,
    /// Error during build/execute.
    Error { message: String },
}

/// Cart state — managed by the caller, rendered by the widget.
pub struct TxCartState {
    pub items: Vec<TxCartItem>,
    pub planned_txs: Vec<TxCartPlannedTx>,
    pub phase: TxCartPhase,
}

impl Default for TxCartState {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            planned_txs: Vec::new(),
            phase: TxCartPhase::Editing,
        }
    }
}

impl TxCartState {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn pending_count(&self) -> usize {
        self.items
            .iter()
            .filter(|i| matches!(i.status, TxCartItemStatus::Pending))
            .count()
    }

    pub fn add_item(&mut self, item: TxCartItem) {
        self.items.push(item);
        self.phase = TxCartPhase::Editing;
    }

    pub fn remove_item(&mut self, id: &str) {
        self.items.retain(|i| i.id != id);
        if self.items.is_empty() {
            self.phase = TxCartPhase::Editing;
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.planned_txs.clear();
        self.phase = TxCartPhase::Editing;
    }

    /// Update item statuses for a given TX's items.
    pub fn set_items_status(&mut self, item_ids: &[String], status: TxCartItemStatus) {
        for item in &mut self.items {
            if item_ids.contains(&item.id) {
                item.status = status.clone();
            }
        }
    }
}

/// Actions emitted by the cart widget for the caller to handle.
#[derive(Debug)]
pub enum TxCartAction {
    /// Remove an item from the cart.
    RemoveItem(String),
    /// Build all pending items into TXs (call /api/build-cart).
    Execute,
    /// Sign and submit a specific planned TX.
    SignTx(usize),
    /// Clear the cart.
    Clear,
}

// ---------------------------------------------------------------------------
// Widget
// ---------------------------------------------------------------------------

/// Render the TX cart widget.
pub fn show(
    ui: &mut Ui,
    state: &mut TxCartState,
    config: &TxCartConfig,
) -> Option<TxCartAction> {
    let mut action = None;

    // Title
    ui.label(
        RichText::new(config.title)
            .color(theme::TEXT_PRIMARY)
            .size(22.0)
            .strong(),
    );
    ui.add_space(8.0);

    if state.items.is_empty() {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new("Your cart is empty")
                    .color(theme::TEXT_MUTED)
                    .size(13.0),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new("Add collection offers from the Browse tab")
                    .color(theme::TEXT_MUTED)
                    .size(10.0),
            );
        });
        return action;
    }

    // Group items by action_label for section display
    let mut groups: Vec<(String, Vec<&TxCartItem>)> = Vec::new();
    for item in &state.items {
        if let Some(group) = groups.iter_mut().find(|(label, _)| *label == item.action_label) {
            group.1.push(item);
        } else {
            groups.push((item.action_label.clone(), vec![item]));
        }
    }

    // Render each group
    egui::ScrollArea::vertical().show(ui, |ui| {
        let mut remove_id = None;

        for (group_label, items) in &groups {
            // Group header with total ADA and clear
            let group_total: f64 = items
                .iter()
                .map(|i| i.ada_per_item * i.quantity as f64)
                .sum();

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(group_label)
                        .color(theme::TEXT_PRIMARY)
                        .size(14.0)
                        .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if matches!(state.phase, TxCartPhase::Editing) {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Clear")
                                        .color(theme::TEXT_MUTED)
                                        .size(10.0),
                                )
                                .frame(false),
                            )
                            .clicked()
                        {
                            action = Some(TxCartAction::Clear);
                        }
                    }
                    ui.label(
                        RichText::new(format!("- \u{20B3} {:.0}", group_total))
                            .color(theme::ACCENT_RED)
                            .size(12.0),
                    );
                });
            });

            ui.add_space(2.0);
            ui.separator();
            ui.add_space(6.0);

            // Item cards
            for item in items {
                let card_rect = ui
                    .horizontal(|ui| {
                        // Collection image
                        if let Some(ref url) = item.image_url {
                            let image = egui::Image::new(url.as_str())
                                .fit_to_exact_size(egui::vec2(56.0, 56.0))
                                .corner_radius(egui::CornerRadius::same(4));
                            ui.add(image);
                        } else {
                            // Placeholder
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(56.0, 56.0),
                                egui::Sense::hover(),
                            );
                            ui.painter().rect_filled(
                                rect,
                                egui::CornerRadius::same(4),
                                theme::BG_HIGHLIGHT,
                            );
                        }

                        ui.add_space(8.0);

                        // Info column
                        ui.vertical(|ui| {
                            // Truncated policy ID
                            let pid = &item.policy_id;
                            let truncated = if pid.len() > 16 {
                                format!("{}...{}", &pid[..8], &pid[pid.len() - 4..])
                            } else {
                                pid.clone()
                            };
                            ui.label(
                                RichText::new(truncated)
                                    .color(theme::TEXT_MUTED)
                                    .size(9.0)
                                    .monospace(),
                            );
                            ui.label(
                                RichText::new(&item.label)
                                    .color(theme::TEXT_PRIMARY)
                                    .size(12.0)
                                    .strong(),
                            );

                            // Status (if not pending)
                            if !matches!(item.status, TxCartItemStatus::Pending) {
                                ui.label(
                                    RichText::new(item.status.label())
                                        .color(item.status.color())
                                        .size(9.0),
                                );
                            }
                        });

                        // Right side: quantity x price + remove
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                // Remove button
                                if matches!(item.status, TxCartItemStatus::Pending)
                                    && matches!(state.phase, TxCartPhase::Editing)
                                {
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                RichText::new("\u{2715}")
                                                    .color(theme::TEXT_MUTED)
                                                    .size(12.0),
                                            )
                                            .frame(false),
                                        )
                                        .clicked()
                                    {
                                        remove_id = Some(item.id.clone());
                                    }
                                    ui.add_space(8.0);
                                }

                                // Quantity x Price
                                let total = item.ada_per_item * item.quantity as f64;
                                ui.label(
                                    RichText::new(format!("\u{20B3} {:.0}", total))
                                        .color(theme::TEXT_PRIMARY)
                                        .size(12.0),
                                );
                                if item.quantity > 1 {
                                    ui.label(
                                        RichText::new(format!("{}x", item.quantity))
                                            .color(theme::TEXT_MUTED)
                                            .size(10.0),
                                    );
                                }
                            },
                        );
                    })
                    .response
                    .rect;

                // Card border
                ui.painter().rect_stroke(
                    card_rect.expand(4.0),
                    egui::CornerRadius::same(8),
                    egui::Stroke::new(0.5, theme::BORDER),
                    egui::StrokeKind::Outside,
                );

                // Error detail
                if let TxCartItemStatus::Error { message } = &item.status {
                    ui.label(
                        RichText::new(message)
                            .color(theme::ACCENT_RED)
                            .size(9.0),
                    );
                }

                ui.add_space(6.0);
            }

            ui.add_space(12.0);
        }

        if let Some(id) = remove_id {
            action = Some(TxCartAction::RemoveItem(id));
        }
    });

    ui.add_space(8.0);

    // Bottom action area
    match &state.phase {
        TxCartPhase::Editing => {
            if state.pending_count() > 0 {
                let total_ada: f64 = state
                    .items
                    .iter()
                    .filter(|i| matches!(i.status, TxCartItemStatus::Pending))
                    .map(|i| i.ada_per_item * i.quantity as f64)
                    .sum();

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("Total: \u{20B3} {:.0}", total_ada))
                            .color(theme::TEXT_SECONDARY)
                            .size(11.0),
                    );

                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("Submit")
                                            .color(theme::BG_PRIMARY)
                                            .size(13.0)
                                            .strong(),
                                    )
                                    .fill(theme::ACCENT_GREEN)
                                    .corner_radius(egui::CornerRadius::same(6))
                                    .min_size(egui::vec2(120.0, 36.0)),
                                )
                                .clicked()
                            {
                                action = Some(TxCartAction::Execute);
                            }
                        },
                    );
                });
            }
        }

        TxCartPhase::Building => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new("Building transactions...")
                        .color(theme::ACCENT_CYAN)
                        .size(12.0),
                );
            });
        }

        TxCartPhase::Preview => {
            ui.label(
                RichText::new(format!(
                    "{} transaction(s) to sign",
                    state.planned_txs.len()
                ))
                .color(theme::TEXT_SECONDARY)
                .size(11.0),
            );
            ui.add_space(4.0);

            for (i, planned) in state.planned_txs.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("TX {}", i + 1))
                            .color(theme::TEXT_MUTED)
                            .size(10.0),
                    );
                    ui.label(
                        RichText::new(&planned.summary)
                            .color(theme::TEXT_PRIMARY)
                            .size(10.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!(
                                "\u{20B3} {:.2} fee",
                                planned.fee as f64 / 1_000_000.0
                            ))
                            .color(theme::TEXT_MUTED)
                            .size(9.0),
                        );
                    });
                });
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("Sign & Submit")
                                    .color(theme::BG_PRIMARY)
                                    .size(13.0)
                                    .strong(),
                            )
                            .fill(theme::ACCENT_GREEN)
                            .corner_radius(egui::CornerRadius::same(6))
                            .min_size(egui::vec2(140.0, 36.0)),
                        )
                        .clicked()
                    {
                        action = Some(TxCartAction::SignTx(0));
                    }
                });
            });
        }

        TxCartPhase::Executing { total, completed } => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new(format!("Signing {completed}/{total}..."))
                        .color(theme::ACCENT_CYAN)
                        .size(12.0),
                );
            });
        }

        TxCartPhase::Done => {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("\u{2714} All transactions submitted")
                        .color(theme::ACCENT_GREEN)
                        .size(14.0)
                        .strong(),
                );
            });
        }

        TxCartPhase::Error { message } => {
            ui.label(
                RichText::new(format!("Error: {message}"))
                    .color(theme::ACCENT_RED)
                    .size(11.0),
            );
            ui.add_space(4.0);
            if state.pending_count() > 0 {
                if ui.button("Retry").clicked() {
                    action = Some(TxCartAction::Execute);
                }
            }
        }
    }

    action
}
