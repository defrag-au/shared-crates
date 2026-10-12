//! Livery panel story — a layer stack, and the question of where the hand goes.
//!
//! Two decisions are being tried here rather than in the mugz editor, because
//! neither is settled:
//!
//! - **Where the transform lives.** One stick with a mode row, or two sticks —
//!   the left panning, the right turning sideways and growing upward at the same
//!   time. The modal hand is one control to learn; the two-stick hand has no
//!   modes at all and both hands can push at once.
//! - **How the panel is arranged.** The stack above the hand, or beside it.
//!
//! Layering is the third thing, and the one with a real answer already: a layer
//! has an order, a selection and an **enabled** flag. Disabled means *not drawn*,
//! never *not there* — the layer stays in the document, keeps its place in the
//! order and keeps its placement, so hiding one is a view of the livery and not an
//! edit to it.
//!
//! Push and hold: the sticks are rates, so nothing here is limited by how far a
//! drag could reach.

use egui_widgets::icons::PhosphorIcon;
use egui_widgets::placement::Placement;
use egui_widgets::theme::{Radius, Space, TextSize, ThemeExt};
use egui_widgets::transform_stick::{Rates, StickMode, TransformStick};
use egui_widgets::viewport::LayoutExt;

use crate::{caption, heading};

/// How wide a panel is, so the bench is a panel and not a page.
const PANEL: f32 = 340.0;

/// The subject's space, in the caller's own units — a decal's unit square.
fn surface() -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))
}

/// One layer of the livery: what it is, whether it is drawn, and where it sits.
struct Layer {
    name: String,
    tint: egui::Color32,
    enabled: bool,
    placement: Placement,
}

/// Where the transform is driven from.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Hand {
    /// One stick, a mode row, three jobs.
    Modal,
    /// Two sticks: pan on the left, shape (turn and grow) on the right.
    TwoSticks,
}

impl Hand {
    const ALL: [Hand; 2] = [Hand::Modal, Hand::TwoSticks];

    fn label(self) -> &'static str {
        match self {
            Hand::Modal => "one stick",
            Hand::TwoSticks => "two sticks",
        }
    }
}

/// How the panel is laid out.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Arrange {
    Stacked,
    Beside,
}

impl Arrange {
    const ALL: [Arrange; 2] = [Arrange::Stacked, Arrange::Beside];

    fn label(self) -> &'static str {
        match self {
            Arrange::Stacked => "stacked",
            Arrange::Beside => "beside",
        }
    }
}

/// What a row asked for, applied after the list has been drawn.
enum Action {
    Select(usize),
    Toggle(usize),
    Rise(usize),
    Sink(usize),
    Delete(usize),
    Add,
}

pub struct LiveryPanelState {
    /// Back to front: the last is drawn on top, and sits at the top of the list.
    layers: Vec<Layer>,
    selected: usize,
    hand: Hand,
    arrange: Arrange,
    /// The modal hand's stick, and the two-stick hand's pair.
    stick: TransformStick,
    pan: TransformStick,
    shape: TransformStick,
    serial: usize,
}

