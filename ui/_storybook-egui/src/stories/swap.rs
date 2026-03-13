use crate::{ACCENT, TEXT_MUTED};

pub fn show(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    swap_modal: &mut egui_widgets::SwapModal,
    progress: &mut egui_widgets::SwapProgress,
) {
    ui.label("Control the swap progress state, then open the modal.");
    ui.add_space(8.0);

    // State control buttons
    ui.label(egui::RichText::new("Progress State").color(ACCENT).strong());
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        if ui.button("Idle").clicked() {
            *progress = egui_widgets::SwapProgress::Idle;
        }
        if ui.button("Preview Loading").clicked() {
            *progress = egui_widgets::SwapProgress::PreviewLoading;
        }
        if ui.button("Preview Ready").clicked() {
            *progress = egui_widgets::SwapProgress::PreviewReady(egui_widgets::SwapPreviewData {
                estimated_output: 614_700,
                price_per_token: 0.000083,
                fee_overhead: 3.5,
                total_cost: 54.5,
                output_token_name: "TestToken".into(),
            });
        }
        if ui.button("Processing").clicked() {
            *progress = egui_widgets::SwapProgress::Processing {
                stage: "Awaiting wallet signature...",
            };
        }
        if ui.button("Success").clicked() {
            *progress = egui_widgets::SwapProgress::Success {
                tx_hash: "abc123def456789012345678901234567890abcdef1234567890abcdef12345678"
                    .into(),
            };
        }
        if ui.button("Error").clicked() {
            *progress = egui_widgets::SwapProgress::Error {
                message: "Insufficient funds for swap order".into(),
            };
        }
    });

    ui.add_space(12.0);

    if ui
        .add(
            egui::Button::new(
                egui::RichText::new("Open Modal")
                    .color(egui::Color32::from_rgb(26, 26, 46))
                    .strong(),
            )
            .fill(ACCENT)
            .corner_radius(4.0),
        )
        .clicked()
    {
        swap_modal.open = true;
    }

    // Render the modal (floating window)
    let action = swap_modal.show(ctx, progress);
    match action {
        egui_widgets::SwapModalAction::None => {}
        ref a => {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(format!("Last action: {a:?}"))
                    .color(TEXT_MUTED)
                    .small(),
            );
        }
    }

    ui.add_space(16.0);
    ui.separator();
    ui.add_space(8.0);
    ui.label(egui::RichText::new("Tips:").color(ACCENT).strong());
    ui.label("\u{2022} Set state to Preview Ready, then open modal to see the full form");
    ui.label("\u{2022} Try Processing to see the spinner animation");
    ui.label("\u{2022} Culture buy buttons: Area 51, Nice, Blaze");
}
