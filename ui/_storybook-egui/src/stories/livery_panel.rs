//! Livery panel story — a layer stack, a subject you can grab, and one dial.
//!
//! The shape being tried is the one a phone forces: the subject on top, and
//! under it a single sheet that is *either* the layer list *or* one layer being
//! edited, never both. A wide column docks the two, list over editor, and keeps
//! everything else the same.
//!
//! Two hands, for two kinds of placement:
//!
//! - **Rough** — grab the subject itself. Drag a layer to move it and the knob
//!   off its edge to size and turn it. A press on another layer selects it.
//! - **Fine** — the [`TransformDial`], one control with no modes. The puck
//!   moves at a rate, the rim sweeps to turn and pulls to size. The numbers
//!   under it take a typed value for anything the hand can't hit exactly.
//!
//! Layering is settled: a layer has an order, a selection and an **enabled**
//! flag. Disabled means *not drawn*, never *not there*. The layer keeps its
//! place and its placement, and while it is selected it shows as a dashed ghost
//! so the hand still knows where it is.

use egui_widgets::icons::PhosphorIcon;
use egui_widgets::placement::{self, Frame, Placement, PlacementEditor, PlacementStyle};
use egui_widgets::theme::{Radius, Space, TextSize, ThemeExt};
use egui_widgets::transform_dial::{HINT, TransformDial};
use egui_widgets::transform_stick::Rates;
use egui_widgets::viewport::LayoutExt;

use crate::{caption, heading};

/// How wide a panel is, so the bench is a panel and not a page.
const PANEL: f32 = 340.0;

/// The subject's largest side. `theme-exempt`: a stand-in for the mug card.
const SUBJECT: f32 = 300.0;

/// Room around the subject's surface for the knob past a layer's edge.
/// `theme-exempt`: pointer affordance, matching `placement`'s puck standoff.
const MARGIN: f32 = 28.0;

/// A layer row's height: a fingertip. `theme-exempt`: touch target.
const ROW: f32 = 40.0;

/// An icon button's side: a fingertip. `theme-exempt`: touch target.
const TAP: f32 = 32.0;

/// The direct-drag control on the subject — the same one the mugz customiser
/// puts on the mug.
const STYLE: PlacementStyle = PlacementStyle::Puck;

/// The smallest and largest a layer may be, as a fraction of the surface's
/// width, so a long pull can't lose one.
const MIN_WIDTH: f32 = 0.03;
const MAX_WIDTH: f32 = 2.0;

/// The subject's space, in the caller's own units — a decal's unit square.
fn surface() -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))
}

/// One layer of the livery: what it is, whether it is drawn, and where it sits.
#[derive(Clone)]
struct Layer {
    name: String,
    tint: egui::Color32,
    enabled: bool,
    placement: Placement,
}

/// Which face of the sheet is showing.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Sheet {
    List,
    Edit,
}

/// What the sheet asked for, applied after it has been drawn.
enum Action {
    Open(usize),
    Toggle(usize),
    Add,
    Back,
    Rise,
    Sink,
    Duplicate,
    Delete,
    Undo,
}

pub struct LiveryPanelState {
    /// Back to front: the last is drawn on top, and sits at the top of the list.
    layers: Vec<Layer>,
    selected: usize,
    sheet: Sheet,
    dial: TransformDial,
    editor: PlacementEditor,
    /// The fine hand, as a toggle — a phone has no shift key.
    fine: bool,
    /// The last delete, kept for one undo.
    deleted: Option<(usize, Layer)>,
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
            sheet: Sheet::List,
            dial: TransformDial::default(),
            editor: PlacementEditor::default(),
            fine: false,
            deleted: None,
            serial: 0,
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut LiveryPanelState) {
    let tokens = ui.tokens();
    heading(ui, "Livery Panel");
    caption(
        ui,
        "Grab a layer on the subject for rough placement; open it for the dial. \
         The puck moves, the rim sweeps to turn and pulls to size.",
    );
    ui.add_space(tokens.space(Space::Sm));

    let gap = tokens.space(Space::Lg);
    if ui.available_width() >= PANEL * 2.0 + gap {
        // Roomy: the subject beside a docked sheet, list over editor.
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(PANEL);
                subject(ui, state);
            });
            ui.add_space(gap);
            ui.vertical(|ui| {
                ui.set_width(PANEL);
                list(ui, state);
                if state.sheet == Sheet::Edit {
                    ui.add_space(tokens.space(Space::Md));
                    ui.separator();
                    edit(ui, state, true);
                }
            });
        });
    } else {
        // A phone: the subject, then one face of the sheet.
        ui.vertical(|ui| {
            ui.set_max_width(ui.fit_width(PANEL));
            subject(ui, state);
            ui.add_space(tokens.space(Space::Sm));
            match state.sheet {
                Sheet::List => list(ui, state),
                Sheet::Edit => edit(ui, state, false),
            }
        });
    }

    if state.dial.engaged() || state.dial.settling() {
        // A rate needs frames, and so does a spring on its way home.
        ui.ctx().request_repaint();
    }
}

