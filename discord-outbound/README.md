# discord-outbound

Posting and editing Discord messages. One message model, two verbs, one HTTP
request method per platform.

Was `discord-client`. Renamed because it only ever sends and edits — guild
member, role and audit-log reads are deliberately out of scope — and the old
name promised a general Discord client it will never be.

## The shape

```rust
use discord_message::{Attachment, MessageBody, PluginBlock};
use discord_outbound::{DiscordOutbound, SendOptions, Target, WasmDiscordClient};

let client = WasmDiscordClient::new(bot_token);

// A plain post.
client.send(&Target::channel(channel_id), &MessageBody::text("hi"), SendOptions::default()).await?;

// A Components V2 card with a rendered graphic, as a reply.
let body = MessageBody::layout(vec![PluginBlock::Container { .. }])
    .with_attachment(Attachment::new("render.png", png_bytes));
client.send(&Target::reply(channel_id, message_id), &body, SendOptions::default()).await?;

// An edit. Same body vocabulary, a different target.
client.edit(&target, &body, SendOptions::default()).await?;
```

Everything that used to be a method name is a property of the body or the
target, decided once in `discord_message::wire::body`:

| Was a method on `discord-api` | Is now |
|---|---|
| `…_with_image` | `body.attachments` non-empty → multipart instead of JSON |
| `…_with_embeds` | `body.embeds` non-empty |
| `…_with_components` | `body.rows` non-empty |
| `…_v2` | `body.layout` non-empty → sets `IS_COMPONENTS_V2`, drops content/embeds |
| `…_reply` | `Target::reply` |
| `send_follow_up` | `Target::followup` |

That collapse is the point. `discord-api` reached fifteen send/edit entry points
because they were not features but cells in a grid — and Components V2 doubled
the grid. See
`augminted-bots/docs/DISCORD_OUTBOUND_CONSOLIDATION_DESIGN.md`.

## Features name HTTP stacks, not targets

```toml
discord-outbound = { git = "…", default-features = false, features = ["native"] }  # reqwest
discord-outbound = { git = "…", default-features = false, features = ["wasm"] }    # gloo-net
```

**`native` means reqwest, not "not wasm".** reqwest compiles for
`wasm32-unknown-unknown` and is what augminted-bots' Workers already use; gloo
is what cnft.dev-workers' Workers use. Pick whichever HTTP client the rest of
your crate links, not whichever sounds like your platform.

## Rate limiting, both halves

- **Proactive** (`ratelimit::RateLimitTracker`): a route known to be closed is
  not sent into. `SendOptions::default()` waits out the window;
  `SendOptions::best_effort()` returns immediately, which is what you want for
  a progress edit that will be superseded.
- **Caller-visible** (`DiscordError::RateLimited`): a 429 that happens anyway
  still reaches the caller. `notification-dispatcher` maps it onto a queue
  retry delay — swallowing it would turn visible backpressure into silent
  latency.

A Cloudflare 1015 is `DiscordError::CloudflareBlocked`, a separate variant: it
is Discord's *edge* refusing an egress IP, not a per-route bucket, and it needs
a different answer than backoff.

## Layout

```
discord-outbound/
├── src/
│   ├── lib.rs         # Errors, feature wiring
│   ├── send.rs        # The DiscordOutbound trait: send, edit, and everything
│   │                  # Discord-specific. A platform supplies `execute` and a
│   │                  # clock; nothing else.
│   ├── target.rs      # Target (where a new message goes), SentMessage
│   ├── ratelimit.rs   # Route-keyed proactive tracker; clock is an argument
│   ├── multipart.rs   # One body builder for every HTTP stack. Pure.
│   ├── native.rs      # reqwest transport (+ the legacy DiscordMessage client)
│   ├── wasm.rs        # gloo-net transport (+ the legacy DiscordMessage client)
│   └── types.rs       # LEGACY: DiscordMessage/DiscordMessageEdit, twilight 0.16
└── Cargo.toml
```

## The legacy surface is on its way out

`DiscordMessage`, `DiscordMessageEdit`, `DiscordClient` and `compat::twilight`
predate this and are the **only** reason the crate still depends on
twilight-model 0.16. They go when cnft.dev-workers migrates to `MessageBody`
(step 5 of the design). Until then, do not add callers: a twilight major in
this crate is exactly what stops augminted-bots (on 0.17) adopting it.
