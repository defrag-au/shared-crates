# Wallet link — screen spec

The Discord wallet-link manager on macroquad: the wallets linked to the reader's Discord account,
shown as moons orbiting their home planet, with linking through the shared wallet modal and
proof through auth-service. One component, `WalletLinker`, drawn into any rect; one app,
`wallet-link`, that is that component full screen.

Phone first: portrait **390 × 844**, on `Stage::Fluid` (one unit per point, laid out each frame
from the window). Every size below is in points.

## Layout

| element | position | size | notes |
|---|---|---|---|
| sky | the whole rect | — | theme `bg_primary`, a vertical wash to a faint accent at the foot, `Starfield` seed 7, 140 stars, drifting −4,1 pt/s |
| header | `safe.top + 14`, gutter 20 | 56 tall | title "Linked wallets" (`Xl2`), subtitle under it (`Md`, muted): "3 wallets linked to boef" |
| view toggle | header row, right edge | 132 × 32 | `segmented` ["Orbit", "List"] |
| orbit field | header bottom + 8 to CTA top − 12 | rest | the planet's centre at 48 % of its height |
| planet | field centre | R = clamp(min(w, h) × 0.14, 44, 84) | baked once: lit sphere, banded, cratered, an atmosphere of the theme accent out to 1.35 R |
| rings | concentric ellipses round the planet | rx from 1.95 R to min(w/2 − 44, …); ry = rx × 0.36 | the back half drawn before the planet, the front half after it |
| moons | on the rings | r 15 (selected 19), tap target 44 | ring k holds 3 + 2k; a moon is the chain's colour (`chain_tint`) on a baked grey sphere, with `chain_mark` on its face |
| tethers | planet edge → each moon | 1 pt | alpha 0.10, selected 0.45: the link the moon stands for |
| moon label | under its moon, centred | `Sm` | handle (`$boef`) or the short address; front-half moons only, and the selected one |
| CTA | `safe.bottom − 16 − 52`, gutter 20, at most 360 wide, centred | 52 | `Pill::filled("Link a wallet")` in the accent |
| list (List view) | the orbit field | rows 64, gap 8 | in a `surface`; a mini moon, label, "Cardano · stake1u89…rhq9", a chevron; scrolls through `Scroll` |
| detail sheet | `sheet_card` | content + chrome | title = label, subtitle = chain name, tint = chain colour; a `well` of Chain, Handle (copy), Address (copy, mono); footer `Pill::danger("Unlink")` |
| confirm | same sheet | + 2 lines | "This removes the link between $boef and your Discord account. You can link it again any time." footer: Keep (ghost) · Unlink (danger) |

Depth: a moon at angle θ on a ring sits at `(cx + rx cos θ, cy + ry sin θ)`. sin θ < 0 is the far
side: scale 0.82, alpha 0.7, drawn before the planet. Near side: scale up to 1.0, drawn after.

## Motion

| what | how long | ease |
|---|---|---|
| planet rising in on load | 0.7 s, scale 0.6 → 1, fade | `ExpoOut` |
| a ring drawing in | 0.9 s, its sweep from 0 to a full ellipse, 0.12 s after the ring inside it | `CubicOut` |
| moons emerging on load | 0.8 s each, from the planet's centre to the ring, 0.09 s apart | `BackOut` |
| orbit | ring k turns once in 70 + 25k s | linear |
| selecting a moon | the orbit's speed eases to 0 | 0.6 s `ExpoOut` (and back to 1 on close, 0.9 s `CubicInOut`) |
| re-spacing after a change | each moon's slot angle, by the shortest way round | 0.8 s `CubicInOut` |
| a new link | the moon launches from the planet to its slot, 0.9 s `BackOut`; a burst of 26 bits of its colour on arrival |
| an unlink | the moon flies out to 1.8 × its ring and fades | 0.6 s `CubicIn` |
| Orbit ⇄ List | cross-fade | 0.35 s `CubicOut` |
| linked, in the modal | the modal's own ring closes and its check pops; held 1.4 s, then the modal closes and the moon launches | — |

## States

| flow phase | screen |
|---|---|
| Loading | planet rising, rings breathing, "Loading your wallets…" under the planet |
| Unreachable | planet dimmed; `Message` "Couldn't load your wallets" + the reason + "Try again" |
| Expired | planet dimmed; `Message` "Session expired" + the hint (default: run /link-wallet again) |
| Idle, none linked | the planet and one dashed ring; "No wallets linked yet" under it; the CTA |
| Idle | the system |
| Link · Connect | the wallet modal (`ProofWords::LINK`), on the chain step |
| Link · Prepare / Sign / Check | the modal's answer step: "Preparing the message…", "Approve the message in Eternl" (in wallet), "Checking the signature" |
| Link · Linked / Already | the modal's account view, "Linked with Eternl · Linked to Discord" (or "· Already linked") in success, then the launch |
| Link · Declined / Failed | the modal's answer step with Try again / Switch wallet |
| Confirming / Unlinking | the detail sheet's confirm; Unlink shows "Unlinking…" and takes no tap |

Readability: the label is the handle when there is one (handles are display only, never an
identity); the address is always one tap away in the sheet, in full, with Copy. No wallet is
marked primary and the order is the server's.

## The component

```rust
let mut linker = WalletLinker::live(auth_base, token);   // or ::new(io, connector, claims)
// each frame, inside a gamekit Game:
if let Some(event) = linker.show(frame, &painter, rect) { /* Linked / Unlinked */ }
```

The host stamps the plugins its page needs: `wallet.js`, `evm.js` (wallet-miniquad),
`platform.js` (miniquad-platform) and `app.js` (macroquad-app, for HTTP).
