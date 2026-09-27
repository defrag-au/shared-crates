//! `RegionEditor` — draw, move and resize named rectangular slots over a base image.
//!
//! This is the spatial half of a layer template. A trait lives in a *slot* — a
//! named rectangle on the canonical canvas — and this is where that rectangle is
//! drawn: the editor shows the image, overlays every slot, and lets a reader
//! select one, drag it, and pull a corner to resize it.
//!
//! ## Coordinates are normalised, because the canvas is not the screen
//!
//! A slot is stored as a [`NormRect`] in `0..=1`, not in pixels. The same template
//! is then drawable at a thumbnail and at a master, and it is the same value a
//! generator, an extractor and a compositor all read. Screen conversion happens
//! here and nowhere else.
//!
//! ## What the caller owns
//!
//! The [`Region`] list and the image texture. The editor holds only the two things
//! a frame cannot carry: which slot is selected, and a drag in flight. Keep a
//! [`RegionEditor`] in your state and call [`RegionEditor::show`].
//!
//! Pure: `egui` only — no I/O, no runtime bindings, so a storybook story and a
//! wasm frontend drive the same code.

use egui::{
    Align2, Color32, CursorIcon, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, TextureId, Ui, Vec2,
    pos2, vec2,
};

use crate::theme::{Radius, Space, TextSize, ThemeExt};

/// Smallest slot, as a fraction of the canvas — below this a slot is unclickable.
const MIN_SIDE: f32 = 0.02;

/// Corner-handle hit radius in pixels. `theme-exempt`: a pointer affordance, not a
/// themed size.
const GRAB: f32 = 11.0;

/// Drawn corner-handle size in pixels. `theme-exempt`: pointer affordance.
const HANDLE: f32 = 7.0;

/// A named rectangular slot on the canvas, in normalised coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    pub name: String,
    pub rect: NormRect,
}

/// A rectangle in `0..=1`, origin top-left, relative to the canvas.
///
/// Resolution-free on purpose: the template survives being drawn at any size, and
/// the same numbers drive generation, extraction and composition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NormRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl NormRect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// Keep the rect inside the unit square with at least [`MIN_SIDE`] on each edge.
    fn clamped(self) -> Self {
        let w = self.w.clamp(MIN_SIDE, 1.0);
        let h = self.h.clamp(MIN_SIDE, 1.0);
        let x = self.x.clamp(0.0, 1.0 - w);
        let y = self.y.clamp(0.0, 1.0 - h);
        Self { x, y, w, h }
    }
}

/// Which corner a resize drag is pulling.
#[derive(Debug, Clone, Copy)]
enum Corner {
    Nw,
    Ne,
    Sw,
    Se,
}

/// A drag in flight.
#[derive(Debug, Clone, Copy)]
enum Drag {
    Move(usize),
    Resize(usize, Corner),
}

/// What the editor draws over, and the slots it edits.
pub struct RegionEditorView<'a> {
    pub regions: &'a mut [Region],
    /// The image behind the slots. `None` paints a neutral field, so the editor
    /// is usable before any image is loaded.
    pub texture: Option<TextureId>,
    /// Displayed image width ÷ height.
    pub aspect: f32,
    /// Display width. Defaults to the available width.
    pub width: Option<f32>,
}

/// The editor's cross-frame state: selection and any drag in flight.
#[derive(Default)]
pub struct RegionEditor {
    selected: Option<usize>,
    drag: Option<Drag>,
}

impl RegionEditor {
    /// The selected slot, if any.
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// Select a slot by index (or clear it).
    pub fn select(&mut self, index: Option<usize>) {
        self.selected = index;
    }

    /// Draw the editor. Returns `true` when a slot moved, resized, or the
    /// selection changed.
    pub fn show(&mut self, ui: &mut Ui, view: RegionEditorView<'_>) -> bool {
        let RegionEditorView {
            regions,
            texture,
            aspect,
            width,
        } = view;

        let available = ui.available_width();
        let aspect = aspect.max(f32::EPSILON);
        // Default to the largest canvas that fits BOTH ways. Taking the full width
        // would make a tall canvas whose bottom the reader cannot reach.
        let w = match width {
            Some(width) => width.min(available).max(1.0),
            None => available.min(ui.available_height() * aspect).max(1.0),
        };
        let h = (w / aspect).max(1.0);
        let response = ui.allocate_response(vec2(w, h), Sense::click_and_drag());
        let canvas = response.rect;
        let mut changed = false;

        self.paint_background(ui, canvas, texture);
        let drag_delta = response.drag_delta();
        let pointer = response.interact_pointer_pos();

        if response.drag_started() {
            self.drag = pointer.and_then(|p| self.begin_drag(regions, canvas, p));
        }

        if response.dragged() {
            if let Some(drag) = self.drag {
                apply_drag(&mut regions[drag_index(drag)].rect, drag, drag_delta, canvas);
                changed = true;
            }
        }

        if response.drag_stopped() {
            self.drag = None;
        }

        if response.clicked() {
            if let Some(p) = pointer {
                let hit = region_at(regions, canvas, p);
                if hit != self.selected {
                    self.selected = hit;
                    changed = true;
                }
            }
        }

        self.paint_regions(ui, canvas, regions);
        self.paint_cursor(ui, &response, canvas, regions);

        changed
    }

    fn begin_drag(&self, regions: &[Region], canvas: Rect, p: Pos2) -> Option<Drag> {
        if let Some(index) = self.selected {
            if let Some(corner) = corner_at(to_screen(regions[index].rect, canvas), p) {
                return Some(Drag::Resize(index, corner));
            }
        }
        region_at(regions, canvas, p).map(Drag::Move)
    }