impl Default for LiveryPanelState {
    fn default() -> Self {
        let layer = |name: &str, tint: egui::Color32, center, size, angle| Layer {
            name: name.to_owned(),
            tint,
            enabled: true,
            placement: Placement {
                center,
                size,
                angle,
            },
        };
        let mut pan = TransformStick::default();
        pan.set_mode(StickMode::Move);
        let mut shape = TransformStick::default();
        shape.set_mode(StickMode::Shape);
        Self {
            layers: vec![
                layer(
                    "stripe",
                    egui::Color32::from_rgb(0x7a, 0xa2, 0xf7),
                    egui::pos2(0.5, 0.32),
                    egui::vec2(0.62, 0.10),
                    0.0,
                ),
                layer(
                    "sun",
                    egui::Color32::from_rgb(0xf7, 0xc9, 0x5e),
                    egui::pos2(0.30, 0.62),
                    egui::vec2(0.24, 0.24),
                    0.0,
                ),
                layer(
                    "moon",
                    egui::Color32::from_rgb(0xc9, 0xd6, 0xe8),
                    egui::pos2(0.68, 0.60),
                    egui::vec2(0.26, 0.40),
                    -0.18,
                ),
            ],
            selected: 2,
            hand: Hand::Modal,
            arrange: Arrange::Stacked,
            stick: TransformStick::default(),
            pan,
            shape,
            serial: 0,
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut LiveryPanelState) {
    let tokens = ui.tokens();
    heading(ui, "Livery Panel");
    caption(
        ui,
        "A layer stack with selection, order and an enabled flag, and two ways to \
         drive the selected layer's transform. Push and hold — the sticks are rates.",
    );
    ui.add_space(tokens.space(Space::Sm));

    ui.horizontal_wrapped(|ui| {
        ui.weak("hand");
        for hand in Hand::ALL {
            if ui
                .selectable_label(state.hand == hand, hand.label())
                .clicked()
            {
                state.hand = hand;
            }
        }
        ui.separator();
        ui.weak("panel");
        for arrange in Arrange::ALL {
            if ui
                .selectable_label(state.arrange == arrange, arrange.label())
                .clicked()
            {
                state.arrange = arrange;
            }
        }
    });
    ui.add_space(tokens.space(Space::Sm));

    match state.arrange {
        Arrange::Stacked => {
            // The list, then the hand beneath it: the shape a phone gets.
            stack(ui, state);
            ui.add_space(tokens.space(Space::Sm));
            hand(ui, state);
        }
        Arrange::Beside => {
            // The list and the hand side by side: what a roomy column can do.
            ui.columns(2, |columns| {
                stack(&mut columns[0], state);
                hand(&mut columns[1], state);
            });
        }
    }
    ui.add_space(tokens.space(Space::Md));
    preview(ui, state);

    if state.stick.engaged() || state.pan.engaged() || state.shape.engaged() {
        // A rate needs frames, and only while something is held.
        ui.ctx().request_repaint();
    }
}

/// The layer stack — front at the top, the way the drawing order reads.
fn stack(ui: &mut egui::Ui, state: &mut LiveryPanelState) {
    let tokens = ui.tokens();
    ui.set_max_width(ui.fit_width(PANEL));
    let mut action = None;

    ui.horizontal(|ui| {
        ui.strong("layers");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("add").clicked() {
                action = Some(Action::Add);
            }
        });
    });

    let size = ui.text_size(TextSize::Base);
    for index in (0..state.layers.len()).rev() {
        let layer = &state.layers[index];
        let selected = state.selected == index;
        ui.horizontal(|ui| {
            // Enabled is a view of the livery, not an edit to it: an eye, dimmed
            // rather than replaced by a different glyph.
            let eye = PhosphorIcon::Eye.rich_text(
                size,
                if layer.enabled {
                    tokens.color.text_secondary
                } else {
                    tokens.color.text_muted.gamma_multiply(0.5)
                },
            );
            if ui.selectable_label(!layer.enabled, eye).clicked() {
                action = Some(Action::Toggle(index));
            }
            let (swatch, _) = ui.allocate_exact_size(egui::Vec2::splat(size), egui::Sense::hover());
            ui.painter().rect_filled(
                swatch.shrink(2.0),
                tokens.corner(Radius::Xs),
                layer.tint.gamma_multiply(if layer.enabled { 1.0 } else { 0.4 }),
            );
            let name = egui::RichText::new(&layer.name).color(if layer.enabled {
                tokens.color.text_primary
            } else {
                tokens.color.text_muted
            });
            if ui.selectable_label(selected, name).clicked() {
                action = Some(Action::Select(index));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(PhosphorIcon::Trash.rich_text(size, tokens.color.text_secondary))
                    .clicked()
                {
                    action = Some(Action::Delete(index));
                }
                if ui
                    .button(PhosphorIcon::ArrowDown.rich_text(size, tokens.color.text_secondary))
                    .clicked()
                {
                    action = Some(Action::Sink(index));
                }
                if ui
                    .button(PhosphorIcon::ArrowUp.rich_text(size, tokens.color.text_secondary))
                    .clicked()
                {
                    action = Some(Action::Rise(index));
                }
            });
        });
    }

    if let Some(action) = action {
        apply(state, action);
    }

    ui.add_space(tokens.space(Space::Xs));
    let selected = &state.layers[state.selected];
    ui.weak(if selected.enabled {
        "enabled — drawn"
    } else {
        "disabled — kept, and not drawn"
    });
}

