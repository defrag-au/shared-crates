# utxo-optimizer

Cardano wallet UTxO optimization engine. Computes how to reorganize fragmented
UTxOs into clean, consolidated outputs using greedy bin-packing with transaction
size awareness.

## Features

- **Greedy bin-packing**: Consolidates tokens by policy, respecting configurable
  bundle sizes
- **Multi-step optimization**: Automatically splits optimization into multiple
  transactions when a single TX would exceed the 16KB max size
- **Configurable**: Bundle size, fungible/NFT isolation, ADA rollup/split
- **No UI dependency**: Pure algorithm crate, works in native and WASM targets
- **Detailed plans**: Produces step-by-step optimization plans with input/output
  tracking for visualization

## Acknowledgements

The optimization algorithm in this crate is based on
[unfrackit](https://github.com/crypto2099/unfrackit) by Adam Dean (crypto2099),
licensed under [CC-BY-4.0](https://creativecommons.org/licenses/by/4.0/).
The original JavaScript implementation was ported to Rust with modifications
including multi-step transaction support for large wallets.
