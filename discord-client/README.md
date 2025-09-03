# Discord Client Shared Crate

Cross-platform Discord webhook client with support for both native (reqwest) and WASM (gloo-net) environments.

## Purpose

This crate provides unified Discord webhook functionality for:
- **augminted-bots**: Native Rust environment using `reqwest`
- **cnft.dev-workers**: Cloudflare Workers WASM environment using `gloo-net`

## Key Features

- **Atomic multipart uploads**: Send Discord messages with file attachments in a single request
- **Cross-platform**: Works in both native and WASM environments
- **Rate limiting**: Proper Discord rate limit handling with retry-after support
- **Type safety**: Shared types for Discord messages, embeds, and attachments

## Architecture

```
discord-client/
├── src/
│   ├── lib.rs          # Main exports and error types
│   ├── types.rs        # Shared Discord API types
│   ├── native.rs       # reqwest implementation for native
│   └── wasm.rs         # gloo-net implementation for WASM
└── Cargo.toml          # Feature flags: "native" (default) | "wasm"
```

## Current Implementation Status

✅ **Completed**:
- Basic crate structure with feature flags
- Shared Discord types (DiscordMessage, DiscordAttachment, DiscordEmbed)
- Trait definition for common webhook operations
- Native implementation skeleton using reqwest multipart
- WASM implementation skeleton using gloo-net FormData

## TODO: Complete Implementation

### 1. Fix Native Implementation (`src/native.rs`)
The native implementation is based on the existing augminted-bots pattern but needs:
- [ ] Import existing multipart logic from `augminted-bots/discord-api/src/outbound.rs` lines 84-121
- [ ] Adapt the `send_follow_up` function to work as a standalone webhook client
- [ ] Test with actual Discord webhook URLs

### 2. Fix WASM Implementation (`src/wasm.rs`)
The WASM implementation needs:
- [ ] Complete the FormData multipart construction
- [ ] Test Blob creation and FormData append operations
- [ ] Verify gloo-net request sending with multipart data
- [ ] Handle WASM-specific error cases

### 3. Integration Testing
- [ ] Create test cases for both native and WASM implementations
- [ ] Test with actual R2 image data and Discord webhooks
- [ ] Verify rate limiting behavior
- [ ] Confirm attachment:// URL references work correctly

### 4. Documentation
- [ ] Add usage examples for both environments
- [ ] Document rate limiting best practices
- [ ] Add integration guides for both repos

## Integration Plan

Once this crate is complete and published:

### For augminted-bots
```toml
discord-client = { git = "https://github.com/defrag-au/shared-crates", features = ["native"] }
```

### For cnft.dev-workers  
```toml
discord-client = { git = "https://github.com/defrag-au/shared-crates", features = ["wasm"] }
```

## Key Insights from Research

1. **Discord multipart format**: Use `payload_json` field for JSON data + `files[{index}]` for attachments
2. **Atomic operations**: Single multipart request avoids rate limiting issues from two-phase uploads
3. **Platform constraints**: reqwest not WASM-compatible, gloo-net not native-compatible
4. **Existing patterns**: augminted-bots already has working multipart logic to adapt

## File Size Limits

- Discord: 8MB for regular users, 100MB for Nitro (using 8MB for safety)
- Cloudflare Workers: 100MB request limit (should be fine for images)

## Rate Limiting Strategy

- Handle 429 responses with proper retry-after parsing
- Support both global and per-route rate limits
- Allow caller to decide retry strategy (immediate fail vs queue for later)