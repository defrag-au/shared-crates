//! `Toast` / `ToastQueue` storybook story.

use crate::{ACCENT, TEXT_MUTED};
use egui_widgets::{show_toasts, IdPill, PhosphorIcon, Toast, ToastKind, ToastQueue};

/// Per-story state — owns the queue so toasts persist across paint
/// frames and self-dismiss on the countdown.
pub struct ToastState {
    pub queue: ToastQueue,
    /// Fake completion for the progress demo.
    pub excavation: f32,
    /// A story should show its widget without being clicked first.
    pub seeded: bool,
    /// Backing string for the IdPill demo so we have something to copy.
    pub demo_policy: String,
}

impl Default for ToastState {
    fn default() -> Self {
        Self {
            queue: ToastQueue::new(),
            excavation: 0.0,
            seeded: false,
            demo_policy: "8532f316dd0973a8e2c5b7d0fa194deebd4451aabdfe3a8c2bd45d87a1b".to_string(),
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut ToastState) {
    // Seed a running task so the progress kind is visible on arrival — a
    // story that only renders after a click screenshots as an empty screen.
    if !state.seeded {
        state.seeded = true;
        state.excavation = 0.56;
        state.queue.progress(
            "excavation",
            "scan: 56% of 8,865 chunks",
            Some(state.excavation),
        );
        state.queue.progress("resolve", "naming senders…", None);
    }
    ui.label(egui::RichText::new("Toast").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Transient overlay messages with frame-countdown auto-dismiss. \
             Host owns the queue; push from action handlers and call \
             `show_toasts(ctx, &mut queue)` once per paint to render the \
             bottom-right stack. Lifetime is frame-based (no `js_sys` \
             dependency) so it works the same in native and wasm.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(12.0);

    // ── Kinds ──────────────────────────────────────────────────────────
    ui.label(egui::RichText::new("Kinds").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Four severities drive the palette and (where available) the \
             default Phosphor glyph: Success/CheckCircle (green), \
             Error/X (red), Warning/Warning (amber), Info (blue, no icon).",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui.button("Success").clicked() {
            state.queue.success("Saved successfully");
        }
        if ui.button("Error").clicked() {
            state.queue.error("Save failed: network unreachable");
        }
        if ui.button("Warning").clicked() {
            state.queue.warning("Wallet balance is low");
        }
        if ui.button("Info").clicked() {
            state.queue.info("3 new orders since last refresh");
        }
    });

    ui.add_space(10.0);
    ui.label(
        egui::RichText::new(
            "Progress — background work. Keyed, so repeated pushes replace rather than stack; \
             never auto-dismisses; resolves in place.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        // The same key each click: the toast advances instead of piling up.
        if ui.button("Advance excavation").clicked() {
            state.excavation = (state.excavation + 0.18).min(1.0);
            let pct = (state.excavation * 100.0).round();
            state.queue.progress(
                "excavation",
                format!("scan: {pct:.0}% of 8,865 chunks"),
                Some(state.excavation),
            );
        }
        if ui.button("Indeterminate").clicked() {
            state
                .queue
                .progress("resolve", "naming senders…", None);
        }
        if ui.button("Resolve").clicked() {
            state.excavation = 0.0;
            state.queue.resolve(
                "excavation",
                ToastKind::Success,
                "excavation complete — 3,203 transactions",
            );
            state.queue.dismiss("resolve");
        }
    });

    ui.add_space(16.0);

    // ── Copy-to-clipboard pattern ──────────────────────────────────────
    ui.label(
        egui::RichText::new("IdPill — clipboard acknowledgement")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Common case: confirm a copy from an `IdPill`. The widget \
             returns `response.copied`; the host pushes an info toast. \
             No coupling between primitive and controller.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(6.0);
    if IdPill::new("policy", &state.demo_policy).show(ui).copied {
        state.queue.push(
            Toast::new("policy copied to clipboard", ToastKind::Info).icon(PhosphorIcon::Copy),
        );
    }

    ui.add_space(16.0);

    // ── Builders ───────────────────────────────────────────────────────
    ui.label(
        egui::RichText::new("Custom icon + lifetime")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Build a `Toast` directly to override the icon (e.g. `Copy` for \
             a clipboard ack instead of the default `CheckCircle`) or to \
             extend the auto-dismiss duration for messages that need to \
             linger.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui.button("Long-lived").clicked() {
            state.queue.push(
                Toast::new("This one stays for 10 seconds", ToastKind::Info).duration_frames(600),
            );
        }
        if ui.button("Custom icon").clicked() {
            state
                .queue
                .push(Toast::new("Shipping", ToastKind::Success).icon(PhosphorIcon::Lightning));
        }
        if ui.button("Icon-suppressed").clicked() {
            state
                .queue
                .push(Toast::new("Plain text only", ToastKind::Success).no_icon());
        }
    });

    ui.add_space(16.0);

    // ── Queue state ────────────────────────────────────────────────────
    ui.label(egui::RichText::new("Queue state").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Live count + a clear-all (e.g. on a route change). Default \
             cap is 5 simultaneously-visible toasts; older entries drop \
             off the front of the queue as new ones land.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(format!("Active toasts: {}", state.queue.len()));
        if ui.button("Clear all").clicked() {
            state.queue.clear();
        }
    });

    // The overlay itself is detached — it goes through `ctx` and floats
    // above the storybook scroll. Calling it from inside the story
    // routes it through the same code path a host app would use.
    show_toasts(ui.ctx(), &mut state.queue);
}
