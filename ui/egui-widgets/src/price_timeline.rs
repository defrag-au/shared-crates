//! Price timeline widget — a time-axis scatter of realized prices (sales,
//! offer fills) with optional reference lines (current floor / listing price)
//! and a reference band (e.g. a realized p10–p90 range).
//!
//! The one chart in the family with a REAL x-axis: points are positioned by
//! timestamp, not equally spaced (see `sparkline` for the equally-spaced
//! variant). Handles both sparse asset histories (1–20 points, optionally
//! connected) and dense collection scatters (hundreds of points, auto-shrunk
//! marks, optional log y for outlier-heavy price data).
//!
//! Domain-free by design: values are plain `f64`s, hover reports point
//! *indices* and the caller renders its own tooltip rows.
//!
//! Interaction: pinch / ctrl+scroll zooms the time axis around the cursor
//! (plain scroll stays with any enclosing ScrollArea), drag or horizontal
//! scroll pans while zoomed, and double-click resets (animated). Clicking
//! near points PINS the tooltip as an
//! interactive popup so its links/buttons can actually be clicked; Esc,
//! clicking empty plot, or changing the view unpins. Zoom + pin state
//! persist in egui memory per widget id.

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Vec2};

// ============================================================================
// Public types
// ============================================================================

/// Mark shape for a point — callers typically map an event kind onto this
/// (e.g. sale / offer-accept / collection-offer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointShape {
    Circle,
    Diamond,
    Square,
}

/// Visual weight of a point: `Faint` for background context (e.g. the rest of
/// the collection behind one asset's history), `Highlight` for the subject.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointEmphasis {
    Normal,
    Faint,
    Highlight,
}

/// One plotted point.
pub struct TimelinePoint {
    /// Unix seconds — the real x position.
    pub timestamp_secs: i64,
    /// Y value in domain units (e.g. ADA).
    pub value: f64,
    pub color: Color32,
    pub shape: PointShape,
    pub emphasis: PointEmphasis,
}

/// A horizontal reference line (e.g. "current floor", "your listing").
pub struct ReferenceLine {
    pub value: f64,
    /// Short label painted at the right edge of the line.
    pub label: String,
    pub color: Color32,
    pub dashed: bool,
}

/// A horizontal reference band (e.g. a realized p10–p90 range).
pub struct ReferenceBand {
    pub low: f64,
    pub high: f64,
    /// Fill color — use a low-alpha color.
    pub fill: Color32,
}

/// Y-axis scale selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogMode {
    Off,
    /// Log scale when the data spans more than ~20x (price scatters with a
    /// premium tail), linear otherwise.
    Auto,
    On,
}

/// Appearance configuration.
pub struct PriceTimelineConfig {
    /// Chart height in pixels.
    pub height: f32,
    /// "Now" in unix seconds — injected so stories/tests are deterministic.
    pub now_secs: i64,
    /// X domain: `[now - window_secs, now]`. `0` fits the data extent.
    pub window_secs: i64,
    pub log_y: LogMode,
    /// Draw a connecting polyline through the points in time order (sparse
    /// asset histories read better connected). Color of the line.
    pub connect: Option<Color32>,
    /// Base mark radius; automatically shrunk for dense scatters.
    pub point_radius: f32,
    pub background: Color32,
    pub grid_color: Color32,
    pub label_color: Color32,
}

impl Default for PriceTimelineConfig {
    fn default() -> Self {
        Self {
            height: 140.0,
            now_secs: 0,
            window_secs: 0,
            log_y: LogMode::Auto,
            connect: None,
            point_radius: 3.0,
            background: Color32::from_rgb(30, 32, 42),
            grid_color: Color32::from_rgb(52, 56, 72),
            label_color: Color32::from_rgb(140, 145, 165),
        }
    }
}