fn apply(state: &mut LiveryPanelState, action: Action) {
    match action {
        Action::Select(index) => state.selected = index,
        Action::Toggle(index) => {
            let enabled = state.layers[index].enabled;
            state.layers[index].enabled = !enabled;
        }
        // The list reads front at the top, so rising means moving later in the
        // stack and sinking earlier.
        Action::Rise(index) if index + 1 < state.layers.len() => {
            state.layers.swap(index, index + 1);
            state.selected = index + 1;
        }
        Action::Sink(index) if index > 0 => {
            state.layers.swap(index, index - 1);
            state.selected = index - 1;
        }
        Action::Delete(index) => {
            if state.layers.len() > 1 {
                state.layers.remove(index);
                state.selected = state.selected.min(state.layers.len() - 1);
            }
        }
        Action::Add => {
            state.serial += 1;
            let serial = state.serial;
            state.layers.push(Layer {
                name: format!("shape {serial}"),
                tint: egui::Color32::from_rgb(0xe0, 0x82, 0xc8),
                enabled: true,
                placement: Placement {
                    center: egui::pos2(0.5, 0.5),
                    size: egui::vec2(0.22, 0.22),
                    angle: 0.0,
                },
            });
            state.selected = state.layers.len() - 1;
        }
        Action::Rise(_) | Action::Sink(_) => {}
    }
}

/// The hand: the modal stick with its row, or the pair.
fn hand(ui: &mut egui::Ui, state: &mut LiveryPanelState) {
    let tokens = ui.tokens();
    ui.set_max_width(ui.fit_width(PANEL));
    let dt = ui.ctx().input(|input| input.stable_dt).min(0.1);
    let fine = ui.ctx().input(|input| input.modifiers.shift);
    let rates = Rates::default();
    let space = surface();
    let index = state.selected;

    match state.hand {
        Hand::Modal => {
            state.stick.mode_row(ui);
            ui.add_space(tokens.space(Space::Sm));
            state.stick.show(ui, dt);
            let placement = &mut state.layers[index].placement;
            state.stick.apply(placement, space, rates, dt, fine);
        }
        Hand::TwoSticks => {
            ui.columns(2, |columns| {
                columns[0].weak("move");
                state.pan.show(&mut columns[0], dt);
                columns[1].weak("shape");
                state.shape.show(&mut columns[1], dt);
            });
            let placement = &mut state.layers[index].placement;
            state.pan.apply(placement, space, rates, dt, fine);
            state.shape.apply(placement, space, rates, dt, fine);
        }
    }

    ui.add_space(tokens.space(Space::Sm));
    let layer = &state.layers[index];
    let (u, v) = (layer.placement.center.x, layer.placement.center.y);
    let turn = layer.placement.angle.to_degrees();
    let width = layer.placement.size.x;
    ui.weak(format!(
        "{}:  u {u:.2}   v {v:.2}   width {width:.2}   turn {turn:.0}",
        layer.name
    ));
    ui.weak("hold shift for a fine hand");
}

/// The livery as it draws: back to front, hidden layers left out.
fn preview(ui: &mut egui::Ui, state: &LiveryPanelState) {
    let tokens = ui.tokens();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(320.0, 200.0), egui::Sense::hover());
    let painter = ui.painter().clone();
    painter.rect_filled(rect, tokens.corner(Radius::Sm), tokens.color.bg_secondary);
    painter.rect_stroke(
        rect,
        tokens.corner(Radius::Sm),
        egui::Stroke::new(1.0_f32, tokens.color.border),
        egui::StrokeKind::Inside,
    );

    for (index, layer) in state.layers.iter().enumerate() {
        if !layer.enabled {
            continue;
        }
        let corners = layer
            .placement
            .corners()
            .map(|corner| rect.min + egui::vec2(corner.x, corner.y) * rect.size());
        let selected = state.selected == index;
        painter.add(egui::epaint::PathShape::convex_polygon(
            corners.to_vec(),
            layer.tint.gamma_multiply(0.75),
            egui::Stroke::new(
                if selected { 2.0 } else { 1.0 },
                if selected {
                    tokens.color.accent
                } else {
                    tokens.color.border
                },
            ),
        ));
    }

    ui.add_space(tokens.space(Space::Xs));
    ui.weak("the livery, back to front");
}
