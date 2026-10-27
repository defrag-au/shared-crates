# scenedrv

Clip drivers for [`scenegraph`](../scenegraph). Closed ops (`fan`,
`orbit`, `to`) compile to [`sceneanim`](../sceneanim) clips. The evaluator
is still `pose(scene, clip, t)`. Flavor that only exists while inspecting
is a scene write: `inspect` clones rest and inserts Labels, then pose.

```sh
cargo test -p scenedrv
```

```rust
let drv = Driver::from_dump("drv1\nshowcase\nfan tag=layer\norbit cam=cam\n")?;
let clip = drv.clip(&scene)?;
let posed = clip.pose(&scene, 1.0)?;

let mut mx = Mixer::new();
mx.go(Driver::new("th-shredder").op(To::lift("th-shredder")).clip(&scene)?);
mx.then(
    Driver::new("flavor-shredder").op(To::fade("flavor-shredder")).clip(&scene)?,
    "th-shredder",
);
let posed = present(&scene, &mx, |_| "Tear it apart.", None)?;
```

Dump grammar: `AGENTS.md`.