/// The livery as it draws, with the selected layer under the direct hand.
fn subject(ui: &mut egui::Ui, state: &mut LiveryPanelState) {
    let tokens = ui.tokens();
    let side = ui.available_width().min(SUBJECT);
    let (full, _) = ui.allocate_exact_size(egui::Vec2::splat(side), egui::Sense::hover());
    let bench = full.shrink(MARGIN);
    let to_px = |point: egui::Pos2| bench.min + point.to_vec2() * bench.size();
    let from_px = |point: egui::Pos2| {
        egui::pos2(
            (point.x - bench.min.x) / bench.width(),
            (point.y - bench.min.y) / bench.height(),
        )
    };
    let frame_of = |placement: &Placement| Frame {
        corners: placement.corners().map(to_px),
    };

    // A press picks the layer it lands on — unless it landed on the held
    // layer or its knob, which belong to the editor.
    let press = ui.input(|input| {
        if input.pointer.any_pressed() {
            input.pointer.press_origin()
        } else {
            None
        }
    });
    if let Some(at) = press.filter(|at| full.contains(*at))
        && !state.editor.dragging()
    {
        let local = from_px(at);
        let held = &state.layers[state.selected].placement;
        let on_held =
            held.contains(local) || placement::on_handle(held, &frame_of(held), at, STYLE);
        if on_held {
            state.sheet = Sheet::Edit;
        } else if let Some(index) = (0..state.layers.len()).rev().find(|index| {
            let layer = &state.layers[*index];
            layer.enabled && layer.placement.contains(local)
        }) {
            state.selected = index;
            state.sheet = Sheet::Edit;
        }
    }

    let painter = ui.painter().clone();
    painter.rect_filled(bench, tokens.corner(Radius::Sm), tokens.color.bg_secondary);
    painter.rect_stroke(
        bench,
        tokens.corner(Radius::Sm),
        egui::Stroke::new(1.0_f32, tokens.color.border),
        egui::StrokeKind::Inside,
    );
    for layer in state.layers.iter().filter(|layer| layer.enabled) {
        painter.add(egui::epaint::PathShape::convex_polygon(
            frame_of(&layer.placement).corners.to_vec(),
            layer.tint.gamma_multiply(0.85),
            egui::Stroke::new(1.0_f32, tokens.color.border),
        ));
    }
    let held = &state.layers[state.selected];
    if !held.enabled {
        // Hidden, and still held: a ghost, so the hand knows where it is.
        let mut ring = frame_of(&held.placement).corners.to_vec();
        ring.push(ring[0]);
        painter.extend(egui::Shape::dashed_line(
            &ring,
            egui::Stroke::new(1.0_f32, tokens.color.text_muted),
            6.0,
            4.0,
        ));
    }

    let frame = frame_of(&state.layers[state.selected].placement);
    let pointer = ui.input(|input| input.pointer.interact_pos()).map(from_px);
    let placement = &mut state.layers[state.selected].placement;
    let changed = ui
        .scope_builder(egui::UiBuilder::new().max_rect(full), |ui| {
            state.editor.show_in(ui, placement, &frame, pointer, STYLE)
        })
        .inner;
    if changed {
        keep_sane(placement);
    }
}

/// The layer list — front at the top, the way the drawing order reads.
fn list(ui: &mut egui::Ui, state: &mut LiveryPanelState) {
    let tokens = ui.tokens();
    let mut action = None;

    ui.horizontal(|ui| {
        ui.strong("Layers");
        ui.weak(state.layers.len().to_string());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icon_button(ui, PhosphorIcon::Plus, true) {
                action = Some(Action::Add);
            }
        });
    });
    if let Some((_, layer)) = &state.deleted {
        ui.horizontal(|ui| {
            ui.weak(format!("deleted {}", layer.name));
            if ui.small_button("undo").clicked() {
                action = Some(Action::Undo);
            }
        });
    }
    ui.add_space(tokens.space(Space::Xs));

    for index in (0..state.layers.len()).rev() {
        let selected = state.selected == index && state.sheet == Sheet::Edit;
        match row(ui, &state.layers[index], selected) {
            Some(RowHit::Eye) => action = Some(Action::Toggle(index)),
            Some(RowHit::Open) => action = Some(Action::Open(index)),
            None => {}
        }
    }

    if let Some(action) = action {
        apply(state, action);
    }
}

