//! Wallet fixtures for stories — connectors as they look after a real connect,
//! without a browser.
//!
//! A storybook has no wallet extensions, so every story that draws a *connected*
//! surface has to seed one. That was eight hand-rolled connectors across the
//! wallet stories, each poking the fields it happened to know about; this is the
//! one place the shape lives, so a change to it is one edit rather than eight.
//!
//! The stake address is the estate's shared fixture (see `user_badge`,
//! `wallet_cta` and `services/wallet-link`'s tests) — not a live wallet.

use egui_widgets::wallet::{
    CardanoConnection, ChainConnection, ChainFamily, EvmConnection, EvmWalletId, EvmWalletInfo,
    Network, WalletBalance, WalletConnector, WalletInfo, WalletProvider,
};

/// The stake address every wallet fixture in the estate uses.
pub const STAKE: &str = "stake1u8962x3wtddcq2syq258ka3d9mxxkx5md5xawzx67pac9tgc5rhq9";

/// A change address, hex, in the shape CIP-30 hands back. Synthetic on purpose:
/// a real one would be a real wallet's.
fn address_hex() -> String {
    format!("01{}", "ab".repeat(28))
}

/// A CIP-30 session, as `apply_connect_result` builds one.
pub fn cardano_connection() -> CardanoConnection {
    CardanoConnection {
        provider: WalletProvider::Eternl,
        address_hex: address_hex(),
        stake_address: Some(STAKE.to_string()),
        network: Network::Mainnet,
        // No handle in a fixture: the sign path needs one, display does not, and
        // a story that wants it sets it.
        api: None,
        icon: None,
    }
}

/// An EIP-1193 session, as an EVM connect builds one.
pub fn evm_connection() -> EvmConnection {
    EvmConnection {
        wallet: EvmWalletInfo {
            id: EvmWalletId::from("io.rabby".to_string()),
            name: "Rabby".to_string(),
            icon: None,
            rdns: Some("io.rabby".to_string()),
        },
        address: format!("0x{}", "1f".repeat(20)),
    }
}

/// A connector with Cardano connected.
pub fn cardano() -> WalletConnector {
    let mut connector = WalletConnector::default();
    connector
        .connections
        .set_connected(ChainConnection::Cardano(cardano_connection()));
    connector
}

/// The same, plus the `$handle` a balance read derives — which is the name most
/// wallet surfaces actually draw — and a balance to draw beside it.
///
/// A balance is what `apply_balance` produces for Cardano: the reading the
/// button and the account pill both show, and the one an EVM connection has
/// none of.
pub fn cardano_with_handle(handle: &str) -> WalletConnector {
    let mut connector = cardano();
    connector.handle = Some(handle.to_string());
    connector.balance = Some(WalletBalance {
        lovelace: 4_080_590_650,
        assets: Default::default(),
    });
    connector
}

/// The same, mid-connect: the wallet has been asked and has not answered.
pub fn connecting() -> WalletConnector {
    let mut connector = WalletConnector::default();
    connector.connections.set_connecting(ChainFamily::Cardano);
    connector
}

/// **Both chains connected at once** — the state the set model exists for, and
/// the one a single-slot connector could not represent.
///
/// Offers are seeded too: a real browser has the extensions installed at the
/// same time as it has them connected, and a chain the flow cannot offer is a
/// different (much rarer) picture.
pub fn two_chains() -> WalletConnector {
    let mut connector = cardano_with_handle("$boef");
    connector.available_wallets = vec![WalletInfo {
        api_name: "eternl".to_string(),
        name: "Eternl".to_string(),
        icon: None,
    }];
    connector.available_evm_wallets = vec![evm_connection().wallet];
    connector
        .connections
        .set_connected(ChainConnection::Evm(evm_connection()));
    connector
}

/// EVM on its own — what a surface looks like when the only connection is one
/// with no balance to draw.
pub fn evm_only() -> WalletConnector {
    let mut connector = WalletConnector {
        available_evm_wallets: vec![evm_connection().wallet],
        ..Default::default()
    };
    connector
        .connections
        .set_connected(ChainConnection::Evm(evm_connection()));
    connector
}
