//! Story: `ErrorNote` — distils ugly machine error blobs (Debug-wrapped,
//! escaped-JSON submit errors) down to the human reason + a "show raw" toggle.

use egui_widgets::ErrorNote;

use crate::{ACCENT, TEXT_MUTED};

pub fn show(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Error Note").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Engine/submit errors arrive as Rust-Debug blobs around triple-escaped \
             JSON. ErrorNote de-escapes them, surfaces the deepest natural-language \
             reason + any HTTP status, and tucks the full text behind \"show raw\".",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    // (caption, raw error)
    let cases: [(&str, &str); 4] = [
        (
            "Mempool: inputs already spent (the bug-report screenshot)",
            r#"submit_transaction: Http(Custom("Transaction submission failed (status 400): {\"contents\":{\"contents\":{\"contents\":{\"era\":\"ShelleyBasedEraConway\",\"error\":[{\"ConwayMempoolFailure \\\"All inputs are spent. Transaction has probably already been included\\\"\"}],\"kind\":\"ShelleyTxValidationError\"},\"tag\":\"TxValidationErrorInCardanoMode\"},\"tag\":\"TxCmdTxSubmitValidationError\"},\"tag\":\"TxSubmitFail\"}"))"#,
        ),
        (
            "Validation: value not conserved",
            r#"submit_transaction: Http(Custom("Transaction submission failed (status 400): {\"error\":[{\"ConwayUtxowFailure \\\"Value not conserved. Consumed 4000000, produced 4200000\\\"\"}],\"tag\":\"TxSubmitFail\"}"))"#,
        ),
        (
            "Plain reason (no JSON)",
            "collection sold out before this order could be filled",
        ),
        (
            "Underfunded wallet",
            r#"mint failed: Http(Custom("Insufficient funds: wallet balance 1.2 ADA below the 5.5 ADA lane budget"))"#,
        ),
    ];

    for (caption, raw) in cases {
        ui.label(egui::RichText::new(caption).small().strong());
        ui.add_space(2.0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ErrorNote::new(raw).show(ui);
        });
        ui.add_space(12.0);
    }
}
