//! Stories for `macroquad_gamekit::wallet_modal` — the one-button wallet
//! connector, its modal flow, and every state the flow can be in.
//!
//! The storybook plays the host. [`FakeHost`] applies the modal's actions the
//! way a real bridge would — the wallet is asked, it answers a moment later —
//! and writes the outcome back as the chain's phase. Which answer comes back is
//! decided by the wallet, so every outcome is one click away in the live
//! stories:
//!
//! | wallet | answers |
//! |---|---|
//! | Vespr | declines |
//! | Typhon | fails, with a sentence long enough to wrap |
//! | any other | connects |
//!
//! The addresses a connect answers with are the estate's fixtures (the shared
//! stake address, and the synthetic `0x1f…` EVM address the egui stories use),
//! not live wallets.
//!
//! The step stories open the modal settled at one state, so a capture at the
//! default moment is that state. The walkthrough scripts a whole connect on a
//! loop, so `--at` can capture any moment of any transition:
//!
//! | `--at` | what is on screen |
//! |---|---|
//! | 0.75 | the card rising in |
//! | 1.5 | the chain step, settled |
//! | 2.08 | the slide from chains to Cardano's wallets |
//! | 3.0 | Cardano's wallets, settled |
//! | 3.28 | the slide to the answer, the card shrinking |
//! | 3.7 | waiting on Eternl |
//! | 5.0 | connected, the ring drawing |
//! | 6.0 | connected, settled |
//! | 6.48 | the card leaving |
//! | 7.5 | the button, connected |

use macroquad::prelude::*;
use macroquad_gamekit::paint::{Gradient, fill_gradient};
use macroquad_gamekit::wallet_modal::{
    ChainFamily, ChainOption, ChainPhase, WalletModal, WalletModalAction, WalletModalVm,
    WalletOption, wallet_button,
};
use macroquad_gamekit::{fill_rounded, stroke_rounded};
use macroquad_widgets::{Painter, TextSize};

/// The stake address every wallet fixture in the estate uses.
const STAKE: &str = "stake1u8962x3wtddcq2syq258ka3d9mxxkx5md5xawzx67pac9tgc5rhq9";

/// Seconds a fake wallet takes to answer: long enough to see the wait.
const ANSWER_SECS: f32 = 1.6;
/// The walkthrough's loop.
const LOOP_SECS: f32 = 8.0;
/// A phone's width, in points.
const PHONE_W: f32 = 390.0;

/// A synthetic EVM address, as the egui wallet fixtures use: a real one would
/// be a real wallet's.
fn evm_address() -> String {
    format!("0x{}", "1f".repeat(20))
}

/// Which story this is.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ModalView {
    /// The button in each of its states.
    Buttons,
    /// The button and the whole flow, live, against the fake host.
    Live,
    /// The flow opened at one state.
    Step(Preset),
    /// A connect scripted on a loop.
    Walkthrough,
}

/// The state a step story opens at.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Chains,
    Wallets,
    NoWallets,
    Connecting,
    Connected,
    Declined,
    Failed,
    /// An action only EVM can satisfy: no chain step.
    OnlyEvm,
    /// Cardano and EVM connected at once.
    TwoConnected,
}

/// What the walkthrough does, and when.
#[derive(Clone, Copy)]
enum Cue {
    Open,
    Chain(ChainFamily),
    Wallet(&'static str),
    Done,
}

const SCRIPT: [(f32, Cue); 4] = [
    (0.6, Cue::Open),
    (2.0, Cue::Chain(ChainFamily::Cardano)),
    (3.2, Cue::Wallet("eternl")),
    (6.4, Cue::Done),
];

/// The roadmap as a host would list it today: two chains with bridges, four
/// on the way.
fn roadmap(icon: Option<Texture2D>) -> WalletModalVm {
    let mut eternl = WalletOption::new("eternl", "Eternl");
    if let Some(icon) = icon {
        eternl = eternl.with_icon(icon);
    }
    WalletModalVm::new(vec![
        ChainOption::ready(
            ChainFamily::Cardano,
            vec![
                eternl,
                WalletOption::new("lace", "Lace"),
                WalletOption::new("vespr", "Vespr"),
                WalletOption::new("typhoncip30", "Typhon"),
            ],
        ),
        ChainOption::ready(
            ChainFamily::Evm,
            vec![
                WalletOption::new("io.rabby", "Rabby"),
                WalletOption::new("io.metamask", "MetaMask"),
            ],
        ),
        ChainOption::soon(ChainFamily::Solana),
        ChainOption::soon(ChainFamily::Midnight),
        ChainOption::soon(ChainFamily::Tezos),
        ChainOption::soon(ChainFamily::Xrpl),
    ])
    .with_intro("Connecting shares your address. Nothing is signed or sent.")
}

fn connected(family: ChainFamily, wallet: &str) -> ChainPhase {
    ChainPhase::Connected {
        wallet: wallet.to_string(),
        address: match family {
            ChainFamily::Evm => evm_address(),
            _ => STAKE.to_string(),
        },
    }
}

/// Answers the modal's actions as a bridge would, a moment later.
#[derive(Default)]
struct FakeHost {
    /// The wallet asked, on which chain, and the seconds until it answers.
    pending: Option<(ChainFamily, String, f32)>,
}

impl FakeHost {
    fn apply(&mut self, vm: &mut WalletModalVm, action: &WalletModalAction) {
        match action {
            WalletModalAction::Connect { family, wallet } => {
                vm.set_phase(
                    *family,
                    ChainPhase::Connecting {
                        wallet: wallet.clone(),
                    },
                );
                self.pending = Some((*family, wallet.clone(), ANSWER_SECS));
            }
            WalletModalAction::Cancel(family) | WalletModalAction::Disconnect(family) => {
                if self.pending.as_ref().is_some_and(|(f, _, _)| f == family) {
                    self.pending = None;
                }
                vm.set_phase(*family, ChainPhase::Idle);
            }
        }
    }