/// Result: hover carries the indices of nearby points (nearest first) so the
/// caller can render a tooltip from its own row data. A click PINS the current
/// hover set: [`Self::show_tooltip`] then renders it as an interactive popup
/// (buttons/links inside work — a plain hover tooltip vanishes before the
/// pointer can reach it). Esc, clicking empty plot, or zoom/pan unpins.
pub struct PriceTimelineResponse {
    pub response: egui::Response,
    /// Plot area (inside the axis gutters), for custom overlays.
    pub plot_rect: Rect,
    /// Indices into the `points` slice near the cursor, nearest first.
    pub hovered: Option<Vec<usize>>,
    /// Anchor for a positioned tooltip; present when `hovered` is.
    pub tooltip_anchor: Option<Pos2>,
    /// Point indices pinned by a click — render interactive content for these.
    pub pinned: Option<Vec<usize>>,
    /// Anchor of the pinned popup.
    pub pinned_anchor: Option<Pos2>,
    /// Which member of the active tooltip set (pinned if pinned, else
    /// hovered) is focused — the scroll wheel cycles this while a tooltip is
    /// showing (the scroll is consumed, so the page doesn't move). Callers
    /// typically expand `set[focus]` and compact the rest.
    pub focus: usize,
}

impl PriceTimelineResponse {
    /// True when a click has pinned the tooltip open — the caller should
    /// include its interactive affordances (links, buttons) in the content.
    pub fn is_pinned(&self) -> bool {
        self.pinned.is_some()
    }

    /// Show the tooltip: the pinned interactive popup when a selection is
    /// pinned, else a transient hover tooltip. Returns `None` when there is
    /// nothing to show.
    pub fn show_tooltip(&self, add_contents: impl FnOnce(&mut egui::Ui, &[usize])) -> Option<()> {
        if let (Some(indices), Some(anchor)) = (self.pinned.as_ref(), self.pinned_anchor) {
            let ctx = self.response.ctx.clone();
            let area = egui::Area::new(self.response.id.with("pinned_popup"))
                .fixed_pos(anchor)
                .order(egui::Order::Foreground)
                .constrain(true)
                .show(&ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| add_contents(ui, indices));
                });
            // Remember where the popup is so next frame's scroll handling can
            // keep cycling (and hold the page still) while the pointer is
            // over the popup itself, not just the plot.
            ctx.data_mut(|d| {
                d.insert_temp(self.response.id.with("popup_rect"), area.response.rect);
            });
            return Some(());
        }
        let hovered = self.hovered.as_ref()?;
        let anchor = self.tooltip_anchor?;
        egui::Tooltip::always_open(
            self.response.ctx.clone(),
            self.response.layer_id,
            self.response.id,
            anchor,
        )
        .show(|ui| add_contents(ui, hovered));
        Some(())
    }
}

// ============================================================================
// Constants
// ============================================================================

/// Half-width (px) of the vertical time-column selected on hover: points whose
/// x (time) is within this band of the cursor are picked regardless of price
/// (y), so moving horizontally through time selects the whole column of trades
/// at that instant rather than a blob around the pointer.
const HOVER_X_BAND: f32 = 6.0;

/// Point count past which marks start shrinking.
const DENSE_THRESHOLD: usize = 200;

/// Log scale kicks in (in `Auto`) when max/min exceeds this.
const AUTO_LOG_RATIO: f64 = 20.0;

const CROSSHAIR_COLOR: Color32 = Color32::from_rgb(160, 160, 180);
const HIGHLIGHT_RING: Color32 = Color32::from_rgb(240, 240, 255);

/// Left gutter for y tick labels / right gutter for reference-line labels.
const GUTTER_LEFT: f32 = 6.0;
const GUTTER_BOTTOM: f32 = 16.0;

// ============================================================================
// Drawing
// ============================================================================

