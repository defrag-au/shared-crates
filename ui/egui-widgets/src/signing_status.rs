//! Signing status widget — concurrent signing checklist for the trade desk.
//!
//! Displays a two-row status panel showing whether each party has signed,
//! a Sign button for the local party (if they haven't signed yet), and
//! a submitting state once both parties have signed.

use egui::RichText;

use crate::icons::PhosphorIcon;
use crate::theme;

// ============================================================================
// Types
// ============================================================================

/// The current state of the signing process.
#[derive(Clone, Debug, PartialEq)]
pub enum SigningPhase {
    /// Both parties need to sign. Local party hasn't signed yet.
    AwaitingSignatures,
    /// Local party has signed, waiting for peer.
    WaitingForPeer,
    /// Both signed, TX is being submitted.
    Submitting,
    /// TX submitted, waiting for confirmation.
    Submitted { tx_hash: String },
    /// TX confirmed on-chain.
    Confirmed { tx_hash: String },
}

/// Configuration for the signing status display.
pub struct SigningStatusConfig {
    /// Display label for the local party.
    pub you_label: &'static str,
    /// Display label for the remote party.
    pub them_label: &'static str,
    /// Name/handle of the peer (for "Waiting for $name..." text).
    pub peer_name: String,
    /// Font size for status text.
    pub font_size: f32,
    /// Font size for headings.
    pub heading_size: f32,
}

impl Default for SigningStatusConfig {
    fn default() -> Self {
        Self {
            you_label: "You",
            them_label: "Them",
            peer_name: "peer".into(),
            font_size: 10.0,
            heading_size: 12.0,
        }
    }
}

/// Action emitted by the signing status widget.
#[derive(Debug)]
pub enum SigningAction {
    /// User clicked the "Sign with wallet" button.
    Sign,
    /// User clicked cancel.
    Cancel,
}

/// Response from rendering the signing status.
pub struct SigningStatusResponse {
    pub action: Option<SigningAction>,
}

// ============================================================================
// Widget
// ============================================================================

/// Render the signing status panel.
pub fn show(
    ui: &mut egui::Ui,
    phase: &SigningPhase,
    peer_signed: bool,
    config: &SigningStatusConfig,
) -> SigningStatusResponse {
    crate::install_phosphor_font(ui.ctx());

    let mut action = None;

    let heading = match phase {
        SigningPhase::Submitting => "SUBMITTING",
        SigningPhase::Submitted { .. } => "SUBMITTED",
        SigningPhase::Confirmed { .. } => "CONFIRMED",
        _ => "SIGNING",
    };

    egui::Frame::new()
        .fill(theme::BG_SECONDARY)
        .corner_radius(6.0)
        .inner_margin(12.0)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .show(ui, |ui| {
            ui.label(
                RichText::new(heading)
                    .color(theme::TEXT_SECONDARY)
                    .size(config.heading_size)
                    .strong(),
            );
            ui.add_space(6.0);

            let you_signed = matches!(
                phase,
                SigningPhase::WaitingForPeer
                    | SigningPhase::Submitting
                    | SigningPhase::Submitted { .. }
                    | SigningPhase::Confirmed { .. }
            );

            // You row
            draw_status_row(ui, config.you_label, you_signed, config.font_size);

            // Them row
            draw_status_row(ui, config.them_label, peer_signed, config.font_size);

            ui.add_space(6.0);

            match phase {
                SigningPhase::AwaitingSignatures => {
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Sign with wallet")
                                        .color(theme::BG_PRIMARY)
                                        .size(config.font_size),
                                )
                                .fill(theme::ACCENT_GREEN),
                            )
                            .clicked()
                        {
                            action = Some(SigningAction::Sign);
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("Cancel")
                                            .color(theme::TEXT_MUTED)
                                            .size(config.font_size),
                                    )
                                    .frame(false),
                                )
                                .clicked()
                            {
                                action = Some(SigningAction::Cancel);
                            }
                        });
                    });

                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Both signatures required to execute.")
                            .color(theme::TEXT_MUTED)
                            .size(9.0),
                    );
                }
                SigningPhase::WaitingForPeer => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            RichText::new(format!("Waiting for {} to sign...", config.peer_name))
                                .color(theme::TEXT_SECONDARY)
                                .size(config.font_size),
                        );
                    });

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("Cancel")
                                            .color(theme::TEXT_MUTED)
                                            .size(config.font_size),
                                    )
                                    .frame(false),
                                )
                                .clicked()
                            {
                                action = Some(SigningAction::Cancel);
                            }
                        });
                    });
                }
                SigningPhase::Submitting => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            RichText::new("Submitting transaction...")
                                .color(theme::ACCENT_CYAN)
                                .size(config.font_size),
                        );
                    });
                }
                SigningPhase::Submitted { tx_hash } => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            RichText::new("Awaiting confirmation...")
                                .color(theme::ACCENT_CYAN)
                                .size(config.font_size),
                        );
                    });
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(crate::utils::truncate_hex(tx_hash, 8, 8))
                            .color(theme::TEXT_MUTED)
                            .size(9.0),
                    );
                }
                SigningPhase::Confirmed { tx_hash } => {
                    ui.horizontal(|ui| {
                        PhosphorIcon::CheckCircle.show(
                            ui,
                            config.heading_size,
                            theme::ACCENT_GREEN,
                        );
                        ui.label(
                            RichText::new("Trade complete!")
                                .color(theme::ACCENT_GREEN)
                                .size(config.heading_size),
                        );
                    });
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(crate::utils::truncate_hex(tx_hash, 8, 8))
                            .color(theme::TEXT_MUTED)
                            .size(9.0),
                    );
                }
            }
        });

    SigningStatusResponse { action }
}

fn draw_status_row(ui: &mut egui::Ui, label: &str, signed: bool, font_size: f32) {
    ui.horizontal(|ui| {
        if signed {
            PhosphorIcon::CheckCircle.show(ui, 14.0, theme::ACCENT_GREEN);
            ui.label(
                RichText::new(format!("{label}:"))
                    .color(theme::TEXT_PRIMARY)
                    .size(font_size),
            );
            ui.label(
                RichText::new("signed")
                    .color(theme::ACCENT_GREEN)
                    .size(font_size),
            );
        } else {
            PhosphorIcon::Clock.show(ui, 14.0, theme::TEXT_MUTED);
            ui.label(
                RichText::new(format!("{label}:"))
                    .color(theme::TEXT_PRIMARY)
                    .size(font_size),
            );
            ui.label(
                RichText::new("pending")
                    .color(theme::TEXT_MUTED)
                    .size(font_size),
            );
        }
    });
}
