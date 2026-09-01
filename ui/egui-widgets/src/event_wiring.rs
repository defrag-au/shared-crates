//! `EventWiring` — one event-source node wired to its action cards, IFTTT made visible.
//!
//! An event node on the left (kind + the match patterns that fire it, as
//! removable chips with an inline add field) and the dispatched actions on
//! the right, with drawn wires between the ports. `flow_matrix` argues —
//! correctly — that node-link views hairball at scale; this widget is the
//! small-end exception it allows for: one source, a handful of actions, where
//! the wire IS the semantics being edited ("when this fires, these run").
//! Anything bigger belongs in a matrix, not more wires.
//!
//! VM-in / actions-out: the caller owns the binding model and rebuilds the
//! VMs per frame; the widget reports what the user did this frame in
//! [`EventWiringResponse`] and holds no model state (the pattern-input draft
//! rides egui temp memory, keyed by `id_salt`).
//!
//! Action config expands IN the card ([`EventWiring::expanded`] — the caller
//! supplies the fields as a closure, and the card widens to hold them). The
//! inflection rule: config that fits a widened card stays inline in the
//! flow; anything bigger pushes to a centred `egui::Modal` rather than
//! growing the card until the flow stops reading as a flow.

use egui::{Color32, CursorIcon, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};

use crate::theme;
use crate::{Chip, ChipVariant, PhosphorIcon};

/// The event-source side of the node.
pub struct EventNodeVm {
    /// Kind label, e.g. "ON_MESSAGE".
    pub kind_label: String,
    pub icon: PhosphorIcon,
    /// The match patterns that fire this event.
    pub patterns: Vec<String>,
    pub enabled: bool,
    /// Per-user firing cooldown in seconds (None = off).
    pub cooldown_seconds: Option<u32>,
}

/// One wired action card.
pub struct ActionCardVm {
    /// Stable id, echoed in click responses.
    pub id: String,
    pub icon: PhosphorIcon,
    /// e.g. "Random owned asset".
    pub title: String,
    /// e.g. the catalog command name, or the emoji.
    pub subtitle: Option<String>,
}

/// What the user did this frame. `Option`/`bool` fields mean "this happened".
#[derive(Default)]
pub struct EventWiringResponse {
    /// A new pattern was committed in the inline add field.
    pub pattern_added: Option<String>,
    /// A pattern chip's `×` was clicked (index into `patterns`).
    pub pattern_removed: Option<usize>,
    /// An action card's `×` was clicked (index into `actions`).
    pub action_removed: Option<usize>,
    /// An action card body was clicked (index) — open its config.
    pub action_clicked: Option<usize>,
    /// The "+ action" port was clicked — open the add-action palette.
    pub add_action_clicked: bool,
    /// The cooldown control changed — the new value in seconds (0 = off,
    /// which the caller maps to None).
    pub cooldown_set: Option<u32>,
    /// The enabled toggle changed (new value = !vm.enabled).
    pub enabled_toggled: bool,
    /// The binding's remove affordance was clicked.
    pub remove_clicked: bool,
}

/// Caller-supplied rendering for an expanded action card.
///
/// The config fields inside a card are app domain — a policy id, a style
/// picker — so this crate takes them as a closure rather than growing a
/// vocabulary for every app that has one.
type ExpandedContent<'a> = Box<dyn FnOnce(&mut Ui) + 'a>;

/// The wiring node group for one binding.
pub struct EventWiring<'a> {
    id_salt: &'a str,
    event: &'a EventNodeVm,
    actions: &'a [ActionCardVm],
    /// An action card expanded in place, with caller-rendered content.
    expanded: Option<usize>,
    expanded_content: Option<ExpandedContent<'a>>,
}

const NODE_WIDTH: f32 = 230.0;
const CARD_WIDTH: f32 = 240.0;
/// An expanded card gets room for real fields (a 56-hex policy id, a style
/// picker). This is the "inline" side of the inflection point — anything
/// that doesn't fit here should push to an `egui::Modal` instead of growing
/// the card further.
const EXPANDED_CARD_WIDTH: f32 = 380.0;
const WIRE_GAP: f32 = 56.0;
const PORT_RADIUS: f32 = 4.0;

impl<'a> EventWiring<'a> {
    pub fn new(id_salt: &'a str, event: &'a EventNodeVm, actions: &'a [ActionCardVm]) -> Self {
        Self {
            id_salt,
            event,
            actions,
            expanded: None,
            expanded_content: None,
        }
    }

    /// Expand the action card at `index` in place, rendering `content`
    /// (its config UI) inside the card body. The card widens to
    /// [`EXPANDED_CARD_WIDTH`]; wires follow.
    pub fn expanded(mut self, index: usize, content: impl FnOnce(&mut Ui) + 'a) -> Self {
        self.expanded = Some(index);
        self.expanded_content = Some(Box::new(content));
        self
    }

