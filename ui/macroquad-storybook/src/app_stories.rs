//! Stories for `macroquad-app`, the app shell: the app bar, the navigation,
//! the text field, chips, toasts and the virtualised grid, each at a phone's
//! width (and the rail at a desk's), so a change to one is reviewed by looking
//! at it.
//!
//! The grid's cards are generated swatches, numbered: the grid is geometry and
//! a scroll, and these stories are about that, not about any collection.
//! `ui/macroquad-app/examples/shell.rs` is the whole shell in one app.

use macroquad::prelude::*;
use macroquad_app::{
    AppBar, ArtGrid, BarAction, Chip, FIELD_H, FieldKind, Glyph, GridSpec, Insets, Keyboard,
    Leading, NavItem, NavPlace, TextField, Toasts, Tone, bottom_nav, chip_row, nav_rail,
    shell_layout,
};
use macroquad_widgets::{
    Align, Gesture, Painter, ScrollInput, TextSize, fill_rounded, stroke_rounded,
};

/// A phone's width: where every one of these is designed first.
const PHONE_W: f32 = 390.0;

/// Which part of the shell a story shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppView {
    /// The bar at the top level, inside a screen, and raised over content.
    Bar,
    /// The bottom bar at a phone's width, and the rail at a desk's.
    Nav,
    /// The text field empty and filled; click one to type into it.
    Field,
    /// Filter chips wrapping across a phone, some on, some with counts.
    Chips,
    /// A stack of toasts, one a job with its progress.
    Toasts,
    /// Two thousand cards, of which only those in view are drawn.
    Grid,
    /// The whole shell at a phone's size: the bottom bar and phone insets.
    ShellPhone,
    /// The whole shell at a desk's size: the rail and no insets.
    ShellDesk,
}

pub struct AppStory {
    view: AppView,
    keyboard: Keyboard,
    empty: TextField,
    filled: TextField,
    chips: [bool; 6],
    toasts: Toasts,
    grid: ArtGrid,
    last: String,
}

impl AppStory {
    pub fn new(view: AppView) -> Self {
        let mut filled = TextField::new(2);
        let keyboard = Keyboard::new();
        filled.set(&keyboard, "stake1u8a9...0x3f");
        let mut toasts = Toasts::new();
        if view == AppView::Toasts {
            toasts.push("Copied the policy id", Tone::Success);
            toasts.push("Listing data is two minutes old", Tone::Warning);
            toasts.job("names", "Naming wallets 12/40", Some(0.3));
            // Past their entrances, so a capture shows them whole.
            toasts.update(0.5);
        }
        Self {
            view,
            keyboard,
            empty: TextField::new(1),
            filled,
            chips: [true, false, false, true, false, false],
            toasts,
            grid: ArtGrid::new(),
            last: String::new(),
        }
    }

    /// The size this story is drawn for, in points: a phone's width, and the
    /// height its content needs. A `--no-chrome` capture is exactly this.
    pub fn canvas(&self) -> Vec2 {
        match self.view {
            AppView::Bar => vec2(PHONE_W, 380.0),
            AppView::Nav => vec2(PHONE_W, 560.0),
            AppView::Field => vec2(PHONE_W, 300.0),
            AppView::Chips => vec2(PHONE_W, 240.0),
            AppView::Toasts => vec2(PHONE_W, 320.0),
            AppView::Grid => vec2(PHONE_W, 844.0),
            AppView::ShellPhone => vec2(PHONE_W, 844.0),
            AppView::ShellDesk => vec2(1280.0, 800.0),
        }
    }