    fn paint_background(&self, ui: &Ui, canvas: Rect, texture: Option<TextureId>) {
        let tokens = ui.tokens();
        let painter = ui.painter();
        match texture {
            Some(id) => {
                painter.image(
                    id,
                    canvas,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            None => {
                painter.rect_filled(canvas, tokens.corner(Radius::Sm), tokens.color.bg_secondary);
                painter.text(
                    canvas.center(),
                    Align2::CENTER_CENTER,
                    "no image",
                    FontId::proportional(ui.text_size(TextSize::Base)),
                    tokens.color.text_muted,
                );
            }
        }
        painter.rect_stroke(
            canvas,
            tokens.corner(Radius::Sm),
            Stroke::new(1.0_f32, tokens.color.border),
            StrokeKind::Inside,
        );
    }

    fn paint_regions(&self, ui: &Ui, canvas: Rect, regions: &[Region]) {
        let tokens = ui.tokens();
        let painter = ui.painter();
        let font = FontId::proportional(ui.text_size(TextSize::Sm));

        for (index, region) in regions.iter().enumerate() {
            let rect = to_screen(region.rect, canvas);
            let selected = self.selected == Some(index);

            let (fill, edge, width) = if selected {
                (
                    tokens.color.accent_blue.gamma_multiply(0.18),
                    tokens.color.accent_blue,
                    2.0_f32,
                )
            } else {
                (
                    tokens.color.accent_blue.gamma_multiply(0.08),
                    tokens.color.border,
                    1.0_f32,
                )
            };

            painter.rect_filled(rect, tokens.corner(Radius::Sm), fill);
            painter.rect_stroke(
                rect,
                tokens.corner(Radius::Sm),
                Stroke::new(width, edge),
                StrokeKind::Inside,
            );
            painter.text(
                rect.min + vec2(tokens.space(Space::Sm), tokens.space(Space::Xs)),
                Align2::LEFT_TOP,
                &region.name,
                font.clone(),
                tokens.color.text_primary,
            );

            if selected {
                for corner in [Corner::Nw, Corner::Ne, Corner::Sw, Corner::Se] {
                    let center = corner_point(rect, corner);
                    let handle = Rect::from_center_size(center, Vec2::splat(HANDLE));
                    painter.rect_filled(handle, tokens.corner(Radius::Xs), tokens.color.accent_blue);
                }
            }
        }
    }

    fn paint_cursor(&self, ui: &Ui, response: &egui::Response, canvas: Rect, regions: &[Region]) {
        let Some(p) = response.hover_pos() else {
            return;
        };
        let icon = if let Some(index) = self.selected {
            match corner_at(to_screen(regions[index].rect, canvas), p) {
                Some(Corner::Nw | Corner::Se) => CursorIcon::ResizeNwSe,
                Some(Corner::Ne | Corner::Sw) => CursorIcon::ResizeNeSw,
                None => self.hover_icon(regions, canvas, p),
            }
        } else {
            self.hover_icon(regions, canvas, p)
        };
        ui.ctx().set_cursor_icon(icon);
    }

    fn hover_icon(&self, regions: &[Region], canvas: Rect, p: Pos2) -> CursorIcon {
        match region_at(regions, canvas, p) {
            Some(_) => CursorIcon::Grab,
            None => CursorIcon::Default,
        }
    }
}

fn drag_index(drag: Drag) -> usize {
    match drag {
        Drag::Move(i) | Drag::Resize(i, _) => i,
    }
}

fn apply_drag(rect: &mut NormRect, drag: Drag, delta: Vec2, canvas: Rect) {
    let d = vec2(delta.x / canvas.width(), delta.y / canvas.height());
    let moved = match drag {
        Drag::Move(_) => NormRect {
            x: rect.x + d.x,
            y: rect.y + d.y,
            ..*rect
        },
        Drag::Resize(_, corner) => {
            let mut r = *rect;
            match corner {
                Corner::Nw => {
                    r.x += d.x;
                    r.y += d.y;
                    r.w -= d.x;
                    r.h -= d.y;
                }
                Corner::Ne => {
                    r.y += d.y;
                    r.w += d.x;
                    r.h -= d.y;
                }
                Corner::Sw => {
                    r.x += d.x;
                    r.w -= d.x;
                    r.h += d.y;
                }
                Corner::Se => {
                    r.w += d.x;
                    r.h += d.y;
                }
            }
            r
        }
    };
    *rect = moved.clamped();
}

fn to_screen(rect: NormRect, canvas: Rect) -> Rect {
    Rect::from_min_size(
        canvas.min + vec2(rect.x * canvas.width(), rect.y * canvas.height()),
        vec2(rect.w * canvas.width(), rect.h * canvas.height()),
    )
}

fn corner_point(rect: Rect, corner: Corner) -> Pos2 {
    match corner {
        Corner::Nw => rect.left_top(),
        Corner::Ne => rect.right_top(),
        Corner::Sw => rect.left_bottom(),
        Corner::Se => rect.right_bottom(),
    }
}

fn corner_at(rect: Rect, p: Pos2) -> Option<Corner> {
    [
        Corner::Nw,
        Corner::Ne,
        Corner::Sw,
        Corner::Se,
    ]
    .into_iter()
    .find(|&corner| (corner_point(rect, corner) - p).length() <= GRAB)
}

/// Topmost region under the pointer.
fn region_at(regions: &[Region], canvas: Rect, p: Pos2) -> Option<usize> {
    regions
        .iter()
        .enumerate()
        .rev()
        .find(|(_, region)| to_screen(region.rect, canvas).contains(p))
        .map(|(index, _)| index)
}
