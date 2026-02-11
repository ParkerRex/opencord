# Rust Client Architecture

## Design Constraints

- Platform-neutral core first (no Apple-only assumptions in base crates).
- Trait-based boundaries for auth and storage.
- Strict typed payloads shared from `discord_api_types`.

## Core Data Flow

1. `discord_auth::TokenProvider` provides token.
2. `discord_client` calls `discord_http` with token.
3. Responses deserialize into `discord_api_types`.
4. `discord_storage` optionally caches typed payloads.
5. `discord_gateway` opens and parses realtime event stream.

## Error Strategy

- Each crate has local typed errors.
- `discord_client::ClientError` composes upstream crate errors.
