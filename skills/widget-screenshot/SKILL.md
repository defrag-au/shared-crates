---
name: widget-screenshot
description: Use when adding or changing an egui or macroquad widget and you need to see it rendered, at desktop or mobile width, before reporting it done. Serves the storybook and captures one story with browser-shot, including the narrow-width case.
---

# Looking at a widget

Unit tests do not catch layout. Render the widget and look at it before reporting it done —
see `rules/org/defrag/look-at-what-you-built`.

## 0. Let browser-shot own the browser

**Every capture is `browser-shot`, run from the repo root.** It launches its own headless browser
with a profile at `.tmp/browser-shot-<pid>`, reports the PID and browser version, and removes the
profile on exit. Do not start Brave yourself, do not open a remote-debugging port, and do not
`pkill` a browser — there is nothing to clean up by hand.

```sh
browser-shot "<url>" 1200 820 .tmp/shot.png --settle-ms 6000
```

It is on `PATH` in the repo's devshell, packaged from `agent-playbook`. Every run writes
`<out>.png` and `<out>.txt`. A hard kill can leave the
`.tmp/browser-shot-<pid>` profile behind; a normal exit or an error removes it.

This replaces the old debugging-port procedure, and the incident is why: on 2026-10-16 four to
nine headless instances were left alive for an hour of iteration, and the session ended with the
reader unable to restart their own browser. The flag was there, the cleanup was not. The tool
exists so that cleanup is not a procedure an agent has to remember. If browser-shot fails, read its
output; do not fall back to a hand-launched browser.

## 1. Serve the storybook

Every command runs through the repo's devshell: `direnv exec . <cmd>` reads the already-realised
devshell out of `.direnv/` and needs no nix daemon socket, so it works in a sandboxed agent shell
where `nix develop` cannot reach the socket. The trunk build additionally needs the hardening
override emptied, because nix's cc-wrapper otherwise injects `-fzero-call-used-regs=used-gpr`, which
is not a wasm32 option:

```sh
cd ui/_storybook-egui
direnv exec ../.. env NIX_HARDENING_ENABLE= trunk serve   # http://127.0.0.1:8095, per Trunk.toml
```

`direnv exec ../..` finds the repo's `.envrc`, so it works from the subdirectory. Trunk rebuilds on
change, and it has been seen to die mid-session with `error writing finalized HTML output`; when
that happens `rm -rf dist` and restart it, then check the port answers with `curl` before shooting
anything. Take the port from what trunk prints, not from this file.

`trunk serve` is a long-running process. Start it with a port you chose if 8095 is taken, and when
you stop it, stop that process — not every `trunk` on the machine.

## 2. Screenshot one story

Stories are addressable as `#/<slug>`, where the slug is derived from the story's `label()`, so a
new story is linkable with no extra registration. `?nav=0` drops the sidebar so the story gets the
whole viewport instead of `viewport − 180`.

```sh
browser-shot "http://127.0.0.1:8095/?nav=0#/wallet-button" 1200 820 .tmp/wallet-button.png \
  --settle-ms 6000
```

- **`--settle-ms` is a floor, not a sleep.** browser-shot keeps capturing until two consecutive
  frames are identical after that window, so 5000–6000 covers the wasm boot and the remote font
  fetch, and keeps waiting if the app is still drawing. The default 1200 can accept a blank canvas
  before trunk's wasm has booted.
- **Write screenshots to `.tmp/` in the repo**, not `/tmp` — they stay easy to open and to point
  someone at. `.tmp/` is gitignored.
- Each invocation launches a fresh browser, so a hash-only change is fine between runs; the old
  persistent debugging-port session needed a varying query parameter.
- **The sidecar is the second check.** Read `.tmp/wallet-button.txt` before judging the image:
  `renderer … SOFTWARE` is expected for egui's WebGL canvas on a headless machine, not a failure;
  `warnings`, `layout`, `image` and `ready` are the fields that change what you may claim.
- **Two failed requests are the storybook's remote fonts, not the widget.** The story fetches
  `Inter-Regular.ttf` / `Inter-Bold.ttf` from `https://assets.augmint.bot`, which sends no CORS
  header, so a localhost page logs `TypeError: Failed to fetch` and the story falls back to its
  bundled font. Verified on 2026-10-24: exactly those two failures and six console errors. More
  than that means something else broke — read the `requests` lines.
- Exit `0` passed · `2` artefact written but an assertion failed · `1` operational.

For a modal or a hover state, the storybook has no DOM to click: **click canvas coordinates**.
The single shot clicks before the capture and does not wait afterwards, so use a scenario when the
state needs a beat to appear:

```text
run timeout=30s

peer story url="http://127.0.0.1:8095/?nav=0#/wallet-button" viewport=1200x820

wait story network_idle=500 timeout=10s
pointer story click at=640,410
eval story "new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))" await
capture story out=modal.png
```

A scenario captures after the wait, so the shot is of the open modal rather than the button that
opens it. The field tables are
`~/code/defrag/agent-playbook/tools/browser-shot/SCENARIO.md`.

## 3. Narrow widths — a window size cannot do this

A headless window is **floored near 620px**, laid out at the floor, then cropped to the width you
asked for. A "390px" shot from a window-size flag is therefore a wide layout with its right edge
sliced off — which reads as an overflow bug that is not there, and hides the real one.

browser-shot never sets a window size. It applies `Emulation.setDeviceMetricsOverride` at the
width, height and DPR you ask for, and the sidecar's `layout` line proves what the page actually
laid out at. Use `?nav=0` and the real viewport:

```sh
browser-shot "http://127.0.0.1:8095/?nav=0#/activity-feed" 390 844 .tmp/activity-feed-390.png \
  --dpr 3 --touch --settle-ms 6000
```

- `viewport 390x844 dpr 3 touch` and `image 1170x2532` in the sidecar is the correct result.
- `layout 980x…` is the trap: the page has no viewport meta and laid out as a desktop. The sidecar
  warns about it.
- `--window-size` does not exist here, deliberately.

## 4. Check it against real data

The same tool points at a **deployed app** — raise `--timeout-ms`, keep `--settle-ms` long enough
for the first socket payload, and read `network` in the sidecar. This is the only way to check a
widget against real data at real width, and real data is wider than fixture data: that is how a
wrapped 60-character stake address shipped.

## Native builds

No address bar. Use `STORYBOOK_STORY=flow-ledger` instead of the hash.
