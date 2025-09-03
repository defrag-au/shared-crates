# Repository Guidelines

## Project Structure & Module Organization
- Root is a Rust workspace (`Cargo.toml`, `Cargo.lock`).
- Member crates live in top‑level folders (e.g., `discord-client`, `http-client`, `worker-utils`).
- Each crate uses `src/` for code, `tests/` for integration tests, and optional `examples/` for runnable snippets.
- Some crates support WASM via the `wasm` feature and/or target `wasm32-unknown-unknown`.

## Build, Test, and Development Commands
- Build workspace: `cargo build --workspace`
- Test workspace: `cargo test --workspace`
- Lint (deny warnings): `cargo clippy --workspace --all-targets -- -D warnings`
- Format: `cargo fmt --all`
- Run example (per crate): `cargo run -p discord-client --example native_example`
- WASM target setup (if needed): `rustup target add wasm32-unknown-unknown`

## Coding Style & Naming Conventions
- Rust 2021 edition; use 4‑space indentation.
- Names: `snake_case` for functions/modules, `UpperCamelCase` for types/traits, `SCREAMING_SNAKE_CASE` for consts.
- Keep modules cohesive; prefer small crates with clear APIs.
- Always run `cargo fmt` and fix `cargo clippy` findings before pushing.

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
