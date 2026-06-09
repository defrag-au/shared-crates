//! Stories for `macroquad-widgets`. Native macroquad app — run with
//! `cargo run -p macroquad-storybook`.
//!
//! Left **sidebar** lists stories by category (click / ←→ / number keys, selected
//! highlighted); the stage renders the selected story. Interactive fulfilment
//! stories add a **knobs** panel and a **simulate poll** story that auto-advances
//! the VM (minted ticks up, txs land, then confirm). The **buttons** atom story
//! shows the variant × state matrix — hover and press them to feel the states.
//!
//! Scroll is intentionally deferred until the story list overflows.

use macroquad::prelude::*;
use macroquad_widgets::{
    frame_tap, mint_checkout, order_fulfilment, quantity_stepper, theme, wallet_connect, Button,
    ButtonVariant, CheckoutAction, CheckoutState, Eligibility, FulfilmentAction, FulfilmentStatus,
    FulfilmentTx, MintCheckoutVm, OrderFulfilmentVm, OrderStatus, Painter, QuantityStepperVm,
    StepperAction, Theme, WalletAction, WalletConnectVm, WalletItem, WalletState,
};

const SIDEBAR_W: f32 = 210.0;
const SIM_INTERVAL: f64 = 1.2;
const SIM_CHUNKS: [u32; 3] = [3, 3, 2];

const PAYMENT: &str = "70f20c08ac4b1e9d3f5a2c6b8e0d1f4a7c9b2e5d8f1a3c6b9e2d5f8a1c4b00119c";
const MINT_A: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f01a";
const MINT_B: &str = "9f8e7d6c5b4a39281706f5e4d3c2b1a09f8e7d6c5b4a39281706f5e4d3c2b1b2b";
const MINT_C: &str = "0c1d2e3f405162738495a6b7c8d9e0f10c1d2e3f405162738495a6b7c8d9e03c3";
const MINT_D: &str = "feedfacecafebeef0123456789abcdeffeedfacecafebeef0123456789abcd4d4d";
const POOL: [&str; 4] = [MINT_A, MINT_B, MINT_C, MINT_D];

const NUM_KEYS: [KeyCode; 9] = [
    KeyCode::Key1,
    KeyCode::Key2,
    KeyCode::Key3,
    KeyCode::Key4,
    KeyCode::Key5,
    KeyCode::Key6,
    KeyCode::Key7,
    KeyCode::Key8,
    KeyCode::Key9,
];

#[derive(Clone, Copy)]
enum StoryMode {
    Static,
    Knobs,
    Simulate,
}

#[derive(Clone, Copy)]
enum Knob {
    Status,
    MintUp,
    MintDown,
    AddTx,
    Confirm,
    Reset,
}

struct Fulfilment {
    vm: OrderFulfilmentVm,
    mode: StoryMode,
    sim_accum: f64,
    sim_chunk_idx: usize,
    paused: bool,
}

enum Body {
    Buttons,
    Stepper(u32),
    Wallet(WalletConnectVm),
    Checkout(MintCheckoutVm),
    Fulfilment(Fulfilment),
}

/// Cheap copy of the story kind — lets `draw_main` dispatch without holding a
/// borrow of `self.stories` across the per-kind mutation.
#[derive(Clone, Copy)]
enum Kind {
    Buttons,
    Stepper,
    Wallet,
    Checkout,
    Fulfilment,
}

struct Story {
    category: &'static str,
    name: &'static str,
    body: Body,
}

impl Story {
    fn fulfilment(
        category: &'static str,
        name: &'static str,
        mode: StoryMode,
        vm: OrderFulfilmentVm,
    ) -> Self {
        Self {
            category,
            name,
            body: Body::Fulfilment(Fulfilment {
                vm,
                mode,
                sim_accum: 0.0,
                sim_chunk_idx: 0,
                paused: false,
            }),
        }
    }

    fn buttons(category: &'static str, name: &'static str) -> Self {
        Self {
            category,
            name,
            body: Body::Buttons,
        }
    }

    fn stepper(category: &'static str, name: &'static str, qty: u32) -> Self {
        Self {
            category,
            name,
            body: Body::Stepper(qty),
        }
    }

