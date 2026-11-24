#!/bin/bash
#
# Build an `offthread` service worker into a dist directory.
#
#   build-service.sh <package> <dist-dir> <basename>
#
# Run from the root of the workspace that holds <package>. Produces two files:
#
#   <basename>.js            the host runtime — STABLE name, because the page
#                            names this URL at compile time
#   <basename>-<hash>.wasm   the service — CONTENT-HASHED, so it can be served
#                            immutable and can never be served stale
#
# Same shape as `index.html` + the hashed app bundle: a small stable entry
# point that names a large immutable payload. The runtime is ~3KB and changes
# whenever the hash does, so revalidating it costs nothing.
#
# Deliberately NOT trunk. trunk drives wasm-bindgen, and an offthread service
# has no wasm-bindgen in it — that is the property that lets any frontend load
# it, including macroquad/miniquad ones that can never load wasm-bindgen glue.
# What is left is a plain cargo build plus wasm-opt, which is this file.
#
# Lives beside the crate so a consumer finds it wherever the crate is (a git
# checkout, or a local path under a `[patch]`): ask cargo for `offthread`'s
# manifest path and run the script beside it. The runtime it stamps is the one
# beside it too, so the two cannot come from different revisions.

set -euo pipefail

PACKAGE="${1:?usage: build-service.sh <package> <dist-dir> <basename>}"
DIST="${2:?usage: build-service.sh <package> <dist-dir> <basename>}"
BASENAME="${3:?usage: build-service.sh <package> <dist-dir> <basename>}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUNTIME_JS="$SCRIPT_DIR/js/offthread.js"

# Nix's cc-wrapper injects hardening flags clang rejects when cross-compiling C
# to wasm — `-fzero-call-used-regs=used-gpr` is not a wasm32 option. A service
# that reaches C (cnft.dev-workers' cart builder does, through `blst`) fails
# naming the C crate rather than the toolchain without this.
export NIX_HARDENING_ENABLE=""

echo "📦 Building offthread service '$PACKAGE'..."
cargo build --package "$PACKAGE" --target wasm32-unknown-unknown --release

# The consuming workspace's target directory, wherever it is configured.
TARGET_DIR="$(cargo metadata --no-deps --format-version 1 | jq -r .target_directory)"
# cargo replaces `-` with `_` in the artifact name for a cdylib.
CRATE_FILE="${PACKAGE//-/_}"
RAW_WASM="$TARGET_DIR/wasm32-unknown-unknown/release/${CRATE_FILE}.wasm"
[ -f "$RAW_WASM" ] || { echo "Error: no wasm at $RAW_WASM" >&2; exit 1; }

# ⚠️ THE FEATURE FLAGS ARE LOAD-BEARING. Rust emits `i64.trunc_sat_f64_u` and
# bulk-memory ops, and wasm-opt REJECTS the module without being told those
# are allowed:
#
#   [wasm-validator error] unexpected false: all used features should be allowed
#
# This only bites in release, which is how it reached a deploy once already.
OPTIMISED="$(mktemp -t offthread-XXXXXX).wasm"
trap 'rm -f "$OPTIMISED"' EXIT
echo "🗜  Optimising..."
wasm-opt -Oz \
    --enable-bulk-memory \
    --enable-nontrapping-float-to-int \
    -o "$OPTIMISED" \
    "$RAW_WASM"

# `shasum` on macOS, `sha256sum` on Linux/CI. Sixteen hex characters is plenty
# to separate builds and keeps the filename readable.
if command -v sha256sum >/dev/null 2>&1; then
    HASH="$(sha256sum "$OPTIMISED" | cut -c1-16)"
else
    HASH="$(shasum -a 256 "$OPTIMISED" | cut -c1-16)"
fi

mkdir -p "$DIST"
WASM_NAME="${BASENAME}-${HASH}.wasm"
cp "$OPTIMISED" "$DIST/$WASM_NAME"

# The runtime names the hashed module. Root-absolute so it resolves the same
# from any route the host serves.
sed "s|__OFFTHREAD_WASM__|/${WASM_NAME}|" "$RUNTIME_JS" > "$DIST/${BASENAME}.js"

echo "✅ offthread service at $DIST/${BASENAME}.js → $WASM_NAME ($(du -h "$DIST/$WASM_NAME" | cut -f1))"
