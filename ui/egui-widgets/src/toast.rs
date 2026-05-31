//! `Toast` / `ToastQueue` — transient overlay messages with
//! frame-countdown auto-dismiss. The recurring "Copied to clipboard"
//! / "Refuel submitted" / "Save failed" affordance used everywhere a
//! widget completes an action and the host wants a brief acknowledgement
//! without committing to a status bar.
//!
//! ## Shape
//!
//! - [`ToastQueue`] is a host-owned controller. Put one on your app
//!   state. Push messages onto it as actions resolve (`queue.success(…)`
//!   / `queue.error(…)` / etc.). Call [`show_toasts`] once per paint
//!   from the top of your central panel — it renders the active toasts
//!   as a bottom-right overlay stack, ticks the countdown, and disposes
//!   of expired entries.
//! - [`Toast`] is one message: `kind` + `message` + optional Phosphor
//!   icon override + frames-remaining lifetime. Default duration is
//!   [`DEFAULT_DURATION_FRAMES`] (~3 s at 60 fps).
//! - Auto-dismiss is **frame-counted**, not wall-clock — works the same
//!   in native and wasm with no `js_sys` dependency. The renderer calls
//!   `ctx.request_repaint()` so the countdown keeps turning even when
//!   no input arrives.
//!
//! ## Why not auto-trigger from `IdPill::show`?
//!
//! Kept host-driven so the primitive stays composable: `IdPill` returns
//! `response.copied`, the host decides whether/where to surface it. The
//! common case is a one-liner:
//!
//! ```ignore
//! if IdPill::new("policy", policy_id).show(ui).copied {
//!     toasts.info("policy copied to clipboard");
//! }
//! ```
//!
//! ## Example
//!
//! ```ignore
//! use egui_widgets::{show_toasts, ToastQueue};
//!
//! struct App {
//!     toasts: ToastQueue,
//! }
//!
//! impl eframe::App for App {
//!     fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
//!         egui::CentralPanel::default().show(ctx, |ui| {
//!             // … render panels …
//!             if ui.button("Save").clicked() {
//!                 self.toasts.success("Saved");
//!             }
//!         });
//!         show_toasts(ctx, &mut self.toasts);
//!     }
//! }
//! ```

use std::collections::VecDeque;

use egui::{
    Align, Align2, Area, Color32, Context, CornerRadius, Frame, Id, Layout, Margin, Order,
    RichText, Stroke, Ui,
};

use crate::icons::{install_phosphor_font, PhosphorIcon};

/// ~3 seconds at 60 fps. Used as the default lifetime for toasts pushed
/// through the convenience helpers on [`ToastQueue`].
pub const DEFAULT_DURATION_FRAMES: u32 = 180;

/// Severity tint + default icon for a [`Toast`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    /// Green — completed/confirmed actions.
    Success,
    /// Red — failed actions, validation errors.
    Error,
    /// Amber — non-blocking caution.
    Warning,
    /// Blue — neutral confirmations (clipboard, save, etc.).
    Info,
}

impl ToastKind {
    /// `(fill, stroke, icon_tint, text_tint)` — kept aligned with the
    /// crate's framed-block palette (Chip / IdPill / portal Refuel
    /// toast) so toasts visually belong to the same UI family.
    fn palette(self) -> (Color32, Color32, Color32, Color32) {
        match self {
            ToastKind::Success => (
                Color32::from_rgb(24, 36, 28),
                Color32::from_rgb(60, 100, 70),
                Color32::from_rgb(180, 220, 180),
                Color32::from_rgb(220, 235, 220),
            ),
            ToastKind::Error => (
                Color32::from_rgb(40, 24, 24),
                Color32::from_rgb(110, 60, 60),
                Color32::from_rgb(230, 160, 160),
                Color32::from_rgb(240, 210, 210),
            ),
            ToastKind::Warning => (
                Color32::from_rgb(40, 34, 22),
                Color32::from_rgb(120, 100, 50),
                Color32::from_rgb(235, 210, 140),
                Color32::from_rgb(240, 225, 190),
            ),
            ToastKind::Info => (
                Color32::from_rgb(22, 28, 38),
                Color32::from_rgb(60, 80, 110),
                Color32::from_rgb(160, 190, 230),
                Color32::from_rgb(210, 220, 240),
            ),
        }
    }

