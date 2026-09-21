# Repository Guidelines

## Toolchain & Shell Environment
**All Rust tooling lives behind the Nix devshell in `flake.nix`.** `cargo`, `rustc`,
`clippy`, `rustfmt`, the `wasm32-unknown-unknown` target, `trunk`, `wrangler` and
`node` are **not on the default `PATH`**. A bare `cargo …` fails, and the failure
reads like a permissions problem rather than a missing toolchain — so it is worth
recognising before debugging it.

Wrap every command as `nix develop -c <cmd>`:

```sh
nix develop -c cargo build --workspace
nix develop -c cargo test -p egui-widgets
nix develop -c cargo clippy --workspace --all-targets -- -D warnings
```

`nix develop --command <cmd>` is equivalent. Where `.envrc` uses `use flake` and
`direnv` is on `PATH`, `direnv exec . <cmd>` also works — and in a **sandboxed
agent shell it is the one that works**, because `nix develop` needs the nix
daemon socket (`/nix/var/nix/daemon-socket/socket`), which the sandbox refuses
(`error: cannot connect to socket … Operation not permitted`). `direnv exec`
reads the already-realised devshell out of `.direnv/` instead and needs no
daemon. If `.direnv/` is cold, run `direnv allow .` once from an unsandboxed
shell.

Add `--offline` to a cargo invocation when a build would otherwise try to reach
the network for a crate the devshell's cache already holds — and note that a
sandboxed shell also cannot write the shared cargo registry cache
(`~/.cargo/registry`), so a *first* fetch of a new crate can fail with
`Operation not permitted` for that reason rather than a missing toolchain.

Do **not** install a separate Rust toolchain — the devshell pins the channel so
this repo stays in lock-step with `mitos` and `cnft.dev-workers`. `rustup` is not
available, so `rustup target add …` is never the answer; targets come from the
devshell. Add `--offline` to a cargo invocation when a build would otherwise try
to reach the network for a crate the devshell's cache already holds.

Subdirectory devshells: `ui/_storybook-egui` is served with
`nix develop ../.. -c trunk serve` (run from that directory) — see the
screenshot workflow in `CLAUDE.md` before reporting a widget as done.

## Project Structure & Module Organization
- Root is a Rust workspace (`Cargo.toml`, `Cargo.lock`).
- Member crates live in top‑level folders (e.g., `discord-client`, `http-client`, `worker-utils`).
- Each crate uses `src/` for code, `tests/` for integration tests, and optional `examples/` for runnable snippets.
- Some crates support WASM via the `wasm` feature and/or target `wasm32-unknown-unknown`.

## Build, Test, and Development Commands
All commands run inside the devshell (see *Toolchain & Shell Environment* above).
- Build workspace: `nix develop -c cargo build --workspace`
- Test workspace: `nix develop -c cargo test --workspace`
- Lint (deny warnings): `nix develop -c cargo clippy --workspace --all-targets -- -D warnings`
- Format: `nix develop -c cargo fmt --all`
- Run example (per crate): `nix develop -c cargo run -p discord-client --example native_example`
- WASM builds: the `wasm32-unknown-unknown` target is already in the devshell —
  add it to the invocation, e.g.
  `nix develop -c cargo check -p egui-widgets --target wasm32-unknown-unknown`.

## Coding Style & Naming Conventions
- Rust 2021 edition; use 4‑space indentation.
- Names: `snake_case` for functions/modules, `UpperCamelCase` for types/traits, `SCREAMING_SNAKE_CASE` for consts.
- Keep modules cohesive; prefer small crates with clear APIs.
- Always run `nix develop -c cargo fmt` and fix `nix develop -c cargo clippy` findings before pushing.

## Testing Guidelines
- Prefer unit tests near code and integration tests in `tests/`.
- Include examples in `examples/` when useful for API clarity.
- WASM tests use `wasm-bindgen-test` (see `discord-client/tests/`); run in a WASM-capable environment as needed.
- Make tests deterministic; avoid network calls unless feature‑gated or mocked.

## Commit & Pull Request Guidelines
- Commit style: prefix with a type, e.g., `feature: ...`, `chore: ...`, `fix: ...`. Reference issues/PRs when relevant (e.g., `(#14)`).
- Scope commits narrowly; write imperative, present‑tense messages.
- PRs: include a concise description, linked issue, test coverage for changes, and examples or output where applicable. Note any feature flags (`native`, `wasm`).

## Security & Configuration Tips
- Do not commit secrets. Use per‑crate `.env` files (see `discord-client/.env.example`) and keep `.env` out of VCS.
- For WASM builds, audit feature flags and minimize enabled dependencies.
- Prefer `rustls` over OpenSSL where possible (already used in HTTP clients).