/// What a row was tapped on.
enum RowHit {
    Eye,
    Open,
}

/// One layer as a row: an eye, a thumbnail of its shape, its name, and a caret
/// that says the row opens. The whole row is the target.
fn row(ui: &mut egui::Ui, layer: &Layer, selected: bool) -> Option<RowHit> {
    let tokens = ui.tokens();
    let (rect, open) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW), egui::Sense::click());
    // Allocated after the row, so it sits on top of it and takes its own taps.
    let eye_rect = egui::Rect::from_min_size(rect.min, egui::Vec2::splat(ROW));
    let eye = ui.interact(eye_rect, open.id.with("eye"), egui::Sense::click());

    let painter = ui.painter();
    let corner = tokens.corner(Radius::Sm);
    if selected {
        painter.rect_filled(rect, corner, tokens.color.accent.gamma_multiply(0.14));
        painter.rect_filled(
            egui::Rect::from_min_size(rect.min, egui::vec2(3.0, ROW)),
            corner,
            tokens.color.accent,
        );
    } else if open.hovered() && !eye.hovered() {
        painter.rect_filled(rect, corner, tokens.color.bg_highlight);
    }

    let size = ui.text_size(TextSize::Base);
    let eye_colour = if !layer.enabled {
        tokens.color.text_muted.gamma_multiply(0.5)
    } else if eye.hovered() {
        tokens.color.text_primary
    } else {
        tokens.color.text_secondary
    };
    PhosphorIcon::Eye.paint(
        painter,
        eye_rect.center(),
        egui::Align2::CENTER_CENTER,
        size,
        eye_colour,
    );

    // The thumbnail: the layer's own shape, turned as it is placed, so a row
    // says which one it is without reading the name.
    let thumb = egui::Rect::from_center_size(
        egui::pos2(eye_rect.right() + 14.0, rect.center().y),
        egui::Vec2::splat(26.0),
    );
    painter.rect_filled(thumb, tokens.corner(Radius::Xs), tokens.color.bg_secondary);
    let placement = layer.placement;
    let reach = placement.size.length().max(1e-4);
    let fit = (thumb.width() - 6.0) / reach;
    let shape = placement
        .corners()
        .map(|point| thumb.center() + (point - placement.center) * fit);
    painter.add(egui::epaint::PathShape::convex_polygon(
        shape.to_vec(),
        layer
            .tint
            .gamma_multiply(if layer.enabled { 1.0 } else { 0.35 }),
        egui::Stroke::NONE,
    ));

    painter.text(
        egui::pos2(thumb.right() + 10.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        &layer.name,
        egui::FontId::proportional(size),
        if layer.enabled {
            tokens.color.text_primary
        } else {
            tokens.color.text_muted
        },
    );
    PhosphorIcon::CaretRight.paint(
        painter,
        egui::pos2(rect.right() - 12.0, rect.center().y),
        egui::Align2::CENTER_CENTER,
        size,
        tokens.color.text_muted,
    );

    if eye.clicked() {
        Some(RowHit::Eye)
    } else if open.clicked() {
        Some(RowHit::Open)
    } else {
        None
    }
}

/// One layer, open: its actions once in the header, the dial, and its numbers.
/// `docked` is the roomy layout, where the list is still showing and there is
/// nowhere to go back to.
fn edit(ui: &mut egui::Ui, state: &mut LiveryPanelState, docked: bool) {
    let tokens = ui.tokens();
    let index = state.selected;
    let top = index + 1 == state.layers.len();
    let bottom = index == 0;
    let mut action = None;

    ui.horizontal(|ui| {
        if !docked && icon_button(ui, PhosphorIcon::ArrowLeft, true) {
            action = Some(Action::Back);
        }
        ui.strong(&state.layers[index].name);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icon_button(ui, PhosphorIcon::Trash, state.layers.len() > 1) {
                action = Some(Action::Delete);
            }
            if icon_button(ui, PhosphorIcon::ArrowDown, !bottom) {
                action = Some(Action::Sink);
            }
            if icon_button(ui, PhosphorIcon::ArrowUp, !top) {
                action = Some(Action::Rise);
            }
            if icon_button(ui, PhosphorIcon::Copy, true) {
                action = Some(Action::Duplicate);
            }
            if icon_button(ui, PhosphorIcon::Eye, true) {
                action = Some(Action::Toggle(index));
            }
        });
    });
    ui.add_space(tokens.space(Space::Sm));

    let dt = ui.ctx().input(|input| input.stable_dt).min(0.1);
    let fine = state.fine || ui.ctx().input(|input| input.modifiers.shift);
    ui.vertical_centered(|ui| {
        state.dial.show(ui, dt);
        ui.weak(match state.dial.gesture() {
            Some(gesture) => gesture.label(),
            None => HINT,
        });
    });
    let placement = &mut state.layers[index].placement;
    if state
        .dial
        .apply(placement, surface(), Rates::default(), dt, fine)
    {
        keep_sane(placement);
    }

    ui.add_space(tokens.space(Space::Sm));
    ui.horizontal_wrapped(|ui| numbers(ui, placement));
    ui.horizontal(|ui| {
        if ui.selectable_label(state.fine, "fine").clicked() {
            state.fine = !state.fine;
        }
        ui.weak("gears every hand down — or hold shift");
    });

    if let Some(action) = action {
        apply(state, action);
    }
}

