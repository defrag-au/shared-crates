//! [`CardanoModalHost`] — the real Cardano wallets, for `macroquad_gamekit`'s
//! `WalletModal`.
//!
//! The modal owns no connection: it reports actions and draws the phase the
//! host writes back. This is that host for Cardano, built on [`WalletFlow`] —
//! the same connect sequence, network check and remembered wallet every
//! macroquad surface already uses — plus the wallets' own icons, which the flow
//! never needed.
//!
//! ```ignore
//! let mut cardano = CardanoModalHost::new(WalletFlow::new().expect_network(1));
//! // every frame:
//! cardano.poll();
//! vm.chains[0] = cardano.chain();
//! if let Some(action) = modal.show(&p, &vm, area, dt) {
//!     cardano.apply(&action);
//! }
//! ```
//!
//! ## Icons
//!
//! Each detected wallet's CIP-30 `icon` is asked for once, through
//! [`Bridge::icon`], and arrives as RGBA the browser rasterised — SVG or PNG
//! alike. A wallet whose icon is missing or will not load keeps its monogram.
//! Pixels become a texture the first time [`CardanoModalHost::chain`] runs
//! after they arrive, which is why that takes `&mut self` and must run where a
//! graphics context exists.

use macroquad::prelude::{FilterMode, Texture2D};
use macroquad_gamekit::wallet_modal::{
    ChainFamily, ChainOption, ChainPhase, WalletModalAction, WalletOption,
};
use wallet_miniquad::{PollResultDetailed, ReqId};

use crate::{Bridge, MiniquadBridge, Stage, WalletFlow, network_message};

/// The square an icon is rasterised to. Twice the largest size the modal draws
/// one (56 points), so it stays sharp on a 2× display.
pub const ICON_PX: u16 = 128;

/// Polls between re-reads of the extension list while nothing is connected:
/// half a second at 60 frames a second.
const RELIST_EVERY: u32 = 30;

/// One detected wallet's icon, on its way from the bridge to a texture.
enum Icon {
    Asked(ReqId),
    Pixels(Vec<u8>),
    Ready(Texture2D),
    /// None announced, or it would not load: the monogram stays.
    Missing,
}

/// See the [module docs](self).
pub struct CardanoModalHost<B: Bridge = MiniquadBridge> {
    flow: WalletFlow<B>,
    /// The wallet this host last asked — by the modal, or by the flow's own
    /// connect of the remembered wallet — so a phase can name it.
    asked: Option<String>,
    /// Per provider key, in the order the flow lists them.
    icons: Vec<(String, Icon)>,
    /// Polls since the extension list was last read.
    since_list: u32,
}

impl<B: Bridge> CardanoModalHost<B> {
    /// Wrap a flow. A flow that is already connecting its remembered wallet
    /// hands that wallet over, so the button says who it is waiting on.
    pub fn new(flow: WalletFlow<B>) -> Self {
        let asked = flow.attempt.clone();
        let mut host = Self {
            flow,
            asked,
            icons: Vec::new(),
            since_list: 0,
        };
        host.ask_icons();
        host
    }

    /// The flow underneath, for the reads a builder needs: the change address,
    /// the network, the blocker.
    pub fn flow(&self) -> &WalletFlow<B> {
        &self.flow
    }

    pub fn flow_mut(&mut self) -> &mut WalletFlow<B> {
        &mut self.flow
    }

    /// Advance the connect and the icons. Call once per frame.
    pub fn poll(&mut self) {
        self.flow.poll();
        self.since_list += 1;
        if self.since_list >= RELIST_EVERY {
            self.since_list = 0;
            self.relist();
        }
        self.ask_icons();
        for (_, icon) in &mut self.icons {
            let Icon::Asked(req) = *icon else {
                continue;
            };
            *icon = match self.flow.bridge.poll(req) {
                PollResultDetailed::Pending => continue,
                PollResultDetailed::Ok { .. } => self
                    .flow
                    .bridge
                    .take_icon(req, ICON_PX)
                    .map_or(Icon::Missing, Icon::Pixels),
                PollResultDetailed::Err { .. } => Icon::Missing,
            };
        }
    }

    /// Apply what the modal reported. Actions for any other chain are not
    /// this host's, and are ignored.
    pub fn apply(&mut self, action: &WalletModalAction) {
        match action {
            WalletModalAction::Connect {
                family: ChainFamily::Cardano,
                wallet,
            } => {
                self.asked = Some(wallet.clone());
                self.flow.connect(wallet);
            }
            // Back to the picker, with the wallet's dialog let go of. Not a
            // disconnect: nothing was connected, and the remembered wallet is
            // not this request's to forget.
            WalletModalAction::Cancel(ChainFamily::Cardano) => self.flow.retry(),
            WalletModalAction::Disconnect(ChainFamily::Cardano) => {
                self.asked = None;
                self.flow.disconnect();
            }
            _ => {}
        }
    }