/// Draw the timeline and return hover state.
pub fn show(
    ui: &mut egui::Ui,
    points: &[TimelinePoint],
    lines: &[ReferenceLine],
    bands: &[ReferenceBand],
    config: &PriceTimelineConfig,
) -> PriceTimelineResponse {
    let width = ui.available_width().max(120.0);
    let (outer_rect, response) =
        ui.allocate_exact_size(Vec2::new(width, config.height), Sense::click_and_drag());
    let plot_rect = Rect::from_min_max(
        Pos2::new(outer_rect.min.x + GUTTER_LEFT, outer_rect.min.y + 4.0),
        Pos2::new(outer_rect.max.x - 4.0, outer_rect.max.y - GUTTER_BOTTOM),
    );
    let painter = ui.painter().with_clip_rect(outer_rect);
    painter.rect_filled(outer_rect, 4.0, config.background);

    // ---- domains --------------------------------------------------------
    let now = if config.now_secs > 0 {
        config.now_secs
    } else {
        points.iter().map(|p| p.timestamp_secs).max().unwrap_or(0)
    };
    let (base_min, base_max) = if config.window_secs > 0 {
        (now - config.window_secs, now)
    } else {
        let lo = points.iter().map(|p| p.timestamp_secs).min().unwrap_or(0);
        let hi = points
            .iter()
            .map(|p| p.timestamp_secs)
            .max()
            .unwrap_or(1)
            .max(lo + 1);
        // Pad the fitted extent so edge points aren't clipped.
        let pad = ((hi - lo) / 20).max(60);
        (lo - pad, hi + pad)
    };

    // ---- zoom / pan (state keyed on the widget id) ----------------------
    // Zoom = pinch or ctrl+scroll anchored at the cursor (those gestures
    // arrive as `zoom_delta`, so an enclosing ScrollArea keeps plain scroll);
    // drag pans; double-click resets. The window is stored as fractions of
    // the base domain so it survives data refreshes.
    let zoom_id = response.id.with("zoom_window");
    let (mut z_lo, mut z_hi) = ui
        .data_mut(|d| d.get_temp::<(f32, f32)>(zoom_id))
        .unwrap_or((0.0, 1.0));
    // The view changing under a pinned tooltip would leave it dangling over
    // the wrong points — any zoom/pan dismisses the pin (tracked here).
    let mut view_changed = false;
    let cursor_in_plot = response.hover_pos().filter(|pos| plot_rect.contains(*pos));
    if let Some(cursor) = cursor_in_plot {
        let zd = ui.input(|i| i.zoom_delta());
        if (zd - 1.0).abs() > 1e-3 {
            let anchor = z_lo + (cursor.x - plot_rect.left()) / plot_rect.width() * (z_hi - z_lo);
            (z_lo, z_hi) = zoom_window(z_lo, z_hi, anchor, zd);
            view_changed = true;
        }
    }
    if response.dragged() && plot_rect.width() > 0.0 && response.drag_delta().x != 0.0 {
        let dx = response.drag_delta().x / plot_rect.width() * (z_hi - z_lo);
        (z_lo, z_hi) = pan_window(z_lo, z_hi, -dx);
        view_changed = true;
    }
    // Horizontal scroll (trackpad swipe / shift+wheel) also pans. Only
    // consumed while zoomed — at full extent there's nothing to pan into,
    // and the scroll stays available to any horizontally-scrollable parent.
    if cursor_in_plot.is_some() && (z_lo, z_hi) != (0.0, 1.0) && plot_rect.width() > 0.0 {
        let dx = ui.input_mut(|i| std::mem::take(&mut i.smooth_scroll_delta.x));
        if dx != 0.0 {
            let frac = dx / plot_rect.width() * (z_hi - z_lo);
            (z_lo, z_hi) = pan_window(z_lo, z_hi, -frac);
            view_changed = true;
        }
    }
    if response.double_clicked() {
        (z_lo, z_hi) = (0.0, 1.0);
        view_changed = true;
    }
    let zoomed = (z_lo, z_hi) != (0.0, 1.0);
    ui.data_mut(|d| {
        if zoomed {
            d.insert_temp(zoom_id, (z_lo, z_hi));
        } else {
            d.remove_temp::<(f32, f32)>(zoom_id);
        }
    });
    // The stored window is the TARGET; the rendered window eases toward it so
    // reset (and gesture steps) glide instead of snapping. Input math above
    // always uses the target, keeping anchors exact.
    let z_lo_r = ui
        .ctx()
        .animate_value_with_time(zoom_id.with("anim_lo"), z_lo, 0.2);
    let z_hi_r = ui
        .ctx()
        .animate_value_with_time(zoom_id.with("anim_hi"), z_hi, 0.2)
        .max(z_lo_r + 0.001);
    let base_span = (base_max - base_min).max(1) as f64;
    let x_min = base_min + (base_span * z_lo_r as f64) as i64;
    let x_max = (base_min + (base_span * z_hi_r as f64) as i64).max(x_min + 1);
    let x_span = (x_max - x_min).max(1) as f64;
    if zoomed {
        painter.text(
            Pos2::new(plot_rect.max.x - 2.0, plot_rect.min.y + 2.0),
            Align2::RIGHT_TOP,
            "zoomed \u{00b7} double-click to reset",
            FontId::proportional(8.0),
            config.label_color,
        );
    }

    // Which points fall inside the (possibly zoomed) window — everything
    // downstream (y domain, marks, hover) considers only these.
    let visible: Vec<bool> = points
        .iter()
        .map(|p| p.timestamp_secs >= x_min && p.timestamp_secs <= x_max)
        .collect();

    let mut y_values: Vec<f64> = points
        .iter()
        .zip(&visible)
        .filter(|(_, v)| **v)
        .map(|(p, _)| p.value)
        .collect();
    y_values.extend(lines.iter().map(|l| l.value));
    for b in bands {
        y_values.push(b.low);
        y_values.push(b.high);
    }
    y_values.retain(|v| v.is_finite() && *v > 0.0);
    if y_values.is_empty() {
        painter.text(
            outer_rect.center(),
            Align2::CENTER_CENTER,
            "no data",
            FontId::proportional(10.0),
            config.label_color,
        );
        return PriceTimelineResponse {
            response,
            plot_rect,
            hovered: None,
            tooltip_anchor: None,
            pinned: None,
            pinned_anchor: None,
            focus: 0,
        };
    }
    let raw_min = y_values.iter().copied().fold(f64::INFINITY, f64::min);
    let raw_max = y_values.iter().copied().fold(0.0_f64, f64::max);
    let log_scale = match config.log_y {
        LogMode::Off => false,
        LogMode::On => true,
        LogMode::Auto => raw_min > 0.0 && raw_max / raw_min > AUTO_LOG_RATIO,
    };
    // Pad the y domain ~8% (in scale space) so points don't sit on the edges.
    let (y_lo, y_hi) = if log_scale {
        let (a, b) = (raw_min.ln(), raw_max.ln().max(raw_min.ln() + 0.01));
        let pad = (b - a) * 0.08;
        (a - pad, b + pad)
    } else {
        let span = (raw_max - raw_min).max(raw_max * 0.05).max(0.01);
        ((raw_min - span * 0.08).max(0.0), raw_max + span * 0.08)
    };
    let y_span = (y_hi - y_lo).max(1e-9);

    let x_of = |t: i64| -> f32 {
        let frac = ((t - x_min) as f64 / x_span).clamp(0.0, 1.0) as f32;
        plot_rect.min.x + frac * plot_rect.width()
    };
    let y_of = |v: f64| -> f32 {
        let sv = if log_scale { v.max(1e-9).ln() } else { v };
        let frac = ((sv - y_lo) / y_span).clamp(0.0, 1.0) as f32;
        plot_rect.max.y - frac * plot_rect.height()
    };

    // ---- grid + ticks ---------------------------------------------------
    for v in y_ticks(raw_min, raw_max, log_scale) {
        let y = y_of(v);
        painter.line_segment(
            [Pos2::new(plot_rect.min.x, y), Pos2::new(plot_rect.max.x, y)],
            Stroke::new(0.5_f32, config.grid_color),
        );
        painter.text(
            Pos2::new(plot_rect.min.x + 2.0, y - 1.0),
            Align2::LEFT_BOTTOM,
            format_compact(v),
            FontId::proportional(8.5),
            config.label_color,
        );
    }
    // X gridlines at round time intervals (7d, 14d, … back from "now"), drawn
    // full-height so the eye can bucket points by period, labelled by age.
    // Aligned to `now` (not the window edge) so ages stay round while zoomed.
    let step = x_step(x_max - x_min);
    let mut k = ((now - x_max) as f64 / step as f64).ceil() as i64;
    if now - k * step >= x_max {
        k += 1;
    }
    let mut t = now - k * step;
    while t > x_min {
        let x = x_of(t);
        painter.line_segment(
            [Pos2::new(x, plot_rect.min.y), Pos2::new(x, plot_rect.max.y)],
            Stroke::new(0.5_f32, config.grid_color),
        );
        painter.text(
            Pos2::new(x, outer_rect.max.y - 2.0),
            Align2::CENTER_BOTTOM,
            age_label(now - t),
            FontId::proportional(8.5),
            config.label_color,
        );
        t -= step;
    }
    painter.text(
        Pos2::new(plot_rect.max.x, outer_rect.max.y - 2.0),
        Align2::RIGHT_BOTTOM,
        if now - x_max <= 1 {
            "now".to_string()
        } else {
            age_label(now - x_max)
        },
        FontId::proportional(8.5),
        config.label_color,
    );

    // ---- reference bands (under everything else) ------------------------
    for band in bands {
        let rect = Rect::from_min_max(
            Pos2::new(plot_rect.min.x, y_of(band.high)),
            Pos2::new(plot_rect.max.x, y_of(band.low)),
        );
        painter.rect_filled(rect, 0.0, band.fill);
    }

    // ---- connecting polyline (time order, visible points only) ----------
    if let Some(line_color) = config.connect {
        let mut ordered: Vec<&TimelinePoint> = points
            .iter()
            .zip(&visible)
            .filter(|(_, v)| **v)
            .map(|(p, _)| p)
            .collect();
        ordered.sort_by_key(|p| p.timestamp_secs);
        let path: Vec<Pos2> = ordered
            .iter()
            .map(|p| Pos2::new(x_of(p.timestamp_secs), y_of(p.value)))
            .collect();
        if path.len() >= 2 {
            painter.add(Shape::line(path, Stroke::new(1.0_f32, line_color)));
        }
    }

    // ---- points (culled to the window, not clamped to the edges) --------
    let visible_count = visible.iter().filter(|v| **v).count();
    let radius = if visible_count > DENSE_THRESHOLD {
        (config.point_radius * 0.6).max(1.5)
    } else {
        config.point_radius
    };
    let positions: Vec<Option<Pos2>> = points
        .iter()
        .zip(&visible)
        .map(|(p, v)| v.then(|| Pos2::new(x_of(p.timestamp_secs), y_of(p.value))))
        .collect();

    // The whole time-column under the cursor: points in a thin vertical band at
    // the cursor's x (time), any price (y), ordered by vertical closeness so the
    // point under the pointer focuses first.
    let hover_pos = cursor_in_plot;
    let nearby: Vec<usize> = if let Some(cursor) = hover_pos {
        let mut near: Vec<(usize, f32)> = positions
            .iter()
            .enumerate()
            .filter_map(|(i, pos)| {
                let pos = (*pos)?;
                ((pos.x - cursor.x).abs() <= HOVER_X_BAND).then_some((i, (pos.y - cursor.y).abs()))
            })
            .collect();
        near.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        near.into_iter().map(|(i, _)| i).collect()
    } else {
        Vec::new()
    };

    // ---- click-to-pin ---------------------------------------------------
    // A click freezes the current hover set so the tooltip becomes an
    // interactive popup (see `show_tooltip`). Esc / empty-click / view
    // change unpins. State is keyed on the widget id, like the zoom window.
    let pin_id = response.id.with("pinned");
    let mut pinned_state: Option<(Vec<usize>, Pos2)> = ui.data_mut(|d| d.get_temp(pin_id));
    if view_changed || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        pinned_state = None;
    }
    if response.clicked() {
        pinned_state = (!nearby.is_empty()).then(|| {
            let x = cursor_in_plot.map_or(plot_rect.center().x, |c| c.x);
            (nearby.clone(), Pos2::new(x, plot_rect.max.y + 4.0))
        });
    }
    // Drop a pin whose indices no longer exist (caller's data changed).
    if pinned_state
        .as_ref()
        .is_some_and(|(idx, _)| idx.iter().any(|&i| i >= points.len()))
    {
        pinned_state = None;
    }
    let popup_rect_id = response.id.with("popup_rect");
    ui.data_mut(|d| match &pinned_state {
        Some(s) => {
            d.insert_temp(pin_id, s.clone());
        }
        None => {
            d.remove_temp::<(Vec<usize>, Pos2)>(pin_id);
            // `Rect` has no `Default` so it can't be `remove_temp`'d;
            // NOTHING never contains a point, which is equivalent here.
            d.insert_temp(popup_rect_id, Rect::NOTHING);
        }
    });
    let pinned_indices: &[usize] = pinned_state.as_ref().map_or(&[], |(idx, _)| idx);

    // ---- scroll-to-cycle focus ------------------------------------------
    // Same-timestamp fills stack vertically and there's no y zoom, so a
    // cluster can never be separated spatially. Instead: while a tooltip is
    // showing, the scroll wheel cycles which member is focused — the scroll
    // is CONSUMED so the page doesn't move underneath. `(steps, accum px)`
    // per widget id. While pinned this also works with the pointer over the
    // popup itself (its rect is remembered from last frame's `show_tooltip`).
    let active_len = if pinned_indices.is_empty() {
        nearby.len()
    } else {
        pinned_indices.len()
    };
    let focus_id = response.id.with("focus");
    let mut focus_acc: (i32, f32) = ui.data_mut(|d| d.get_temp(focus_id)).unwrap_or((0, 0.0));
    if response.clicked() {
        // A new pin starts at the nearest member.
        focus_acc = (0, 0.0);
    }
    let over_pinned_popup = !pinned_indices.is_empty()
        && ui
            .data_mut(|d| d.get_temp::<Rect>(popup_rect_id))
            .zip(ui.input(|i| i.pointer.hover_pos()))
            .is_some_and(|(rect, pos)| rect.expand(4.0).contains(pos));
    if active_len > 1 && (cursor_in_plot.is_some() || over_pinned_popup) {
        let dy = ui.input_mut(|i| std::mem::take(&mut i.smooth_scroll_delta.y));
        if dy != 0.0 {
            // Scroll down (negative delta) moves DOWN the list. Steps clamp
            // at the ends rather than wrapping — trackpad momentum on a wrap
            // spins the selection.
            const CYCLE_STEP_PX: f32 = 24.0;
            focus_acc.1 += dy;
            while focus_acc.1 >= CYCLE_STEP_PX {
                focus_acc.0 += 1;
                focus_acc.1 -= CYCLE_STEP_PX;
            }
            while focus_acc.1 <= -CYCLE_STEP_PX {
                focus_acc.0 -= 1;
                focus_acc.1 += CYCLE_STEP_PX;
            }
            focus_acc.0 = focus_acc.0.clamp(0, active_len as i32 - 1);
            // Consumed input doesn't schedule a frame by itself — without
            // this, the selection only visibly moves on the NEXT input event.
            ui.ctx().request_repaint();
        }
    }
    let focus = if active_len > 0 {
        focus_acc.0.clamp(0, active_len as i32 - 1) as usize
    } else {
        0
    };
    ui.data_mut(|d| d.insert_temp(focus_id, focus_acc));

    // Vertical guide at the hovered (or pinned) time, so the selected column
    // reads as "every trade at this instant".
    let guide_x = pinned_state
        .as_ref()
        .map(|(_, anchor)| anchor.x)
        .or_else(|| hover_pos.map(|c| c.x));
    if let Some(gx) = guide_x {
        painter.vline(
            gx,
            plot_rect.y_range(),
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(130, 138, 170, 45)),
        );
    }

    for (i, (point, pos)) in points.iter().zip(&positions).enumerate() {
        let Some(pos) = pos else {
            continue;
        };
        let (color, r) = match point.emphasis {
            PointEmphasis::Normal => (point.color, radius),
            PointEmphasis::Faint => (point.color.gamma_multiply(0.35), radius * 0.8),
            PointEmphasis::Highlight => (point.color, radius * 1.4),
        };
        draw_mark(&painter, *pos, r, color, point.shape);
        if point.emphasis == PointEmphasis::Highlight
            || nearby.contains(&i)
            || pinned_indices.contains(&i)
        {
            painter.circle_stroke(*pos, r + 1.5, Stroke::new(1.0_f32, HIGHLIGHT_RING));
        }
    }

    // ---- reference lines (over points, labelled) ------------------------
    for line in lines {
        let y = y_of(line.value);
        let a = Pos2::new(plot_rect.min.x, y);
        let b = Pos2::new(plot_rect.max.x, y);
        let stroke = Stroke::new(1.0_f32, line.color);
        if line.dashed {
            painter.add(Shape::dashed_line(&[a, b], stroke, 4.0, 3.0));
        } else {
            painter.line_segment([a, b], stroke);
        }
        painter.text(
            Pos2::new(plot_rect.max.x - 2.0, y - 1.0),
            Align2::RIGHT_BOTTOM,
            &line.label,
            FontId::proportional(8.5),
            line.color,
        );
    }

    // ---- crosshair + hover result ---------------------------------------
    let mut hovered = None;
    let mut tooltip_anchor = None;
    if let Some(cursor) = hover_pos {
        painter.line_segment(
            [
                Pos2::new(cursor.x, plot_rect.min.y),
                Pos2::new(cursor.x, plot_rect.max.y),
            ],
            Stroke::new(0.5_f32, CROSSHAIR_COLOR),
        );
        if !nearby.is_empty() {
            tooltip_anchor = Some(Pos2::new(cursor.x, plot_rect.max.y + 4.0));
            hovered = Some(nearby);
        }
    }

    let (pinned, pinned_anchor) = match pinned_state {
        Some((idx, anchor)) => (Some(idx), Some(anchor)),
        None => (None, None),
    };
    PriceTimelineResponse {
        response,
        plot_rect,
        hovered,
        tooltip_anchor,
        pinned,
        pinned_anchor,
        focus,
    }
}

