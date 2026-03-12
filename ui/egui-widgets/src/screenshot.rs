use egui::{Color32, Rect, Sense, Stroke, StrokeKind, Vec2};

/// A reusable screenshot-to-clipboard button.
///
/// Renders a small copy icon. On click, captures the specified region of the
/// screen and copies it to the system clipboard. Works on both native (via
/// `arboard`) and WASM (via the browser Clipboard API).
///
/// # Usage
///
/// Store a `ScreenshotButton` alongside your UI state. Each frame, call
/// [`show()`](ScreenshotButton::show) with the `Rect` of the region to capture:
///
/// ```ignore
/// let frame_resp = egui::Frame::new().show(ui, |ui| { /* content */ });
/// screenshot_btn.show(ui, frame_resp.response.rect);
/// ```
pub struct ScreenshotButton {
    /// Region to crop from the next screenshot event.
    pending_rect: Option<Rect>,
    /// Timestamp (seconds) of last successful copy, for feedback flash.
    copied_at: Option<f64>,
}

impl Default for ScreenshotButton {
    fn default() -> Self {
        Self::new()
    }
}

impl ScreenshotButton {
    pub fn new() -> Self {
        Self {
            pending_rect: None,
            copied_at: None,
        }
    }

    /// Render the screenshot button and handle the capture lifecycle.
    ///
    /// `capture_rect` is the UI-space `Rect` of the region to capture (e.g.
    /// from `Frame::show().response.rect`). The button is drawn at the current
    /// cursor position — the caller is responsible for positioning.
    ///
    /// Returns `true` if a copy just completed this frame.
    pub fn show(&mut self, ui: &mut egui::Ui, capture_rect: Rect) -> bool {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        let mut just_copied = false;

        // Step 1: Check for screenshot events from previous frame.
        // IMPORTANT: Extract the image from ctx.input() first, then call
        // copy_image() *outside* the closure. Nesting ctx.copy_image() inside
        // ctx.input() causes a nested RwLock that panics on WASM.
        if self.pending_rect.is_some() {
            let pending = self.pending_rect.take().unwrap();
            let captured_image = ctx.input(|i| {
                i.raw.events.iter().find_map(|event| {
                    if let egui::Event::Screenshot { image, .. } = event {
                        Some(image.clone())
                    } else {
                        None
                    }
                })
            });
            if let Some(image) = captured_image {
                let cropped = image.region(&pending, Some(ctx.pixels_per_point()));
                ctx.copy_image(cropped);
                just_copied = true;
                self.copied_at = Some(now);
                log::info!("[screenshot] Copied region to clipboard");
            } else {
                // Screenshot hasn't arrived yet — re-queue for next frame
                self.pending_rect = Some(pending);
                ctx.request_repaint();
            }
        }

        // Step 2: Determine feedback state
        let showing_feedback = self.copied_at.is_some_and(|t| now - t < 1.5);

        if showing_feedback {
            ctx.request_repaint(); // keep animating until feedback fades
        }

        // Step 3: Draw the button
        let btn_size = Vec2::splat(20.0);
        let (rect, resp) = ui.allocate_exact_size(btn_size, Sense::click());

        // Hover highlight
        if resp.hovered() {
            ui.painter()
                .rect_filled(rect, 3.0, Color32::from_white_alpha(15));
        }

        let clicked = resp.clicked();

        if showing_feedback {
            draw_checkmark(ui, rect);
            resp.on_hover_text("Copied!");
        } else {
            draw_copy_icon(ui, rect);
            resp.on_hover_text("Copy as image");
        }

        // Step 4: Handle click
        if clicked && self.pending_rect.is_none() {
            self.pending_rect = Some(capture_rect);
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            ctx.request_repaint();
        }

        just_copied
    }
}

/// Draw a simple copy/clipboard icon using the painter.
fn draw_copy_icon(ui: &mut egui::Ui, rect: Rect) {
    let p = ui.painter();
    let c = rect.center();
    let s = rect.width() * 0.3;

    let stroke = Stroke::new(1.2, Color32::from_rgb(140, 148, 172));

    // Back rectangle (offset up-left)
    let back = Rect::from_center_size(
        egui::pos2(c.x - s * 0.2, c.y - s * 0.2),
        Vec2::splat(s * 1.4),
    );
    p.rect_stroke(back, 2.0, stroke, StrokeKind::Outside);

    // Front rectangle (offset down-right), filled to occlude back
    let front = Rect::from_center_size(
        egui::pos2(c.x + s * 0.2, c.y + s * 0.2),
        Vec2::splat(s * 1.4),
    );
    p.rect_filled(front, 2.0, Color32::from_rgb(30, 32, 42));
    p.rect_stroke(front, 2.0, stroke, StrokeKind::Outside);
}

/// Draw a checkmark icon for "copied" feedback.
fn draw_checkmark(ui: &mut egui::Ui, rect: Rect) {
    let p = ui.painter();
    let c = rect.center();
    let s = rect.width() * 0.25;

    let color = Color32::from_rgb(158, 206, 106);
    let stroke = Stroke::new(2.0, color);

    let points = [
        egui::pos2(c.x - s, c.y),
        egui::pos2(c.x - s * 0.2, c.y + s * 0.7),
        egui::pos2(c.x + s, c.y - s * 0.5),
    ];
    p.line_segment([points[0], points[1]], stroke);
    p.line_segment([points[1], points[2]], stroke);
}

/// Show a screenshot button overlaid at the top-right corner of a panel rect.
///
/// Convenience for the common pattern of placing the button on a
/// `Frame::show()` response rect.
pub fn show_at_top_right(btn: &mut ScreenshotButton, ui: &mut egui::Ui, panel_rect: Rect) -> bool {
    let btn_size = Vec2::splat(20.0);
    let btn_rect = Rect::from_min_size(
        egui::pos2(panel_rect.max.x - btn_size.x - 4.0, panel_rect.min.y + 4.0),
        btn_size,
    );
    let mut result = false;
    ui.scope_builder(egui::UiBuilder::new().max_rect(btn_rect), |ui| {
        result = btn.show(ui, panel_rect);
    });
    result
}
