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
    mint_checkout, order_fulfilment, quantity_stepper, theme, wallet_connect,
    wallet_list, Button, ButtonVariant, CheckoutAction, CheckoutState, Eligibility, Gestures,
    SwipeDir,
    FulfilmentAction, FulfilmentStatus, FulfilmentTx, MintCheckoutVm, OrderFulfilmentVm,
    squad_picker, OrderStatus, Painter, QuantityStepperVm, SquadCandidate, SquadCommit,
    SquadPickerAction, SquadPickerVm, StepperAction, Theme, WalletAction, WalletConnectVm,
    WalletItem, WalletListAction, WalletListState, WalletListVm, WalletRow, WalletState,
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
    WalletList(WalletListVm),
    Checkout(MintCheckoutVm),
    Fulfilment(Fulfilment),
    SquadPicker(SquadPickerVm),
}

/// Cheap copy of the story kind — lets `draw_main` dispatch without holding a
/// borrow of `self.stories` across the per-kind mutation.
#[derive(Clone, Copy)]
enum Kind {
    Buttons,
    Stepper,
    Wallet,
    WalletList,
    Checkout,
    Fulfilment,
    SquadPicker,
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

    fn wallet_list(category: &'static str, name: &'static str, vm: WalletListVm) -> Self {
        Self {
            category,
            name,
            body: Body::WalletList(vm),
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
            Body::WalletList(_) => Kind::WalletList,
            Body::Checkout(_) => Kind::Checkout,
            Body::Fulfilment(_) => Kind::Fulfilment,
            Body::SquadPicker(_) => Kind::SquadPicker,
        }
    }

    fn squad_picker(category: &'static str, name: &'static str, vm: SquadPickerVm) -> Self {
        Self {
            category,
            name,
            body: Body::SquadPicker(vm),
        }
    }
}

/// Stand-in asset art, generated rather than shipped.
///
/// Real art needs a network and a wallet; the point here is to judge *layout*
/// with something image-shaped in every tile. Each is a distinct two-tone
/// pattern so a grid of them reads like a grid of different assets rather than
/// one repeated — which is exactly what makes a picker's spacing testable.
///
/// The last one is deliberately left `None` so the missing-art fallback (the
/// serial on a plate) is always on screen next to real tiles.
fn placeholder_art(seed: usize) -> Texture2D {
    const N: usize = 16;
    let hues = [
        (0.36, 0.55, 0.85),
        (0.85, 0.45, 0.40),
        (0.45, 0.80, 0.55),
        (0.80, 0.70, 0.35),
        (0.65, 0.45, 0.85),
        (0.40, 0.75, 0.80),
    ];
    let (r, g, b) = hues[seed % hues.len()];
    let mut bytes = Vec::with_capacity(N * N * 4);
    for y in 0..N {
        for x in 0..N {
            // A cheap deterministic pattern — blocky, like pixel-art PFPs.
            let on = ((x / 2) * 7 + (y / 2) * 13 + seed * 5) % 5 < 2;
            let k = if on { 1.0 } else { 0.45 };
            let vignette = if x == 0 || y == 0 || x == N - 1 || y == N - 1 {
                0.5
            } else {
                1.0
            };
            bytes.push((r * k * vignette * 255.0) as u8);
            bytes.push((g * k * vignette * 255.0) as u8);
            bytes.push((b * k * vignette * 255.0) as u8);
            bytes.push(255);
        }
    }
    let tex = Texture2D::from_rgba8(N as u16, N as u16, &bytes);
    // Nearest, so the blocky pattern stays crisp when scaled up rather than
    // turning to mush — and so it reads as art, not as a rendering error.
    tex.set_filter(FilterMode::Nearest);
    tex
}

