//! Scrolling marquee ticker widget.
//!
//! Displays a horizontal sequence of colored text items that scroll right-to-left
//! continuously. When the content fits within the available width, it centers
//! statically instead of scrolling.
//!
//! # Example
//!
//! ```ignore
//! let items = vec![
//!     MarqueeItem { text: "Hello".into(), color: Color32::GREEN },
//!     MarqueeItem { text: "World".into(), color: Color32::WHITE },
//! ];
//! marquee.show(ui, &items);
//! ```

/// Configuration for the marquee widget.
pub struct MarqueeConfig {
    /// Scroll speed in pixels per second.
    pub scroll_speed: f32,
    /// Font for all text.
    pub font: egui::FontId,
    /// Separator between items (e.g. " \u{2022} " for " • ").
    pub separator: String,
    /// Color of separator text.
    pub separator_color: egui::Color32,
    /// Height of the marquee bar in pixels.
    pub height: f32,
}

impl Default for MarqueeConfig {
    fn default() -> Self {
        Self {
            scroll_speed: 40.0,
            font: egui::FontId::monospace(10.0),
            separator: "  \u{2022}  ".into(),
            separator_color: egui::Color32::from_rgb(96, 104, 128),
            height: 14.0,
        }
    }
}

/// A single item to display in the marquee.
pub struct MarqueeItem {
    pub text: String,
    pub color: egui::Color32,
}

/// Scrolling marquee ticker widget.
///
/// Owns persistent scroll state. The caller provides items each frame
/// and handles message lifecycle (TTL, max count, etc.) externally.
pub struct Marquee {
    /// Accumulated scroll position in pixels (wraps at content width).
    scroll_offset: f32,
    /// Last frame time for delta-based scrolling (seconds since app start).
    last_time: Option<f64>,
    /// Configuration.
    config: MarqueeConfig,
}

impl Marquee {
    pub fn new(config: MarqueeConfig) -> Self {
        Self {
            scroll_offset: 0.0,
            last_time: None,
            config,
        }
    }

    /// Render the marquee. Items are displayed in order, separated by the
    /// configured separator.
    ///
    /// When the total text width fits within the viewport, the text is
    /// centered statically (no scrolling). Otherwise it scrolls right-to-left
    /// continuously with seamless looping.
    pub fn show(&mut self, ui: &mut egui::Ui, items: &[MarqueeItem]) {
        if items.is_empty() {
            self.last_time = Some(ui.input(|i| i.time));
            return;
        }

        let now = ui.input(|i| i.time);
        let avail_width = ui.available_width();

        // Build a single sequence LayoutJob
        let single_job = self.build_layout_job(items);

        // Measure the single sequence width
        let mut measure_job = single_job.clone();
        measure_job.wrap = egui::text::TextWrapping {
            max_width: f32::INFINITY,
            ..Default::default()
        };
        let single_galley = ui.painter().layout_job(measure_job);
        let content_width = single_galley.rect.width();

        // Allocate space
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(avail_width, self.config.height),
            egui::Sense::hover(),
        );
        let painter = ui.painter_at(rect);

        if content_width <= avail_width {
            // Static centering — content fits, no scrolling needed
            let x = rect.left() + (avail_width - content_width) / 2.0;
            painter.galley(
                egui::pos2(x, rect.top()),
                single_galley,
                self.config.separator_color,
            );
            self.last_time = Some(now);
            return;
        }

        // Build duplicated LayoutJob for seamless loop
        let mut looped_job = single_job.clone();
        // Append separator between the two copies
        looped_job.append(
            &self.config.separator,
            0.0,
            egui::TextFormat::simple(self.config.font.clone(), self.config.separator_color),
        );
        // Append the second copy
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                looped_job.append(
                    &self.config.separator,
                    0.0,
                    egui::TextFormat::simple(self.config.font.clone(), self.config.separator_color),
                );
            }
            looped_job.append(
                &item.text,
                0.0,
                egui::TextFormat::simple(self.config.font.clone(), item.color),
            );
        }
        looped_job.wrap = egui::text::TextWrapping {
            max_width: f32::INFINITY,
            ..Default::default()
        };
        let looped_galley = ui.painter().layout_job(looped_job);

        // The width of one full cycle (content + trailing separator)
        let cycle_width = content_width + self.measure_separator_width(ui);

        // Delta-time scroll offset accumulation
        if let Some(last) = self.last_time {
            let dt = (now - last) as f32;
            self.scroll_offset += dt * self.config.scroll_speed;
            if cycle_width > 0.0 {
                self.scroll_offset %= cycle_width;
            }
        }
        self.last_time = Some(now);

        // Paint with clipping
        let text_pos = egui::pos2(rect.left() - self.scroll_offset, rect.top());
        painter.galley(text_pos, looped_galley, self.config.separator_color);

        // Request repaint for smooth animation (~30fps)
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));
    }

    /// Build a LayoutJob for a single sequence of items with separators.
    fn build_layout_job(&self, items: &[MarqueeItem]) -> egui::text::LayoutJob {
        let mut job = egui::text::LayoutJob::default();
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                job.append(
                    &self.config.separator,
                    0.0,
                    egui::TextFormat::simple(self.config.font.clone(), self.config.separator_color),
                );
            }
            job.append(
                &item.text,
                0.0,
                egui::TextFormat::simple(self.config.font.clone(), item.color),
            );
        }
        job
    }

    /// Measure the pixel width of the separator string.
    fn measure_separator_width(&self, ui: &egui::Ui) -> f32 {
        let mut job = egui::text::LayoutJob::default();
        job.append(
            &self.config.separator,
            0.0,
            egui::TextFormat::simple(self.config.font.clone(), self.config.separator_color),
        );
        job.wrap = egui::text::TextWrapping {
            max_width: f32::INFINITY,
            ..Default::default()
        };
        ui.painter().layout_job(job).rect.width()
    }
}

impl Default for Marquee {
    fn default() -> Self {
        Self::new(MarqueeConfig::default())
    }
}
