//! What a frame costs the wallet connector, counted rather than guessed.
//!
//! ```sh
//! cargo run -p macroquad-wallet-flow --example connector_cost --release
//! ```
//!
//! A counting allocator tallies heap allocations while each loop runs, and the
//! clock times it. Three loops, over the stress case — every chain family
//! listed, ten wallets each, every chain connected:
//!
//! 1. **Rebuild every frame**: each chain's listing written out again every
//!    frame — the shape the stories used before the connector.
//! 2. **The connector**: `WalletConnector::poll`, which rewrites a listing only
//!    when its host says it changed.
//! 3. **A real `CardanoHost`**, connected and idle over a bridge that answers
//!    nothing new: what one real host's own frame costs.
//!
//! Drawing is not measured: it needs a window. This is the host side, which
//! is what the connector changed.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use macroquad_gamekit::wallet_connection::{Connection, ConnectionEvent};
use macroquad_gamekit::wallet_modal::{
    Account, ChainFamily, ChainOption, WalletModalAction, WalletOption,
};
use macroquad_wallet_flow::{Bridge, CardanoHost, ChainHost, WalletConnector, WalletFlow};
use wallet_miniquad::{PollResultDetailed, Provider, ReqId};

struct Counting;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

const FRAMES: usize = 10_000;
const WALLETS: usize = 10;

/// A connected chain with nothing moving: the steady state of a session.
struct Settled {
    family: ChainFamily,
    connection: Connection,
}

impl Settled {
    fn new(family: ChainFamily) -> Self {
        let mut connection = Connection::new();
        connection.handle(&ConnectionEvent::Ask("wallet-0".into()));
        connection.handle(&ConnectionEvent::Answered {
            wallet: "wallet-0".into(),
            account: Account::new(format!("addr-{family:?}"))
                .with_label("$name")
                .copyable("Handle", "$name")
                .copyable("Receiving address", "addr1")
                .copyable("Stake address", "stake1")
                .fact("Network", "Mainnet"),
        });
        Self { family, connection }
    }
}

impl ChainHost for Settled {
    fn family(&self) -> ChainFamily {
        self.family
    }
    fn poll(&mut self) -> bool {
        false
    }
    fn option(&mut self) -> ChainOption {
        let wallets = (0..WALLETS)
            .map(|i| WalletOption::new(format!("wallet-{i}"), format!("Wallet {i}")))
            .collect();
        ChainOption::ready(self.family, wallets).with_phase(self.connection.phase())
    }
    fn apply(&mut self, _action: &WalletModalAction) {}
    fn connection(&self) -> &Connection {
        &self.connection
    }
}

/// A CIP-30 bridge that connects once and then has nothing new to say.
struct Quiet {
    answers: Vec<PollResultDetailed>,
}

impl Bridge for Quiet {
    fn available(&self) -> bool {
        true
    }
    fn list_providers(&self) -> Vec<Provider> {
        (0..WALLETS)
            .map(|i| Provider {
                key: format!("wallet-{i}"),
                name: format!("Wallet {i}"),
                version: "1".into(),
            })
            .collect()
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
    fn poll(&mut self, _id: ReqId) -> PollResultDetailed {
        if self.answers.is_empty() {
            PollResultDetailed::Pending
        } else {
            self.answers.remove(0)
        }
    }
    fn cancel(&mut self, _id: ReqId) {}
    fn disconnect(&mut self) {}
}

fn families() -> [ChainFamily; 6] {
    ChainFamily::ALL
}

/// Run `frame` `FRAMES` times: allocations and microseconds per frame.
fn measure(mut frame: impl FnMut()) -> (f64, f64) {
    let start_allocs = ALLOCATIONS.load(Ordering::Relaxed);
    let start = Instant::now();
    for _ in 0..FRAMES {
        frame();
    }
    let micros = start.elapsed().as_secs_f64() * 1e6;
    let allocs = ALLOCATIONS.load(Ordering::Relaxed) - start_allocs;
    (allocs as f64 / FRAMES as f64, micros / FRAMES as f64)
}

fn main() {
    // 1. Every listing written out again, every frame.
    let mut hosts: Vec<Settled> = families().into_iter().map(Settled::new).collect();
    let mut listed: Vec<ChainOption> = hosts.iter_mut().map(|h| h.option()).collect();
    let rebuild = measure(|| {
        for (host, entry) in hosts.iter_mut().zip(&mut listed) {
            *entry = host.option();
        }
    });

    // 2. The connector: listings rewritten only on change.
    let mut connector = families()
        .into_iter()
        .fold(WalletConnector::new(), |c, family| {
            c.host(Settled::new(family))
        });
    let cached = measure(|| connector.poll());

    // 3. One real Cardano host, connected and idle.
    let base = format!("01{}{}", "11".repeat(28), "22".repeat(28));
    let ok = |data: &str| PollResultDetailed::Ok {
        data: data.to_string(),
    };
    let bridge = Quiet {
        answers: vec![ok("wallet-0"), ok("1"), ok(&base), ok("")],
    };
    let mut cardano = CardanoHost::new(WalletFlow::with_bridge(bridge));
    cardano.apply(&WalletModalAction::Connect {
        family: ChainFamily::Cardano,
        wallet: "wallet-0".into(),
    });
    for _ in 0..8 {
        cardano.poll();
    }
    assert!(cardano.connection().is_connected(), "the host connected");
    let _ = cardano.option();
    let real = measure(|| {
        cardano.poll();
    });

    println!(
        "{} chains x {WALLETS} wallets, all connected, {FRAMES} frames",
        families().len()
    );
    println!("| loop | allocations / frame | µs / frame |");
    println!("|---|---|---|");
    for (name, (allocs, micros)) in [
        ("rebuild every listing every frame", rebuild),
        ("WalletConnector::poll", cached),
        ("one real CardanoHost, idle", real),
    ] {
        println!("| {name} | {allocs:.2} | {micros:.3} |");
    }
}
