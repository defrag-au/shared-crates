//! Optimizer settings panel with ideal-state shelf preview.
//!
//! Uses `compute_ideal_state` for instant preview as the user tweaks settings.
//! Uses `build_optimization_steps` when viewing individual TX steps.

use egui::Ui;

use cardano_assets::utxo::UtxoApi;
use utxo_optimizer::{AdaStrategy, FeeParams, IdealState, OptimizationPlan, OptimizeConfig};

use crate::theme;
use crate::utxo_shelf::{classify_utxos, ShelfConfig, ShelfData};
use crate::utxo_shelf_anim::ShelfStepViewer;

/// Protocol parameter for shelf classification.
const COINS_PER_UTXO_BYTE: u64 = 4310;

// ============================================================================
// Panel state
// ============================================================================

/// Persistent state for the optimizer panel.
#[derive(Default)]
pub struct OptimizerPanel {
    /// Optimization settings.
    pub config: OptimizeConfig,
    /// Protocol fee parameters.
    pub fee_params: FeeParams,
    /// Cached ideal state (recomputed on config change).
    ideal_state: Option<IdealState>,
    /// Cached shelf data for ideal state display.
    ideal_shelf_data: Option<ShelfData>,
    /// Lazily-computed step plan (only when user requests step view).
    step_plan: Option<OptimizationPlan>,
    /// Step viewer for browsing individual TX steps.
    pub step_viewer: ShelfStepViewer,
    /// Whether we're showing step-by-step view vs ideal end state.
    show_steps: bool,
    /// Fingerprint of the last config used for computation.
    config_fingerprint: u64,
}

impl OptimizerPanel {
    fn config_hash(&self) -> u64 {
        let mut h: u64 = self.config.bundle_size as u64;
        h = h
            .wrapping_mul(31)
            .wrapping_add(self.config.isolate_fungible as u64);
        h = h
            .wrapping_mul(31)
            .wrapping_add(self.config.isolate_nonfungible as u64);
        h = h
            .wrapping_mul(31)
            .wrapping_add(self.config.ada_strategy as u64);
        h = h
            .wrapping_mul(31)
            .wrapping_add(self.config.collateral.count as u64);
        h = h
            .wrapping_mul(31)
            .wrapping_add(self.config.collateral.ceiling_lovelace);
        for &t in &self.config.collateral.targets_lovelace {
            h = h.wrapping_mul(31).wrapping_add(t);
        }
        h
    }

