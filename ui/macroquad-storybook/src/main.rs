//! Stories for `macroquad-widgets`. Native macroquad app — run with
//! `cargo run -p macroquad-storybook`.
//!
//! Left **sidebar** lists stories by category (click / ←→ / number keys, selected
//! highlighted); the stage renders the selected story. Interactive fulfilment
//! stories add a **knobs** panel and a **simulate poll** story that auto-advances
//! the VM (minted ticks up, txs land, then confirm). The **buttons** atom story
//! shows the variant × state matrix — hover and press them to feel the states.
//!
//! The story list scrolls with the mouse wheel or trackpad over the sidebar,
//! and a story chosen by key or `--story` is scrolled into view.

mod chain_fixture;
mod gamekit_stories;
mod route_stories;
mod trading_stories;

use chain_fixture::{ChainSim, Scenario};
use gamekit_stories::{KitStory, KitView};
use macroquad::prelude::*;
use macroquad_widgets::{
    BlockPulseVm, BlockTrainVm, Button, ButtonVariant, CalloutTone, CalloutVm, CheckoutAction,
    CheckoutState, Eligibility, FloatingStick, FulfilmentAction, FulfilmentStatus, FulfilmentTx,
    Gesture, Gestures, MintCheckoutVm, OrderFulfilmentVm, OrderStatus, Painter, PulseDetail,
    PulseTicker, QuantityStepperVm, RiderState, SheetState, SquadCandidate, SquadCommit,
    SquadPickerAction, SquadPickerVm, StepperAction, StickView, SwipeDir, Theme, TrainRider,
    TrainState, WalletAction, WalletConnectVm, WalletItem, WalletListAction, WalletListState,
    WalletListVm, WalletRow, WalletState, block_pulse, block_train, callout, floating_stick,
    mint_checkout, order_fulfilment, quantity_stepper, segmented, segmented_pills, squad_picker,
    theme, wallet_connect, wallet_list,
};
use route_stories::RouteStory;
use trading_stories::{TradingStory, TradingView};
use ui_theme::{TextSize, Token};

const SIDEBAR_W: f32 = 210.0;
/// The sidebar's fixed title band; the story list scrolls beneath it.
const SIDEBAR_TITLE_H: f32 = 58.0;
/// The stick story's well radius: roughly a thumb's reach at desktop scale.
const STICK_RADIUS: f32 = 64.0;
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

/// A chain story: a seeded feed, the widgets' host-owned state, and whatever is
/// riding. The sim runs a REAL `Heartbeat`, so the widgets are reviewed folding
/// the data they fold in production rather than a hand-built snapshot.
struct ChainStory {
    sim: ChainSim,
    train: TrainState,
    /// One ticker per pulse on screen: each remembers the tip it last drew, so
    /// two sharing one would rob each other of their pops.
    pulse: PulseTicker,
    pulse_full: PulseTicker,
    riders: Vec<TrainRider>,
    /// Tip height when a rider was put on, so it lands in the NEXT block rather
    /// than instantly. The landing ring is the moment worth reviewing.
    waiting_from: Option<u64>,
    span_secs: f32,
    view: ChainView,
}

/// Which chain widget a story is showing. One story per widget, per the
/// crate's charter — and an enum rather than a `show_pulse: bool`, so the
/// choice has a name at every call site.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ChainView {
    Train,
    Pulse,
}

enum Body {
    Buttons,
    /// A segmented control, in the track shape or the pill one.
    Segmented {
        pills: bool,
    },
    /// The callout's six tones, with or without an action, or one row per ramp
    /// step.
    Callout {
        action: bool,
        sizes: bool,
    },
    // Boxed: `ChainStory` holds a whole `Heartbeat` (a 256-slot ring plus the
    // scripted feed) inline, which every other variant would otherwise pay for.
    Chain(Box<ChainStory>),
    Stepper(u32),
    Wallet(WalletConnectVm),
    WalletList(WalletListVm),
    Checkout(MintCheckoutVm),
    Fulfilment(Fulfilment),
    SquadPicker(SquadPickerVm),
    /// The live stick and the last thing it reported, for the readout.
    Stick(Box<StickStory>),
    /// A `macroquad-gamekit` story, driven by the storybook clock.
    Kit(Box<KitStory>),
    /// A route graph and its strip, driven by the storybook clock.
    Route(Box<RouteStory>),
    /// A trading view, driven by the storybook clock.
    Trading(Box<TradingStory>),
    /// The operator panel, whose commands are live.
    Admin(Box<AdminStory>),
}

