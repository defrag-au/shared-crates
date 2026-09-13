#!/usr/bin/env bash
#
# theme-gauntlet.sh — screenshot a set of stories under every theme preset, so a
# theming change can be REVIEWED rather than merely compiled.
#
# The reason this exists: a theme axis can be fully wired, fully tested and
# entirely invisible. `cargo test` proves a token has a value; only a picture
# proves the token reached the screen. The first pass of the theming work
# reported "done" on an axis that repainted about 4% of the pixels, because
# nothing in the loop ever looked at it.
#
# It also catches the opposite failure — a theme that changes layout enough to
# BREAK it. A roomier spacing ramp widened every chip in the order-list filter
# strip until the row overflowed and stacked a label one character per line.
# Nothing errored; the tests passed; it was only visible in a screenshot.
#
# Usage:
#   tools/theme-gauntlet.sh                    # default story set, all presets
#   tools/theme-gauntlet.sh chip toast id-pill # explicit slugs
#
# Requires `trunk serve` already running (see the header of cdp-shot.mjs).
# Output: shared-crates/.tmp/theme-gauntlet/<slug>__<theme>.png
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT="$HERE/../../../.tmp/theme-gauntlet"
PORT="${PORT:-8095}"
W="${W:-1400}"
H="${H:-700}"
SETTLE="${SETTLE:-13000}"

# Must match `Theme::PRESETS`. Kept as a list rather than read from the binary
# because this is a review tool, not a test — if it drifts, the missing column
# is obvious in the output directory.
THEMES=("tokyo night" "tokyo night mono" "opensea" "industrial")

# Stories worth looking at on every theming change: the ones that carry status
# colour, dense text ramps, chrome, and charts. Not the whole catalogue — the
# point is a set small enough that someone actually reviews all of it.
DEFAULT_STORIES=(
  chip
  order-list
  toast
  metric-card
  stat-strip
  data-table
  tx-cart
  wallet-list
  trade-table
  buttons
  select
  progress-bar
)

STORIES=("$@")
if [ ${#STORIES[@]} -eq 0 ]; then STORIES=("${DEFAULT_STORIES[@]}"); fi

if ! curl -sf -o /dev/null "http://127.0.0.1:$PORT/"; then
  echo "no storybook on :$PORT — run 'nix develop ~/code/defrag/shared-crates -c trunk serve' first" >&2
  exit 1
fi

mkdir -p "$OUT"
# One cache-buster for the whole run, in the QUERY string: a fragment-only
# change does not reload, so the app would keep the story it booted with and
# every shot after the first would silently be the wrong widget.
STAMP=$(date +%s)

for slug in "${STORIES[@]}"; do
  for theme in "${THEMES[@]}"; do
    key=$(echo "$theme" | tr ' ' '-')
    enc=$(echo "$theme" | sed 's/ /%20/g')
    url="http://127.0.0.1:$PORT/?nav=0&theme=$enc&t=$STAMP#/$slug"
    if node "$HERE/cdp-shot.mjs" "$url" "$W" "$H" "$OUT/${slug}__${key}.png" "$SETTLE" >/dev/null 2>&1; then
      printf '  %-16s %s\n' "$slug" "$key"
    else
      printf '  %-16s %s  FAILED\n' "$slug" "$key" >&2
    fi
  done
done

echo
echo "$OUT"
