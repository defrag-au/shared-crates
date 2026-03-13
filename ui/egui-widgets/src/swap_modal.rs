//! Reusable swap modal widget for egui frontends.
//!
//! Renders a floating `egui::Window` with:
//! - Amount buttons (culture buys + custom)
//! - Custom amount text input (shown only when "Custom" selected)
//! - Slippage selector (1%, 5%, 10%)
//! - Swap preview (estimated output, price, fees)
//! - Confirm/processing/success/error states
//!
//! The widget does NOT spawn async tasks — it returns [`SwapModalAction`] values
//! that the caller dispatches through their own message channel.

use egui::{Align, Color32, Layout, RichText};

use crate::buttons::UiButtonExt;

// ============================================================================
// Config
// ============================================================================

/// Theme colors for the swap modal.
pub struct SwapModalTheme {
    pub accent: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,
    pub error: Color32,
    pub success: Color32,
    pub bg: Color32,
}

impl Default for SwapModalTheme {
    fn default() -> Self {
        Self {
            accent: Color32::from_rgb(68, 255, 68),
            text_primary: Color32::from_rgb(200, 255, 220),
            text_secondary: Color32::from_rgb(120, 180, 140),
            text_muted: Color32::from_rgb(60, 100, 70),
            error: Color32::from_rgb(255, 68, 68),
            success: Color32::from_rgb(68, 255, 136),
            bg: Color32::from_rgb(15, 25, 20),
        }
    }
}

/// A quick-buy button with a preset ADA amount.
pub struct CultureBuy {
    /// ADA amount (whole number).
    pub ada_amount: u64,
    /// Short label shown on the button.
    pub label: String,
}

/// Configuration for the swap modal — caller provides token and DEX details.
pub struct SwapModalConfig {
    /// Display name of the output token (e.g. "Aliens").
    pub token_name: String,
    /// Optional ticker (e.g. "51A").
    pub token_ticker: Option<String>,
    /// Quick-buy preset buttons.
    pub culture_buys: Vec<CultureBuy>,
    /// Theme colors.
    pub theme: SwapModalTheme,
}

// ============================================================================
// Progress (caller provides)
// ============================================================================

/// Swap preview data shown in the modal.
#[derive(Clone)]
pub struct SwapPreviewData {
    /// Estimated tokens the user will receive.
    pub estimated_output: u64,
    /// Price in ADA per output token.
    pub price_per_token: f64,
    /// DEX fee overhead in ADA (e.g. Splash deposit costs).
    pub fee_overhead: f64,
    /// Total cost in ADA (input + fees).
    pub total_cost: f64,
    /// Output token name for display.
    pub output_token_name: String,
}

/// Current progress of the swap, provided by the caller each frame.
pub enum SwapProgress {
    /// No swap in progress — show the input form.
    Idle,
    /// Preview is being fetched.
    PreviewLoading,
    /// Preview is ready to display.
    PreviewReady(SwapPreviewData),
    /// Transaction is being built/signed/submitted.
    Processing { stage: &'static str },
    /// Transaction submitted successfully.
    Success { tx_hash: String },
    /// Something went wrong.
    Error { message: String },
}

// ============================================================================
// Actions (caller handles)
// ============================================================================

/// Actions returned by [`SwapModal::show()`] that the caller must handle.
#[derive(Debug)]
pub enum SwapModalAction {
    /// No action needed.
    None,
    /// User selected an amount (culture button or custom input).
    /// Caller should debounce (for text input) then fetch a preview.
    /// Value is in lovelace.
    AmountChanged(u64),
    /// User changed slippage. Caller should store and re-fetch preview if one is active.
    SlippageChanged(u32),
    /// User clicked Confirm Swap. Caller should execute the swap.
    ConfirmSwap { input_lovelace: u64 },
    /// User clicked "New swap" after success/error. Caller should reset state.
    Reset,
    /// User closed the modal.
    Closed,
}

/// Amount selection mode.
#[derive(Clone, Copy, PartialEq)]
enum AmountSelection {
    /// A culture buy button is selected (index into config.culture_buys).
    CultureBuy(usize),
    /// Custom text input is active.
    Custom,
}

// ============================================================================
// Widget
// ============================================================================

/// Slippage presets in basis points.
const SLIPPAGE_OPTIONS: &[(u32, &str)] = &[(100, "1%"), (500, "5%"), (1000, "10%")];

/// Reusable swap modal widget.
pub struct SwapModal {
    /// Whether the modal is open.
    pub open: bool,
    /// Current text in the custom amount field.
    pub input_text: String,
    /// Current amount selection mode.
    selection: Option<AmountSelection>,
    /// Current slippage in basis points.
    pub slippage_bps: u32,
    /// Cached preview data — preserved through processing/success states.
    last_preview: Option<SwapPreviewData>,
    /// Config provided at creation.
    config: SwapModalConfig,
}

impl SwapModal {
    /// Create a new swap modal with the given config.
    pub fn new(config: SwapModalConfig) -> Self {
        Self {
            open: false,
            input_text: String::new(),
            selection: None,
            slippage_bps: 500,
            last_preview: None,
            config,
        }
    }