    /// Render the optimizer panel.
    pub fn show(&mut self, ui: &mut Ui, utxos: &[UtxoApi]) -> OptimizerResponse {
        let response = OptimizerResponse::default();

        // Recompute ideal state if config changed
        let current_hash = self.config_hash();
        if current_hash != self.config_fingerprint && !utxos.is_empty() {
            let ideal = utxo_optimizer::compute_ideal_state(utxos, &self.config, &self.fee_params);
            self.ideal_shelf_data = Some(classify_utxos(&ideal.as_utxos, COINS_PER_UTXO_BYTE));
            self.ideal_state = Some(ideal);
            // Invalidate step plan — will be rebuilt on demand
            self.step_plan = None;
            self.config_fingerprint = current_hash;
        }

        // Settings section
        ui.heading("Optimize UTxOs");
        ui.add_space(4.0);

        // Bundle size slider
        ui.horizontal(|ui| {
            ui.label("Bundle Size");
            let mut bundle = self.config.bundle_size as f32;
            if ui
                .add(egui::Slider::new(&mut bundle, 10.0..=60.0).step_by(1.0))
                .changed()
            {
                self.config.bundle_size = bundle as u32;
            }
        });

        ui.add_space(2.0);

        // ADA strategy toggle
        ui.horizontal(|ui| {
            ui.label("ADA:");
            for (strategy, label) in [
                (AdaStrategy::Leave, "Leave"),
                (AdaStrategy::Rollup, "Rollup"),
                (AdaStrategy::Split, "Split (7-way)"),
            ] {
                if ui
                    .selectable_label(self.config.ada_strategy == strategy, label)
                    .clicked()
                {
                    self.config.ada_strategy = strategy;
                }
            }
        });

        // Token isolation
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.config.isolate_fungible, "Isolate FT");
            ui.add_space(8.0);
            ui.checkbox(&mut self.config.isolate_nonfungible, "Isolate NFT");
        });

        ui.add_space(4.0);

        // Collateral settings
        ui.horizontal(|ui| {
            ui.label("Collateral");
            let mut count = self.config.collateral.count as f32;
            if ui
                .add(egui::Slider::new(&mut count, 0.0..=5.0).step_by(1.0))
                .changed()
            {
                self.config.collateral.count = count as u32;
            }
        });

        if self.config.collateral.count > 0 {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("Targets:")
                        .small()
                        .color(theme::TEXT_MUTED),
                );
                let target_options: &[(u64, &str)] =
                    &[(5_000_000, "5"), (10_000_000, "10"), (15_000_000, "15")];
                for &(amount, label) in target_options {
                    let active = self.config.collateral.targets_lovelace.contains(&amount);
                    if ui.selectable_label(active, label).clicked() {
                        if active {
                            self.config
                                .collateral
                                .targets_lovelace
                                .retain(|&a| a != amount);
                        } else {
                            self.config.collateral.targets_lovelace.push(amount);
                            self.config.collateral.targets_lovelace.sort_unstable();
                        }
                    }
                }
                ui.label(egui::RichText::new("ADA").small().color(theme::TEXT_MUTED));
            });
        }

        ui.add_space(6.0);
        ui.separator();
        ui.add_space(4.0);

        // Summary
        if let Some(ref ideal) = self.ideal_state {
            let s = &ideal.summary;
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(format!("{} → {} UTxOs", s.utxos_before, s.utxos_after));
            });

            let freed = s.ada_freed as f64 / 1_000_000.0;
            if freed > 0.1 {
                ui.label(
                    egui::RichText::new(format!("Est. {freed:.1} ADA freed from locked min-UTxO"))
                        .color(theme::ACCENT_GREEN)
                        .small(),
                );
            }

            if s.utxos_before == s.utxos_after {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Wallet is already optimized!").color(theme::ACCENT_GREEN),
                );
            }
        } else if utxos.is_empty() {
            ui.label(
                egui::RichText::new("Connect wallet to preview optimization")
                    .color(theme::TEXT_MUTED),
            );
        }

        ui.add_space(6.0);

        if self.ideal_state.is_some() {
            // View toggle: Ideal end state vs Step-by-step
            ui.horizontal(|ui| {
                if ui.selectable_label(!self.show_steps, "End State").clicked() {
                    self.show_steps = false;
                }
                if ui.selectable_label(self.show_steps, "TX Steps").clicked() {
                    self.show_steps = true;
                    // Lazily build step plan
                    if self.step_plan.is_none() {
                        let plan = utxo_optimizer::build_optimization_steps(
                            utxos,
                            &self.config,
                            &self.fee_params,
                        );
                        self.step_viewer.set_plan(plan.clone());
                        self.step_plan = Some(plan);
                    }
                }
            });

            ui.separator();
            ui.add_space(4.0);

            let shelf_config = ShelfConfig {
                width: ui.available_width().min(600.0),
                ..ShelfConfig::default()
            };

            if self.show_steps {
                // Step plan details
                if let Some(ref plan) = self.step_plan {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.label(
                            egui::RichText::new(format!("{} steps", plan.summary.num_steps))
                                .color(theme::ACCENT)
                                .strong(),
                        );
                        ui.label("·");
                        let fees_ada = plan.summary.total_fees as f64 / 1_000_000.0;
                        ui.label(format!("~{fees_ada:.2} ADA fees"));
                    });
                }

                // Step navigation
                if self.step_viewer.num_steps() > 0 {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let step = self.step_viewer.current_step();
                        let total = self.step_viewer.num_steps();

                        if ui
                            .add_enabled(step > 0, egui::Button::new("◀").small())
                            .clicked()
                        {
                            self.step_viewer.prev_step();
                        }

                        ui.label(format!("Step {} / {total}", step + 1));

                        if ui
                            .add_enabled(step + 1 < total, egui::Button::new("▶").small())
                            .clicked()
                        {
                            self.step_viewer.next_step();
                        }
                    });

                    // Step details
                    if let Some(ref plan) = self.step_plan {
                        let step_idx = self.step_viewer.current_step();
                        if step_idx < plan.steps.len() {
                            let step = &plan.steps[step_idx];
                            ui.add_space(2.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} inputs → {} outputs  (est. {} bytes, {:.3} ADA fee)",
                                    step.inputs.len(),
                                    step.outputs.len(),
                                    step.estimated_size,
                                    step.estimated_fee as f64 / 1_000_000.0,
                                ))
                                .small()
                                .color(theme::TEXT_MUTED),
                            );
                        }
                    }

                    ui.add_space(4.0);
                    self.step_viewer.show(ui, &shelf_config, utxos);
                }
            } else {
                // Show ideal end state shelf
                if let Some(ref data) = self.ideal_shelf_data {
                    let mut state = crate::utxo_shelf::ShelfState::default();
                    shelf_config.show(ui, data, &mut state);
                }
            }
        }

        response
    }
}

/// Response from the optimizer panel.
#[derive(Default)]
pub struct OptimizerResponse {
    /// User clicked "Execute" to submit the optimization transactions.
    pub execute_requested: bool,
}
