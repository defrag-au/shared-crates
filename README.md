# defrag shared crates

Honestly, this isn't for public consumption but if you happen across it and something is useful to you then you're more than welcome to use it.  If you consider it super useful then happy to look at publishing crates also, but for the moment quite happy just running with git for crate imports, etc.

The macroquad UI and game layer — the widgets, the gamekit, the app shell, the bundlers and the Cardano macroquad edges — lives in [`macroquad-kit`](https://github.com/defrag-au/macroquad-kit). That repo consumes this one for the pure crates it needs; no crate here may depend on it.

## LICENSE

Dual licensed under MIT + Apache 2 (see LICENSE)
