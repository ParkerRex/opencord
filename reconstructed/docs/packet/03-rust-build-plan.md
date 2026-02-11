# Rust Build Plan

## Phase 1: Foundation (done)

- Workspace and six crates scaffolded.
- Shared dependencies and baseline APIs wired.
- Compile-ready interfaces for HTTP/gateway/auth/storage/client façade.

## Phase 2: API Expansion

- Add typed request/response models for key routes.
- Add route-specific methods in `discord_http`.
- Add fixture-based deserialization tests.

## Phase 3: Gateway Runtime

- Add identify/resume/heartbeat state machine.
- Add event dispatcher abstraction.
- Add reconnect and backoff policies.

## Phase 4: Production Hardening

- Structured logging and tracing.
- Retry/rate-limit policy layer.
- Integration tests against recorded fixtures.