    fn wallet(category: &'static str, name: &'static str, state: WalletState) -> Self {
        Self {
            category,
            name,
            body: Body::Wallet(WalletConnectVm { state }),
        }
    }

    fn checkout(category: &'static str, name: &'static str, vm: MintCheckoutVm) -> Self {
        Self {
            category,
            name,
            body: Body::Checkout(vm),
        }
    }

    fn kind(&self) -> Kind {
        match self.body {
            Body::Buttons => Kind::Buttons,
            Body::Stepper(_) => Kind::Stepper,
            Body::Wallet(_) => Kind::Wallet,
            Body::Checkout(_) => Kind::Checkout,
            Body::Fulfilment(_) => Kind::Fulfilment,
        }
    }
}

fn fx(
    status: OrderStatus,
    quantity: u32,
    minted: u32,
    fulfilments: Vec<FulfilmentTx>,
    ago: u32,
) -> OrderFulfilmentVm {
    OrderFulfilmentVm {
        status,
        quantity,
        minted,
        payment_tx: PAYMENT.into(),
        fulfilments,
        updated_secs_ago: Some(ago),
    }
}

fn tx(hash: &str, minted: u32, status: FulfilmentStatus) -> FulfilmentTx {
    FulfilmentTx {
        tx_hash: hash.into(),
        minted,
        status,
    }
}

fn playground_vm() -> OrderFulfilmentVm {
    fx(
        OrderStatus::Fulfilling,
        6,
        2,
        vec![tx(MINT_A, 2, FulfilmentStatus::Submitted)],
        2,
    )
}

fn simulate_vm() -> OrderFulfilmentVm {
    fx(OrderStatus::Pending, 8, 0, vec![], 0)
}

const STAKE_ADDR: &str = "stake_test1uqxvtxc9k7yg3m2p0lz8w4n6s5d7f9h0j2k4m6n8p0r3s599tqx";

/// `sample_icon` stands in for a decoded CIP-30 icon on the first wallet, so the
/// texture path renders; the rest fall back to monogram avatars.
fn stories(sample_icon: Option<Texture2D>) -> Vec<Story> {
    use FulfilmentStatus as F;
    use OrderStatus as S;
    use StoryMode::*;
    vec![
        Story::buttons("atoms", "buttons"),
        Story::stepper("atoms", "quantity stepper", 2),
        Story::wallet(
            "wallet",
            "disconnected",
            WalletState::Disconnected(vec![
                WalletItem {
                    key: "eternl".into(),
                    name: "Eternl".into(),
                    icon: sample_icon,
                },
                WalletItem::new("vespr", "Vespr"),
                WalletItem::new("lace", "Lace"),
            ]),
        ),
        Story::wallet("wallet", "no wallet", WalletState::Disconnected(vec![])),
        Story::wallet("wallet", "connecting", WalletState::Connecting),
        Story::wallet(
            "wallet",
            "connected",
            WalletState::Connected {
                name: "Eternl".into(),
                address: STAKE_ADDR.into(),
            },
        ),
        Story::wallet(
            "wallet",
            "error",
            WalletState::Error("user declined the connection".into()),
        ),
        Story::checkout(
            "checkout",
            "eligible",
            MintCheckoutVm {
                phase_label: Some("public".into()),
                eligibility: Eligibility::Eligible { max_per_wallet: 5 },
                unit_price_lovelace: 40_000_000,
                qty: 1,
                state: CheckoutState::Idle,
            },
        ),
        Story::checkout(
            "checkout",
            "ineligible",
            MintCheckoutVm {
                phase_label: Some("allowlist".into()),
                eligibility: Eligibility::Ineligible {
                    reason: "not eligible — wrong phase, sold out, or limit reached".into(),
                },
                unit_price_lovelace: 40_000_000,
                qty: 1,
                state: CheckoutState::Idle,
            },
        ),
        Story::checkout(
            "checkout",
            "working",
            MintCheckoutVm {
                phase_label: Some("public".into()),
                eligibility: Eligibility::Eligible { max_per_wallet: 5 },
                unit_price_lovelace: 40_000_000,
                qty: 2,
                state: CheckoutState::Working("awaiting signature for 80 ADA...".into()),
            },
        ),
        Story::fulfilment(
            "fulfilment",
            "pending",
            Static,
            fx(S::Pending, 3, 0, vec![], 1),
        ),
        Story::fulfilment(
            "fulfilment",
            "fulfilling 1 tx",
            Static,
            fx(S::Fulfilling, 5, 2, vec![tx(MINT_A, 2, F::Submitted)], 3),
        ),
        Story::fulfilment(
            "fulfilment",
            "fulfilling N txs",
            Static,
            fx(
                S::Fulfilling,
                10,
                7,
                vec![tx(MINT_A, 4, F::Confirmed), tx(MINT_B, 3, F::Submitted)],
                0,
            ),
        ),
        Story::fulfilment(
            "fulfilment",
            "confirmed",
            Static,
            fx(S::Confirmed, 3, 3, vec![tx(MINT_A, 3, F::Confirmed)], 30),
        ),
        Story::fulfilment(
            "fulfilment",
            "sold out",
            Static,
            fx(S::Unfulfilled, 2, 0, vec![], 12),
        ),
        Story::fulfilment("interactive", "knobs playground", Knobs, playground_vm()),
        Story::fulfilment("interactive", "simulate poll", Simulate, simulate_vm()),
    ]
}

