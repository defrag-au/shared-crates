//! The app shell, whole: a filterable grid of two thousand cards, a detail
//! screen behind the back button, the app bar, the navigation, a search field
//! and toasts, on the fluid stage.
//!
//! The cards are generated swatches, numbered, standing for any collection:
//! the example is about the shell, not the data.
//!
//! ```sh
//! cargo run -p macroquad-app --example shell
//! # a phone with a notch, at a moment:
//! cargo run -p macroquad-app --example shell -- --shot .tmp/shell.png --size 390x844 --insets 47,0,34,0
//! # a deep route, as a shared link opens it:
//! cargo run -p macroquad-app --example shell -- --route /item/41 --shot .tmp/item.png --size 390x844
//! ```
//!
//! Escape goes back, as the browser's back button does on the web.

use macroquad::prelude::*;
use macroquad_app::{
    AppBar, ArtGrid, BarAction, BarHit, Chip, FIELD_H, FieldKind, Glyph, GridSpec, Insets,
    Keyboard, Leading, NavItem, NavPlace, Route, Router, TextField, Toasts, Tone, bottom_nav,
    chip_row, nav_rail, shell_layout,
};
use macroquad_gamekit::{Backdrop, Frame, Game, Stage};
use macroquad_widgets::{
    Align, Line, Painter, ScrollInput, Theme, fill_rounded, stroke_rounded, theme::with_alpha,
};
use ui_theme::TextSize;

const COUNT: usize = 2000;
const TABS: [(Glyph, &str); 4] = [
    (Glyph::Grid, "Browse"),
    (Glyph::Dots, "Traits"),
    (Glyph::Tag, "Sell"),
    (Glyph::Chart, "Market"),
];
const FILTERS: [&str; 4] = ["Even", "Odd", "Warm", "Cool"];

#[derive(Clone, Debug, PartialEq)]
enum Screen {
    Tab(usize),
    Item(usize),
}

impl Route for Screen {
    fn parse(hash: &str) -> Self {
        let parts: Vec<&str> = hash.trim_matches('/').split('/').collect();
        match parts[..] {
            ["item", n] => n
                .parse()
                .ok()
                .filter(|n| *n < COUNT)
                .map_or(Screen::Tab(0), Screen::Item),
            [name] => TABS
                .iter()
                .position(|(_, l)| l.eq_ignore_ascii_case(name))
                .map_or(Screen::Tab(0), Screen::Tab),
            _ => Screen::Tab(0),
        }
    }

    fn to_hash(&self) -> String {
        match self {
            Screen::Tab(0) => "/".to_string(),
            Screen::Tab(t) => format!("/{}", TABS[*t].1.to_lowercase()),
            Screen::Item(n) => format!("/item/{n}"),
        }
    }
}

/// A swatch's hue, from its number: the golden angle, so neighbours differ.
fn hue(n: usize) -> f32 {
    (n as f32 * 0.618_034).fract()
}

fn swatch(n: usize) -> Color {
    let h = hue(n);
    let rgb = |o: f32| 0.5 + 0.38 * ((h + o) * std::f32::consts::TAU).cos();
    Color::new(rgb(0.0), rgb(2.0 / 3.0), rgb(1.0 / 3.0), 1.0)
}

fn warm(n: usize) -> bool {
    let h = hue(n);
    !(0.25..0.75).contains(&h)
}

struct Demo {
    font: Option<Font>,
    mono: Option<Font>,
    theme: Theme,
    router: Router<Screen>,
    keyboard: Keyboard,
    search: TextField,
    filters: [bool; 4],
    shown: Vec<usize>,
    grid: ArtGrid,
    toasts: Toasts,
    insets: Insets,
}

impl Demo {
    fn new(font: Option<Font>, mono: Option<Font>) -> Self {
        let mut demo = Self {
            font,
            mono,
            theme: Theme::tokyo_night(),
            router: Router::new(),
            keyboard: Keyboard::new(),
            search: TextField::new(1),
            filters: [false; 4],
            shown: Vec::new(),
            grid: ArtGrid::new(),
            toasts: Toasts::new(),
            insets: Insets::read(),
        };
        demo.refilter();
        demo
    }

    fn refilter(&mut self) {
        let query = self
            .search
            .value()
            .trim()
            .trim_start_matches('#')
            .to_string();
        let f = self.filters;
        self.shown = (0..COUNT)
            .filter(|n| query.is_empty() || n.to_string().contains(&query))
            .filter(|n| !f[0] || n % 2 == 0)
            .filter(|n| !f[1] || n % 2 == 1)
            .filter(|n| !f[2] || warm(*n))
            .filter(|n| !f[3] || !warm(*n))
            .collect();
        self.grid.to_top();
    }

