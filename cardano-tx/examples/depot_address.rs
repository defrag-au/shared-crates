//! Work out what `DEPOT_SIGNERS` should be, and what depot it gives.
//!
//! ```sh
//! # From an address you control — the safest input, because the payment
//! # credential is read out of it rather than guessed:
//! cargo run -p cardano-tx --example depot_address -- addr_test1qz…
//!
//! # Or straight from key hashes, in the order they go into the config:
//! cargo run -p cardano-tx --example depot_address -- db8fcdb4… 9ad4da1c…
//! ```
//!
//! ## Why this exists
//!
//! A depot is configured with **payment key hashes**, and every 28-byte hash
//! looks alike. A stake key hash is the same width and the same encoding, so a
//! wrong one passes every format check and produces a perfectly valid-looking
//! address that no wallet can ever open. You would find out at retirement, with
//! the ADA already locked.
//!
//! Pasting an ADDRESS removes the guess: this reads the payment credential out
//! of it, and refuses a stake address outright.

use cardano_tx::depot::Depot;
use pallas_addresses::{Address, Network, ShelleyPaymentPart};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!(
            "Usage: depot_address <payment-address | key-hash> [more…]\n\n\
             Prints the DEPOT_SIGNERS value and the depot address it derives, per network.\n\
             Order matters: it is part of the address."
        );
        std::process::exit(1);
    }

    let mut hashes: Vec<String> = Vec::new();
    for arg in &args {
        match resolve(arg) {
            Ok((hash, how)) => {
                println!("{arg}\n  → {hash}  ({how})");
                hashes.push(hash);
            }
            Err(e) => {
                eprintln!("{arg}\n  ✗ {e}");
                std::process::exit(1);
            }
        }
    }
    println!();

    let depot = match Depot::from_signers(&hashes) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    println!("DEPOT_SIGNERS = \"{}\"", hashes.join(","));
    println!();
    println!("  script hash : {}", depot.hash_hex());
    println!(
        "  preprod     : {}",
        depot
            .address(Network::Testnet)
            .to_bech32()
            .unwrap_or_default()
    );
    println!(
        "  mainnet     : {}",
        depot
            .address(Network::Mainnet)
            .to_bech32()
            .unwrap_or_default()
    );
    println!();
    println!(
        "The two addresses are different depots derived from the same signers — the network\n\
         id is part of the address. Each needs its own config entry, and anything parked in\n\
         one is invisible to the other."
    );
}

/// A key hash, and how it was arrived at.
fn resolve(input: &str) -> Result<(String, &'static str), String> {
    let raw = input.trim();

    // A bare hash: accept it, but say plainly that its KIND is unverifiable.
    if raw.len() == 56 && raw.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok((
            raw.to_ascii_lowercase(),
            "taken as given — nothing in a bare hash says whether it is a PAYMENT or a stake \
             credential; paste the address instead to be sure",
        ));
    }

    let address = Address::from_bech32(raw)
        .map_err(|e| format!("not a 28-byte hex hash and not a bech32 address: {e}"))?;

    match address {
        Address::Shelley(sh) => match sh.payment() {
            ShelleyPaymentPart::Key(hash) => Ok((
                hash.to_string(),
                "payment credential, read from the address",
            )),
            ShelleyPaymentPart::Script(_) => Err(
                "this is a SCRIPT address. A depot is opened by a key signature, so a script \
                 credential could never satisfy it."
                    .to_string(),
            ),
        },
        Address::Stake(_) => Err(
            "this is a STAKE address, and its credential is a stake credential. A wallet signs \
             a transaction with its PAYMENT key, so a stake credential never appears in the \
             witness set and a depot built from one could never be opened. Paste a payment \
             address (addr1…/addr_test1…) from the same wallet instead."
                .to_string(),
        ),
        Address::Byron(_) => Err(
            "this is a Byron-era address, which has no separable payment credential.".to_string(),
        ),
    }
}