/// The operator panel with a game's numbers in it and its buttons wired to an
/// echo, the way a host would report what a command did.
struct AdminStory {
    snapshot: arcade_admin_quad::Snapshot,
    /// The sample history a host would keep: one reading a quarter second, so
    /// the story shows the same sparklines a game does rather than a fixture
    /// shape invented for the picture.
    frame_trend: Vec<f32>,
    rtt_trend: Vec<f32>,
    player_trend: Vec<f32>,
    commands: Vec<arcade_admin_quad::Command>,
    outcome: Option<arcade_admin_quad::Outcome>,
    /// The panel's motion. Opened all the way, so a capture is the settled
    /// console; `CLOSE` shuts it and a tap on the stage opens it again.
    sheet: SheetState,
    /// Whether the story is given a desktop's width. The panel is laid out in
    /// window points now, so the wide form is the one a person sees; the narrow
    /// one is the same console on a phone.
    wide: bool,
    /// The rect the panel is given; the cramped one is there to prove a short
    /// panel says what it left out instead of quietly drawing half of itself.
    height: f32,
}

/// A floating stick the reader can drag with the mouse, beside fixed samples
/// so a capture shows the anatomy without a hand on it.
struct StickStory {
    stick: FloatingStick,
    last: String,
}

/// Cheap copy of the story kind — lets `draw_main` dispatch without holding a
/// borrow of `self.stories` across the per-kind mutation.
#[derive(Clone, Copy)]
enum Kind {
    Buttons,
    Segmented,
    Callout,
    Chain,
    Stepper,
    Wallet,
    WalletList,
    Checkout,
    Fulfilment,
    SquadPicker,
    Stick,
    Kit,
    Route,
    Trading,
    Admin,
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

    fn chain(
        category: &'static str,
        name: &'static str,
        view: ChainView,
        scenario: Scenario,
        span_secs: f32,
    ) -> Self {
        Self {
            category,
            name,
            body: Body::Chain(Box::new(ChainStory {
                sim: ChainSim::new(scenario),
                train: TrainState::new(),
                pulse: PulseTicker::new(),
                pulse_full: PulseTicker::new(),
                riders: Vec::new(),
                waiting_from: None,
                span_secs,
                view,
            })),
        }
    }

    fn stepper(category: &'static str, name: &'static str, qty: u32) -> Self {
        Self {
            category,
            name,
            body: Body::Stepper(qty),
        }
    }

    fn segmented(category: &'static str, name: &'static str, pills: bool) -> Self {
        Self {
            category,
            name,
            body: Body::Segmented { pills },
        }
    }

    fn callout(category: &'static str, name: &'static str, action: bool, sizes: bool) -> Self {
        Self {
            category,
            name,
            body: Body::Callout { action, sizes },
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
            Body::Segmented { .. } => Kind::Segmented,
            Body::Callout { .. } => Kind::Callout,
            Body::Chain(_) => Kind::Chain,
            Body::Stepper(_) => Kind::Stepper,
            Body::Wallet(_) => Kind::Wallet,
            Body::WalletList(_) => Kind::WalletList,
            Body::Checkout(_) => Kind::Checkout,
            Body::Fulfilment(_) => Kind::Fulfilment,
            Body::SquadPicker(_) => Kind::SquadPicker,
            Body::Stick(_) => Kind::Stick,
            Body::Kit(_) => Kind::Kit,
            Body::Route(_) => Kind::Route,
            Body::Trading(_) => Kind::Trading,
            Body::Admin(_) => Kind::Admin,
        }
    }

    fn route(name: &'static str, story: RouteStory) -> Self {
        Self {
            category: "route",
            name,
            body: Body::Route(Box::new(story)),
        }
    }

    fn kit(name: &'static str, view: KitView) -> Self {
        Self {
            category: "gamekit",
            name,
            body: Body::Kit(Box::new(KitStory::new(
                view,
                (0..6).map(placeholder_art).collect(),
            ))),
        }
    }

    fn admin(name: &'static str, height: f32, wide: bool) -> Self {
        Self {
            category: "admin",
            name,
            body: Body::Admin(Box::new(AdminStory {
                sheet: SheetState::new().opened(),
                wide,
                snapshot: arcade_admin_quad::Snapshot {
                    client: arcade_admin_quad::ClientCost {
                        fps: 59.6,
                        frame_ms: 16.8,
                        worst_ms: 41.2,
                        memory_bytes: Some(412 * 1024 * 1024),
                    },
                    net: arcade_admin_quad::NetHealth {
                        links_open: 3,
                        links_expected: 4,
                        links_relayed: 1,
                        rtt_tenths: Some(124),
                        churn: 2,
                        offered: 7,
                        answered: 7,
                        failed: 1,
                    },
                    world: Some(arcade_admin_quad::World {
                        players: 12,
                        connected: 11,
                        tick: 40_112,
                        running: true,
                        colo: Some("BNE".into()),
                        age_ms: 1_200,
                    }),
                },
                frame_trend: (0..60)
                    .map(|i| {
                        16.0 + (i as f32 * 0.35).sin() * 1.8 + if i == 42 { 18.0 } else { 0.0 }
                    })
                    .collect(),
                // A gap in the middle: the line breaks, which is what a pair
                // that stopped reporting looks like.
                rtt_trend: (0..60)
                    .map(|i| {
                        if (20..=21).contains(&i) {
                            f32::NAN
                        } else {
                            12.0 + (i as f32 * 0.22).cos() * 1.5
                        }
                    })
                    .collect(),
                player_trend: (0..60)
                    .map(|i| {
                        if i < 20 {
                            10.0
                        } else if i < 40 {
                            11.0
                        } else {
                            12.0
                        }
                    })
                    .collect(),
                commands: vec![
                    arcade_admin_quad::Command {
                        id: "resync",
                        label: "Resync",
                        enabled: true,
                        destructive: false,
                    },
                    arcade_admin_quad::Command {
                        id: "restart",
                        label: "Restart world",
                        enabled: true,
                        destructive: true,
                    },
                    arcade_admin_quad::Command {
                        id: "broadcast",
                        label: "Broadcast",
                        enabled: false,
                        destructive: false,
                    },
                ],
                outcome: None,
                height,
            })),
        }
    }