    fn painter(&self, frame: &Frame, tap: Option<Vec2>) -> Painter<'_> {
        painter(&self.font, &self.mono, self.theme, frame, tap)
    }

    /// The browse screen's body: the search field and the filters pinned at
    /// the top, the grid under them.
    fn browse(&mut self, frame: &Frame, body: Rect, gutter: f32, tap: Option<Vec2>) {
        let p = painter(&self.font, &self.mono, self.theme, frame, tap);
        let c = p.theme.color;
        let field = Rect::new(
            body.x + gutter,
            body.y + 8.0,
            body.w - gutter * 2.0,
            FIELD_H,
        );
        let reply = self.search.show(
            &p,
            &mut self.keyboard,
            field,
            FieldKind::Search,
            "Search by number",
        );
        let chips: Vec<Chip> = FILTERS
            .iter()
            .zip(self.filters)
            .map(|(l, on)| Chip::new(l, on))
            .collect();
        let origin = vec2(field.x, field.bottom() + 10.0);
        let row = chip_row(&p, origin, field.w, &chips);
        let count = format!("{} of {COUNT}", self.shown.len());
        let count_box = Rect::new(field.x, origin.y + row.height + 4.0, field.w, 24.0);
        p.text_in(
            &count,
            count_box,
            p.size(TextSize::Sm),
            c.text_muted,
            Align::Start,
        );

        let area = Rect::new(
            body.x,
            count_box.bottom(),
            body.w,
            (body.bottom() - count_box.bottom()).max(0.0),
        );
        let spec = GridSpec {
            min_cell: 160.0,
            ..GridSpec::default()
        };
        let input = ScrollInput::read(frame.input.gesture.drag, frame.pointer());
        let geo = self
            .grid
            .update(area, &input, self.shown.len(), spec, frame.dt);
        let grid_tap = self.grid.filter_tap(tap);
        let p = painter(&self.font, &self.mono, self.theme, frame, grid_tap);
        let shown = &self.shown;
        self.grid
            .draw(area, &geo, shown.len(), c.text_muted, |i, r| {
                card(&p, shown[i], r)
            });
        let opened = self.grid.hit(area, &geo, shown.len(), grid_tap);

        if reply.changed {
            self.refilter();
        }
        if let Some(i) = row.tapped {
            self.filters[i] = !self.filters[i];
            self.refilter();
        }
        if let Some(i) = opened {
            let n = self.shown[i];
            self.router.push(Screen::Item(n));
        }
    }

    fn detail(&self, frame: &Frame, body: Rect, gutter: f32, n: usize) {
        let p = self.painter(frame, None);
        let c = p.theme.color;
        let w = (body.w - gutter * 2.0).min(560.0);
        let x = body.x + (body.w - w) * 0.5;
        let art = Rect::new(x, body.y + 16.0, w, w.min(body.h * 0.6));
        fill_rounded(art, 18.0, swatch(n));
        let facts = Rect::new(x, art.bottom() + 16.0, w, 64.0);
        let title = format!("Swatch #{n}");
        let sub = format!(
            "hue {:.3} · {}",
            hue(n),
            if warm(n) { "warm" } else { "cool" }
        );
        p.stack_in(
            &[
                Line::new(&title, p.size(TextSize::Xl3), c.text_primary),
                Line::new(&sub, p.size(TextSize::Md), c.text_muted).mono(),
            ],
            facts,
            Align::Start,
            12.0,
        );
    }
}

/// A painter over the fonts alone, so the rest of the app stays free to
/// change while it draws.
fn painter<'a>(
    font: &'a Option<Font>,
    mono: &'a Option<Font>,
    theme: Theme,
    frame: &Frame,
    tap: Option<Vec2>,
) -> Painter<'a> {
    Painter::new(font.as_ref(), mono.as_ref(), theme, tap)
        .pixel_scale(frame.px)
        .at_time(frame.clock as f64)
}

/// One card: the swatch as its art, its number under it.
fn card(p: &Painter, n: usize, r: Rect) {
    let c = p.theme.color;
    fill_rounded(r, 14.0, c.bg_secondary);
    let art = Rect::new(r.x, r.y, r.w, r.w);
    fill_rounded(art, 14.0, swatch(n));
    stroke_rounded(r, 14.0, 1.0, c.border);
    let caption = Rect::new(r.x + 10.0, art.bottom(), r.w - 20.0, r.h - r.w);
    let label = format!("Swatch #{n}");
    p.text_in(
        &label,
        caption,
        p.size(TextSize::Md),
        c.text_primary,
        Align::Start,
    );
    if warm(n) {
        let dot = vec2(caption.right() - 4.0, caption.center().y);
        draw_circle(dot.x, dot.y, 4.0, with_alpha(c.accent_orange, 0.9));
    }
}

impl Game for Demo {
    fn update(&mut self, frame: &Frame) {
        self.insets = Insets::read();
        self.toasts.update(frame.dt);
        if let Some(Screen::Item(n)) = self.router.poll() {
            let n = *n;
            self.toasts.push(format!("Back at swatch #{n}"), Tone::Info);
        }
        if frame.input.keys_pressed.contains(&KeyCode::Escape) && !self.keyboard.editing() {
            self.router.back();
        }
    }

