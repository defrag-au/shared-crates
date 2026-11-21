# defrag shared crates

The shared engine behind defrag's workers, sites and games. It is public so a fork can
build without a private repository, but it is **not a published library**: consumers pin
it by git revision and commit their `Cargo.lock`, and there is no semver promise yet. If
something here is useful to you, you are welcome to use it under the licence below.

A consumer pins the crates it uses to one revision:

```toml
[dependencies]
worker_stack = { git = "https://github.com/defrag-au/shared-crates", package = "worker_stack", rev = "…" }
worker_utils = { git = "https://github.com/defrag-au/shared-crates", package = "worker_utils", rev = "…" }
```

For local work, a `[patch."https://github.com/defrag-au/shared-crates"]` block points the
same packages at a sibling checkout. Toggle the whole block as a unit — a line left in
both the patch and the dependency table is a duplicate-key error.

The macroquad UI and game layer — the widgets, the gamekit, the app shell, the bundlers and the Cardano macroquad edges — lives in [`macroquad-kit`](https://github.com/defrag-au/macroquad-kit). That repo consumes this one for the pure crates it needs; no crate here may depend on it.

## LICENSE

Dual licensed under MIT + Apache 2 (see LICENSE). The workspace manifest declares the same
expression, so a crate with `license.workspace = true` inherits it.
