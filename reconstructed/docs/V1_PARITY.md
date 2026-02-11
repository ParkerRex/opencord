# V1 Parity Matrix (UIKit + Rust Backend)

Source of truth:
- `docs/packet/*`
- Discord official REST + Gateway + Voice docs

Status key:
- `DONE`: implemented and test-covered in workspace
- `IN_PROGRESS`: scaffolded with partial behavior
- `TODO`: not implemented yet

## Auth and Session

| Feature | Status | Notes |
| --- | --- | --- |
| OAuth2 PKCE start/callback endpoints | DONE | `discord_service` supports OAuth start/callback, optional client deep-link redirect (`client_redirect_uri`), persisted token metadata, and automatic refresh-token rotation on authenticated requests. |
| Logout/session revoke | DONE | `POST /v1/auth/logout` clears backend session mapping. |
| Gateway resume persistence | DONE | `discord_client` + `discord_storage` persist and restore `{session_id, seq, resume_url}`. |
| Postgres-backed session persistence | DONE | `discord_persistence_pg::PostgresKeyValueStore` implements `KeyValueStore` and migration routine. |

## REST Core Parity

| Feature | Status | Notes |
| --- | --- | --- |
| `GET /users/@me` | DONE | `discord_http`, `discord_client`, and `discord_service /v1/me`. |
| `GET /users/@me/guilds` | DONE | Typed query + service route `/v1/guilds`. |
| `GET /guilds/{id}/channels` | DONE | Typed route + service route `/v1/guilds/{guild_id}/channels`. |
| `GET /channels/{id}/messages` | DONE | Typed query + service route `/v1/channels/{channel_id}/messages`. |
| `POST /channels/{id}/messages` | DONE | Typed payload + service route + realtime emit. |
| `PATCH /channels/{id}/messages/{id}` | DONE | Added typed payload/route + HTTP/client/service support. |
| `DELETE /channels/{id}/messages/{id}` | DONE | Added typed route + HTTP/client/service support. |

## Gateway and Realtime

| Feature | Status | Notes |
| --- | --- | --- |
| Gateway identify/resume/heartbeat state machine | DONE | Deterministic and integration tests under `discord_gateway/tests/*`. |
| Runtime reconnect and resume | DONE | `runtime_reconnect_integration.rs` coverage. |
| Backend event stream endpoint | DONE | `GET /v1/stream` now auto-starts per-session gateway workers, emits normalized dispatch envelopes, and scopes events by authenticated session. |

## Voice

| Feature | Status | Notes |
| --- | --- | --- |
| Voice session lifecycle | DONE | `/v1/voice/sessions` now tracks owner-scoped lifecycle, supports runtime bootstrap config, and cleans up workers on delete/logout. |
| Speaking state and transport | DONE | `discord_voice` runtime is orchestrated by `discord_service` and emits realtime voice gateway/reconnect events through `/v1/stream`. |

## UIKit Client

| Feature | Status | Notes |
| --- | --- | --- |
| UIKit shell modules and adapters | DONE | `ios/OpenCordUIKit` includes client-core DTOs/protocols, networking adapters, persistent session store, and UIKit shell controllers. |
| Backend API/realtime integration | DONE | `OpenCordNetworking` now covers auth start/logout + API/realtime routes; `ios/OpenCordUIKitApp` provides a runnable scene lifecycle app with OAuth deep-link callback handling. |

## Guardrails

| Feature | Status | Notes |
| --- | --- | --- |
| 429 retry policy | DONE | `discord_http` bounded retries + regression tests. |
| Public API lock tests | DONE | Present across core crates. |
| Service-level smoke tests | DONE | Added for `/v1/me` and delete-message realtime emission. |
| Production telemetry/error taxonomy | DONE | `discord_service` now emits structured error codes in API payloads and applies HTTP trace middleware in the production binary. |

## Remaining Hardening (Optional)

1. Add full end-to-end integration tests that run backend service + Swift clients against local mock Discord REST/Gateway/Voice servers in one CI job.
2. Expand voice path from control-plane + gateway runtime to full media pipeline encryption/send/receive coverage.