    /// Programmatically select a culture buy button by index.
    pub fn select_culture_buy(&mut self, idx: usize) {
        if idx < self.config.culture_buys.len() {
            self.selection = Some(AmountSelection::CultureBuy(idx));
        }
    }

    /// Access the cached preview data (preserved through processing/success states).
    pub fn last_preview(&self) -> Option<&SwapPreviewData> {
        self.last_preview.as_ref()
    }

    /// Current input amount in lovelace, parsed from either culture button or text.
    pub fn input_lovelace(&self) -> Option<u64> {
        match self.selection {
            Some(AmountSelection::CultureBuy(idx)) => self
                .config
                .culture_buys
                .get(idx)
                .map(|cb| cb.ada_amount * 1_000_000),
            Some(AmountSelection::Custom) => self
                .input_text
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|v| *v >= 5.0)
                .map(|v| (v * 1_000_000.0) as u64),
            None => None,
        }
    }

    /// Render the modal. Returns an action the caller must handle.
    ///
    /// Must be called with `ctx` (not inside a panel) so the window floats above all panels.
    pub fn show(&mut self, ctx: &egui::Context, progress: &SwapProgress) -> SwapModalAction {
        if !self.open {
            return SwapModalAction::None;
        }

        let mut action = SwapModalAction::None;
        let mut still_open = true;

        let title = if let Some(ref ticker) = self.config.token_ticker {
            format!("Swap ADA \u{2192} {} ({})", self.config.token_name, ticker)
        } else {
            format!("Swap ADA \u{2192} {}", self.config.token_name)
        };

        egui::Window::new(title)
            .open(&mut still_open)
            .resizable(false)
            .collapsible(false)
            .default_width(340.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(self.config.theme.bg)
                    .inner_margin(16.0),
            )
            .show(ctx, |ui| {
                action = self.draw_content(ui, progress);
            });

        if !still_open {
            self.open = false;
            return SwapModalAction::Closed;
        }

        action
    }

    fn draw_content(&mut self, ui: &mut egui::Ui, progress: &SwapProgress) -> SwapModalAction {
        // Cache preview data so it persists through processing/success/error
        if let SwapProgress::PreviewReady(p) = progress {
            self.last_preview = Some(p.clone());
        }

        match progress {
            SwapProgress::Idle | SwapProgress::PreviewLoading | SwapProgress::PreviewReady(_) => {
                self.draw_form(ui, progress, None)
            }
            SwapProgress::Processing { stage } => {
                // Show the form with cached preview + processing status instead of confirm button
                self.draw_form(ui, progress, Some(stage))
            }
            SwapProgress::Success { tx_hash } => self.draw_success(ui, tx_hash),
            SwapProgress::Error { message } => self.draw_error(ui, message),
        }
    }

