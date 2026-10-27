# sceneinput

Pointer session for [`scenegraph`](../scenegraph). Interactives are hits on
named nodes, not widget kinds. The document is still `sg1`; this crate
holds tool / pointer / selection and writes a new scene when orbit, drag,
or walk moves rest pose.

```sh
cargo test -p sceneinput
```

```rust
let (scene, session) = session.step(&scene, &list, Event::down(x, y, aspect))?;
```

Dump grammar: `AGENTS.md`. Viewer: `cargo run -p scene-view`.