    /// Default Phosphor glyph used when [`Toast::icon`] isn't set. The
    /// crate's `PhosphorIcon` enum has no dedicated "info" symbol — the
    /// `Info` kind ships iconless and relies on the blue tint for
    /// affordance.
    fn default_icon(self) -> Option<PhosphorIcon> {
        match self {
            ToastKind::Success => Some(PhosphorIcon::CheckCircle),
            ToastKind::Error => Some(PhosphorIcon::X),
            ToastKind::Warning => Some(PhosphorIcon::Warning),
            ToastKind::Info => None,
        }
    }
}

/// One toast message in the queue.
pub struct Toast {
    /// The body text rendered next to the icon.
    pub message: String,
    /// Severity — drives the palette and default icon.
    pub kind: ToastKind,
    /// Leading Phosphor glyph override. `None` falls back to
    /// [`ToastKind::default_icon`]; pass [`Toast::no_icon`] to suppress
    /// the icon explicitly.
    pub icon: Option<Option<PhosphorIcon>>,
    /// Frames remaining before auto-dismissal. Decremented once per
    /// paint by [`show_toasts`]; the entry drops at 0.
    pub frames_remaining: u32,
}

impl Toast {
    /// Construct with the default icon (per `kind`) and the default
    /// duration. Override either with the builders.
    pub fn new(message: impl Into<String>, kind: ToastKind) -> Self {
        Self {
            message: message.into(),
            kind,
            icon: None,
            frames_remaining: DEFAULT_DURATION_FRAMES,
        }
    }

    /// Override the leading Phosphor glyph (e.g. `Copy` for a
    /// clipboard ack rather than the default `CheckCircle`).
    pub fn icon(mut self, icon: PhosphorIcon) -> Self {
        self.icon = Some(Some(icon));
        self
    }

    /// Suppress the leading icon entirely.
    pub fn no_icon(mut self) -> Self {
        self.icon = Some(None);
        self
    }

    /// Override the auto-dismiss duration in frames (60 ≈ 1 s).
    pub fn duration_frames(mut self, frames: u32) -> Self {
        self.frames_remaining = frames;
        self
    }

    fn resolved_icon(&self) -> Option<PhosphorIcon> {
        self.icon.unwrap_or_else(|| self.kind.default_icon())
    }
}

/// Host-owned toast controller. Place one on app state, push toasts as
/// actions resolve, and call [`show_toasts`] once per paint to surface
/// them.
pub struct ToastQueue {
    toasts: VecDeque<Toast>,
    max_visible: usize,
}

impl Default for ToastQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl ToastQueue {
    /// Empty queue with a default cap of 5 simultaneously-visible
    /// toasts. Older entries drop off the front when the cap is hit.
    pub fn new() -> Self {
        Self {
            toasts: VecDeque::new(),
            max_visible: 5,
        }
    }

    /// Override the max-visible cap. Useful for dense dashboards where
    /// you want a longer stack, or single-shot contexts where 1 is
    /// enough.
    pub fn with_max_visible(mut self, n: usize) -> Self {
        self.max_visible = n.max(1);
        self
    }

    /// Append a fully-built [`Toast`].
    pub fn push(&mut self, toast: Toast) {
        self.toasts.push_back(toast);
        while self.toasts.len() > self.max_visible {
            self.toasts.pop_front();
        }
    }

    /// Push a green confirmation toast with the default duration +
    /// `CheckCircle` icon.
    pub fn success(&mut self, message: impl Into<String>) {
        self.push(Toast::new(message, ToastKind::Success));
    }

    /// Push a red failure toast with the default duration + `X` icon.
    pub fn error(&mut self, message: impl Into<String>) {
        self.push(Toast::new(message, ToastKind::Error));
    }

    /// Push an amber caution toast with the default duration +
    /// `Warning` icon.
    pub fn warning(&mut self, message: impl Into<String>) {
        self.push(Toast::new(message, ToastKind::Warning));
    }

