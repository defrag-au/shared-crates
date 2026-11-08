#!/bin/sh
# Linux side of the headless work, run by the `linux-check` stack.
#
# Three questions, in order:
#   1. does the tree compile for Linux with `Conf::headless` threaded through
#      the X11/Wayland backends,
#   2. does a capture write a picture with no display server at all,
#   3. is that picture the one macOS wrote (compare the PNG and the report).
#
# The capture runs with `DISPLAY` empty and nothing listening on it. That is
# the whole point of the EGL backend: if this script needed Xvfb to pass, the
# offscreen path would not be the one being tested.
#
# `--locked` everywhere: a lock change here would be a silent edit to the host
# working tree, which this job has no business making.
#
# The build and the capture are one shot. The runtime records no exit code, so
# the log this script writes and the artefacts under `.tmp/linux-check/out/`
# are the result.
set -eu

REPO=$(cd "$(dirname "$0")/../.." && pwd)
OUT="$REPO/.tmp/linux-check/out"

apt-get update -qq
apt-get install -y -qq --no-install-recommends \
    libx11-6 libgl1 libglx-mesa0 libegl1 libgl1-mesa-dri

export CARGO_TERM_COLOR=never
export CARGO_HOME="$REPO/.tmp/linux-check/cargo-home"
export CARGO_TARGET_DIR="$OUT/target"
export LIBGL_ALWAYS_SOFTWARE=1

mkdir -p "$OUT"

echo "== toolchain =="
rustc --version
cargo --version

echo "== check: shared-crates macroquad-gamekit =="
cd "$REPO"
cargo check --locked -p macroquad-gamekit

echo "== build: tile5-viewer =="
cd /work/code/DamonOehlman/tile5-macroquad
cargo build --locked -p tile5-viewer

echo "== capture: tile5-viewer --shot, no display server =="
DISPLAY= "$CARGO_TARGET_DIR/debug/tile5-viewer" \
    --shot "$OUT/world-linux.png" --size 1000x700

echo "== artefact =="
ls -l "$OUT/world-linux.png" "$OUT/world-linux.txt"
echo "== done =="