fn draw_mark(painter: &egui::Painter, pos: Pos2, r: f32, color: Color32, shape: PointShape) {
    match shape {
        PointShape::Circle => {
            painter.circle_filled(pos, r, color);
        }
        PointShape::Square => {
            painter.rect_filled(
                Rect::from_center_size(pos, Vec2::splat(r * 1.8)),
                0.5,
                color,
            );
        }
        PointShape::Diamond => {
            let d = r * 1.3;
            painter.add(Shape::convex_polygon(
                vec![
                    Pos2::new(pos.x, pos.y - d),
                    Pos2::new(pos.x + d, pos.y),
                    Pos2::new(pos.x, pos.y + d),
                    Pos2::new(pos.x - d, pos.y),
                ],
                color,
                Stroke::NONE,
            ));
        }
    }
}

/// 2–4 round tick values spanning the data. In log mode, powers-of-ten-ish
/// steps; in linear, a 1/2/5 ladder.
fn y_ticks(min: f64, max: f64, log_scale: bool) -> Vec<f64> {
    if !(min.is_finite() && max.is_finite()) || max <= 0.0 {
        return Vec::new();
    }
    if log_scale {
        let mut ticks = Vec::new();
        let mut v = 10.0_f64.powf(min.max(1e-9).log10().floor());
        while v <= max * 1.01 && ticks.len() < 12 {
            if v >= min * 0.99 {
                ticks.push(v);
            }
            v *= 10.0;
        }
        // A decade with no interior tick (e.g. 30..800) still deserves marks.
        if ticks.len() < 2 {
            return vec![min, max];
        }
        return ticks;
    }
    let span = (max - min).max(max * 0.05).max(1e-9);
    let raw_step = span / 3.0;
    let mag = 10.0_f64.powf(raw_step.log10().floor());
    let step = [1.0, 2.0, 5.0, 10.0]
        .iter()
        .map(|m| m * mag)
        .find(|s| *s >= raw_step)
        .unwrap_or(mag * 10.0);
    let mut v = (min / step).ceil() * step;
    let mut ticks = Vec::new();
    while v <= max + step * 0.01 && ticks.len() < 5 {
        ticks.push(v);
        v += step;
    }
    ticks
}

