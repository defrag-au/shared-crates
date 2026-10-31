# Wallet link — notes

What was tried, what each capture showed, and the traps met. The review loop is the capture
plan, run from shared-crates:

```sh
cargo build -p macroquad-wallet-link
./target/debug/wallet-link --demo --plan <plan.txt> --size 390x844 --insets 47,0,34,0
./target/debug/wallet-link --demo --demo-wallets 12 --shot .tmp/link/crowd.png --size 390x844 --at 5
```

## The design, in two passes

1. **One planet, wallets as moons.** It worked, but the planet stood for nothing, and a crowd of
   moons on one system said nothing about chains.
2. **The identity is the sun, chains are planets, wallets are moons** (this is what the code draws
   now). The tethers are the link itself: moon to planet to sun. A selected wallet lights its
   whole path home. A linkable chain with no wallet yet is a ghost planet, and a tap on it links
   one.

## What the captures caught

- **Logic in `show` froze under capture.** The kit's `--at` and `--plan` run `Game::update`
  without drawing until the last 30 frames. The flow, the I/O, the connector's polling and the
  motion all live in `WalletLinker::update`. `WalletConnector::draw` was added so the modal can be
  drawn without polling the chains a second time.
- **A plan's `set` line ran before the list loaded.** The app queues them until the list has
  loaded. `wait N` exists because an unlink planned during the entrance faded a moon that had not
  yet appeared.
- **The closing modal drew a blank "?" card.** The wallet was released while the card was still
  sliding out. Release now waits until the modal is no longer visible.
- **The "Linked" toast fired under the open modal.** It now comes with the moon's launch.
- **Labels collided** (planet against moon, and ran off the screen edge at a launch's overshoot).
  Labels are placed in priority order and skip any that would overlap. They are clamped to the
  field.
- **The capture report flags every `…` as "cut short".** The moon labels' middle ellipsis is
  deliberate; the full address is in the sheet.

## Still open

- **A sheet on a phone with a home indicator** draws its footer over the bottom inset. Every kit
  sheet does this, so the fix belongs in `sheet_card`, for all of them.
- **The token is read from `?token=` and stays in the URL.** The HTML link page scrubs it once a
  wallet is detected. Nothing in the miniquad plugins scrubs a query string yet.
