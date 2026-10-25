---
id: shared-crates-storybook-registration
title: A story has three registration sites plus a module declaration
layer: project
activation: project:shared-crates
priority: 50
overrides:
targets:
---

## Directive

`_storybook-egui/src/lib.rs` has **three** places a story must be registered:

1. The `stories! { … }` entry — one line, generating the enum variant, the sidebar ordering,
   the group heading and the render dispatch
2. `label()`
3. The blurb `match`

plus `pub mod` in `stories/mod.rs`.

Miss one → it either fails to compile or silently never appears in the sidebar. The silent case
is the expensive one.

`trunk build` is worth running on its own: the storybook is `crate-type = ["cdylib"]`, so
`cargo build -p storybook-egui` compiles without proving the wasm target works.

## Rationale

**Why it is not one site.** This said "six" until the `stories!` macro landed;
`src/registry.rs` documents what was broken before. `label()` and the blurb stay hand-written
**on purpose** — they are exhaustive matches, so the compiler catches an omission, and moving
~130 prose strings into a macro would risk pairing one with the wrong story for no safety gain.
