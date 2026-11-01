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

## Signing in (2026-11-01)

The page needed a way in that is not `/link-wallet`. No existing route could mint a token
auth-service accepts for this origin, so `projects/wallet-link` in cnft.dev-workers gained a
worker: `/auth/discord/login` and `/auth/discord/callback`, discord-auth's `begin_login` /
`finish_login` with the Hodlcroft Discord app and the `identify` scope only, minting a session
under `AUTHZ_SESSION_SECRET` (which auth-service verifies) and returning to `/orbit/#session=…`.
The app reads the session through the kit's `auth` (discord-auth's miniquad edge) when there is
no `?token=`, offers "Sign in with Discord" when it has neither, "Sign in again" when
auth-service refuses the token, and "Sign out" on the sun's sheet for a session it holds.

The callback URL has to be registered on the Discord app (OAuth2 → Redirects) or Discord
answers "Invalid OAuth2 redirect_uri". A forged callback (no matching `state` cookie) is a 400.

Later the same day the app became the page itself: `mq-bundle --base /` into `public/`, the HTML
page moved to `public/classic/`, and the worker 301s `/orbit/*` to `/` with the query kept, so
`/link-wallet`'s `?token=` link opens the orbit view with no change to the bot. The HTML page is
kept for a wallet's in-app browser, and for the one thing the orbit app does not do yet: on a
phone with no injected wallet it only says "No Cardano wallet found", where the classic page
offers a copy-this-link flow and a MetaMask deep link.

## Still open

- **A sheet on a phone with a home indicator** draws its footer over the bottom inset. Every kit
  sheet does this, so the fix belongs in `sheet_card`, for all of them.
- **The token is read from `?token=` and stays in the URL.** The HTML link page scrubs it once a
  wallet is detected. Nothing in the miniquad plugins scrubs a query string yet.