fn next_status(s: OrderStatus) -> OrderStatus {
    use OrderStatus::*;
    match s {
        Pending => Fulfilling,
        Fulfilling => Submitted,
        Submitted => Confirmed,
        Confirmed => Delivered,
        Delivered => Unfulfilled,
        Unfulfilled => Failed,
        Failed => Pending,
    }
}

/// The buttons atom story — variant × state matrix + accent swatches.
fn button_gallery(p: &Painter, x: f32, mut y: f32, _w: f32) -> Option<String> {
    let mut clicked = None;
    let (bw, bh, gap) = (130.0, 38.0, 12.0);
    for (name, variant) in [
        ("filled", ButtonVariant::Filled),
        ("tonal", ButtonVariant::Tonal),
        ("ghost", ButtonVariant::Ghost),
    ] {
        p.text(name, x, y, 13.0, p.theme.muted);
        y += 10.0;
        if Button::new("Mint")
            .variant(variant)
            .show(p, Rect::new(x, y, bw, bh))
        {
            clicked = Some(format!("clicked {name}"));
        }
        Button::new("Disabled")
            .variant(variant)
            .enabled(false)
            .show(p, Rect::new(x + bw + gap, y, bw, bh));
        y += bh + 14.0;
    }

    p.text("accents", x, y, 13.0, p.theme.muted);
    y += 10.0;
    let mut bx = x;
    for (name, accent) in [
        ("accent", p.theme.accent),
        ("link", p.theme.link),
        ("danger", p.theme.danger),
    ] {
        if Button::new("tap")
            .accent(accent)
            .show(p, Rect::new(bx, y, 90.0, bh))
        {
            clicked = Some(format!("clicked {name}"));
        }
        bx += 90.0 + gap;
    }
    y += bh + 16.0;
    p.text(
        "hover + press to feel the states",
        x,
        y,
        12.0,
        p.theme.muted,
    );
    clicked
}

struct Storybook {
    stories: Vec<Story>,
    selected: usize,
    theme_idx: usize,
    last_action: Option<String>,
}

impl Storybook {
    fn new() -> Self {
        let icon = Texture2D::from_file_with_format(
            include_bytes!("../assets/sample-icon.png"),
            Some(ImageFormat::Png),
        );
        Self {
            stories: stories(Some(icon)),
            selected: 0,
            theme_idx: 0,
            last_action: None,
        }
    }

    fn current_theme(&self) -> Theme {
        Theme::PRESETS[self.theme_idx]()
    }

    fn frame(&mut self, p: &Painter) {
        self.handle_keys();
        self.draw_sidebar(p);
        self.advance_sim(self.selected);
        self.draw_main(p);
    }

