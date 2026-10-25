---
id: shared-crates-subdir-devshell
title: Subdirectory devshells are entered from the parent flake
layer: project
activation: project:shared-crates
priority: 55
overrides:
targets:
---

## Directive

`ui/_storybook-egui` is served with the **root** devshell, run from that directory:

```sh
cd ui/_storybook-egui
nix develop ../.. -c trunk serve      # http://127.0.0.1:8095 (see Trunk.toml)
```

See the `widget-screenshot` skill for the full screenshot loop.

## Rationale

`nix develop` with no path looks for a flake in the current directory, which has none. This is
the invocation that works, and it is easy to lose an afternoon to `trunk: command not found`
before working it out.