    pub fn show(self, ui: &mut Ui) -> EventWiringResponse {
        crate::install_phosphor_font(ui.ctx());
        let mut response = EventWiringResponse::default();
        // Node/card labels are affordances, not copyable data — selectable
        // labels put the cursor into text-select and fight the click targets.
        ui.style_mut().interaction.selectable_labels = false;

        let dim = if self.event.enabled { 1.0 } else { 0.45 };
        let tint = |c: Color32| c.linear_multiply(dim);

        let mut event_port: Option<Pos2> = None;
        let mut action_ports: Vec<Pos2> = Vec::new();

        ui.horizontal_top(|ui| {
            // ── Event node ────────────────────────────────────────────────
            ui.vertical(|ui| {
                ui.set_width(NODE_WIDTH);
                let frame = egui::Frame::group(ui.style())
                    .fill(tint(theme::BG_SECONDARY))
                    .stroke(Stroke::new(1.0_f32, tint(theme::ACCENT)))
                    .inner_margin(10.0);
                let node = frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        self.event.icon.show(ui, 14.0, tint(theme::ACCENT));
                        ui.label(
                            RichText::new(&self.event.kind_label)
                                .color(tint(theme::TEXT_PRIMARY))
                                .strong(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if icon_button(ui, PhosphorIcon::Trash, "remove binding") {
                                response.remove_clicked = true;
                            }
                            let mut enabled = self.event.enabled;
                            if ui.checkbox(&mut enabled, "").changed() {
                                response.enabled_toggled = true;
                            }
                        });
                    });
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("fires on")
                            .color(tint(theme::TEXT_MUTED))
                            .small(),
                    );
                    ui.horizontal_wrapped(|ui| {
                        for (index, pattern) in self.event.patterns.iter().enumerate() {
                            let chip = Chip::new(pattern)
                                .variant(ChipVariant::Tag)
                                .removable(true)
                                .show(ui);
                            if chip.removed {
                                response.pattern_removed = Some(index);
                            }
                        }
                    });

                    // Inline pattern add: draft rides temp memory; Enter commits.
                    let draft_id = egui::Id::new((self.id_salt, "pattern_draft"));
                    let mut draft: String = ui
                        .ctx()
                        .data_mut(|d| d.get_temp::<String>(draft_id))
                        .unwrap_or_default();
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut draft)
                            .hint_text("add pattern…")
                            .desired_width(f32::INFINITY),
                    );
                    // Commit on ANY focus loss with text — not just Enter.
                    // Requiring Enter left "type a pattern, click Save" with
                    // an empty pattern list and a confusing rejection: the
                    // text LOOKED entered but was still a draft.
                    if edit.lost_focus() && !draft.trim().is_empty() {
                        response.pattern_added = Some(draft.trim().to_string());
                        draft.clear();
                        // Only chain focus when Enter committed (rapid entry);
                        // a click-away is the user leaving the field.
                        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            edit.request_focus();
                        }
                    }
                    ui.ctx().data_mut(|d| d.insert_temp(draft_id, draft));

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("cooldown")
                                .color(tint(theme::TEXT_MUTED))
                                .small(),
                        );
                        let mut secs = self.event.cooldown_seconds.unwrap_or(0);
                        if ui
                            .add(
                                egui::DragValue::new(&mut secs)
                                    .range(0..=86_400)
                                    .suffix("s"),
                            )
                            .changed()
                        {
                            response.cooldown_set = Some(secs);
                        }
                        if self.event.cooldown_seconds.is_none() {
                            ui.label(RichText::new("off").color(tint(theme::TEXT_MUTED)).small());
                        }
                    });
                });
                let rect = node.response.rect;
                event_port = Some(Pos2::new(rect.right(), rect.center().y));
            });

            ui.add_space(WIRE_GAP);

            // ── Action cards ──────────────────────────────────────────────
            let column_width = if self.expanded.is_some() {
                EXPANDED_CARD_WIDTH
            } else {
                CARD_WIDTH
            };
            let mut expanded_content = self.expanded_content;
            ui.vertical(|ui| {
                ui.set_width(column_width);
                for (index, action) in self.actions.iter().enumerate() {
                    let is_expanded = self.expanded == Some(index);
                    let card_width = if is_expanded {
                        EXPANDED_CARD_WIDTH
                    } else {
                        CARD_WIDTH
                    };
                    let frame = egui::Frame::group(ui.style())
                        .fill(tint(theme::BG_HIGHLIGHT))
                        .stroke(Stroke::new(
                            1.0_f32,
                            if is_expanded {
                                tint(theme::ACCENT_CYAN)
                            } else {
                                tint(theme::BORDER)
                            },
                        ))
                        .inner_margin(8.0);
                    let card = frame.show(ui, |ui| {
                        ui.set_width(card_width - 16.0);
                        ui.horizontal(|ui| {
                            action.icon.show(ui, 14.0, tint(theme::ACCENT_CYAN));
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(&action.title)
                                        .color(tint(theme::TEXT_PRIMARY))
                                        .strong(),
                                );
                                if let Some(subtitle) = &action.subtitle {
                                    ui.label(
                                        RichText::new(subtitle)
                                            .color(tint(theme::TEXT_SECONDARY))
                                            .small(),
                                    );
                                }
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    // Dedicated click-sensed widgets — an
                                    // `interact()` bolted onto the whole card
                                    // would register LATER and steal these
                                    // clicks (egui hit-tests topmost-last).
                                    if icon_button(ui, PhosphorIcon::X, "remove action") {
                                        response.action_removed = Some(index);
                                    }
                                    let toggle_icon = if is_expanded {
                                        PhosphorIcon::CaretDown
                                    } else {
                                        PhosphorIcon::PencilSimple
                                    };
                                    if icon_button(ui, toggle_icon, "configure action") {
                                        response.action_clicked = Some(index);
                                    }
                                },
                            );
                        });
                        // The in-card config expansion — the caller's fields,
                        // rendered inside the card so the flow stays the view.
                        if is_expanded && let Some(content) = expanded_content.take() {
                            ui.add_space(6.0);
                            ui.separator();
                            ui.add_space(4.0);
                            content(ui);
                        }
                    });
                    let rect = card.response.rect;
                    action_ports.push(Pos2::new(rect.left(), rect.center().y));
                    ui.add_space(6.0);
                }

                // The add-action port — a dashed card inviting the palette.
                // Generous hit target + hover fill: this is the editor's main
                // growth affordance, it must not feel touchy.
                let (rect, add) =
                    ui.allocate_exact_size(Vec2::new(CARD_WIDTH, 36.0), Sense::click());
                let hover = add.hovered();
                let stroke_color = if hover {
                    theme::ACCENT
                } else {
                    tint(theme::TEXT_MUTED)
                };
                if hover {
                    ui.painter().rect_filled(rect, 4.0, theme::BG_HIGHLIGHT);
                }
                dashed_rect(ui, rect, Stroke::new(1.0_f32, stroke_color));
                let painter = ui.painter();
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("{} action", PhosphorIcon::Plus.as_str()),
                    egui::FontId::new(12.0, crate::icons::phosphor_family()),
                    stroke_color,
                );
                if add.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                    response.add_action_clicked = true;
                }
                action_ports.push(Pos2::new(rect.left(), rect.center().y));
            });
        });

        // ── Wires ─────────────────────────────────────────────────────────
        if let Some(from) = event_port {
            let painter = ui.painter();
            painter.circle_filled(from, PORT_RADIUS, tint(theme::ACCENT));
            for to in &action_ports {
                painter.circle_filled(*to, PORT_RADIUS, tint(theme::ACCENT_CYAN));
                let dx = ((to.x - from.x) * 0.5).max(16.0);
                let shape = egui::epaint::CubicBezierShape::from_points_stroke(
                    [
                        from,
                        Pos2::new(from.x + dx, from.y),
                        Pos2::new(to.x - dx, to.y),
                        *to,
                    ],
                    false,
                    Color32::TRANSPARENT,
                    Stroke::new(1.5_f32, tint(theme::ACCENT)),
                );
                painter.add(shape);
            }
        }

        response
    }
}