    /// The whole shell at the story's canvas: the app bar, a scrolling body,
    /// and the navigation. `shell_layout` picks the rail on a wide canvas and
    /// the bottom bar on a phone, and the phone variant models its insets, so
    /// one body covers both compositions.
    fn shell(&mut self, p: &Painter, area: Rect, gesture: &Gesture, dt: f32) {
        let insets = if self.view == AppView::ShellPhone {
            Insets {
                top: 47.0,
                bottom: 34.0,
                ..Insets::default()
            }
        } else {
            Insets::default()
        };
        let layout = shell_layout(vec2(area.w, area.h), insets, true);
        let shift = |r: Rect| Rect::new(r.x + area.x, r.y + area.y, r.w, r.h);
        let c = p.theme.color;

        // The body: a grid of swatches, scrolling in its own viewport.
        let body = shift(layout.body);
        let input = ScrollInput::read(gesture.drag, Vec2::from(mouse_position()));
        let spec = GridSpec {
            min_cell: 150.0,
            ..GridSpec::default()
        };
        let geo = self.grid.update(body, &input, 2000, spec, dt);
        self.grid.draw(body, &geo, 2000, c.text_muted, |i, r| {
            let hue = (i as f32 * 0.618_034).fract();
            let ch = |o: f32| 0.5 + 0.38 * ((hue + o) * std::f32::consts::TAU).cos();
            fill_rounded(r, 14.0, c.bg_secondary);
            let art = Rect::new(r.x, r.y, r.w, r.w);
            fill_rounded(art, 14.0, Color::new(ch(0.0), ch(0.667), ch(0.333), 1.0));
            let cap = Rect::new(r.x + 10.0, art.bottom(), r.w - 20.0, r.h - r.w);
            let name = format!("Swatch #{i}");
            p.text_in(
                &name,
                cap,
                p.size(TextSize::Md),
                c.text_primary,
                Align::Start,
            );
        });

        let actions = [
            BarAction::Icon(Glyph::Search),
            BarAction::Chip {
                glyph: Some(Glyph::Wallet),
                label: "Connect",
            },
        ];
        let _ = AppBar::new("Collections")
            .subtitle("2,000 assets · 12 holders")
            .actions(&actions)
            .show(p, shift(layout.bar), shift(layout.bar_row));

        let items = [
            NavItem::new(Glyph::Grid, "Browse"),
            NavItem::new(Glyph::Dots, "Traits").locked(true),
            NavItem::new(Glyph::Tag, "Sell").locked(true),
            NavItem::new(Glyph::Chart, "Market"),
        ];
        match layout.nav {
            Some(NavPlace::Bottom { band, row }) => {
                let _ = bottom_nav(p, shift(band), shift(row), &items, 0);
            }
            Some(NavPlace::Rail(rail)) => {
                let _ = nav_rail(p, shift(rail), &items, 0);
            }
            None => {}
        }
    }