    fn draw_form(
        &mut self,
        ui: &mut egui::Ui,
        progress: &SwapProgress,
        processing_stage: Option<&str>,
    ) -> SwapModalAction {
        // Copy theme colors up front (Color32 is Copy) to avoid borrowing self.config
        // through the closures that also need &mut self fields.
        let accent = self.config.theme.accent;
        let bg = self.config.theme.bg;
        let text_secondary = self.config.theme.text_secondary;
        let text_muted = self.config.theme.text_muted;

        let mut action = SwapModalAction::None;

        // Amount buttons — centered row: culture buys + Custom
        ui.add_space(4.0);
        ui.vertical_centered(|ui| {
            ui.horizontal(|ui| {
                // Calculate total width to center
                let btn_count = self.config.culture_buys.len() + 1; // +1 for Custom
                let btn_width = 70.0;
                let spacing = ui.spacing().item_spacing.x;
                let total_width = btn_count as f32 * btn_width + (btn_count as f32 - 1.0) * spacing;
                let avail = ui.available_width();
                if avail > total_width {
                    ui.add_space((avail - total_width) / 2.0);
                }

                // Culture buy buttons
                for (idx, cb) in self.config.culture_buys.iter().enumerate() {
                    let is_selected = self.selection == Some(AmountSelection::CultureBuy(idx));
                    let btn_label = format!("{} ADA", cb.ada_amount);

                    let button = if is_selected {
                        egui::Button::new(RichText::new(&btn_label).color(bg).strong().size(12.0))
                            .fill(accent)
                            .corner_radius(6.0)
                            .min_size(egui::vec2(btn_width, 36.0))
                    } else {
                        egui::Button::new(RichText::new(&btn_label).color(accent).size(12.0))
                            .fill(Color32::TRANSPARENT)
                            .stroke(egui::Stroke::new(1.0, accent))
                            .corner_radius(6.0)
                            .min_size(egui::vec2(btn_width, 36.0))
                    };

                    if ui.add_clickable(button).clicked() {
                        self.selection = Some(AmountSelection::CultureBuy(idx));
                        self.input_text.clear();
                        action = SwapModalAction::AmountChanged(cb.ada_amount * 1_000_000);
                    }
                }

                // Custom button
                let is_custom = self.selection == Some(AmountSelection::Custom);
                let custom_btn = if is_custom {
                    egui::Button::new(RichText::new("Custom").color(bg).strong().size(12.0))
                        .fill(accent)
                        .corner_radius(6.0)
                        .min_size(egui::vec2(btn_width, 36.0))
                } else {
                    egui::Button::new(RichText::new("Custom").color(accent).size(12.0))
                        .fill(Color32::TRANSPARENT)
                        .stroke(egui::Stroke::new(1.0, accent))
                        .corner_radius(6.0)
                        .min_size(egui::vec2(btn_width, 36.0))
                };

                if ui.add_clickable(custom_btn).clicked() {
                    self.selection = Some(AmountSelection::Custom);
                }
            });
        });

        // Custom amount input — only shown when Custom is selected
        if self.selection == Some(AmountSelection::Custom) {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let total_width = 140.0 + 30.0; // input + "ADA" label approx
                let avail = ui.available_width();
                if avail > total_width {
                    ui.add_space((avail - total_width) / 2.0);
                }

                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.input_text)
                        .desired_width(140.0)
                        .font(egui::FontId::monospace(14.0))
                        .hint_text("Amount"),
                );
                ui.label(RichText::new("ADA").color(text_muted).size(12.0));