    fn handle_keys(&mut self) {
        let n = self.stories.len();
        if is_key_pressed(KeyCode::Right) || is_key_pressed(KeyCode::Down) {
            self.select((self.selected + 1) % n);
        }
        if is_key_pressed(KeyCode::Left) || is_key_pressed(KeyCode::Up) {
            self.select((self.selected + n - 1) % n);
        }
        if is_key_pressed(KeyCode::T) {
            self.theme_idx = (self.theme_idx + 1) % Theme::PRESETS.len();
        }
        for (i, key) in NUM_KEYS.iter().enumerate() {
            if i < n && is_key_pressed(*key) {
                self.select(i);
            }
        }
    }

    fn select(&mut self, i: usize) {
        self.selected = i;
        self.last_action = None;
    }

    fn draw_sidebar(&mut self, p: &Painter) {
        draw_rectangle(0.0, 0.0, SIDEBAR_W, screen_height(), p.theme.panel);
        draw_line(
            SIDEBAR_W,
            0.0,
            SIDEBAR_W,
            screen_height(),
            1.0,
            p.theme.track,
        );
        let (mx, my) = mouse_position();
        let mouse = vec2(mx, my);

        let mut y = 40.0;
        p.text("STORYBOOK", 16.0, y, 16.0, p.theme.accent);
        y += 30.0;

        let mut last_cat = "";
        let mut clicked = None;
        for (i, s) in self.stories.iter().enumerate() {
            if s.category != last_cat {
                last_cat = s.category;
                y += 6.0;
                p.text(s.category, 16.0, y, 11.0, p.theme.muted);
                y += 18.0;
            }
            let row = Rect::new(6.0, y - 13.0, SIDEBAR_W - 12.0, 24.0);
            let selected = i == self.selected;
            if selected {
                draw_rectangle(
                    row.x,
                    row.y,
                    row.w,
                    row.h,
                    theme::with_alpha(p.theme.accent, 0.16),
                );
                draw_rectangle(row.x, row.y, 3.0, row.h, p.theme.accent);
            } else if row.contains(mouse) {
                draw_rectangle(
                    row.x,
                    row.y,
                    row.w,
                    row.h,
                    theme::with_alpha(p.theme.fg, 0.05),
                );
            }
            let label = format!("{}. {}", i + 1, s.name);
            let baseline = p.centre_baseline(row.y, row.h, 13.0);
            p.text(
                &label,
                18.0,
                baseline,
                13.0,
                if selected { p.theme.accent } else { p.theme.fg },
            );
            if p.tapped(row) {
                clicked = Some(i);
            }
            y += 26.0;
        }
        if let Some(i) = clicked {
            self.select(i);
        }
    }

    fn draw_main(&mut self, p: &Painter) {
        let sel = self.selected;
        let x0 = SIDEBAR_W + 28.0;
        let col_w = (screen_width() - x0 - 28.0).clamp(280.0, 460.0);
        let mut y = 44.0;

        p.text(
            &format!(
                "{}  >  {}",
                self.stories[sel].category, self.stories[sel].name
            ),
            x0,
            y,
            19.0,
            p.theme.fg,
        );
        y += 20.0;
        p.text(
            &format!("arrows / click to switch  ·  t = theme: {}", p.theme.name),
            x0,
            y,
            12.0,
            p.theme.muted,
        );
        y += 30.0;

        match self.stories[sel].kind() {
            Kind::Buttons => {
                if let Some(a) = button_gallery(p, x0, y, col_w) {
                    self.last_action = Some(a);
                }
                self.echo(p, x0);
            }
            Kind::Stepper => self.draw_stepper(p, sel, x0, y),
            Kind::Wallet => self.draw_wallet(p, sel, x0, y, col_w),
            Kind::Checkout => self.draw_checkout(p, sel, x0, y, col_w),
            Kind::Fulfilment => self.draw_fulfilment(p, sel, x0, y, col_w),
        }
    }

    fn echo(&self, p: &Painter, x: f32) {
        if let Some(a) = &self.last_action {
            p.text(a, x, screen_height() - 32.0, 13.0, p.theme.accent);
        }
    }

