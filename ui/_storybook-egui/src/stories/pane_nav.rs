//! `PaneNavBar` storybook story — the shell nav for an app made of panes.
//!
//! The cases worth looking at side by side are the ones that are easy to get
//! wrong by eye: a locked entry next to available ones, a nav that hides
//! itself, and the constrained-column layout that made a side panel clip in
//! the first place.

use crate::{ACCENT, TEXT_MUTED};
use egui_widgets::{PaneNavBar, PaneNavEntry, PhosphorIcon};

pub struct PaneNavState {
    pub selected: u64,
    pub locked_selected: u64,
    pub narrow_selected: u64,
    /// Shared across every rung of the width ladder, so selecting on one shows
    /// the selected state at every width at once.
    pub ladder_selected: u64,
    /// Width of the constrained-column demo, so the wrap behaviour can be
    /// dragged rather than imagined.
    pub column_width: f32,
}

impl Default for PaneNavState {
    fn default() -> Self {
        Self {
            selected: 0,
            locked_selected: 0,
            narrow_selected: 0,
            ladder_selected: 1,
            column_width: 420.0,
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut PaneNavState) {
    ui.label(egui::RichText::new("PaneNavBar").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Shell nav for an app made of capability panes. Persistent \
             selection (unlike ButtonGroup, which is fire-and-forget), and an \
             entry the reader is not entitled to renders LOCKED WITH ITS \
             REASON rather than vanishing — \"you may not, and here is why\" \
             is a different message from \"this does not exist\".",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    // ── 1. The ordinary case ──────────────────────────────────────────
    section(ui, "Two panes, both available");
    let resp = PaneNavBar::new(state.selected)
        .add(PaneNavEntry::new(0, "Clients"))
        .add(PaneNavEntry::new(1, "Gateway"))
        .show(ui);
    if let Some(id) = resp.selected {
        state.selected = id;
    }
    ui.label(
        egui::RichText::new(format!("selected = {}", state.selected))
            .color(TEXT_MUTED)
            .small(),
    );

    ui.add_space(16.0);

    // ── 2. Locked entry ───────────────────────────────────────────────
    section(ui, "One locked — hover it for the reason");
    ui.label(
        egui::RichText::new(
            "The reason is the entitlement's OWN locked_hint, not a sentence \
             written at the call site, so what a reader is told matches what \
             the backend enforces. Locking is never the control — a locked \
             pane must still be refused by its backend.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    let resp = PaneNavBar::new(state.locked_selected)
        .add(PaneNavEntry::new(0, "Clients").icon(PhosphorIcon::User))
        .add(
            PaneNavEntry::new(1, "Gateway")
                .icon(PhosphorIcon::Lightning)
                .locked("Gateway operator access — granted by Augminted"),
        )
        .add(
            PaneNavEntry::new(2, "Custody")
                .icon(PhosphorIcon::Lock)
                .locked("Not enabled for this account yet"),
        )
        .show(ui);
    if let Some(id) = resp.selected {
        state.locked_selected = id;
    }

    ui.add_space(16.0);

    // ── 3. Hides itself ───────────────────────────────────────────────
    section(ui, "One destination — draws nothing");
    ui.label(
        egui::RichText::new(
            "A nav offering one destination is furniture. Nothing renders \
             between this line and the next, and the response says shown = \
             false so the caller skips its own spacing too.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    let resp = PaneNavBar::new(0)
        .add(PaneNavEntry::new(0, "Clients"))
        .show(ui);
    ui.label(
        egui::RichText::new(format!("↑ nothing drew · shown = {}", resp.shown))
            .color(TEXT_MUTED)
            .small(),
    );

    ui.add_space(10.0);
    // Caption BEFORE the widget: the previous block draws nothing, so a
    // caption after this one would appear to belong to the empty space above
    // it rather than to the strip below.
    ui.label(
        egui::RichText::new("…and the same nav with always_show(true):")
            .color(TEXT_MUTED)
            .small(),
    );
    let resp = PaneNavBar::new(0)
        .add(PaneNavEntry::new(0, "Clients"))
        .always_show(true)
        .show(ui);
    ui.label(
        egui::RichText::new(format!("shown = {}", resp.shown))
            .color(TEXT_MUTED)
            .small(),
    );

    ui.add_space(16.0);

    // ── 4. The layout that broke ──────────────────────────────────────
    section(ui, "Inside a constrained column (the real shell)");
    ui.label(
        egui::RichText::new(
            "This is why the nav is a horizontal strip and not an \
             egui::Panel. A Panel claims a full region; a shell whose content \
             sits in a centred, width-limited scroll column has no full \
             region to give it, so the panel gets squeezed and clips. Drag \
             the width down until the strip wraps — it spills onto a second \
             row rather than pushing destinations off the edge where they \
             cannot be reached.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add(egui::Slider::new(&mut state.column_width, 140.0..=700.0).text("column width"));
    ui.add_space(4.0);

    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_max_width(state.column_width);
        let resp = PaneNavBar::new(state.narrow_selected)
            .add(PaneNavEntry::new(0, "Clients"))
            .add(PaneNavEntry::new(1, "Gateway"))
            .add(PaneNavEntry::new(2, "Custody"))
            .add(PaneNavEntry::new(3, "Reports").locked("Not enabled yet"))
            .show(ui);
        if let Some(id) = resp.selected {
            state.narrow_selected = id;
        }
        ui.separator();
        ui.label("…pane content would render here, in the same column.");
    });

    ui.add_space(16.0);

    // ── 5. The width ladder ───────────────────────────────────────────
    //
    // The slider above is for poking at; this is for REVIEWING. Narrow
    // behaviour that you have to drag a slider to reach is narrow behaviour
    // nobody looks at — the nav above shipped into a phone layout unexamined
    // because checking it meant three manual steps. Every width at once means
    // one screenshot answers "does this work on a phone".
    section(ui, "Width ladder — every breakpoint at once");
    ui.label(
        egui::RichText::new(
            "The real five-destination nav from the collection detail page, at \
             the widths that actually occur. Read down: the point is where the \
             strip stops being one row, and whether what it becomes is usable \
             with a thumb.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(6.0);

    for (width, note) in WIDTH_LADDER {
        ui.label(
            egui::RichText::new(format!("{width:.0}pt — {note}"))
                .color(TEXT_MUTED)
                .small(),
        );
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_max_width(*width);
            ui.set_min_width(*width);
            // Apply the COMPACT touch sizing the phone actually gets, locally.
            // Without this the ladder renders desktop-sized buttons at phone
            // widths and reports that everything fits — a rung that lies about
            // the one thing it exists to check. Set here rather than via
            // `viewport::apply_touch_sizing`, which writes the global style and
            // would resize the whole storybook.
            let bp = egui_widgets::Breakpoint::Compact;
            ui.spacing_mut().interact_size.y = bp.min_touch();
            ui.spacing_mut().button_padding = bp.button_padding();
            let resp = PaneNavBar::new(state.ladder_selected)
                .add(PaneNavEntry::new(0, "Browse").icon(PhosphorIcon::List))
                .add(PaneNavEntry::new(1, "Market").icon(PhosphorIcon::Coins))
                .add(PaneNavEntry::new(2, "Traits").icon(PhosphorIcon::Star))
                .add(PaneNavEntry::new(3, "Sell").icon(PhosphorIcon::Handshake))
                .add(PaneNavEntry::new(4, "Holders").icon(PhosphorIcon::User))
                .show(ui);
            if let Some(id) = resp.selected {
                state.ladder_selected = id;
            }
        });
        ui.add_space(6.0);
    }
}

/// Widths a real reader actually arrives at, narrow → wide.
///
/// Not a smooth sweep: these are the devices and the breakpoint edges, so a
/// regression shows up against a width someone is holding rather than against
/// an arbitrary number.
const WIDTH_LADDER: &[(f32, &str)] = &[
    (320.0, "iPhone SE, the narrowest phone still in use"),
    (390.0, "iPhone 15 / Pixel 8 — the common case"),
    (430.0, "Pro Max"),
    (560.0, "large phone landscape / split window"),
    (
        700.0,
        "the Compact ceiling — a side panel starts to fit here",
    ),
];

fn section(ui: &mut egui::Ui, title: &str) {
    ui.label(egui::RichText::new(title).strong());
    ui.add_space(4.0);
}
