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
6. `discord_gateway::GatewayStateMachine` applies pure protocol transitions and emits actions.
7. `discord_gateway::GatewayClient::run_with_shutdown` (runtime driver) executes socket I/O from those actions.

## REST Route Layer

- `discord_api_types::routes` defines typed route contracts and payloads.
- `discord_http` consumes those route models via `get_route`, `get_query_route`, and `post_route`.
- Current concrete route coverage:
  - `GET /users/@me`
  - `GET /gateway/bot`
  - `GET /users/@me/guilds`
  - `GET /guilds/{guild.id}/channels`
  - `GET /channels/{channel.id}/messages`
  - `POST /channels/{channel.id}/messages`
- Route query/body DTOs live in `discord_api_types` and are reused by callers.

## Error Strategy

- Each crate has local typed errors.
- `discord_client::ClientError` composes upstream crate errors.

## Gateway Runtime Layer

- State transitions are pure and isolated in `GatewayStateMachine`.
- Runtime-side effects (websocket connect/read/write, heartbeat timers, reconnect sleep, shutdown handling) are isolated in `src/runtime.rs`.
- Runtime reconnect policy uses capped exponential backoff with jitter.
- Session continuity rules:
  - `READY` captures `session_id` + sequence.
  - `HELLO` sends `RESUME` when resumable session exists; otherwise sends `IDENTIFY`.
  - non-resumable `INVALID_SESSION` clears session and forces `IDENTIFY` on next connect.
- Integration tests use an in-process mock websocket server to verify reconnect/resume behavior end to end.
