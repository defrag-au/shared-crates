# linux-check

Builds `macroquad-gamekit` and `tile5-viewer` on Linux, and captures the viewer
with no window and no display server.

It answers the Linux half of the headless work:

- the tree compiles with `Conf::headless` threaded through the X11 and Wayland
  backends;
- a capture writes a PNG and its frame report with `DISPLAY` empty and no X
  server running, through an EGL pbuffer on `EGL_MESA_platform_surfaceless`;
- that picture matches the macOS headless capture within glyph antialiasing.

Xvfb is deliberately not installed. If a capture needs a display server, this
stack fails instead of quietly going back to a window; the script runs the
viewer with `DISPLAY=` and nothing listening on it.

## Run

```sh
container-compose -f .stacks/linux-check/stack.toml up -d
container-compose -f .stacks/linux-check/stack.toml logs
```

The first run pulls `rust:1.99-bookworm` and installs the GL/EGL runtime
packages; later runs are quick because `.tmp/linux-check/out/target` and
`.tmp/linux-check/cargo-home` survive.

## Read the result

The log ends with `== done ==`. The artefacts are
`.tmp/linux-check/out/world-linux.png` and `world-linux.txt`.

Compare against a macOS capture of the same scene:

```sh
image-shot diff ../tile5-macroquad/.tmp/world-named.png \
    .tmp/linux-check/out/world-linux.png --tolerance 2
```

At tolerance 2 the two are within 10 pixels, all in the bottom-right label; the
difference is glyph antialiasing. At tolerance 0 the tile fills differ by ±1,
from Metal and llvmpipe rounding differently. The text report should be
identical except the file name line.

## Why the paths look like that

`volumes = ["../../../..:/work/code"]` mounts the root that holds the sibling
checkouts, because the fork is pinned in every consumer by a relative
`[patch]` path. `workdir` and the script path name this repository inside that
mount. Copying the directory to another repository means changing those two
paths, the `name`, and what the script builds.
