# Wallet link — screen spec

The Discord wallet-link manager on macroquad, drawn as a solar system. The reader's Discord
identity is the sun, every chain is a planet orbiting it, and every wallet linked on a chain is a
moon orbiting that chain's planet. Linking goes through the shared wallet modal, and auth-service
checks the signed proof. There is one component, `WalletLinker`, drawn into any rect. There is
one app, `wallet-link`, which is that component full screen.

Phone first: portrait **390 × 844**, on `Stage::Fluid` (one unit per point, laid out each frame
from the window). Sizes are in points; `short` is the field's shorter side.

## Layout

| element | position | size | notes |
|---|---|---|---|
| sky | the whole rect | — | theme `bg_primary`, a wash to a faint accent at the foot, `Starfield` seed 7, 140 stars |
| header | `safe.top + 14`, gutter 20 | 56 tall | "Linked wallets" (`Xl2`); "3 wallets linked to boef" (`Md`, muted) |
| view toggle | header row, right | 132 × 32 | `segmented` ["Orbit", "List"] |
| field | header + 8 to CTA − 12 | rest | the sun at 48 % of its height |
| sun | field centre | r = clamp(short × 0.10, 28, 56) | the reader's Discord avatar in a circle, else their initial on an accent disc; an accent ring, a breathing corona out to 2 r; their name under it |
| planets' orbit | round the sun | rx fits a planet's moons inside the field; ry = rx × tilt | tilt = clamp(h/w × 0.48, 0.36, 0.74): rounder on a tall phone |
| planet | on the orbit, one per chain, evenly spaced | r = clamp(short × 0.08, 22, 44) | baked lit sphere in `chain_tint`; tethered to the sun; its chain's name under it |
| ghost planet | a linkable chain with no wallet | as a planet | dashed outline round `chain_mark`, "Link EVM"; a tap links on that chain |
| moons | round their planet, 5 to a ring | r = planet × 0.36, ring = planet × 1.6 + 6 | the chain's colour on a baked grey sphere, tethered to the planet |
| labels | under their body | `Sm` on a dark plate | placed sun, selected wallet, planets, then moons; a label landing on one placed is left out; moons are named only up to 4 wallets |
| CTA | `safe.bottom − 16 − 52`, at most 360 wide | 52 | `Pill::filled("Link a wallet")`; fades out while the modal is up |
| list (List view) | the field | rows 64, gap 8 | a mini moon, label, "Cardano · stake1u89…rhq9", a chevron; scrolls through `Scroll` |
| detail sheet | `sheet_card` | content + chrome | title = label, subtitle = chain; a `well` of Handle (copy) and Address (copy, mono); `Pill::danger("Unlink")` |
| confirm | same sheet | + 2 lines | "This removes the link between $boef and your Discord account…" · Keep · Unlink |

Depth: everything is sorted by how near it is (the sine of its angle on its orbit). Far bodies
draw first, smaller (0.8×) and dimmer. The planets' orbit is split at the sun: the far half is
drawn behind, the near half in front.

The path home: a selected wallet's moon gets a pulsing ring. Its tether to its planet and its
planet's tether to the sun brighten, and the system eases to a stop.

## Motion

| what | how long | ease |
|---|---|---|
| the sun rising in | 0.6 s, scale 0.6 → 1 | `ExpoOut` |
| the orbit drawing in | 0.8 s sweep | `CubicOut` |
| planets emerging from the sun | 0.9 s each, 0.15 s apart | `BackOut` |
| moons emerging from their planets | 0.8 s each, 0.08 s apart, after 0.8 s | `BackOut` |
| orbits | planets 150 s a turn; moons 48 s (outer rings slower) | linear |
| selecting | the system eases to a stop | 0.6 s `ExpoOut`; back to speed 0.9 s `CubicInOut` |
| re-spacing | slot angles, the short way round | 0.8 s `CubicInOut` |
| a planet's first wallet | ghost to world | 0.5 s `CubicOut` |
| a new link | the modal's check, held 1.4 s; then the moon launches from its planet, 1.1 s `BackOut`, and bursts in its chain's colour |
| already linked | the moon that already was this wallet bursts |
| an unlink | the moon flies out to 2× its ring and fades | 0.6 s `CubicIn` |
| Orbit ⇄ List | cross-fade | 0.35 s `CubicOut` |

## States

| flow phase | screen |
|---|---|
| Loading | the sun rising, the orbit breathing, "Loading your wallets" |
| Unreachable | the system dimmed; `Message` with the reason and "Try again" |
| Expired | the system dimmed; `Message` "Session expired" and the hint |
| Idle, none linked | the sun and a ghost planet per linkable chain |
| Link · Connect … Check | the wallet modal (`ProofWords::LINK`): chain, wallet, "Sign with Eternl", "Checking the signature" |
| Link · Linked / Already | the modal's account view, "Linked with Eternl · Linked to Discord" (or "· Already linked") |
| Link · Declined / Failed | the modal's answer step: Try again, Switch wallet |
| Confirming / Unlinking | the sheet's confirm; "Unlinking" takes no tap |

The label is the handle when there is one. The address is always one tap away, in full, with
Copy. No wallet is marked primary, and the order is the server's.

## The component

```rust
let mut linker = WalletLinker::live(DEFAULT_AUTH_BASE, token);
// Game::update — the capture loop runs updates without draws:
if let Some(event) = linker.update(frame) { /* Linked / Unlinked */ }
// Game::draw:
linker.show(frame, &painter, rect);
```

The page needs `wallet.js` and `evm.js` (wallet-miniquad), `platform.js` (miniquad-platform)
and `app.js` (macroquad-app).