/// The placement as numbers that take a typed value: the hand gets close, and
/// these get exact.
fn numbers(ui: &mut egui::Ui, placement: &mut Placement) {
    ui.add(
        egui::DragValue::new(&mut placement.center.x)
            .range(0.0..=1.0)
            .speed(0.002)
            .fixed_decimals(2)
            .prefix("u "),
    );
    ui.add(
        egui::DragValue::new(&mut placement.center.y)
            .range(0.0..=1.0)
            .speed(0.002)
            .fixed_decimals(2)
            .prefix("v "),
    );
    // Width sizes the whole layer, so its proportions hold.
    let mut width = placement.size.x;
    if ui
        .add(
            egui::DragValue::new(&mut width)
                .range(MIN_WIDTH..=MAX_WIDTH)
                .speed(0.002)
                .fixed_decimals(2)
                .prefix("w "),
        )
        .changed()
    {
        placement.size *= width / placement.size.x.max(1e-6);
    }
    let mut degrees = placement.angle.to_degrees();
    if ui
        .add(
            egui::DragValue::new(&mut degrees)
                .speed(0.5)
                .fixed_decimals(0)
                .suffix("°"),
        )
        .changed()
    {
        placement.angle = degrees.to_radians();
    }
}

/// A fingertip-sized icon button, frameless until hovered.
fn icon_button(ui: &mut egui::Ui, icon: PhosphorIcon, enabled: bool) -> bool {
    let tokens = ui.tokens();
    let size = ui.text_size(TextSize::Base);
    let button = egui::Button::new(icon.rich_text(size, tokens.color.text_secondary))
        .frame(false)
        .min_size(egui::Vec2::splat(TAP));
    ui.add_enabled(enabled, button).clicked()
}

/// Keep a layer findable: a width the hand can still see and still grab, with
/// its proportions held.
fn keep_sane(placement: &mut Placement) {
    let width = placement.size.x.max(1e-6);
    let clamped = width.clamp(MIN_WIDTH, MAX_WIDTH);
    if clamped != width {
        placement.size *= clamped / width;
    }
}

fn apply(state: &mut LiveryPanelState, action: Action) {
    let index = state.selected;
    match action {
        Action::Open(index) => {
            state.selected = index;
            state.sheet = Sheet::Edit;
        }
        Action::Toggle(index) => {
            let enabled = state.layers[index].enabled;
            state.layers[index].enabled = !enabled;
        }
        Action::Add => {
            state.serial += 1;
            let serial = state.serial;
            state.layers.push(Layer {
                name: format!("shape {serial}"),
                tint: egui::Color32::from_rgb(0xe0, 0x82, 0xc8),
                enabled: true,
                placement: Placement::new(egui::pos2(0.5, 0.5), egui::vec2(0.22, 0.22)),
            });
            state.selected = state.layers.len() - 1;
            state.sheet = Sheet::Edit;
        }
        Action::Back => state.sheet = Sheet::List,
        // The list reads front at the top, so rising means moving later in the
        // stack and sinking earlier.
        Action::Rise if index + 1 < state.layers.len() => {
            state.layers.swap(index, index + 1);
            state.selected = index + 1;
        }
        Action::Sink if index > 0 => {
            state.layers.swap(index, index - 1);
            state.selected = index - 1;
        }
        Action::Rise | Action::Sink => {}
        Action::Duplicate => {
            let mut copy = state.layers[index].clone();
            copy.name = format!("{} copy", copy.name);
            copy.placement.center += egui::vec2(0.04, 0.04);
            state.layers.insert(index + 1, copy);
            state.selected = index + 1;
        }
        Action::Delete => {
            if state.layers.len() > 1 {
                let layer = state.layers.remove(index);
                state.deleted = Some((index, layer));
                state.selected = index.min(state.layers.len() - 1);
                state.sheet = Sheet::List;
            }
        }
        Action::Undo => {
            if let Some((at, layer)) = state.deleted.take() {
                let at = at.min(state.layers.len());
                state.layers.insert(at, layer);
                state.selected = at;
            }
        }
    }
}