/// Compact value label: 1234567 -> "1.2M", 4200 -> "4.2k", 35.6 -> "36".
fn format_compact(v: f64) -> String {
    if v >= 1_000_000.0 {
        format!("{:.1}M", v / 1_000_000.0)
    } else if v >= 10_000.0 {
        format!("{:.0}k", v / 1_000.0)
    } else if v >= 1_000.0 {
        format!("{:.1}k", v / 1_000.0)
    } else if v >= 10.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}

/// Shrink/grow the zoom window (fractions of the base domain) around `anchor`,
/// keeping the anchor's on-screen position fixed. Clamped to [0, 1] with a
/// minimum width so zoom-in can't degenerate.
fn zoom_window(lo: f32, hi: f32, anchor: f32, zoom: f32) -> (f32, f32) {
    let w = (hi - lo).max(1e-4);
    let new_w = (w / zoom.max(0.01)).clamp(0.002, 1.0);
    let rel = ((anchor - lo) / w).clamp(0.0, 1.0);
    pan_window(anchor - rel * new_w, anchor + (1.0 - rel) * new_w, 0.0)
}

/// Shift the zoom window by `delta` (fractions), clamped inside [0, 1] without
/// changing its width.
fn pan_window(lo: f32, hi: f32, delta: f32) -> (f32, f32) {
    let w = (hi - lo).min(1.0);
    let lo = (lo + delta).clamp(0.0, 1.0 - w);
    (lo, lo + w)
}