    fn tick(&mut self, vm: &mut WalletModalVm, dt: f32) {
        let Some((family, wallet, left)) = &mut self.pending else {
            return;
        };
        *left -= dt;
        if *left > 0.0 {
            return;
        }
        let phase = match wallet.as_str() {
            "vespr" => ChainPhase::Declined {
                wallet: wallet.clone(),
            },
            "typhoncip30" => ChainPhase::Failed {
                wallet: wallet.clone(),
                message: "Typhon did not answer. Check the extension is unlocked and on \
                          mainnet, then try again."
                    .to_string(),
            },
            _ => connected(*family, wallet),
        };
        vm.set_phase(*family, phase);
        self.pending = None;
    }
}

pub struct WalletModalStory {
    view: ModalView,
    /// Laid out at a phone's width, where the card is a bottom sheet.
    phone: bool,
    modal: WalletModal,
    vm: WalletModalVm,
    icon: Option<Texture2D>,
    host: FakeHost,
    /// Story seconds, for the walkthrough's script.
    t: f32,
    /// The next cue the walkthrough plays.
    cue: usize,
    /// The last thing the modal or a button reported.
    last: String,
}

impl WalletModalStory {
    pub fn new(view: ModalView, phone: bool, icon: Option<Texture2D>) -> Self {
        let mut story = Self {
            view,
            phone,
            modal: WalletModal::new(),
            vm: roadmap(icon.clone()),
            icon,
            host: FakeHost::default(),
            t: 0.0,
            cue: 0,
            last: String::new(),
        };
        if let ModalView::Step(preset) = view {
            story.preset(preset);
        }
        story
    }

    fn preset(&mut self, preset: Preset) {
        use ChainFamily::{Cardano, Evm};
        let vm = &mut self.vm;
        let mut modal = WalletModal::new();
        match preset {
            Preset::Chains => modal.open(),
            Preset::Wallets => modal.open_on(Cardano),
            Preset::NoWallets => {
                if let Some(chain) = vm.chain_mut(Cardano) {
                    chain.wallets.clear();
                }
                modal.open_on(Cardano);
            }
            Preset::Connecting => {
                vm.set_phase(
                    Cardano,
                    ChainPhase::Connecting {
                        wallet: "eternl".into(),
                    },
                );
                modal.open_on(Cardano);
            }
            Preset::Connected => {
                vm.set_phase(Cardano, connected(Cardano, "eternl"));
                modal.open_on(Cardano);
            }
            Preset::Declined | Preset::Failed => {
                let phase = if preset == Preset::Declined {
                    ChainPhase::Declined {
                        wallet: "vespr".into(),
                    }
                } else {
                    ChainPhase::Failed {
                        wallet: "typhoncip30".into(),
                        message: "Typhon did not answer. Check the extension is unlocked and \
                                  on mainnet, then try again."
                            .into(),
                    }
                };
                vm.set_phase(Cardano, phase);
                modal.open_on(Cardano);
                // A refusal is an answer to a wallet just asked, so the flow
                // is on that answer rather than on the list.
                let key = if preset == Preset::Declined {
                    "vespr"
                } else {
                    "typhoncip30"
                };
                modal.choose_wallet(key);
            }
            Preset::OnlyEvm => modal.open_on(Evm),
            Preset::TwoConnected => {
                vm.set_phase(Cardano, connected(Cardano, "eternl"));
                vm.set_phase(Evm, connected(Evm, "io.rabby"));
                modal.open();
            }
        }
        self.modal = modal.settled();
    }

    pub fn draw(&mut self, p: &Painter, area: Rect, dt: f32) {
        let stage = if self.phone {
            Rect::new(area.x, area.y, PHONE_W.min(area.w), area.h.min(760.0))
        } else {
            area
        };
        backdrop(p, stage);
        match self.view {
            ModalView::Buttons => self.buttons(p, stage),
            _ => self.flow(p, stage, dt),
        }
        if !self.last.is_empty() {
            p.mono(
                &self.last,
                stage.x,
                stage.bottom() + 16.0,
                p.size(TextSize::Md),
                p.theme.color.accent,
            );
        }
    }

