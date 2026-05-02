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