    /// Cardano as the modal lists it: the detected wallets, with their icons
    /// once loaded, and the connection's phase.
    pub fn chain(&mut self) -> ChainOption {
        let mut wallets = Vec::with_capacity(self.flow.providers.len());
        for provider in &self.flow.providers {
            let mut option = WalletOption::new(provider.key.clone(), provider.name.clone());
            if let Some((_, icon)) = self.icons.iter_mut().find(|(k, _)| *k == provider.key) {
                if let Icon::Pixels(rgba) = icon {
                    let texture = Texture2D::from_rgba8(ICON_PX, ICON_PX, rgba);
                    texture.set_filter(FilterMode::Linear);
                    *icon = Icon::Ready(texture);
                }
                if let Icon::Ready(texture) = icon {
                    option = option.with_icon(texture.clone());
                }
            }
            wallets.push(option);
        }
        ChainOption::ready(ChainFamily::Cardano, wallets).with_phase(self.phase())
    }

    /// The connection, in the modal's words.
    pub fn phase(&self) -> ChainPhase {
        phase_for(self.flow.stage(), self.asked.as_deref(), self.flow.notice())
    }

    /// Re-read the extension list while the picker is up. A wallet injects
    /// `window.cardano.<key>` when it pleases — some after the page has loaded
    /// — and the flow reads the list only when it starts or resets, so without
    /// this a late wallet would be missing until a reload.
    fn relist(&mut self) {
        if self.flow.pending.is_some() || !matches!(self.flow.stage, Stage::Disconnected) {
            return;
        }
        let providers = self.flow.bridge.list_providers();
        let same = providers.len() == self.flow.providers.len()
            && providers
                .iter()
                .zip(&self.flow.providers)
                .all(|(a, b)| a.key == b.key);
        if !same {
            self.flow.providers = providers;
            let stage = self.flow.stage.clone();
            self.flow.set_stage(stage);
        }
    }

    /// Ask for the icon of every listed wallet not yet asked about.
    fn ask_icons(&mut self) {
        for provider in &self.flow.providers {
            if self.icons.iter().any(|(k, _)| *k == provider.key) {
                continue;
            }
            let icon = self
                .flow
                .bridge
                .icon(&provider.key, ICON_PX)
                .map_or(Icon::Missing, Icon::Asked);
            self.icons.push((provider.key.clone(), icon));
        }
    }
}

