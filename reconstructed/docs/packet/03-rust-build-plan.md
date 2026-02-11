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

- Done: add identify/resume/heartbeat state machine.
- Done: keep transition logic pure and protocol-driven.
- Done: add runtime/driver layer for websocket I/O and shutdown control.
- Done: add reconnect policy with capped backoff and jitter.
- Done: add integration tests for reconnect+resume and invalid-session fallback flows.

### Phase 3 Deliverables Implemented

- `discord_gateway::GatewayStateMachine`:
  - Handles `HELLO`, `READY`, `RESUMED`, `HEARTBEAT_ACK`, `RECONNECT`, and `INVALID_SESSION`.
  - Tracks sequence/session to choose `IDENTIFY` vs `RESUME`.
  - Emits action enum (`GatewayStateAction`) for side-effect execution.
- `discord_gateway::runtime`:
  - Executes `GatewayStateAction` over a real websocket.
  - Drives heartbeat interval and heartbeat timeout recovery.
  - Supports graceful shutdown via `tokio::sync::watch::Receiver<bool>`.
  - Emits runtime lifecycle events (`GatewayRuntimeEvent`) for hosting layers.
- Tests:
  - State machine regressions in `crates/discord_gateway/tests/state_machine_regression.rs`.
  - Runtime reconnect/resume integration tests with a mock websocket server in `crates/discord_gateway/tests/runtime_reconnect_integration.rs`.

## Phase 4: Production Hardening

- Structured logging and tracing.
- Retry/rate-limit policy layer.
- Integration tests against recorded fixtures.
