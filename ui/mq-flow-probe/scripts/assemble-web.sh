#!/usr/bin/env bash
# Assemble the probe as a static web bundle.
#
# Mirrors augminted-bots' `widgets/activity-hello/scripts/assemble-web.sh` —
# the proven macroquad-on-web shape — so there is one way to do this.
#
# Files gathered:
#   from web/                              index.html
#   from cargo registry                    gl.js (miniquad), sapp_jsutils.js
#   from cargo git checkouts               quad-net.js  (git dep, not registry)
#   from target/wasm32-…/release/          mq-flow-probe.wasm

set -euo pipefail

TARGET_DIR="${1:-web}"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CRATE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOT="$(cd "$CRATE_DIR/../.." && pwd)"
SRC_WEB="$CRATE_DIR/web"

mkdir -p "$TARGET_DIR"
TARGET="$(cd "$TARGET_DIR" && pwd)"

echo "→ building wasm32-unknown-unknown (release)..."
cd "$ROOT"

# `--allow-undefined` restores what rustc did before 1.96, when it stopped
# passing it to wasm-ld. miniquad, sapp-jsutils and quad-net all declare bare
# `extern "C"` blocks whose bodies JS supplies at instantiate; without this
# they fail to LINK (note `cargo check` still passes, because check does not
# link).
# https://blog.rust-lang.org/2026/04/04/changes-to-webassembly-targets-and-handling-undefined-symbols/
RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=--allow-undefined" \
    cargo build --target wasm32-unknown-unknown --release -p mq-flow-probe

if [[ "$TARGET" != "$SRC_WEB" ]]; then
    cp "$SRC_WEB/index.html" "$TARGET/"
fi

cp "$ROOT/target/wasm32-unknown-unknown/release/mq-flow-probe.wasm" "$TARGET/"

echo "→ resolving JS deps ..."
GL_JS=$(find "$HOME/.cargo/registry/src" -path '*/miniquad-*/js/gl.js' 2>/dev/null | sort -V | tail -1)
SAPP_JS=$(find "$HOME/.cargo/registry/src" -path '*/sapp-jsutils-*/js/sapp_jsutils.js' 2>/dev/null | sort -V | tail -1)
# quad-net is a *git* dependency, so it lands in git/checkouts, not the registry.
QUAD_NET_JS=$(find "$HOME/.cargo/git/checkouts" -path '*/quad-net-*/js/quad-net.js' 2>/dev/null | sort -V | tail -1)

if [[ -z "${GL_JS:-}" || -z "${SAPP_JS:-}" || -z "${QUAD_NET_JS:-}" ]]; then
    echo "✗ couldn't find gl.js, sapp_jsutils.js or quad-net.js." >&2
    echo "  run \`cargo fetch\` and retry." >&2
    exit 1
fi

cp "$GL_JS"       "$TARGET/gl.js"
cp "$SAPP_JS"     "$TARGET/sapp_jsutils.js"
cp "$QUAD_NET_JS" "$TARGET/quad-net.js"

echo ""
echo "✓ assembled at $TARGET"
echo "  serve it and open in a browser:"
echo "    python3 -m http.server 8080 --directory $TARGET"
