//! `detail_split` — a content column beside a detail column, laid out by hand.
//!
//! A main view with a detail pane down the right is one of the most ordinary
//! layouts there is, and in egui it has a trap in it that costs an afternoon.
//!
//! ## The trap: `SidePanel::show_inside` does nothing useful in a top-down `Ui`
//!
//! The obvious construction is a `SidePanel::right(..).show_inside(ui, ..)`
//! followed by a `CentralPanel::default().show_inside(ui, ..)`. It compiles, it
//! looks right, and the panel renders — **floating over the content**, hiding
//! whatever sits against the content's right edge. In a transaction feed that
//! is the amount column, which is the one thing a reader came for.
//!
//! It is not a bug in `SidePanel`. Reserving space works like this
//! (`containers/panel.rs`, `show_inside_dyn`):
//!
//! ```text
//! VerticalSide::Right => cursor.max.x = rect.min.x,
//! ```
//!
//! and the parent is then supposed to notice. But `available_rect_before_wrap`
//! resolves through `Layout::available_from_cursor_max_rect`, whose top-down
//! arm is:
//!
//! ```text
//! Direction::TopDown => {
//!     avail.min.y = cursor.min.y;
//!     avail.max.x = avail.max.x.max(avail.min.x);   // max.x is NEVER read
//! ```
//!
//! A top-down layout takes only `min.y` from the cursor. The horizontal
//! reservation is dropped on the floor, so the following `CentralPanel` takes
//! the full width and paints over the strip the side panel just claimed.
//!
//! The pattern works at the **eframe root**, where panels are siblings managed
//! by the `Context` rather than by a parent `Ui`'s cursor. It does not work
//! inside a `Ui` you are already laying out top-down — which is every
//! `eframe::App::ui` implementation in this workspace.
//!
//! ## What to do instead
//!
//! Split the rectangle yourself. It costs the drag-resize handle and buys a
//! layout that cannot be wrong about which pixels belong to whom:
//!
//! ```no_run
//! # use egui_widgets::detail_split::{detail_split, SplitWidths};
//! # struct App;
//! # fn draw_feed(ui: &mut egui::Ui, app: &mut App) {}
//! # fn draw_detail(ui: &mut egui::Ui, app: &mut App) {}
//! # fn demo(ui: &mut egui::Ui, app: &mut App, selected: bool) {
//! if selected {
//!     detail_split(ui, SplitWidths::default(), app, draw_feed, draw_detail);
//! } else {
//!     // No selection: the content is genuinely full width, not a column
//!     // that happens to fill.
//!     draw_feed(ui, app);
//! }
//! # }
//! ```
//!
//! [`SplitWidths::resolve`] is pure, so the sizing rule is testable without a
//! frame: the detail gets its preferred width or a share of what is there,
//! whichever is smaller, and the content keeps a floor — on a narrow window
//! both give ground rather than the main view becoming unreadable.

use egui::{Align, Layout, Rect, Sense, Ui, UiBuilder, vec2};

/// How the two columns share the available width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SplitWidths {
    /// What the detail column would like, in points.
    pub detail: f32,
    /// The most of the total the detail may take, as a fraction. Stops the
    /// detail from swallowing a narrow window.
    pub detail_max_fraction: f32,
    /// Below this the detail stops shrinking; the content gives way instead.
    pub detail_min: f32,
    /// The content never goes below this.
    pub content_min: f32,
}

impl Default for SplitWidths {
    fn default() -> Self {
        Self {
            detail: 360.0,
            detail_max_fraction: 0.4,
            detail_min: 240.0,
            content_min: 280.0,
        }
    }
}

impl SplitWidths {
    /// Resolve to `(content, detail)` for a given available width and gap.
    ///
    /// Pure — the sizing rule is the part worth testing, and it does not need a
    /// `Ui` to exercise.
    pub fn resolve(&self, available: f32, gap: f32) -> (f32, f32) {
        let usable = (available - gap).max(0.0);
        let detail = self
            .detail
            .min(usable * self.detail_max_fraction)
            .max(self.detail_min);
        let content = (usable - detail).max(self.content_min);
        (content, detail)
    }
}

/// Lay `content` out beside `detail`, with a separator between them.
///
/// See the module docs for why this exists rather than a `SidePanel`.
///
/// `state` is threaded through both halves rather than captured, because in
/// practice both need `&mut` on the same app state — and two closures cannot
/// hold that at once. Passing it in hands each half the borrow in turn.
pub fn detail_split<S, R>(
    ui: &mut Ui,
    widths: SplitWidths,
    state: &mut S,
    content: impl FnOnce(&mut Ui, &mut S) -> R,
    detail: impl FnOnce(&mut Ui, &mut S),
) -> R {
    let gap = ui.spacing().item_spacing.x;
    let full = ui.available_rect_before_wrap();
    let (content_w, detail_w) = widths.resolve(full.width(), gap);

    // FIXED rectangles, not sequential layout.
    //
    // `horizontal_top` + `allocate_ui_with_layout` sizes a region but does not
    // constrain what goes in it: a child wider than its column (a stat strip
    // of N cards, a wide table) reports a wider rect, and the next thing is
    // placed after THAT — shoving the detail column off the right edge of the
    // window. Handing each column an explicit rect and clip means an oversized
    // child is clipped where it sits instead of moving its neighbour.
    let content_rect = Rect::from_min_size(full.min, vec2(content_w, full.height()));
    let detail_rect = Rect::from_min_size(
        egui::pos2(content_rect.max.x + gap, full.min.y),
        vec2(detail_w, full.height()),
    );

    let out = column(ui, content_rect, |ui| content(ui, state));
    ui.painter().vline(
        content_rect.max.x + gap * 0.5,
        full.y_range(),
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
    column(ui, detail_rect, |ui| detail(ui, state));

    // Claim the whole area so whatever follows starts below both columns.
    ui.allocate_rect(
        full.intersect(content_rect.union(detail_rect)),
        Sense::hover(),
    );
    out
}

/// One column, pinned to `rect` and clipped to it.
fn column<R>(ui: &mut Ui, rect: Rect, add: impl FnOnce(&mut Ui) -> R) -> R {
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::top_down(Align::Min)),
    );
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    add(&mut child)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_window_gives_the_detail_its_preference() {
        let (content, detail) = SplitWidths::default().resolve(1400.0, 8.0);
        assert_eq!(detail, 360.0);
        assert_eq!(content, 1400.0 - 8.0 - 360.0);
    }

    #[test]
    fn the_detail_never_swallows_a_narrow_window() {
        // 40% of 700 is 280 — less than the 360 preference, so the fraction
        // wins and the content still gets the larger share.
        let (content, detail) = SplitWidths::default().resolve(708.0, 8.0);
        assert_eq!(detail, 280.0);
        assert!(
            content > detail,
            "the main view stays the main view: {content} vs {detail}"
        );
    }

    #[test]
    fn both_give_ground_before_the_content_becomes_unreadable() {
        // Genuinely cramped: the detail hits its floor and the content hits
        // its own, so the pair overflows rather than the feed collapsing to
        // nothing. Overflow is visible and fixable by resizing; a zero-width
        // column just looks broken.
        let (content, detail) = SplitWidths::default().resolve(400.0, 8.0);
        assert_eq!(detail, 240.0);
        assert_eq!(content, 280.0);
    }

    #[test]
    fn zero_width_does_not_produce_a_negative_column() {
        let (content, detail) = SplitWidths::default().resolve(0.0, 8.0);
        assert!(content >= 0.0 && detail >= 0.0, "{content} {detail}");
    }
}
