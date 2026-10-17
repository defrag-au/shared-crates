//! Storybook demo for `WalletConnectModal` — the chain-then-wallet flow.

use egui_widgets::wallet::{ChainConnection, WalletConnector, WalletInfo};
use egui_widgets::wallet_connect_modal::{ChainFamily, ChainRequirement, WalletConnectModal};

use crate::muted;
use crate::stories::wallet_fixture;

/// Which screen of the flow this story holds open.
///
/// One screen at a time, because a modal covers the surface it was opened from
/// and two of them would stack. `Live` is the one to read first: it is this
/// browser, detected, with nothing seeded. The rest are seeded, because a state
/// that needs a *connection* cannot be reached without a wallet to click.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    Live,
    Chain,
    Wallet,
    Both,
    Nothing,
    Asked,
    Answered,
    Refused,
}

impl Screen {
    const ALL: [Self; 8] = [
        Self::Live,
        Self::Chain,
        Self::Wallet,
        Self::Both,
        Self::Nothing,
        Self::Asked,
        Self::Answered,
        Self::Refused,
    ];

    /// The chip's word, and the flow's salt — one per screen, so switching
    /// screens starts the flow fresh rather than resuming the last one.
    fn slug(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Chain => "chain",
            Self::Wallet => "wallet",
            Self::Both => "both",
            Self::Nothing => "nothing",
            Self::Asked => "asked",
            Self::Answered => "answered",
            Self::Refused => "refused",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Live => "this browser",
            Self::Chain => "which chain",
            Self::Wallet => "which wallet",
            Self::Both => "two chains connected",
            Self::Nothing => "nothing detected",
            Self::Asked => "asked",
            Self::Answered => "answered",
            Self::Refused => "refused",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Live | Self::Chain | Self::Wallet | Self::Both => "Connect a wallet",
            _ => "Link $djo",
        }
    }

    fn requirement(self) -> ChainRequirement {
        match self {
            // An EVM proof, with nothing in the browser to prove it with.
            Self::Nothing => ChainRequirement::Only(ChainFamily::Evm),
            _ => ChainRequirement::Any,
        }
    }

    fn note(self) -> &'static str {
        match self {
            Self::Live => {
                "THIS browser, detected — both halves, nothing seeded. Compare the count \
                 with the Wallet Button story: a listed wallet here is one the library \
                 actually found."
            }
            Self::Chain => {
                "Seeded: two chains have wallets, so the chain comes first. A chain with \
                 nothing to offer is not a row here — the screen below is what says so."
            }
            Self::Wallet => {
                "Seeded: one chain has wallets, so the chain step is skipped: a choice with \
                 one answer is not a choice."
            }
            Self::Both => {
                "Seeded: Cardano AND EVM connected at once — the state a single connection \
                 slot cannot hold. Each chain names the wallet it is connected as, and \
                 disconnecting one leaves the other alone."
            }
            Self::Nothing => {
                "Seeded: the action needs EVM and the browser announced none. The flow \
                 names the chain that is missing and says what to do about it."
            }
            Self::Asked => {
                "Seeded: one wallet detected, so both earlier steps are skipped and it is \
                 simply asked. A host spawns the connect on the choice this screen emits."
            }
            Self::Answered => {
                "Seeded: what the wallet answered. The account is the point of this screen \
                 — a wallet holds several, and naming the one that signed is what makes a \
                 downstream \"does not sign with that key\" answerable."
            }
            Self::Refused => {
                "Seeded: refusing is an answer too — the message is the wallet's own, and \
                 Try again re-asks the same wallet rather than guessing at another."
            }
        }
    }
}

/// Every screen's connector, seeded once: a storybook browser has no extensions,
/// so nothing would render otherwise.
pub struct WalletConnectStoryState {
    screen: Screen,
    /// Whether this screen's modal has been asked to open. Set on the first
    /// frame, cleared when a chip is clicked — so the reader can dismiss a modal
    /// and open the next screen, rather than having it forced back open.
    opened: bool,
    /// **Detection.** What this browser actually offers — both halves, the same
    /// call the Wallet Button story makes. Nothing seeded.
    live: WalletConnector,
    both_chains: WalletConnector,
    cardano_only: WalletConnector,
    one_wallet: WalletConnector,
    answered: WalletConnector,
    refused: WalletConnector,
    two_connected: WalletConnector,
}