    fn flow(&mut self, p: &Painter, stage: Rect, dt: f32) {
        if self.view == ModalView::Walkthrough {
            self.play(dt);
        }
        self.host.tick(&mut self.vm, dt);

        // The screen behind the modal takes no tap while the modal is up.
        let behind = Painter::new(
            p.font,
            p.mono,
            p.theme,
            if self.modal.is_visible() { None } else { p.tap },
        )
        .at_time(p.time());
        let w = if self.phone { 180.0 } else { 220.0 };
        let button = Rect::new(stage.right() - w - 16.0, stage.y + 16.0, w, 44.0);
        if wallet_button(&behind, &self.vm, button) {
            self.modal.open();
            self.last = "button: open".into();
        }

        if let Some(action) = self.modal.show(p, &self.vm, stage, dt) {
            self.last = format!("action: {action:?}");
            self.host.apply(&mut self.vm, &action);
        }
    }

    /// The walkthrough's script, on a loop.
    fn play(&mut self, dt: f32) {
        self.t += dt;
        if self.t >= LOOP_SECS {
            self.t -= LOOP_SECS;
            self.cue = 0;
            self.modal = WalletModal::new();
            self.vm = roadmap(self.icon.clone());
            self.host = FakeHost::default();
        }
        while let Some(&(at, cue)) = SCRIPT.get(self.cue) {
            if self.t < at {
                break;
            }
            self.cue += 1;
            match cue {
                Cue::Open => self.modal.open(),
                Cue::Chain(family) => self.modal.choose_chain(&self.vm, family),
                Cue::Wallet(key) => {
                    if let Some(action) = self.modal.choose_wallet(key) {
                        self.host.apply(&mut self.vm, &action);
                    }
                }
                Cue::Done => self.modal.close(),
            }
        }
    }

    /// The button in each state it can be in, side by side, each live to hover.
    fn buttons(&mut self, p: &Painter, stage: Rect) {
        use ChainFamily::{Cardano, Evm};
        let c = p.theme.color;
        let mut vms: Vec<(&str, WalletModalVm)> = Vec::new();
        vms.push(("nothing connected", roadmap(None)));
        let mut vm = roadmap(None);
        vm.set_phase(
            Cardano,
            ChainPhase::Connecting {
                wallet: "eternl".into(),
            },
        );
        vms.push(("connecting", vm));
        let mut vm = roadmap(None);
        vm.set_phase(Cardano, connected(Cardano, "eternl"));
        vms.push(("Cardano connected", vm));
        let mut vm = roadmap(None);
        vm.set_phase(Evm, connected(Evm, "io.rabby"));
        vms.push(("EVM connected", vm));
        let mut vm = roadmap(None);
        vm.set_phase(Cardano, connected(Cardano, "eternl"));
        vm.set_phase(Evm, connected(Evm, "io.rabby"));
        vms.push(("two chains connected", vm));

        // Below the backdrop's "your game" plate.
        let x = stage.x + 24.0;
        let mut y = stage.y + 84.0;
        for (label, vm) in &vms {
            p.text_mid(label, x, y + 22.0, p.size(TextSize::Md), c.text_muted);
            for (i, w) in [220.0, 170.0].into_iter().enumerate() {
                let bx = x + 170.0 + i as f32 * 240.0;
                if bx + w > stage.right() - 8.0 {
                    break;
                }
                if wallet_button(p, vm, Rect::new(bx, y, w, 44.0)) {
                    self.last = format!("tapped: {label}");
                }
            }
            y += 64.0;
        }
        p.text(
            "the right column is the same button at a phone's width",
            x,
            y + 8.0,
            p.size(TextSize::Md),
            c.text_muted,
        );
    }
}

/// A stand-in game screen, so the modal is judged over something.
fn backdrop(p: &Painter, stage: Rect) {
    let c = p.theme.color;
    fill_gradient(
        stage,
        12.0,
        Gradient::Vertical {
            top: c.bg_secondary,
            bottom: c.bg_primary,
        },
    );
    let step = 32.0;
    let ink = Color::new(c.border.r, c.border.g, c.border.b, 0.35);
    let mut gx = stage.x + step;
    while gx < stage.right() {
        draw_line(gx, stage.y, gx, stage.bottom(), 1.0, ink);
        gx += step;
    }
    let mut gy = stage.y + step;
    while gy < stage.bottom() {
        draw_line(stage.x, gy, stage.right(), gy, 1.0, ink);
        gy += step;
    }
    let plate = Rect::new(stage.x + 16.0, stage.y + 16.0, 150.0, 44.0);
    fill_rounded(plate, 10.0, c.bg_highlight);
    p.text_mid(
        "YOUR GAME",
        plate.x + 14.0,
        plate.center().y,
        p.size(TextSize::Lg),
        c.text_secondary,
    );
    stroke_rounded(stage, 12.0, 1.0, c.border);
}