/// A small click-sensed icon affordance (hover brightens + pointer cursor).
/// A `Label` with `Sense::click()` — NOT `PhosphorIcon::show().interact()`,
/// which re-registers the response and loses reliably to later widgets.
fn icon_button(ui: &mut Ui, icon: PhosphorIcon, hover: &str) -> bool {
    let resp = ui
        .add(egui::Label::new(icon.rich_text(12.0, theme::TEXT_MUTED)).sense(Sense::click()))
        .on_hover_text(hover)
        .on_hover_cursor(CursorIcon::PointingHand);
    resp.clicked()
}

/// A dashed rounded-rect outline (egui strokes are solid; fake the dash by
/// segmenting the perimeter).
fn dashed_rect(ui: &Ui, rect: Rect, stroke: Stroke) {
    let painter = ui.painter();
    let dash = 6.0;
    let gap = 4.0;
    let mut edges = Vec::new();
    edges.push((rect.left_top(), rect.right_top()));
    edges.push((rect.right_top(), rect.right_bottom()));
    edges.push((rect.right_bottom(), rect.left_bottom()));
    edges.push((rect.left_bottom(), rect.left_top()));
    for (a, b) in edges {
        let len = a.distance(b);
        let dir = (b - a) / len;
        let mut t = 0.0;
        while t < len {
            let end = (t + dash).min(len);
            painter.line_segment([a + dir * t, a + dir * end], stroke);
            t = end + gap;
        }
    }
}