/// Round time step for the x gridlines: the smallest ladder step that yields
/// at most ~6 lines across the window, so labels read as natural ages
/// ("7d, 14d, 21d" rather than "22d, 45d, 67d").
fn x_step(window_secs: i64) -> i64 {
    const LADDER: [i64; 10] = [
        3_600,       // 1h
        3 * 3_600,   // 3h
        6 * 3_600,   // 6h
        12 * 3_600,  // 12h
        86_400,      // 1d
        2 * 86_400,  // 2d
        7 * 86_400,  // 1w
        14 * 86_400, // 2w
        30 * 86_400, // ~1mo
        90 * 86_400, // ~3mo
    ];
    LADDER
        .into_iter()
        .find(|s| window_secs / s <= 6)
        .unwrap_or(LADDER[LADDER.len() - 1])
}

/// Age label for x ticks: "3h", "12d", "5w".
fn age_label(delta_secs: i64) -> String {
    let d = delta_secs.max(0);
    if d < 3600 {
        format!("{}m", (d / 60).max(1))
    } else if d < 86_400 {
        format!("{}h", d / 3600)
    } else if d < 14 * 86_400 {
        format!("{}d", d / 86_400)
    } else {
        format!("{}w", d / (7 * 86_400))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_ticks_are_round_and_bounded() {
        let ticks = y_ticks(28.0, 460.0, false);
        assert!(ticks.len() >= 2 && ticks.len() <= 5, "ticks: {ticks:?}");
        for t in &ticks {
            assert!(*t >= 28.0 && *t <= 470.0, "tick out of range: {t}");
        }
    }

    #[test]
    fn log_ticks_cover_decades() {
        let ticks = y_ticks(5.0, 5000.0, true);
        assert!(
            ticks.contains(&10.0) && ticks.contains(&1000.0),
            "{ticks:?}"
        );
    }

    #[test]
    fn compact_formatting() {
        assert_eq!(format_compact(1_260_000.0), "1.3M");
        assert_eq!(format_compact(4200.0), "4.2k");
        assert_eq!(format_compact(36.4), "36");
        assert_eq!(format_compact(3.12), "3.1");
    }

    #[test]
    fn zoom_window_anchors_and_clamps() {
        // Zooming in around the middle halves the window symmetrically.
        let (lo, hi) = zoom_window(0.0, 1.0, 0.5, 2.0);
        assert!(
            (lo - 0.25).abs() < 1e-4 && (hi - 0.75).abs() < 1e-4,
            "{lo} {hi}"
        );
        // Zooming out past full extent clamps to [0, 1].
        assert_eq!(zoom_window(0.25, 0.75, 0.5, 0.1), (0.0, 1.0));
        // Anchor near an edge keeps the window in bounds.
        let (lo, hi) = zoom_window(0.0, 1.0, 0.02, 4.0);
        assert!(lo >= 0.0 && hi <= 1.0 && hi > lo, "{lo} {hi}");
    }

    #[test]
    fn pan_window_preserves_width() {
        let (lo, hi) = pan_window(0.2, 0.4, 0.3);
        assert!((hi - lo - 0.2).abs() < 1e-5);
        // Clamped at the right edge.
        let (lo, hi) = pan_window(0.7, 0.9, 0.5);
        assert!(
            (hi - 1.0).abs() < 1e-5 && (hi - lo - 0.2).abs() < 1e-5,
            "{lo} {hi}"
        );
    }

    #[test]
    fn x_step_yields_round_ages() {
        // 90d window → 2w lines (6 of them), not awkward 22.5d quarters.
        assert_eq!(x_step(90 * 86_400), 14 * 86_400);
        // 30d window → 1w lines.
        assert_eq!(x_step(30 * 86_400), 7 * 86_400);
        // 1d window → 6h lines.
        assert_eq!(x_step(86_400), 6 * 3_600);
        for window in [3_600, 86_400, 30 * 86_400, 365 * 86_400] {
            assert!(window / x_step(window) <= 6, "too many lines for {window}");
        }
    }

    #[test]
    fn age_labels_scale() {
        assert_eq!(age_label(90), "1m");
        assert_eq!(age_label(7200), "2h");
        assert_eq!(age_label(3 * 86_400), "3d");
        assert_eq!(age_label(30 * 86_400), "4w");
    }
}