                if response.changed() {
                    if let Some(lovelace) = self.input_lovelace() {
                        action = SwapModalAction::AmountChanged(lovelace);
                    }
                }
            });
        }

        ui.add_space(8.0);

        // Slippage selector
        ui.horizontal(|ui| {
            ui.label(RichText::new("Slippage:").color(text_secondary).size(11.0));
            for &(bps, label) in SLIPPAGE_OPTIONS {
                let is_selected = self.slippage_bps == bps;
                let btn = if is_selected {
                    egui::Button::new(RichText::new(label).color(bg).strong().size(10.0))
                        .fill(accent)
                        .corner_radius(4.0)
                        .min_size(egui::vec2(36.0, 22.0))
                } else {
                    egui::Button::new(RichText::new(label).color(text_muted).size(10.0))
                        .fill(Color32::TRANSPARENT)
                        .stroke(egui::Stroke::new(1.0, text_muted))
                        .corner_radius(4.0)
                        .min_size(egui::vec2(36.0, 22.0))
                };
                if ui.add_clickable(btn).clicked() && self.slippage_bps != bps {
                    self.slippage_bps = bps;
                    action = SwapModalAction::SlippageChanged(bps);
                }
            }
        });

        ui.add_space(8.0);

        // Preview section — always visible once an amount is selected
        let has_amount = self.selection.is_some();
        let is_loading = matches!(progress, SwapProgress::PreviewLoading);

        // Use live preview if available, otherwise fall back to cached
        let preview_data = if let SwapProgress::PreviewReady(p) = progress {
            Some(p)
        } else {
            self.last_preview.as_ref()
        };

        if has_amount {
            self.draw_preview(ui, preview_data, is_loading);

            ui.add_space(12.0);

            if let Some(stage) = processing_stage {
                // Processing — show status instead of confirm button
                ui.vertical_centered(|ui| {
                    ui.horizontal(|ui| {
                        let total_width = 20.0 + 8.0 + 150.0; // spinner + gap + text approx
                        let avail = ui.available_width();
                        if avail > total_width {
                            ui.add_space((avail - total_width) / 2.0);
                        }
                        ui.spinner();
                        ui.label(RichText::new(stage).color(text_secondary).size(12.0));
                    });
                });
            } else {
                // Confirm button — disabled while loading or no preview
                let can_confirm = preview_data.is_some() && self.input_lovelace().is_some();
                let confirm_btn = ui.add_clickable_sized(
                    [ui.available_width(), 36.0],
                    egui::Button::new(RichText::new("Confirm Swap").color(bg).strong().size(14.0))
                        .fill(if can_confirm {
                            accent
                        } else {
                            accent.gamma_multiply(0.3)
                        })
                        .corner_radius(6.0),
                );
                if can_confirm && confirm_btn.clicked() {
                    if let Some(lovelace) = self.input_lovelace() {
                        action = SwapModalAction::ConfirmSwap {
                            input_lovelace: lovelace,
                        };
                    }
                }
            }
        } else {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("Select an amount to get started")
                        .color(text_muted)
                        .size(10.0),
                );
            });
        }

        action
    }

    fn draw_preview(&self, ui: &mut egui::Ui, preview: Option<&SwapPreviewData>, loading: bool) {
        let theme = &self.config.theme;

        ui.separator();
        ui.add_space(6.0);

        if let Some(p) = preview {
            self.preview_row(
                ui,
                "You receive",
                &format!("~{}", format_amount(p.estimated_output)),
                &p.output_token_name,
                theme.accent,
            );
            self.preview_row(
                ui,
                "Price",
                &format!("{:.6} ADA", p.price_per_token),
                &format!("/{}", p.output_token_name),
                theme.text_primary,
            );
            self.preview_row(
                ui,
                "DEX fees",
                &format!("~{:.1} ADA", p.fee_overhead),
                "",
                theme.text_secondary,
            );
            self.preview_row(
                ui,
                "Total cost",
                &format!("~{:.1} ADA", p.total_cost),
                "",
                theme.text_primary,
            );
        } else {
            // Skeleton rows — pulsing placeholder bars while loading
            let alpha = if loading {
                let t = ui.input(|i| i.time);
                ((t * 3.0).sin() * 0.3 + 0.5) as f32
            } else {
                0.2
            };
            let skel_color = theme.text_muted.gamma_multiply(alpha);

            for label in ["You receive", "Price", "DEX fees", "Total cost"] {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).color(theme.text_muted).size(11.0));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(80.0, 12.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 3.0, skel_color);
                    });
                });
            }

            if loading {
                ui.ctx().request_repaint();
            }
        }

        ui.add_space(6.0);
        ui.separator();
    }

    fn preview_row(
        &self,
        ui: &mut egui::Ui,
        label: &str,
        value: &str,
        suffix: &str,
        value_color: Color32,
    ) {
        let theme = &self.config.theme;
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).color(theme.text_muted).size(11.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if !suffix.is_empty() {
                    ui.label(RichText::new(suffix).color(theme.text_muted).size(11.0));
                }
                ui.label(RichText::new(value).color(value_color).size(12.0));
            });
        });
    }

    fn draw_success(&mut self, ui: &mut egui::Ui, tx_hash: &str) -> SwapModalAction {
        let theme = &self.config.theme;
        let mut action = SwapModalAction::None;

        ui.add_space(12.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new("Order submitted!")
                    .color(theme.success)
                    .strong()
                    .size(14.0),
            );
            ui.add_space(8.0);

            let short = if tx_hash.len() > 20 {
                format!("{}...{}", &tx_hash[..10], &tx_hash[tx_hash.len() - 10..])
            } else {
                tx_hash.to_string()
            };
            ui.hyperlink_to(
                RichText::new(short).color(theme.accent).size(11.0),
                format!("https://cardanoscan.io/transaction/{tx_hash}"),
            );

            ui.add_space(16.0);
            if ui
                .add_clickable(
                    egui::Button::new(RichText::new("New Swap").color(theme.accent).size(12.0))
                        .corner_radius(4.0),
                )
                .clicked()
            {
                self.input_text.clear();
                self.selection = None;
                self.last_preview = None;
                action = SwapModalAction::Reset;
            }
        });
        ui.add_space(12.0);

        action
    }

    fn draw_error(&mut self, ui: &mut egui::Ui, message: &str) -> SwapModalAction {
        let theme = &self.config.theme;
        let mut action = SwapModalAction::None;

        ui.add_space(12.0);
        ui.vertical_centered(|ui| {
            let display = if message.len() > 80 {
                format!("{}...", &message[..77])
            } else {
                message.to_string()
            };
            ui.label(RichText::new(display).color(theme.error).size(11.0));

            ui.add_space(12.0);
            if ui
                .add_clickable(
                    egui::Button::new(RichText::new("Try Again").color(theme.accent).size(12.0))
                        .corner_radius(4.0),
                )
                .clicked()
            {
                action = SwapModalAction::Reset;
            }
        });
        ui.add_space(12.0);

        action
    }
}

/// Format a token amount with K/M suffixes for display.
fn format_amount(amount: u64) -> String {
    if amount >= 1_000_000 {
        format!("{:.1}M", amount as f64 / 1_000_000.0)
    } else if amount >= 1_000 {
        format!("{:.1}K", amount as f64 / 1_000.0)
    } else {
        format!("{amount}")
    }
}
