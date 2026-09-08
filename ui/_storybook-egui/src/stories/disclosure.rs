//! Story: `Disclosure` — detail that opens under the row it explains.
//!
//! ```sh
//! node ui/_storybook-egui/tools/cdp-shot.mjs \
//!   "http://127.0.0.1:8095/?nav=0&t=$(date +%s)#/disclosure" 900 700 .tmp/disclosure.png 12000
//! ```
//!
//! Two things to check, and neither shows in a still:
//!
//! - **It grows, not appears.** Open a row and watch the height ease. If the
//!   body pops in at full size the animation is not running.
//! - **Text does not reflow while opening.** The body is measured at its
//!   natural width and clipped; if paragraphs re-wrap during the transition,
//!   something is growing the available width instead of the visible height.
//!
//! The rows here are deliberately plain. The widget's job is the region
//! *under* a row, so a story with elaborate rows would be reviewing the rows.

use egui_widgets::disclosure::Disclosure;

use crate::{ACCENT, TEXT_MUTED};

/// Which row is open — held by the CALLER, which is the whole point: accordion
/// semantics ("only one at a time") fall out of an `Option` rather than a mode
/// flag inside the widget.
#[derive(Default)]
pub struct State {
    open: Option<usize>,
}

const ROWS: [(&str, &str); 4] = [
    (
        "Perp1854",
        "addr1v825u96vkgrr5lke2c36sgvxa72szq4d50wq3jdk4crylncgd0x55",
    ),
    (
        "Perp2214",
        "addr1q83h0r5jnulea5v05qf5w6jgdqyatxguhve9q7ar0rv2df66mgnjlv",
    ),
    (
        "Perp0044",
        "addr1zxdpe859k8mn6u2ewj4rgkcm20duyy6z7xjgsfa4xrann5d0fm4l4j",
    ),
    (
        "Perp4717",
        "addr1qyvtqfc4yf97ea5jswtfy3enaupphjculxzaln5ytp9eqweffkuwh9",
    ),
];

pub fn show(ui: &mut egui::Ui, state: &mut State) {
    ui.label(egui::RichText::new("Disclosure").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Detail that opens beneath the row it explains — eased open, tied \
             to the row by a rule, and anchored so the list does not shove \
             under the pointer. For SHORT detail; long detail belongs in \
             detail_split, beside the content.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    for (i, (name, addr)) in ROWS.iter().enumerate() {
        let open = state.open == Some(i);
        // The row. A real caller draws whatever it likes here — a `TxCard`, a
        // table row — and the disclosure is what follows it.
        let row = ui.add(
            egui::Button::new(egui::RichText::new(*name).color(match open {
                true => ACCENT,
                false => egui_widgets::theme::TEXT_PRIMARY,
            }))
            .min_size(egui::vec2(ui.available_width(), 28.0)),
        );
        if row.clicked() {
            // A second click closes: without this the only way out is to find
            // another row to open.
            state.open = match open {
                true => None,
                false => Some(i),
            };
        }

        // KEYED ON THE ROW'S IDENTITY, not its index — see `Disclosure::new`.
        // Here they coincide; in a paging feed they very much do not.
        let drew = Disclosure::new(*name, open).show(ui, |ui| {
            ui.label(egui::RichText::new("from").color(TEXT_MUTED).small());
            ui.label(
                egui::RichText::new(*addr)
                    .monospace()
                    .color(egui_widgets::theme::TEXT_SECONDARY)
                    .size(10.0),
            );
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                let _ = ui.small_button("copy");
                let _ = ui.small_button("cardanoscan ↗");
            });
        });
        // Trailing space only under a row that actually drew something, so a
        // closed row does not leave a gap where its detail would be.
        ui.add_space(match drew {
            true => 8.0,
            false => 4.0,
        });
    }
}