    fn draw_stepper(&mut self, p: &Painter, sel: usize, x: f32, y: f32) {
        let qty = match &self.stories[sel].body {
            Body::Stepper(q) => *q,
            _ => return,
        };
        let svm = QuantityStepperVm {
            qty,
            min: 1,
            max: 10,
        };
        let resp = quantity_stepper(p, &svm, x, y, 36.0, true);
        if let Some(StepperAction::Changed(n)) = resp.action {
            if let Body::Stepper(q) = &mut self.stories[sel].body {
                *q = n;
            }
        }
        p.text_top(
            &format!("min 1 · max 10 · qty = {qty}"),
            x,
            y + 52.0,
            13.0,
            p.theme.muted,
        );
    }

    fn draw_wallet(&mut self, p: &Painter, sel: usize, x: f32, y: f32, w: f32) {
        let action = match &self.stories[sel].body {
            Body::Wallet(vm) => wallet_connect(p, vm, x, y, w).action,
            _ => return,
        };
        if let Some(a) = action {
            self.last_action = Some(match a {
                WalletAction::Connect(k) => format!("action: Connect({k})"),
                WalletAction::Disconnect => "action: Disconnect".into(),
                WalletAction::Retry => "action: Retry".into(),
            });
        }
        self.echo(p, x);
    }

    fn draw_checkout(&mut self, p: &Painter, sel: usize, x: f32, y: f32, w: f32) {
        let action = match &self.stories[sel].body {
            Body::Checkout(vm) => mint_checkout(p, vm, x, y, w).action,
            _ => return,
        };
        match action {
            Some(CheckoutAction::QtyChanged(n)) => {
                if let Body::Checkout(vm) = &mut self.stories[sel].body {
                    vm.qty = n;
                }
            }
            Some(CheckoutAction::Mint) => self.last_action = Some("action: Mint".into()),
            None => {}
        }
        self.echo(p, x);
    }

    fn draw_fulfilment(&mut self, p: &Painter, sel: usize, x0: f32, y: f32, col_w: f32) {
        let (action, bottom, mode) = match &self.stories[sel].body {
            Body::Fulfilment(f) => {
                let resp = order_fulfilment(p, &f.vm, x0, y, col_w);
                (resp.action, resp.bottom, f.mode)
            }
            _ => return,
        };
        let mut bottom = bottom;
        match mode {
            StoryMode::Static => {}
            StoryMode::Knobs => bottom = self.draw_knobs(p, sel, x0, bottom + 14.0, col_w),
            StoryMode::Simulate => bottom = self.draw_sim_controls(p, sel, x0, bottom + 14.0),
        }
        if let Some(FulfilmentAction::OpenTx(h)) = action {
            self.last_action = Some(format!("action: OpenTx({h})"));
        }
        if let Some(a) = &self.last_action {
            p.mono(a, x0, bottom + 18.0, 13.0, p.theme.accent);
        }
    }

    fn draw_knobs(&mut self, p: &Painter, sel: usize, x: f32, y: f32, w: f32) -> f32 {
        const ITEMS: [(&str, Knob); 6] = [
            ("status>", Knob::Status),
            ("mint+", Knob::MintUp),
            ("mint-", Knob::MintDown),
            ("+tx", Knob::AddTx),
            ("confirm", Knob::Confirm),
            ("reset", Knob::Reset),
        ];
        p.text("knobs", x, y, 13.0, p.theme.muted);
        let (bw, bh, gap) = (92.0, 30.0, 8.0);
        let mut bx = x;
        let mut by = y + 8.0;
        let mut acts: Vec<Knob> = Vec::new();
        for (label, knob) in ITEMS {
            if bx + bw > x + w + 0.5 {
                bx = x;
                by += bh + gap;
            }
            if Button::new(label)
                .variant(ButtonVariant::Tonal)
                .font_size(15.0)
                .show(p, Rect::new(bx, by, bw, bh))
            {
                acts.push(knob);
            }
            bx += bw + gap;
        }
        for k in acts {
            self.apply_knob(sel, k);
        }
        by + bh
    }

