use egui::{Color32, Pos2, Response, Sense, Stroke, Ui, Vec2};

/// A single band in the distribution chart.
pub struct DistBand {
    /// Display label (used in legend and tooltip).
    pub label: String,
    /// Numeric value — arc sweep is proportional to value / total.
    pub value: f64,
    /// Fill color for this ring's arc.
    pub color: Color32,
}

/// Layout style for arc starting positions.
#[derive(Clone, Copy, PartialEq)]
pub enum ArcLayout {
    /// All arcs start from 12 o'clock.
    Aligned,
    /// Each arc starts where the previous one ended.
    Cascading,
}

/// Concentric orbital rings distribution chart with persistent state.
///
/// Each band is rendered as a thick arc stroke on its own concentric ring,
/// largest on the outside, smallest on the inside. Click to toggle between
/// aligned and cascading arc layouts with smooth animation.
pub struct DistributionChart {
    /// Current layout style.
    layout: ArcLayout,
    /// Animation progress: 0.0 = Aligned, 1.0 = Cascading.
    transition: f32,
    /// Outer radius of the outermost ring.
    pub radius: f32,
    /// Thickness of each ring stroke.
    pub ring_thickness: f32,
    /// Gap between rings.
    pub ring_gap: f32,
}

impl Default for DistributionChart {
    fn default() -> Self {
        Self {
            layout: ArcLayout::Aligned,
            transition: 0.0,
            radius: 80.0,
            ring_thickness: 6.0,
            ring_gap: 4.0,
        }
    }
}

impl DistributionChart {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn radius(&mut self, r: f32) -> &mut Self {
        self.radius = r;
        self
    }

    pub fn ring_thickness(&mut self, t: f32) -> &mut Self {
        self.ring_thickness = t;
        self
    }

    pub fn ring_gap(&mut self, g: f32) -> &mut Self {
        self.ring_gap = g;
        self
    }

    /// Draw the chart and return the overall response.
    pub fn show(&mut self, ui: &mut Ui, bands: &[DistBand]) -> Response {
        let size = Vec2::splat(self.radius * 2.0);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());

        // Handle click to toggle layout
        if response.clicked() {
            self.layout = match self.layout {
                ArcLayout::Aligned => ArcLayout::Cascading,
                ArcLayout::Cascading => ArcLayout::Aligned,
            };
        }

        // Animate transition
        let target = match self.layout {
            ArcLayout::Aligned => 0.0_f32,
            ArcLayout::Cascading => 1.0,
        };
        let speed = 3.0; // transition speed (per second)
        let dt = ui.input(|i| i.stable_dt).min(0.1);
        let diff = target - self.transition;
        if diff.abs() > 0.001 {
            self.transition += diff.signum() * speed * dt;
            self.transition = self.transition.clamp(0.0, 1.0);
            ui.ctx().request_repaint();
        } else {
            self.transition = target;
        }

        if !ui.is_rect_visible(rect) {
            return response;
        }

        let center = rect.center();
        let total: f64 = bands.iter().map(|b| b.value).sum();

        if total <= 0.0 {
            return response;
        }

        let painter = ui.painter_at(rect);

        // Sort bands by value descending (largest = outermost ring)
        let mut sorted_indices: Vec<usize> = (0..bands.len()).collect();
        sorted_indices.sort_by(|&a, &b| {
            bands[b]
                .value
                .partial_cmp(&bands[a].value)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let base_angle: f32 = -std::f32::consts::FRAC_PI_2; // 12 o'clock
        let mut cascade_offset: f32 = 0.0; // accumulated sweep for cascading mode

        let hover_pos = response.hover_pos();
        let mut hovered_band: Option<usize> = None;

        for (ring_idx, &band_idx) in sorted_indices.iter().enumerate() {
            let band = &bands[band_idx];
            if band.value <= 0.0 {
                continue;
            }

            let ring_step = self.ring_thickness + self.ring_gap;
            let mid_r = self.radius - (ring_idx as f32 * ring_step) - self.ring_thickness / 2.0;

            if mid_r < self.ring_thickness / 2.0 {
                continue;
            }

            let fraction = (band.value / total) as f32;
            let sweep = fraction * std::f32::consts::TAU;

            // Interpolate start angle between aligned (base) and cascading (base + offset)
            let start_angle = base_angle + cascade_offset * self.transition;

            // Draw faint full orbit circle (track)
            let track_color = band.color.gamma_multiply(0.15);
            painter.circle_stroke(center, mid_r, Stroke::new(self.ring_thickness, track_color));

            // Check hover
            if let Some(pos) = hover_pos {
                let dx = pos.x - center.x;
                let dy = pos.y - center.y;
                let dist = dx.hypot(dy);
                let half_t = self.ring_thickness / 2.0;
                if dist >= (mid_r - half_t) && dist <= (mid_r + half_t) {
                    let mut angle = dy.atan2(dx) - start_angle;
                    if angle < 0.0 {
                        angle += std::f32::consts::TAU;
                    }
                    if angle <= sweep {
                        hovered_band = Some(band_idx);
                    }
                }
            }

            let is_hovered = hovered_band == Some(band_idx);
            let color = if is_hovered {
                brighten(band.color, 0.3)
            } else {
                band.color
            };

            // Draw the arc as a polyline stroke
            let points_per_radian = 30.0_f32;
            let n_points = ((sweep * points_per_radian) as usize).max(2);

            let points: Vec<Pos2> = (0..=n_points)
                .map(|j| {
                    let t = j as f32 / n_points as f32;
                    let a = start_angle + sweep * t;
                    Pos2::new(center.x + mid_r * a.cos(), center.y + mid_r * a.sin())
                })
                .collect();

            painter.add(egui::Shape::line(
                points,
                Stroke::new(self.ring_thickness, color),
            ));

            cascade_offset += sweep;
        }

        // Tooltip for hovered band
        if let Some(idx) = hovered_band {
            let band = &bands[idx];
            let pct = band.value / total * 100.0;
            let tooltip = format!("{}: {} ({:.1}%)", band.label, format_value(band.value), pct);
            response.clone().on_hover_text(tooltip);
        }

        response
    }
}

/// Draw a single legend row: colored dot + label + right-aligned value.
pub fn legend_row(ui: &mut Ui, color: Color32, label: &str, value: &str) {
    ui.horizontal(|ui| {
        let dot_size = 8.0;
        let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(dot_size), Sense::hover());
        ui.painter()
            .circle_filled(dot_rect.center(), dot_size / 2.0, color);

        ui.label(
            egui::RichText::new(label)
                .color(Color32::from_rgb(120, 180, 140))
                .small(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(value).color(color).small());
        });
    });
}

/// Brighten a color by blending toward white.
fn brighten(color: Color32, amount: f32) -> Color32 {
    let r = color.r() as f32 + (255.0 - color.r() as f32) * amount;
    let g = color.g() as f32 + (255.0 - color.g() as f32) * amount;
    let b = color.b() as f32 + (255.0 - color.b() as f32) * amount;
    Color32::from_rgb(r as u8, g as u8, b as u8)
}

/// Format a large number for display (e.g. 3500000 → "3.5M").
pub fn format_value(v: f64) -> String {
    if v >= 1_000_000.0 {
        format!("{:.1}M", v / 1_000_000.0)
    } else if v >= 1_000.0 {
        format!("{:.1}K", v / 1_000.0)
    } else {
        format!("{:.0}", v)
    }
}
