# Rust Build Plan

## Phase 1: Foundation (done)

- Workspace and six crates scaffolded.
- Shared dependencies and baseline APIs wired.
- Compile-ready interfaces for HTTP/gateway/auth/storage/client façade.

## Phase 2: API Expansion

- Done: add typed request/response models for key routes.
- Done: add route-specific methods in `discord_http`.
- Done: add regression tests for route/path correctness.

### Phase 2 Deliverables Implemented

- `discord_api_types`:
  - Route DTOs for guild listing, channel messages, and message creation.
  - `routes` module with typed route structs and route traits.
  - Numeric Discord channel type mapping with unknown-value fallback.
- `discord_http`:
  - Generic route execution helpers for plain, query, and JSON body routes.
  - Endpoint wrappers for user, guild, channel message list, and message create routes.
- Tests:
  - `discord_api_types` route path + channel type roundtrip coverage.
  - `discord_http` endpoint normalization and invalid-path coverage.

## Phase 3: Gateway Runtime

- Add identify/resume/heartbeat state machine.
- Add event dispatcher abstraction.
- Add reconnect and backoff policies.

## Phase 4: Production Hardening

- Structured logging and tracing.
- Retry/rate-limit policy layer.
- Integration tests against recorded fixtures.