    fn draw(&mut self, frame: &Frame) {
        clear_background(self.theme.color.bg_primary);
        let screen = self.router.current().clone();
        let layout = shell_layout(
            frame.viewport.size,
            self.insets,
            matches!(screen, Screen::Tab(_)),
        );
        let tap = frame.input.gesture.tap;
        // A tap on the bars or a toast is theirs alone.
        let in_chrome = |t: Vec2| {
            layout.bar.contains(t)
                || match layout.nav {
                    Some(NavPlace::Bottom { band, .. }) => band.contains(t),
                    Some(NavPlace::Rail(r)) => r.contains(t),
                    None => false,
                }
        };
        let body_tap = tap.filter(|t| !in_chrome(*t));

        self.keyboard.begin(frame);
        match screen {
            Screen::Tab(0) => self.browse(frame, layout.body, layout.gutter, body_tap),
            Screen::Tab(t) => {
                let p = self.painter(frame, None);
                let note = format!("{} would be here.", TABS[t].1);
                p.text_in(
                    &note,
                    layout.body,
                    p.size(TextSize::Lg),
                    p.theme.color.text_muted,
                    Align::Center,
                );
            }
            Screen::Item(n) => self.detail(frame, layout.body, layout.gutter, n),
        }
        self.keyboard.end();

        let p = painter(&self.font, &self.mono, self.theme, frame, tap);
        let filtering = self.filters.iter().any(|f| *f);
        let actions = [
            if filtering {
                BarAction::Marked(Glyph::Filter)
            } else {
                BarAction::Icon(Glyph::Filter)
            },
            BarAction::Chip {
                glyph: Some(Glyph::Wallet),
                label: "Connect",
            },
        ];
        let (title, leading) = match screen {
            Screen::Tab(t) => (TABS[t].1.to_string(), Leading::None),
            Screen::Item(n) => (format!("Swatch #{n}"), Leading::Back),
        };
        let raised = match screen {
            Screen::Tab(0) => (self.grid.scroll().offset() / 24.0).min(1.0),
            _ => 0.0,
        };
        let bar = AppBar::new(&title)
            .subtitle("macroquad-app shell")
            .leading(leading)
            .actions(&actions)
            .raised(raised)
            .show(&p, layout.bar, layout.bar_row);
        let items: Vec<NavItem> = TABS
            .iter()
            .enumerate()
            .map(|(i, (g, l))| NavItem::new(*g, l).locked(i == 2))
            .collect();
        let selected = match screen {
            Screen::Tab(t) => t,
            Screen::Item(_) => 0,
        };
        let nav = match layout.nav {
            Some(NavPlace::Bottom { band, row }) => bottom_nav(&p, band, row, &items, selected),
            Some(NavPlace::Rail(rail)) => nav_rail(&p, rail, &items, selected),
            None => None,
        };
        let foot = match layout.nav {
            Some(NavPlace::Bottom { band, .. }) => band.y,
            _ => layout.safe.bottom(),
        };
        self.toasts.show(&p, layout.body.x, layout.body.w, foot);

        match bar {
            Some(BarHit::Leading) => self.router.up(Screen::Tab(0)),
            Some(BarHit::Action(0)) => self.toasts.push("Filters would open here", Tone::Info),
            Some(BarHit::Action(_)) => self.toasts.push("No wallet in this example", Tone::Warning),
            None => {}
        }
        match nav {
            Some(2) => self.toasts.push("Sell needs a wallet first", Tone::Warning),
            Some(t) if Screen::Tab(t) == screen => self.grid.to_top(),
            Some(t) => self.router.replace(Screen::Tab(t)),
            None => {}
        }
    }

    fn backdrop(&self) -> Backdrop {
        let c = self.theme.color;
        Backdrop {
            top: c.bg_primary,
            bottom: c.bg_primary,
            accent: c.accent,
        }
    }

    fn setup(&mut self, line: &str) -> Result<(), String> {
        match line.split_once(' ') {
            Some(("search", q)) => {
                self.search.set(&self.keyboard, q);
                self.refilter();
                Ok(())
            }
            Some(("toast", text)) => {
                self.toasts.push(text, Tone::Success);
                Ok(())
            }
            _ => Err(format!(
                "unknown set line `{line}`: try `search 12` or `toast Copied`"
            )),
        }
    }
}

fn window_conf() -> Conf {
    Conf {
        window_title: "macroquad-app shell".to_string(),
        window_width: 390,
        window_height: 844,
        high_dpi: true,
        window_resizable: true,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let font = macroquad_gamekit::load_font(include_bytes!(
        "../../macroquad-storybook/assets/NotoSans-Bold.ttf"
    ))
    .ok();
    let mono = macroquad_gamekit::load_font(include_bytes!(
        "../../macroquad-storybook/assets/JetBrainsMono-Regular.ttf"
    ))
    .ok();
    macroquad_gamekit::app::run_on(Stage::Fluid, Demo::new(font, mono)).await;
}