    fn trading(name: &'static str, view: TradingView) -> Self {
        Self {
            category: "trading",
            name,
            body: Body::Trading(Box::new(TradingStory::new(view))),
        }
    }

    fn stick(category: &'static str, name: &'static str) -> Self {
        Self {
            category,
            name,
            body: Body::Stick(Box::new(StickStory {
                stick: FloatingStick::new(STICK_RADIUS),
                last: "press and drag anywhere in the stage".into(),
            })),
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
        Story::segmented("atoms", "segmented · track", false),
        Story::segmented("atoms", "segmented · pills", true),
        Story::callout("atoms", "callout · tones", false, false),
        Story::callout("atoms", "callout · with an action", true, false),
        Story::callout("atoms", "callout · sizes", false, true),
        Story::stick("input", "floating stick"),
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
                WalletRow::new(STAKE_ADDR).with_handle("damo"),
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
            SquadPickerVm::new(squad_roster(12), 4)
                .chosen(vec!["tool0000".into(), "tool0001".into()]),
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
        Story::squad_picker("squad", "empty roster", SquadPickerVm::new(Vec::new(), 4)),
        // Chain tempo. Past story 9, so reach these with
        // `--story chain` / `--story rollback` rather than a number key.
        Story::chain(
            "chain",
            "train · 20 min",
            ChainView::Train,
            Scenario::Live,
            20.0 * 60.0,
        ),
        Story::chain(
            "chain",
            "train · 5 min",
            ChainView::Train,
            Scenario::Live,
            5.0 * 60.0,
        ),
        Story::chain(
            "chain",
            "rollback",
            ChainView::Train,
            Scenario::Rollback,
            5.0 * 60.0,
        ),
        Story::chain(
            "chain",
            "reconnect",
            ChainView::Train,
            Scenario::Reconnect,
            20.0 * 60.0,
        ),
        Story::chain(
            "chain",
            "quiet feed",
            ChainView::Train,
            Scenario::QuietFeed,
            20.0 * 60.0,
        ),
        Story::chain(
            "chain",
            "offline",
            ChainView::Train,
            Scenario::Offline,
            20.0 * 60.0,
        ),
        Story::chain(
            "chain",
            "pulse",
            ChainView::Pulse,
            Scenario::Live,
            20.0 * 60.0,
        ),
        Story::chain(
            "chain",
            "pulse · quiet",
            ChainView::Pulse,
            Scenario::QuietFeed,
            20.0 * 60.0,
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
        Story::kit("easing", KitView::Easing),
        Story::kit("card hand", KitView::Hand),
        Story::kit("rich text", KitView::Text),
        Story::kit("hit effects", KitView::Fx),
        Story::kit("rounded art", KitView::Art),
        Story::kit("preloader", KitView::Preload),
        Story::kit("card face", KitView::Face),
        Story::admin("operator panel", 640.0, true),
        // Tall enough for the one-column form: a portrait phone's 16 facts,
        // its sparklines and the commands.
        Story::admin("operator panel · phone", 640.0, false),
        Story::admin("operator panel · cramped", 300.0, true),
        Story::route("two-way split", RouteStory::split()),
        Story::route("declined split", RouteStory::declined()),
        Story::route("single venue", RouteStory::single()),
        Story::route("four routes", RouteStory::four()),
        Story::route("filling", RouteStory::filling()),
        Story::route("closed unfilled", RouteStory::ended(false)),
        Story::route("failed on chain", RouteStory::ended(true)),
        // Trading views. The market data is a layout fixture, not a token's
        // history — see `trading_stories`.
        Story::trading("price chart · sliding", TradingView::Chart),
        Story::trading("price chart · phone", TradingView::ChartPhone),
        Story::trading("price chart · scrub", TradingView::ChartScrub),
        Story::trading("price chart · new high", TradingView::ChartHigh),
        Story::trading("life strip · brush", TradingView::Strip),
        Story::trading("life strip · phone", TradingView::StripPhone),
        Story::trading("life strip · pending", TradingView::StripPending),
        Story::trading("stat tiles", TradingView::Stats),
        Story::trading("split bar", TradingView::Split),
        Story::trading("change chip", TradingView::Change),
        Story::trading("feed · arriving", TradingView::Feed),
        Story::trading("feed · scrolled back", TradingView::FeedScrolled),
        Story::trading("holder board", TradingView::Board),
        Story::trading("holder board · partial", TradingView::BoardPartial),
        Story::trading("holder map", TradingView::Map),
        Story::trading("token header · replay", TradingView::Header),
        Story::trading("token header · live", TradingView::HeaderLive),
        Story::trading("treemap", TradingView::Treemap),
    ]
}

/// The stories' time. Paused, stepped and slowed from the keyboard, so an
/// animation can be judged frame by frame; fixed at 1/60 s for `--shot`, so a
/// capture is the same every run.
struct Clock {
    paused: bool,
    speed: usize,
    step: bool,
    fixed: bool,
    /// Story time, seconds.
    t: f64,
}

const SPEEDS: [f32; 3] = [1.0, 0.25, 0.1];
const FIXED_DT: f32 = 1.0 / 60.0;

impl Clock {
    fn new() -> Self {
        Self {
            paused: false,
            speed: 0,
            step: false,
            fixed: false,
            t: 0.0,
        }
    }

    /// This frame's story `dt`.
    fn tick(&mut self, frame_dt: f32) -> f32 {
        let base = if self.fixed {
            FIXED_DT
        } else {
            frame_dt.min(0.1)
        };
        let dt = if self.paused {
            if std::mem::take(&mut self.step) {
                FIXED_DT
            } else {
                0.0
            }
        } else {
            base * SPEEDS[self.speed]
        };
        self.t += f64::from(dt);
        dt
    }

    fn label(&self) -> String {
        let state = if self.paused {
            "paused".to_string()
        } else {
            format!("{}×", SPEEDS[self.speed])
        };
        format!("t {:.2}s · {state}", self.t)
    }
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
        p.text(name, x, y, 13.0, p.theme.color.text_muted);
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

    p.text("accents", x, y, 13.0, p.theme.color.text_muted);
    y += 10.0;
    let mut bx = x;
    // Tokens, not resolved colours: `Button::accent` takes `impl Into<Ink>`, so
    // the story names the palette entry and the theme decides what it is worth.
    for (name, accent) in [
        ("accent", Token::Accent),
        ("accent_blue", Token::AccentBlue),
        ("error", Token::Error),
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
        p.theme.color.text_muted,
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
    /// This frame's whole gesture, for the gamekit stories.
    gesture: Gesture,
    clock: Clock,
    /// This frame's story `dt`, from the clock.
    dt: f32,
    /// How far the sidebar list is scrolled, in points.
    scroll: f32,
    /// Scroll the selected story into view on the next sidebar draw: set when
    /// a story is chosen by key or `--story`, which may be off-screen.
    reveal: bool,
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
            gesture: Gesture::default(),
            clock: Clock::new(),
            dt: 0.0,
            scroll: 0.0,
            reveal: false,
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
        // Story time: P pauses, `.` steps one frame, S cycles slow motion.
        if is_key_pressed(KeyCode::P) {
            self.clock.paused = !self.clock.paused;
        }
        if is_key_pressed(KeyCode::Period) {
            self.clock.paused = true;
            self.clock.step = true;
        }
        if is_key_pressed(KeyCode::S) {
            self.clock.speed = (self.clock.speed + 1) % SPEEDS.len();
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
        self.reveal = true;
    }

    fn draw_sidebar(&mut self, p: &Painter) {
        draw_rectangle(
            0.0,
            0.0,
            SIDEBAR_W,
            screen_height(),
            p.theme.color.bg_secondary,
        );
        draw_line(
            SIDEBAR_W,
            0.0,
            SIDEBAR_W,
            screen_height(),
            1.0,
            p.theme.color.bg_highlight,
        );
        let (mx, my) = mouse_position();
        let mouse = vec2(mx, my);

        // The list scrolls under a fixed title band. Rows are laid out in list
        // coordinates first, so the content height is known before drawing.
        let list_top = SIDEBAR_TITLE_H;
        let visible = (screen_height() - list_top).max(0.0);
        let mut rows = Vec::with_capacity(self.stories.len());
        let mut headings = Vec::new();
        let mut y = 12.0;
        let mut last_cat = "";
        for s in &self.stories {
            if s.category != last_cat {
                last_cat = s.category;
                y += 6.0;
                headings.push((y, s.category));
                y += 18.0;
            }
            rows.push(y);
            y += 26.0;
        }
        let content = y;

        if mouse.x < SIDEBAR_W {
            let (_, wheel) = mouse_wheel();
            // A wheel reports notches (about ±1); a trackpad reports points.
            let step = if wheel.abs() <= 1.5 {
                wheel * 36.0
            } else {
                wheel
            };
            self.scroll -= step;
        }
        if std::mem::take(&mut self.reveal)
            && let Some(&row_y) = rows.get(self.selected)
        {
            let (top, bottom) = (row_y - 13.0, row_y + 13.0);
            if top < self.scroll {
                self.scroll = top - 8.0;
            } else if bottom > self.scroll + visible {
                self.scroll = bottom - visible + 8.0;
            }
        }
        let max_scroll = (content - visible).max(0.0);
        self.scroll = self.scroll.clamp(0.0, max_scroll);
        let off = list_top - self.scroll;

        for (hy, cat) in &headings {
            p.text(cat, 16.0, hy + off, 11.0, p.theme.color.text_muted);
        }
        let mut clicked = None;
        for (i, s) in self.stories.iter().enumerate() {
            let y = rows[i] + off;
            let row = Rect::new(6.0, y - 13.0, SIDEBAR_W - 12.0, 24.0);
            // Off-screen rows are neither drawn nor tappable; a row half under
            // the title band is drawn (the band covers it) but not tappable.
            if row.y + row.h < list_top || row.y > screen_height() {
                continue;
            }
            let selected = i == self.selected;
            if selected {
                draw_rectangle(
                    row.x,
                    row.y,
                    row.w,
                    row.h,
                    theme::with_alpha(p.theme.color.accent, 0.16),
                );
                draw_rectangle(row.x, row.y, 3.0, row.h, p.theme.color.accent);
            } else if row.contains(mouse) {
                draw_rectangle(
                    row.x,
                    row.y,
                    row.w,
                    row.h,
                    theme::with_alpha(p.theme.color.text_primary, 0.05),
                );
            }
            let label = format!("{}. {}", i + 1, s.name);
            let baseline = p.centre_baseline(row.y, row.h, 13.0);
            p.text(
                &label,
                18.0,
                baseline,
                13.0,
                if selected {
                    p.theme.color.accent
                } else {
                    p.theme.color.text_primary
                },
            );
            if row.y >= list_top - 1.0 && p.tapped(row) {
                clicked = Some(i);
            }
        }

        // Title band, drawn last so scrolled rows pass beneath it.
        draw_rectangle(0.0, 0.0, SIDEBAR_W, list_top, p.theme.color.bg_secondary);
        p.text("STORYBOOK", 16.0, 40.0, 16.0, p.theme.color.accent);
        draw_line(
            0.0,
            list_top,
            SIDEBAR_W,
            list_top,
            1.0,
            theme::with_alpha(
                p.theme.color.bg_highlight,
                if self.scroll > 0.0 { 1.0 } else { 0.0 },
            ),
        );

        // Scrollbar, only when the list overflows.
        if max_scroll > 0.0 {
            let track = visible - 8.0;
            let thumb = (track * visible / content).max(24.0);
            let ty = list_top + 4.0 + (track - thumb) * (self.scroll / max_scroll);
            draw_rectangle(
                SIDEBAR_W - 5.0,
                ty,
                3.0,
                thumb,
                theme::with_alpha(p.theme.color.text_muted, 0.6),
            );
        }

        if let Some(i) = clicked {
            // Clicked rows are already in view; don't jump.
            self.select(i);
            self.reveal = false;
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
            p.theme.color.text_primary,
        );
        y += 20.0;
        let help = match self.stories[sel].kind() {
            Kind::Kit | Kind::Route | Kind::Trading => format!(
                "arrows / click to switch  ·  t = theme: {}  ·  p pause · . step · s slow  ·  {}",
                p.theme.name,
                self.clock.label()
            ),
            _ => format!("arrows / click to switch  ·  t = theme: {}", p.theme.name),
        };
        p.text(&help, x0, y, 12.0, p.theme.color.text_muted);
        y += 30.0;

        match self.stories[sel].kind() {
            Kind::Buttons => {
                if let Some(a) = button_gallery(p, x0, y, col_w) {
                    self.last_action = Some(a);
                }
                self.echo(p, x0);
            }
            Kind::Segmented => {
                let options = ["1H", "24H", "7D", "30D", "ALL"];
                let pills = matches!(self.stories[sel].body, Body::Segmented { pills: true });
                // The pill row is a chart's span picker: five chips and their
                // gaps, at the size the arena draws them. The track is the
                // heavier control, for a whole pane.
                let row = match pills {
                    true => Rect::new(x0, y, 5.0 * 34.0 + 4.0 * 4.0, 16.0),
                    false => Rect::new(x0, y, col_w.min(340.0), 28.0),
                };
                if pills {
                    segmented_pills(p, row, &options, 1);
                } else {
                    let _ = segmented(p, row, &options, 1, true);
                }
            }
            Kind::Callout => {
                let action = matches!(self.stories[sel].body, Body::Callout { action: true, .. });
                let sizes = matches!(self.stories[sel].body, Body::Callout { sizes: true, .. });
                let mut ry = y;
                if sizes {
                    // The four steps the adopting surfaces already drew at, so
                    // a host picks its own and the adoption is not a restyle.
                    for (label, step) in [
                        ("Base", TextSize::Base),
                        ("Md", TextSize::Md),
                        ("Xl", TextSize::Xl),
                        ("Xl2", TextSize::Xl2),
                    ] {
                        let row = Rect::new(x0 + 44.0, ry, col_w.min(420.0) - 44.0, 24.0);
                        p.text_mid(
                            label,
                            x0,
                            row.y + row.h * 0.5,
                            p.size(TextSize::Xs),
                            p.theme.color.text_muted,
                        );
                        let vm = CalloutVm {
                            tone: CalloutTone::Positive,
                            text: "LANDED in block 10,441,882",
                            action: None,
                            size: step,
                        };
                        let _ = callout(p, &vm, row);
                        ry += 34.0;
                    }
                } else {
                    // The sentences five surfaces actually draw, one per tone.
                    let rows = [
                        ("SENT · 0f3a…9c2e", CalloutTone::Normal),
                        ("waiting on the batcher", CalloutTone::Muted),
                        (
                            "IN BLOCK 10,441,882 · minswap-v2 filling",
                            CalloutTone::Busy,
                        ),
                        ("LANDED in block 10,441,882", CalloutTone::Positive),
                        ("settled, but one order unfilled", CalloutTone::Warn),
                        ("could not open the block feed", CalloutTone::Negative),
                    ];
                    for (text, tone) in rows {
                        let vm = CalloutVm {
                            tone,
                            text,
                            action: action.then_some("Try again"),
                            size: TextSize::Base,
                        };
                        let row = Rect::new(x0, ry, col_w.min(420.0), 22.0);
                        let _ = callout(p, &vm, row);
                        ry += 30.0;
                    }
                }
            }
            Kind::Chain => self.draw_chain(p, sel, x0, y),
            Kind::Stepper => self.draw_stepper(p, sel, x0, y),
            Kind::Wallet => self.draw_wallet(p, sel, x0, y, col_w),
            Kind::WalletList => self.draw_wallet_list(p, sel, x0, y, col_w),
            Kind::Checkout => self.draw_checkout(p, sel, x0, y, col_w),
            Kind::Fulfilment => self.draw_fulfilment(p, sel, x0, y, col_w),
            Kind::SquadPicker => self.draw_squad_picker(p, sel, x0, y, col_w),
            Kind::Stick => self.draw_stick(p, sel, x0, y),
            Kind::Route => {
                let t = self.clock.t as f32;
                if let Body::Route(story) = &self.stories[sel].body {
                    story.draw(p, x0, y, t);
                }
            }
            Kind::Kit => {
                let (dt, t, g) = (self.dt, self.clock.t, self.gesture);
                if let Body::Kit(story) = &mut self.stories[sel].body {
                    let area = Rect::new(
                        x0,
                        y,
                        screen_width() - x0 - 20.0,
                        screen_height() - y - 20.0,
                    );
                    story.draw(p, area, &g, dt, t);
                }
            }
            Kind::Trading => {
                let (t, g) = (self.clock.t, self.gesture);
                if let Body::Trading(story) = &mut self.stories[sel].body {
                    let area = Rect::new(
                        x0,
                        y,
                        screen_width() - x0 - 20.0,
                        screen_height() - y - 20.0,
                    );
                    story.draw(p, area, &g, t);
                }
            }
            Kind::Admin => {
                let (story_height, wide) = match &self.stories[sel].body {
                    Body::Admin(story) => (story.height, story.wide),
                    _ => (430.0, false),
                };
                let width = if wide {
                    screen_width() - x0 - 20.0
                } else {
                    col_w.min(360.0)
                };
                let area = Rect::new(x0, y, width, story_height);
                if let Body::Admin(story) = &mut self.stories[sel].body {
                    let trends = arcade_admin_quad::Trends {
                        frame_ms: &story.frame_trend,
                        rtt_ms: &story.rtt_trend,
                        players: &story.player_trend,
                        span_secs: 15,
                    };
                    let panel = arcade_admin_quad::Panel {
                        snapshot: &story.snapshot,
                        commands: &story.commands,
                        outcome: story.outcome.as_ref(),
                        trends: Some(&trends),
                    };
                    let reply = arcade_admin_quad::show_sheet(p, &panel, &story.sheet, area);
                    if reply.close {
                        story.sheet.close();
                    } else if !story.sheet.visible() && p.tap.is_some() {
                        story.sheet.open();
                    }
                    if let Some(id) = reply.pressed {
                        let ok = story
                            .commands
                            .iter()
                            .find(|c| c.id == id)
                            .is_some_and(|c| c.enabled);
                        story.outcome = Some(arcade_admin_quad::Outcome {
                            text: format!("{id}: {}", if ok { "done" } else { "refused" }),
                            ok,
                        });
                    }
                }
            }
        }
    }

    /// Three fixed samples — at rest, half a push, a full diagonal — then the
    /// live stick, which appears wherever the mouse is pressed in the stage.
    fn draw_stick(&mut self, p: &Painter, sel: usize, x0: f32, y: f32) {
        let Body::Stick(story) = &mut self.stories[sel].body else {
            return;
        };
        let r = STICK_RADIUS;
        let samples = [
            ("at rest", vec2(0.0, 0.0)),
            ("half push", vec2(0.0, -0.5)),
            ("full diagonal", vec2(0.707, 0.707)),
        ];
        for (i, (label, deflection)) in samples.iter().enumerate() {
            let anchor = vec2(x0 + r + i as f32 * (r * 2.0 + 32.0), y + r + 8.0);
            floating_stick(
                p,
                &StickView {
                    anchor,
                    deflection: *deflection,
                    radius: r,
                },
            );
            let w = p.measure(label, 13.0).width;
            p.text(
                label,
                anchor.x - w * 0.5,
                anchor.y + r + 22.0,
                13.0,
                p.theme.color.text_muted,
            );
        }

        let stage_top = y + r * 2.0 + 56.0;
        let stage = Rect::new(
            x0,
            stage_top,
            screen_width() - x0 - 28.0,
            screen_height() - stage_top - 48.0,
        );
        draw_rectangle_lines(
            stage.x,
            stage.y,
            stage.w,
            stage.h,
            1.0,
            p.theme.color.border,
        );
        let frame = story.stick.update(stage);
        if let Some(view) = &frame.view {
            floating_stick(p, view);
        }
        if frame.view.is_some() {
            story.last = format!(
                "push ({:+.2}, {:+.2})  ·  {:?}",
                frame.push.x,
                frame.push.y,
                frame.cardinal()
            );
        }
        if let Some(released) = frame.released {
            story.last = format!("released: {released:?}");
        }
        p.text(
            &story.last,
            stage.x + 12.0,
            stage.y + 24.0,
            13.0,
            p.theme.color.text_muted,
        );
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
        if let Some(dir) = self.pending_swipe.take()
            && let Some(page) = vm.swipe(dir, w)
        {
            self.last_action = Some(format!("swipe → page {}", page + 1));
            return;
        }

        let r = squad_picker(p, vm, x, y, w);
        if let Some(action) = r.action {
            let echo = match action {
                SquadPickerAction::Toggle(id) => {
                    match vm.chosen.iter().position(|c| c == &id) {
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
                    }
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

    /// The chain stories run wide: the train's whole point is irregular spacing
    /// over real time, and a 460 px column cannot show it. So this one ignores
    /// `col_w` and takes the window.
    fn draw_chain(&mut self, p: &Painter, sel: usize, x: f32, y: f32) {
        let Body::Chain(story) = &mut self.stories[sel].body else {
            return;
        };
        story.sim.advance();
        let now_ms = story.sim.now_ms();
        let tip = story.sim.heartbeat.tip().map(|b| b.height);

        // A rider put on earlier lands in the next block the feed reports —
        // which is what makes the landing ring reviewable on demand instead of
        // only when mainnet happens to produce one.
        if let (Some(from), Some(tip)) = (story.waiting_from, tip)
            && tip > from
        {
            story.riders = vec![TrainRider::new(
                "your buy",
                RiderState::InBlock { height: tip },
            )];
            story.waiting_from = None;
        }

        let w = screen_width() - x - 28.0;
        // One widget per story. The train already carries the pulse's mark as a
        // status light beside "full", so drawing a header pulse above it put two
        // marks 20 px apart at the same x — which reads as a rendering fault
        // rather than as two widgets.
        let bottom = match story.view {
            ChainView::Train => {
                let vm = BlockTrainVm::new(&story.sim.heartbeat, now_ms)
                    .riders(&story.riders)
                    .span_secs(story.span_secs)
                    .plot_height(76.0);
                block_train(p, &vm, &mut story.train, x, y + 8.0, w).bottom
            }
            ChainView::Pulse => {
                let compact =
                    BlockPulseVm::new(&story.sim.heartbeat, now_ms).detail(PulseDetail::Compact);
                block_pulse(p, &compact, &mut story.pulse, x, y + 14.0);
                p.text(
                    "compact — for a crowded status strip",
                    x,
                    p.top_baseline(y + 30.0, 12.0),
                    12.0,
                    p.theme.color.text_muted,
                );
                let full =
                    BlockPulseVm::new(&story.sim.heartbeat, now_ms).detail(PulseDetail::Full);
                block_pulse(p, &full, &mut story.pulse_full, x, y + 76.0);
                p.text(
                    "full — also the height, and a word about the feed",
                    x,
                    p.top_baseline(y + 92.0, 12.0),
                    12.0,
                    p.theme.color.text_muted,
                );
                y + 112.0
            }
        };
        let mut by = bottom + 18.0;

        // Scenario. A rollback or a reconnect is otherwise unreachable without
        // waiting for one to happen on mainnet.
        let mut chosen = None;
        let mut bx = x;
        for s in Scenario::ALL {
            let label = s.label();
            let bw = p.measure(label, 14.0).width + 24.0;
            let variant = if story.sim.scenario == s {
                ButtonVariant::Filled
            } else {
                ButtonVariant::Tonal
            };
            if Button::new(label)
                .variant(variant)
                .text_size(TextSize::Md)
                .show(p, Rect::new(bx, by, bw, 28.0))
            {
                chosen = Some(s);
            }
            bx += bw + 8.0;
        }
        by += 36.0;

        // Speed, and the rider trigger.
        let mut speed = None;
        let mut bx = x;
        for mult in [1.0_f32, 5.0, 30.0] {
            let label = format!("{mult:.0}x");
            let variant = if (story.sim.speed - mult).abs() < 0.01 {
                ButtonVariant::Filled
            } else {
                ButtonVariant::Tonal
            };
            if Button::new(&label)
                .variant(variant)
                .text_size(TextSize::Md)
                .show(p, Rect::new(bx, by, 56.0, 28.0))
            {
                speed = Some(mult);
            }
            bx += 64.0;
        }
        let ride = Button::new("+rider")
            .variant(ButtonVariant::Tonal)
            .text_size(TextSize::Md)
            .show(p, Rect::new(bx + 16.0, by, 84.0, 28.0));
        let clear = Button::new("clear")
            .variant(ButtonVariant::Ghost)
            .text_size(TextSize::Md)
            .show(p, Rect::new(bx + 108.0, by, 76.0, 28.0));
        by += 34.0;
        p.text(
            "speed drives the SIM clock, not the widget — the train derives its own motion",
            x,
            p.top_baseline(by, 12.0),
            12.0,
            p.theme.color.text_muted,
        );

        if let Some(s) = chosen {
            story.sim.set_scenario(s);
            story.riders.clear();
            story.waiting_from = None;
            story.train = TrainState::new();
        }
        if let Some(mult) = speed {
            story.sim.speed = mult;
        }
        if ride {
            story.riders = vec![TrainRider::new("your buy", RiderState::Waiting)];
            story.waiting_from = tip;
        }
        if clear {
            story.riders.clear();
            story.waiting_from = None;
        }
    }

    fn echo(&self, p: &Painter, x: f32) {
        if let Some(a) = &self.last_action {
            p.text(a, x, screen_height() - 32.0, 13.0, p.theme.color.accent);
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
        if let Some(StepperAction::Changed(n)) = resp.action
            && let Body::Stepper(q) = &mut self.stories[sel].body
        {
            *q = n;
        }
        p.text_top(
            &format!("min 1 · max 10 · qty = {qty}"),
            x,
            y + 52.0,
            13.0,
            p.theme.color.text_muted,
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
            p.mono(a, x0, bottom + 18.0, 13.0, p.theme.color.accent);
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
        p.text("knobs", x, y, 13.0, p.theme.color.text_muted);
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
                .text_size(TextSize::Lg)
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
        p.text("simulate", x, y, 13.0, p.theme.color.text_muted);
        let by = y + 8.0;
        let reset = Button::new("reset")
            .variant(ButtonVariant::Tonal)
            .text_size(TextSize::Lg)
            .show(p, Rect::new(x, by, 88.0, 30.0));
        let toggle = Button::new(if paused { "play" } else { "pause" })
            .variant(ButtonVariant::Tonal)
            .text_size(TextSize::Lg)
            .show(p, Rect::new(x + 96.0, by, 88.0, 30.0));
        p.text(
            "ticks minted up, lands txs, then confirms",
            x,
            by + 46.0,
            12.0,
            p.theme.color.text_muted,
        );
        if reset {
            self.reset_sim(sel);
        }
        if toggle && let Body::Fulfilment(f) = &mut self.stories[sel].body {
            f.paused = !f.paused;
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
    // Fully qualified: this file has its own `Args` for `--story`/`--shot`.
    let args = macroquad_gamekit::harness::Args::read();
    Conf {
        window_title: args.window_title("macroquad-widgets storybook"),
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
    /// With `--shot`: capture once story time reaches this many seconds. The
    /// clock is fixed at 1/60 s per frame in shot mode, so it is exact.
    at: Option<f64>,
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
        at: value("--at").and_then(|s| s.parse().ok()),
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
        match book.stories.iter().position(|s| {
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
    book.clock.fixed = args.shot.is_some();
    loop {
        let theme = book.current_theme();
        let gesture = gestures.update();
        // Tick BEFORE drawing, so an animated widget reads the same moment the
        // story draws: with `--shot` the clock is fixed and the capture is
        // repeatable, which only holds if `Painter::time` is the story's clock
        // rather than the wall's.
        book.dt = book.clock.tick(get_frame_time());
        let p =
            Painter::new(font.as_ref(), mono.as_ref(), theme, gesture.tap).at_time(book.clock.t);
        book.pending_swipe = gesture.swipe;
        book.gesture = gesture;

        // Snapshot mode draws into an offscreen target and reads *that*, not
        // the window. `get_screen_data` reads the front buffer, which an
        // unfocused or not-yet-composited macOS window never fills — it
        // returns solid black, indistinguishable from a widget that drew
        // nothing. Rendering to a texture we own sidesteps the window server
        // entirely, so a capture works headless and in CI.
        let due = frames >= SETTLE_FRAMES && args.at.is_none_or(|at| book.clock.t >= at);
        let shot = args.shot.as_ref().filter(|_| due);
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

        clear_background(theme.color.bg_primary);
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