    pub fn draw(&mut self, p: &Painter, area: Rect, gesture: &Gesture, dt: f32) {
        let c = p.theme.color;
        let phone = Rect::new(area.x, area.y, PHONE_W.min(area.w), area.h.min(760.0));
        let label = |text: &str, y: f32| {
            let r = Rect::new(area.x, y, area.w, 18.0);
            p.text_in(text, r, p.size(TextSize::Sm), c.text_muted, Align::Start);
        };
        match self.view {
            AppView::ShellPhone | AppView::ShellDesk => self.shell(p, area, gesture, dt),
            AppView::Bar => {
                let mut y = area.y;
                let filter = [
                    BarAction::Icon(Glyph::Search),
                    BarAction::Marked(Glyph::Filter),
                ];
                let wallet = [BarAction::Chip {
                    glyph: Some(Glyph::Wallet),
                    label: "2 wallets",
                }];
                let bars = [
                    (
                        "top level, a chip",
                        AppBar::new("Collections").actions(&wallet),
                        0.0,
                    ),
                    (
                        "inside a screen: back, a subtitle, a filter in force",
                        AppBar::new("Toolheads")
                            .subtitle("8,888 assets · 2,410 holders")
                            .leading(Leading::Back)
                            .actions(&filter),
                        0.0,
                    ),
                    (
                        "raised: content has scrolled under it, a long title cut",
                        AppBar::new("A collection whose name is far too long for a phone")
                            .leading(Leading::Back)
                            .actions(&filter),
                        1.0,
                    ),
                ];
                for (note, bar, raised) in bars {
                    label(note, y);
                    let band = Rect::new(area.x, y + 22.0, PHONE_W, 56.0);
                    stroke_rounded(band, 0.0, 1.0, c.border);
                    if let Some(hit) = bar.raised(raised).show(p, band, band) {
                        self.last = format!("{hit:?}");
                    }
                    y = band.bottom() + 24.0;
                }
                label(&format!("last tap: {}", self.last), y);
            }
            AppView::Nav => {
                let items = [
                    NavItem::new(Glyph::Grid, "Browse"),
                    NavItem::new(Glyph::Dots, "Traits").locked(true),
                    NavItem::new(Glyph::Tag, "Sell").locked(true),
                    NavItem::new(Glyph::Chart, "Market"),
                ];
                label("bottom bar, a phone (two need a wallet)", area.y);
                let row = Rect::new(area.x, area.y + 24.0, PHONE_W, 64.0);
                let band = Rect::new(row.x, row.y, row.w, 64.0 + 34.0);
                if let Some(i) = bottom_nav(p, band, row, &items, 0) {
                    self.last = items[i].label.to_string();
                }
                label("rail, anything wider", band.bottom() + 20.0);
                let rail = Rect::new(area.x, band.bottom() + 44.0, 88.0, 320.0);
                if let Some(i) = nav_rail(p, rail, &items, 3) {
                    self.last = items[i].label.to_string();
                }
                label(&format!("last tap: {}", self.last), rail.bottom() + 12.0);
            }
            AppView::Field => {
                self.keyboard.begin_at(1.0, Vec2::ZERO);
                let w = PHONE_W - 24.0;
                label("search, empty (click to type)", area.y);
                let a = Rect::new(area.x, area.y + 22.0, w, FIELD_H);
                let ra = self.empty.show(
                    p,
                    &mut self.keyboard,
                    a,
                    FieldKind::Search,
                    "Search by name",
                );
                label("an address, filled: the cross clears it", a.bottom() + 20.0);
                let b = Rect::new(area.x, a.bottom() + 42.0, w, FIELD_H);
                let rb = self.filled.show(
                    p,
                    &mut self.keyboard,
                    b,
                    FieldKind::Address,
                    "$handle, stake1… or 0x…",
                );
                self.keyboard.end();
                if ra.submitted || rb.submitted {
                    self.last = "submitted".to_string();
                }
                label(
                    &format!("editing: {} {}", self.keyboard.editing(), self.last),
                    b.bottom() + 20.0,
                );
            }
            AppView::Chips => {
                let names = [
                    "Listed",
                    "Best deals",
                    "Owned",
                    "Rare first",
                    "Pixel",
                    "1/1s",
                ];
                let counts = ["412", "38", "", "", "1,204", "12"];
                let chips: Vec<Chip> = names
                    .iter()
                    .zip(counts)
                    .zip(self.chips)
                    .map(|((n, k), on)| {
                        let chip = Chip::new(n, on);
                        if k.is_empty() { chip } else { chip.count(k) }
                    })
                    .collect();
                label(
                    "wrapping at a phone's width, not scrolling sideways",
                    area.y,
                );
                let row = chip_row(p, vec2(area.x, area.y + 26.0), PHONE_W - 24.0, &chips);
                if let Some(i) = row.tapped {
                    self.chips[i] = !self.chips[i];
                }
                label(
                    &format!("rows {:.0} high", row.height),
                    area.y + 40.0 + row.height,
                );
            }
            AppView::Toasts => {
                self.toasts.update(dt);
                fill_rounded(phone, 18.0, c.bg_primary);
                stroke_rounded(phone, 18.0, 1.0, c.border);
                self.toasts.show(p, phone.x, phone.w, phone.bottom() - 16.0);
            }
            AppView::Grid => {
                stroke_rounded(phone, 0.0, 1.0, c.border);
                let count = 2000;
                let spec = GridSpec {
                    min_cell: 160.0,
                    ..GridSpec::default()
                };
                let input = ScrollInput::read(gesture.drag, Vec2::from(mouse_position()));
                let geo = self.grid.update(phone, &input, count, spec, dt);
                let shown = self.grid.visible(phone, &geo, count, 0);
                self.grid.draw(phone, &geo, count, c.text_muted, |i, r| {
                    let hue = (i as f32 * 0.618_034).fract();
                    let ch = |o: f32| 0.5 + 0.38 * ((hue + o) * std::f32::consts::TAU).cos();
                    fill_rounded(r, 14.0, c.bg_secondary);
                    let art = Rect::new(r.x, r.y, r.w, r.w);
                    fill_rounded(art, 14.0, Color::new(ch(0.0), ch(0.667), ch(0.333), 1.0));
                    let cap = Rect::new(r.x + 10.0, art.bottom(), r.w - 20.0, r.h - r.w);
                    let name = format!("Swatch #{i}");
                    p.text_in(
                        &name,
                        cap,
                        p.size(TextSize::Md),
                        c.text_primary,
                        Align::Start,
                    );
                });
                let note = format!(
                    "{} of {count} cards drawn this frame",
                    shown.end - shown.start
                );
                label(&note, phone.bottom() + 8.0);
            }
        }
    }
}