    fn apply_knob(&mut self, sel: usize, k: Knob) {
        let Body::Fulfilment(f) = &mut self.stories[sel].body else {
            return;
        };
        let vm = &mut f.vm;
        match k {
            Knob::Status => vm.status = next_status(vm.status),
            Knob::MintUp => vm.minted = (vm.minted + 1).min(vm.quantity),
            Knob::MintDown => vm.minted = vm.minted.saturating_sub(1),
            Knob::AddTx => {
                let h = POOL[vm.fulfilments.len() % POOL.len()];
                vm.fulfilments.push(tx(h, 1, FulfilmentStatus::Submitted));
            }
            Knob::Confirm => {
                if let Some(t) = vm
                    .fulfilments
                    .iter_mut()
                    .find(|t| t.status == FulfilmentStatus::Submitted)
                {
                    t.status = FulfilmentStatus::Confirmed;
                }
            }
            Knob::Reset => *vm = playground_vm(),
        }
    }

    fn draw_sim_controls(&mut self, p: &Painter, sel: usize, x: f32, y: f32) -> f32 {
        let paused = matches!(&self.stories[sel].body, Body::Fulfilment(f) if f.paused);
        p.text("simulate", x, y, 13.0, p.theme.muted);
        let by = y + 8.0;
        let reset = Button::new("reset")
            .variant(ButtonVariant::Tonal)
            .font_size(15.0)
            .show(p, Rect::new(x, by, 88.0, 30.0));
        let toggle = Button::new(if paused { "play" } else { "pause" })
            .variant(ButtonVariant::Tonal)
            .font_size(15.0)
            .show(p, Rect::new(x + 96.0, by, 88.0, 30.0));
        p.text(
            "ticks minted up, lands txs, then confirms",
            x,
            by + 46.0,
            12.0,
            p.theme.muted,
        );
        if reset {
            self.reset_sim(sel);
        }
        if toggle {
            if let Body::Fulfilment(f) = &mut self.stories[sel].body {
                f.paused = !f.paused;
            }
        }
        by + 60.0
    }

    fn reset_sim(&mut self, sel: usize) {
        if let Body::Fulfilment(f) = &mut self.stories[sel].body {
            f.vm = simulate_vm();
            f.sim_accum = 0.0;
            f.sim_chunk_idx = 0;
        }
    }

    fn advance_sim(&mut self, sel: usize) {
        let Body::Fulfilment(f) = &mut self.stories[sel].body else {
            return;
        };
        if !matches!(f.mode, StoryMode::Simulate) || f.paused {
            return;
        }
        f.sim_accum += get_frame_time() as f64;
        f.vm.updated_secs_ago = Some(f.sim_accum as u32);
        if f.sim_accum < SIM_INTERVAL {
            return;
        }
        f.sim_accum = 0.0;
        f.vm.updated_secs_ago = Some(0);

        let vm = &mut f.vm;
        if vm.status == OrderStatus::Pending {
            vm.status = OrderStatus::Fulfilling;
        } else if vm.minted < vm.quantity {
            let remaining = vm.quantity - vm.minted;
            let chunk = SIM_CHUNKS[f.sim_chunk_idx % SIM_CHUNKS.len()].min(remaining);
            let h = POOL[vm.fulfilments.len() % POOL.len()];
            vm.fulfilments
                .push(tx(h, chunk, FulfilmentStatus::Submitted));
            vm.minted += chunk;
            f.sim_chunk_idx += 1;
        } else if let Some(t) = vm
            .fulfilments
            .iter_mut()
            .find(|t| t.status == FulfilmentStatus::Submitted)
        {
            t.status = FulfilmentStatus::Confirmed;
        } else {
            vm.status = OrderStatus::Confirmed;
        }
    }
}

fn window_conf() -> Conf {
    Conf {
        window_title: "macroquad-widgets storybook".to_owned(),
        window_width: 1000,
        window_height: 760,
        window_resizable: true,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    // Proportional sans for chrome, monospace for hashes/data.
    let font = load_ttf_font_from_bytes(include_bytes!("../assets/NotoSans-Bold.ttf")).ok();
    let mono = load_ttf_font_from_bytes(include_bytes!("../assets/JetBrainsMono-Regular.ttf")).ok();
    let mut book = Storybook::new();
    loop {
        let theme = book.current_theme();
        clear_background(theme.bg);
        let p = Painter::new(font.as_ref(), mono.as_ref(), theme, frame_tap());
        book.frame(&p);
        next_frame().await;
    }
}