/// The flow's stage as a modal phase.
///
/// A refused `enable()` leaves the flow on the picker with a notice; with a
/// wallet asked, that is the modal's "declined", so the answer step can offer
/// to try again.
fn phase_for(stage: &Stage, asked: Option<&str>, notice: Option<&str>) -> ChainPhase {
    let wallet = asked.unwrap_or_default().to_string();
    match stage {
        Stage::Unavailable => ChainPhase::Idle,
        Stage::Disconnected => match (asked, notice) {
            (Some(_), Some(_)) => ChainPhase::Declined { wallet },
            _ => ChainPhase::Idle,
        },
        Stage::Connecting
        | Stage::FetchingNetwork
        | Stage::FetchingAddress
        | Stage::FetchingStake => ChainPhase::Connecting { wallet },
        Stage::Ready { address_bech32, .. } => ChainPhase::Connected {
            wallet,
            address: address_bech32.clone(),
        },
        Stage::WrongNetwork {
            network_id,
            expected,
        } => ChainPhase::Failed {
            wallet,
            message: network_message(*network_id, *expected),
        },
        Stage::Failed { message } => ChainPhase::Failed {
            wallet,
            message: message.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use wallet_miniquad::{FailureKind, Provider};

    /// A mainnet base address: header 0x01, a key payment hash, a key stake
    /// hash. Constructed so the headers are visible.
    fn base_address() -> String {
        format!("01{}{}", "11".repeat(28), "22".repeat(28))
    }

    /// Answers polls in order; icons are asked but never arrive, which is what
    /// a test without a browser can honestly say.
    struct Fake {
        results: VecDeque<PollResultDetailed>,
        icons_asked: Vec<String>,
        providers: Vec<Provider>,
    }

    fn eternl() -> Provider {
        Provider {
            key: "eternl".into(),
            name: "Eternl".into(),
            version: "1".into(),
        }
    }

    impl Bridge for Fake {
        fn available(&self) -> bool {
            true
        }
        fn list_providers(&self) -> Vec<Provider> {
            self.providers.clone()
        }
        fn connect(&mut self, _key: &str) -> ReqId {
            ReqId(1)
        }
        fn network_id(&mut self) -> ReqId {
            ReqId(2)
        }
        fn change_address(&mut self) -> ReqId {
            ReqId(3)
        }
        fn reward_address(&mut self) -> ReqId {
            ReqId(4)
        }
        fn poll(&mut self, id: ReqId) -> PollResultDetailed {
            if id.0 >= 100 {
                return PollResultDetailed::Pending;
            }
            self.results
                .pop_front()
                .unwrap_or(PollResultDetailed::Pending)
        }
        fn cancel(&mut self, _id: ReqId) {}
        fn disconnect(&mut self) {}
        fn icon(&mut self, key: &str, _size: u16) -> Option<ReqId> {
            self.icons_asked.push(key.to_string());
            Some(ReqId(100))
        }
    }

    fn ok(data: &str) -> PollResultDetailed {
        PollResultDetailed::Ok {
            data: data.to_string(),
        }
    }

    fn host(results: Vec<PollResultDetailed>) -> CardanoModalHost<Fake> {
        CardanoModalHost::new(
            WalletFlow::with_bridge(Fake {
                results: results.into(),
                icons_asked: Vec::new(),
                providers: vec![eternl()],
            })
            .expect_network(1),
        )
    }

    fn connect(host: &mut CardanoModalHost<Fake>) {
        host.apply(&WalletModalAction::Connect {
            family: ChainFamily::Cardano,
            wallet: "eternl".into(),
        });
    }

    #[test]
    fn every_listed_wallet_has_its_icon_asked_for_once() {
        let mut host = host(Vec::new());
        host.poll();
        host.poll();
        assert_eq!(host.flow.bridge.icons_asked, vec!["eternl".to_string()]);
        let chain = host.chain();
        assert_eq!(chain.wallets.len(), 1);
        assert!(chain.wallets[0].icon.is_none(), "no pixels arrived");
    }

    #[test]
    fn a_connect_reads_as_connecting_then_connected() {
        let mut host = host(vec![ok("eternl"), ok("1"), ok(&base_address()), ok("")]);
        connect(&mut host);
        assert_eq!(
            host.phase(),
            ChainPhase::Connecting {
                wallet: "eternl".into()
            }
        );
        for _ in 0..4 {
            host.poll();
        }
        let ChainPhase::Connected { wallet, address } = host.phase() else {
            panic!("expected connected, got {:?}", host.phase());
        };
        assert_eq!(wallet, "eternl");
        assert!(address.starts_with("addr1"), "{address}");
    }

    #[test]
    fn a_refusal_reads_as_declined_and_a_cancel_as_idle() {
        let mut host = host(vec![PollResultDetailed::Err {
            data: "User declined".into(),
            kind: FailureKind::Declined,
        }]);
        connect(&mut host);
        host.poll();
        assert_eq!(
            host.phase(),
            ChainPhase::Declined {
                wallet: "eternl".into()
            }
        );

        connect(&mut host);
        host.apply(&WalletModalAction::Cancel(ChainFamily::Cardano));
        assert_eq!(host.phase(), ChainPhase::Idle);
    }

    #[test]
    fn a_wrong_network_reads_as_a_failure_that_says_so() {
        let mut host = host(vec![ok("eternl"), ok("0")]);
        connect(&mut host);
        host.poll();
        host.poll();
        let ChainPhase::Failed { message, .. } = host.phase() else {
            panic!("expected failed, got {:?}", host.phase());
        };
        assert!(message.contains("Wrong network"), "{message}");
    }

    /// An extension that injects itself after the page loaded is listed — and
    /// has its icon asked for — without a reload.
    #[test]
    fn a_wallet_that_arrives_late_is_picked_up() {
        let mut host = CardanoModalHost::new(WalletFlow::with_bridge(Fake {
            results: VecDeque::new(),
            icons_asked: Vec::new(),
            providers: Vec::new(),
        }));
        assert!(host.chain().wallets.is_empty());
        host.flow.bridge.providers.push(eternl());
        for _ in 0..RELIST_EVERY {
            host.poll();
        }
        let chain = host.chain();
        assert_eq!(chain.wallets.len(), 1);
        assert_eq!(chain.wallets[0].name, "Eternl");
        assert_eq!(host.flow.bridge.icons_asked, vec!["eternl".to_string()]);
    }

    #[test]
    fn another_chains_action_is_left_alone() {
        let mut host = host(Vec::new());
        host.apply(&WalletModalAction::Connect {
            family: ChainFamily::Evm,
            wallet: "io.rabby".into(),
        });
        assert_eq!(host.phase(), ChainPhase::Idle);
        assert!(matches!(host.flow.stage(), Stage::Disconnected));
    }
}