/// A roster big enough to page, with the awkward cases a real one has: a name
/// too long for its card, an asset that matched no role, and one that can't be
/// picked at all.
fn squad_roster(n: usize) -> Vec<SquadCandidate> {
    let roles = ["mechanic", "medic", "scout", "gunner"];
    (0..n)
        .map(|i| {
            let mut c = SquadCandidate::new(
                format!("tool{i:04}"),
                if i == 4 {
                    "Toolhead #0777 With A Very Long Name".to_string()
                } else {
                    format!("Toolhead #{:04}", 220 + i * 37)
                },
                980 - (i as i32 * 13),
            );
            // Every fourth asset matched no role — a real roster has these, and
            // they must still be pickable.
            if i % 4 != 3 {
                c = c.with_role(roles[i % roles.len()]);
            }
            if i == 6 {
                c = c.unavailable("on another run");
            }
            // One asset without art, so the fallback is always visible beside
            // real tiles rather than only in a story nobody opens.
            if i != 3 {
                c = c.with_image(placeholder_art(i));
            }
            c
        })
        .collect()
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
        Story::wallet_list(
            "wallet list",
            "several linked",
            WalletListVm::new(WalletListState::Ready(vec![
                WalletRow::new(STAKE_ADDR).with_handle("damo"),
                WalletRow::new("stake1u9xk4d2rlq7zvnp3m8ftw0hs6yc4jg5zx2vqde7n0aum4tsp3wxyz")
                    .with_handle("vault"),
                WalletRow::new("stake1uy7d3n0qkfz2xw8ta5rmv6jc9phe4bgs0ld2u3xqvn8m6kcz9plkr"),
            ])),
        ),
        Story::wallet_list(
            "wallet list",
            "one linked",
            WalletListVm::new(WalletListState::Ready(vec![
                WalletRow::new(STAKE_ADDR).with_handle("damo")
            ])),
        ),
        // Mid-unlink: that row's controls are disabled so a second click can't
        // race the first, while the others stay live.
        Story::wallet_list(
            "wallet list",
            "unlink in flight",
            WalletListVm::new(WalletListState::Ready(vec![
                WalletRow::new(STAKE_ADDR).with_handle("damo"),
                WalletRow::new("stake1u9xk4d2rlq7zvnp3m8ftw0hs6yc4jg5zx2vqde7n0aum4tsp3wxyz")
                    .with_handle("vault"),
            ]))
            .busy(1),
        ),
        Story::wallet_list(
            "wallet list",
            "loading",
            WalletListVm::new(WalletListState::Loading),
        ),
        Story::wallet_list(
            "wallet list",
            "none linked",
            WalletListVm::new(WalletListState::Empty),
        ),
        Story::wallet_list(
            "wallet list",
            "error",
            WalletListVm::new(WalletListState::Error(
                "couldn't reach the auth service".into(),
            )),
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
        // Squad picker. The live story is first because picking is the whole
        // widget — the static ones exist to pin states that are awkward to
        // reach by hand.
        Story::squad_picker(
            "squad",
            "pick 4 of 12",
            SquadPickerVm::new(squad_roster(12), 4).chosen(vec![
                "tool0000".into(),
                "tool0001".into(),
            ]),
        ),
        Story::squad_picker(
            "squad",
            "full squad",
            SquadPickerVm::new(squad_roster(12), 4).chosen(vec![
                "tool0000".into(),
                "tool0001".into(),
                "tool0002".into(),
                "tool0003".into(),
            ]),
        ),
        Story::squad_picker(
            "squad",
            "single page",
            SquadPickerVm::new(squad_roster(5), 4).chosen(vec!["tool0000".into()]),
        ),
        Story::squad_picker(
            "squad",
            "empty roster",
            SquadPickerVm::new(Vec::new(), 4),
        ),
        Story::squad_picker(
            "squad",
            "locked (run under way)",
            SquadPickerVm::new(squad_roster(12), 4)
                .chosen(vec!["tool0000".into(), "tool0001".into()])
                .editable(false),
        ),
        Story::squad_picker(
            "squad",
            "committing",
            SquadPickerVm::new(squad_roster(12), 4)
                .chosen(vec!["tool0000".into(), "tool0001".into()])
                .commit(SquadCommit::Sending),
        ),
        Story::squad_picker(
            "squad",
            "commit failed",
            SquadPickerVm::new(squad_roster(12), 4)
                .chosen(vec!["tool0000".into()])
                .commit(SquadCommit::Failed("that asset is not yours".into())),
        ),
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
    /// A swipe recognised this frame, for whichever story consumes gestures.
    /// Set by the main loop before `frame`, cleared by the story that uses it.
    pending_swipe: Option<SwipeDir>,
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
            pending_swipe: None,
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
            Kind::WalletList => self.draw_wallet_list(p, sel, x0, y, col_w),
            Kind::Checkout => self.draw_checkout(p, sel, x0, y, col_w),
            Kind::Fulfilment => self.draw_fulfilment(p, sel, x0, y, col_w),
            Kind::SquadPicker => self.draw_squad_picker(p, sel, x0, y, col_w),
        }
    }

    /// The picker is fully interactive here — the storybook plays the host,
    /// applying each action to the VM exactly as the Activity does. That is
    /// the point of having it: selection, paging and the slot-full rule get
    /// exercised without a Discord session or a live mission.
    fn draw_squad_picker(&mut self, p: &Painter, sel: usize, x: f32, y: f32, w: f32) {
        let Body::SquadPicker(vm) = &mut self.stories[sel].body else {
            return;
        };

        // Drag horizontally with the mouse to page, the same gesture a finger
        // makes on a phone.
        if let Some(dir) = self.pending_swipe.take() {
            if let Some(page) = vm.swipe(dir, w) {
                self.last_action = Some(format!("swipe → page {}", page + 1));
                return;
            }
        }

        let r = squad_picker(p, vm, x, y, w);
        if let Some(action) = r.action {
            let echo = match action {
                SquadPickerAction::Toggle(id) => {
                    let label = match vm.chosen.iter().position(|c| c == &id) {
                        Some(at) => {
                            vm.chosen.remove(at);
                            format!("removed {id}")
                        }
                        None if vm.chosen.len() < vm.max_slots => {
                            vm.chosen.push(id.clone());
                            format!("added {id}")
                        }
                        // The widget disables full-and-unpicked cards, so this
                        // is unreachable by tap — kept because the host, not
                        // the widget, owns the rule.
                        None => format!("ignored {id} (squad full)"),
                    };
                    label
                }
                SquadPickerAction::Page(page) => {
                    vm.page = page;
                    format!("page {}", page + 1)
                }
                SquadPickerAction::Commit => {
                    let names: Vec<String> = vm
                        .chosen
                        .iter()
                        .filter_map(|id| vm.candidates.iter().find(|c| &c.id == id))
                        .map(|c| c.name.clone())
                        .collect();
                    vm.commit = SquadCommit::Done(format!("deployed: {}", names.join(", ")));
                    "commit".to_string()
                }
            };
            self.last_action = Some(echo);
        }
        self.echo(p, x);
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

    fn draw_wallet_list(&mut self, p: &Painter, sel: usize, x: f32, y: f32, w: f32) {
        let action = match &self.stories[sel].body {
            Body::WalletList(vm) => wallet_list(p, vm, x, y, w).action,
            _ => return,
        };
        if let Some(a) = action {
            self.last_action = Some(match a {
                WalletListAction::LinkAnother => "action: LinkAnother".into(),
                WalletListAction::Unlink(addr) => format!("action: Unlink({addr})"),
                WalletListAction::MoveUp(i) => format!("action: MoveUp({i})"),
                WalletListAction::MoveDown(i) => format!("action: MoveDown({i})"),
                WalletListAction::Retry => "action: Retry".into(),
            });
        }
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

/// Command line: `--story <substring>` and `--shot <path>`.
///
/// Both exist for the same reason: the sidebar doesn't scroll and the number
/// keys stop at 9, so a story added late in the list is otherwise unreachable —
/// and a widget you can't get on screen is a widget you stop checking. `--shot`
/// additionally makes the storybook the place a UI change gets *reviewed*,
/// rather than deploying to Discord to find out.
struct Args {
    story: Option<String>,
    shot: Option<String>,
}

fn args() -> Args {
    let argv: Vec<String> = std::env::args().collect();
    let value = |flag: &str| {
        argv.iter()
            .position(|a| a == flag)
            .and_then(|i| argv.get(i + 1))
            .cloned()
    };
    Args {
        story: value("--story"),
        shot: value("--shot"),
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    // Proportional sans for chrome, monospace for hashes/data.
    let font = load_ttf_font_from_bytes(include_bytes!("../assets/NotoSans-Bold.ttf")).ok();
    let mono = load_ttf_font_from_bytes(include_bytes!("../assets/JetBrainsMono-Regular.ttf")).ok();
    let mut book = Storybook::new();
    // Gesture recognition rather than `frame_tap`, so a mouse *drag* exercises
    // the swipe path on desktop — otherwise page-turning could only be tested
    // on a phone, which is where it would then be discovered broken.
    let mut gestures = Gestures::new();

    let args = args();
    if let Some(want) = &args.story {
        let want = want.to_lowercase();
        match book
            .stories
            .iter()
            .position(|s| {
                format!("{} {}", s.category, s.name)
                    .to_lowercase()
                    .contains(&want)
            }) {
            Some(i) => book.select(i),
            // Loud, because silently showing story 0 would look like the
            // widget rendered wrong rather than never rendered at all.
            None => {
                eprintln!("no story matching {want:?}; showing the first");
            }
        }
    }

    // Frames to settle before capturing: hit rects and hover states are read
    // one frame stale, and the first frame has no layout from a previous pass.
    const SETTLE_FRAMES: u32 = 3;
    let mut frames = 0;
    loop {
        let theme = book.current_theme();
        let gesture = gestures.update();
        let p = Painter::new(font.as_ref(), mono.as_ref(), theme, gesture.tap);
        book.pending_swipe = gesture.swipe;

        // Snapshot mode draws into an offscreen target and reads *that*, not
        // the window. `get_screen_data` reads the front buffer, which an
        // unfocused or not-yet-composited macOS window never fills — it
        // returns solid black, indistinguishable from a widget that drew
        // nothing. Rendering to a texture we own sidesteps the window server
        // entirely, so a capture works headless and in CI.
        let shot = args.shot.as_ref().filter(|_| frames >= SETTLE_FRAMES);
        let target = shot.map(|_| {
            let t = render_target(screen_width() as u32, screen_height() as u32);
            t.texture.set_filter(FilterMode::Linear);
            // `from_display_rect` already yields a top-left origin that matches
            // the screen; the render target needs no extra y-flip on top of it.
            let mut cam =
                Camera2D::from_display_rect(Rect::new(0.0, 0.0, screen_width(), screen_height()));
            cam.render_target = Some(t.clone());
            set_camera(&cam);
            t
        });

        clear_background(theme.bg);
        book.frame(&p);

        if let (Some(path), Some(target)) = (shot, target) {
            set_default_camera();
            target.texture.get_texture_data().export_png(path);
            println!("wrote {path}");
            std::process::exit(0);
        }

        next_frame().await;
        frames += 1;
    }
}
