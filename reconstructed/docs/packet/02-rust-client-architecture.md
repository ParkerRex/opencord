# Rust Client Architecture

## Design Constraints

- Platform-neutral core first (no Apple-only assumptions in base crates).
- Trait-based boundaries for auth and storage.
- Strict typed payloads shared from `discord_api_types`.

## Core Data Flow

1. `discord_auth::TokenProvider` provides token.
2. `discord_client::SessionManager` coordinates token + bootstrap + session restore.
3. `discord_client` calls `discord_http` with token.
4. `discord_http` applies bounded 429 retry handling before surfacing errors.
5. Responses deserialize into `discord_api_types`.
6. `discord_storage` persists gateway resume metadata (`session_id`, `seq`, `resume_url`).
7. `discord_gateway` opens and parses realtime event stream.
8. `discord_gateway::GatewayStateMachine` applies pure protocol transitions and emits actions.
9. `discord_gateway::GatewayClient::run_with_shutdown` (runtime driver) executes socket I/O from those actions.

## Session Bootstrap and Resume

- Entry point: `DiscordClient::gateway_startup(intents)` (delegates to `SessionManager`).
- Bootstrap sequence:
  - load token from `TokenProvider`
  - call `GET /gateway/bot`
  - load persisted gateway session from `discord_storage`
  - prefer persisted `resume_url` when valid, otherwise fall back to `gateway_bot.url`
  - initialize `GatewayStateMachine` with restored `session_id` + `seq` when available
- Runtime persistence:
  - `SessionManager::persist_from_state_machine` writes resumable state back to `discord_storage`
  - absent resumable state clears persisted gateway session to avoid stale resumes

## HTTP Reliability Layer

- `discord_http` retry scope:
  - retries only on `429 Too Many Requests`
  - bounded attempts (`MAX_RATE_LIMIT_RETRIES`)
  - retry delay precedence:
    1. `Retry-After`
    2. `x-ratelimit-reset-after`
    3. JSON body `retry_after`
  - delay clamped to a safe min/max window
- Non-429 errors still return `HttpError::Status` with status + body for callers.

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

## Observability and Interface Freeze

- `tracing` instrumentation was added in:
  - `discord_http` request completion + retry paths
  - `discord_gateway` state machine event handling
  - `discord_gateway::runtime` connect/reconnect/heartbeat command lifecycle
- Public interfaces are frozen via crate-level compile-time lock tests (`tests/public_api_lock.rs`).
