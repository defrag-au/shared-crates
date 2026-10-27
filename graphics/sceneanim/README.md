# sceneanim

Clip evaluator for [`scenegraph`](../scenegraph). The document is still `sg1`;
this crate only knows how to write a posed copy.

```sh
cargo test -p sceneanim
```

```rust
let posed = clip.pose(&scene, t)?;
let both = pose_layers(&scene, &[(&hover, t), (&lift, u)])?;
```

Dump grammar: `AGENTS.md`. Viewer playhead: `cargo run -p scene-view`.