/// `?flow=<slug>` opens the story on that screen.
///
/// The same device as `?nav=0`, and needed for the same reason: a screen that is
/// a modal cannot be reached by a click from outside it — the scrim eats the
/// click — so without this a headless browser could only ever shoot the screen
/// the story starts on.
fn screen_from_location() -> Option<Screen> {
    #[cfg(target_arch = "wasm32")]
    let query = web_sys::window()?.location().search().ok()?;
    #[cfg(not(target_arch = "wasm32"))]
    let query = std::env::var("STORYBOOK_FLOW").ok()?;

    let wanted = query
        .trim_start_matches('?')
        .split('&')
        .find_map(|pair| pair.strip_prefix("flow="))?;
    Screen::ALL
        .into_iter()
        .find(|screen| screen.slug() == wanted)
}

/// A detected wallet as the extension reports it. No icon: a headless browser
/// has none to copy, and the rows read fine with the name alone.
fn detected(api_name: &str, name: &str) -> WalletInfo {
    WalletInfo {
        api_name: api_name.to_string(),
        name: name.to_string(),
        icon: None,
    }
}

impl Default for WalletConnectStoryState {
    fn default() -> Self {
        // `new()`, not `default()`, so a detection pass has run before the
        // fields are overwritten, exactly as it would in a host.
        let mut both_chains = WalletConnector::new();
        both_chains.available_wallets =
            vec![detected("eternl", "Eternl"), detected("lace", "Lace")];
        both_chains.set_evm_wallets(egui_widgets::wallet_mock::evm_wallets());

        let mut cardano_only = WalletConnector::new();
        cardano_only.available_wallets =
            vec![detected("eternl", "Eternl"), detected("lace", "Lace")];

        let mut one_wallet = WalletConnector::new();
        one_wallet.available_wallets = vec![detected("eternl", "Eternl")];

        // Connected, and offered: the receipt screen needs both, and the
        // connection is the estate's shared Cardano fixture.
        let mut answered = WalletConnector::new();
        answered.available_wallets = vec![detected("eternl", "Eternl")];
        answered.connections.set_connected(ChainConnection::Cardano(
            wallet_fixture::cardano_connection(),
        ));

        let mut refused = WalletConnector::new();
        refused.available_wallets = vec![detected("eternl", "Eternl")];
        // Captured from a real Eternl/Ledger link, 2026-10-15 — the failure this
        // flow's last screen exists to make legible.
        refused.set_error(
            ChainFamily::Cardano,
            "Couldn't link $djo: this wallet does not sign with \
             stake1u9pnzqcrvnt6njaqkexglkqtcktxrdc4lt7mdtsxafjzdfsgm5ex2 — switch to that \
             account in your wallet and reconnect, or link the account you have connected",
        );

        Self {
            screen: screen_from_location().unwrap_or(Screen::Live),
            opened: false,
            live: WalletConnector::new(),
            both_chains,
            cardano_only,
            one_wallet,
            answered,
            refused,
            two_connected: wallet_fixture::two_chains(),
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut WalletConnectStoryState) {
    crate::caption(
        ui,
        "Choosing a wallet is a sequence — chain, then wallet, then what the wallet \
         answered — and each step is skipped when its answer is already known. \
         \"this browser\" is a real detection; every other chip is seeded, because a \
         state that needs a connection cannot be reached without a wallet to click.",
    );
    ui.add_space(10.0);

    ui.horizontal_wrapped(|ui| {
        for screen in Screen::ALL {
            if ui
                .selectable_label(state.screen == screen, screen.label())
                .clicked()
            {
                state.screen = screen;
                state.opened = false;
            }
        }
    });
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "A modal's screens cannot be clicked through from outside it — \
                             `?flow=<screen>` opens one directly.",
        )
        .color(muted(ui))
        .size(11.0),
    );
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(state.screen.note())
            .color(muted(ui))
            .size(11.0),
    );
    ui.add_space(12.0);

    let screen = state.screen;
    let connector = match screen {
        Screen::Live => &state.live,
        Screen::Chain => &state.both_chains,
        Screen::Wallet => &state.cardano_only,
        Screen::Both => &state.two_connected,
        Screen::Nothing => &state.cardano_only,
        Screen::Asked => &state.one_wallet,
        Screen::Answered => &state.answered,
        Screen::Refused => &state.refused,
    };

    // A host opens the flow from its own trigger — the header button, the CTA.
    // The story opens it on the selected screen instead, so each one can be
    // looked at without a click-through — and `?flow=` picks which, because a
    // click cannot reach through a modal's own scrim.
    let modal = WalletConnectModal::new(WalletConnectModal::id_for(ui, screen.slug()))
        .title(screen.title())
        .requirement(screen.requirement());
    if !state.opened {
        modal.open(ui);
        state.opened = true;
    }

    // The action is a host's to handle (spawn the connect, send the result
    // back). This story is about the screens, so it is reported rather than
    // acted on — and no screen here depends on it having been.
    let _ = modal.show(ui, connector);
}