    /// Push a blue neutral toast with the default duration, no icon.
    pub fn info(&mut self, message: impl Into<String>) {
        self.push(Toast::new(message, ToastKind::Info));
    }

    /// `true` when nothing is currently being displayed.
    pub fn is_empty(&self) -> bool {
        self.toasts.is_empty()
    }

    /// Number of live toasts in the queue.
    pub fn len(&self) -> usize {
        self.toasts.len()
    }

    /// Drop every live toast immediately (e.g. on a route change).
    pub fn clear(&mut self) {
        self.toasts.clear();
    }
}

/// Render the queue's active toasts as a bottom-right overlay stack,
/// tick the frame-countdown, and drop expired entries. Call exactly
/// once per paint, **after** the central panel (so the overlay sits on
/// top).
///
/// Drives `ctx.request_repaint()` while the queue is non-empty so the
/// countdown keeps ticking without external input events.
pub fn show_toasts(ctx: &Context, queue: &mut ToastQueue) {
    // Reap expired entries before measuring.
    queue.toasts.retain(|t| t.frames_remaining > 0);
    if queue.toasts.is_empty() {
        return;
    }

    // Some toast paths land before any other widget has installed the
    // Phosphor font (e.g. an error toast from a boot-time fetch).
    install_phosphor_font(ctx);

    // Tick countdowns + keep the paint pump turning so the toast
    // dismisses itself with no external input event.
    for t in queue.toasts.iter_mut() {
        t.frames_remaining = t.frames_remaining.saturating_sub(1);
    }
    ctx.request_repaint();

    // Pin to the bottom-right of the *content* area (inside chrome) so
    // the overlay doesn't drift off-viewport on platforms with custom
    // window decorations.
    let content_rect = ctx.content_rect();
    let anchor = content_rect.right_bottom() + egui::vec2(-16.0, -16.0);

    Area::new(Id::new("egui_widgets_toast_overlay"))
        .order(Order::Foreground)
        .fixed_pos(anchor)
        .pivot(Align2::RIGHT_BOTTOM)
        .interactable(false)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            // bottom_up so newer toasts surface at the bottom of the
            // stack (closest to the user's cursor on a typical click)
            // while older ones float upward and fade out of the way.
            ui.with_layout(Layout::bottom_up(Align::Max), |ui| {
                for toast in queue.toasts.iter() {
                    render_one(ui, toast);
                }
            });
        });
}

fn render_one(ui: &mut Ui, toast: &Toast) {
    let (fill, stroke, icon_col, text_col) = toast.kind.palette();
    Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, stroke))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(icon) = toast.resolved_icon() {
                    ui.label(icon.rich_text(14.0, icon_col));
                }
                ui.label(RichText::new(&toast.message).color(text_col).small());
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_caps_at_max_visible() {
        let mut q = ToastQueue::new().with_max_visible(2);
        q.info("a");
        q.info("b");
        q.info("c");
        assert_eq!(q.len(), 2);
        // Oldest dropped: queue now holds b, c.
        assert_eq!(q.toasts.front().unwrap().message, "b");
        assert_eq!(q.toasts.back().unwrap().message, "c");
    }

    #[test]
    fn default_icons_match_kind() {
        assert_eq!(ToastKind::Success.default_icon(), Some(PhosphorIcon::CheckCircle));
        assert_eq!(ToastKind::Error.default_icon(), Some(PhosphorIcon::X));
        assert_eq!(ToastKind::Warning.default_icon(), Some(PhosphorIcon::Warning));
        assert_eq!(ToastKind::Info.default_icon(), None);
    }

    #[test]
    fn icon_override_takes_precedence_over_default() {
        let t = Toast::new("x", ToastKind::Success).icon(PhosphorIcon::Copy);
        assert_eq!(t.resolved_icon(), Some(PhosphorIcon::Copy));
    }

    #[test]
    fn no_icon_suppresses_default() {
        let t = Toast::new("x", ToastKind::Success).no_icon();
        assert_eq!(t.resolved_icon(), None);
    }
}
