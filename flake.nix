{
  description = "shared-crates development shell — defrag-au common Rust crates";

  inputs = {
    defrag-nix.url = "github:defrag-au/defrag-nix";
  };

  outputs =
    { defrag-nix, ... }:
    let
      systems = [
        "aarch64-darwin"
        "x86_64-darwin"
        "x86_64-linux"
        "aarch64-linux"
      ];
      # Reuses `rust-worker-stack` from defrag-nix so the toolchain
      # (rustc, cargo, clippy, rustfmt, wasm-bindgen, wrangler, node,
      # aiken) is in lock-step with the mitos and cnft.dev-workers
      # repos that consume these crates. shared-crates legitimately
      # mixes native and wasm32 targets (e.g. `cnft_tools`,
      # `worker-utils`, `datum-parsing`, `cardano-tx` all build for
      # wasm32-unknown-unknown for CF Workers consumers) so the wasm
      # tooling is load-bearing here, not just incidental.
      mkShells =
        system: {
          default = defrag-nix.devShells.${system}.rust-worker-stack;
        };
    in
    {
      devShells = builtins.listToAttrs (
        map (system: {
          name = system;
          value = mkShells system;
        }) systems
      );
    };
}
