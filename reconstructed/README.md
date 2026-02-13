Project name: OpenCord
One liner: OpenCord is a Discord client platform with a Rust core, Swift/iOS UI shell, and FFI bridge for native app integration.
Stack: Rust (Cargo workspace, Tokio, Axum), C FFI (cbindgen), Swift/SwiftPM/UIKit, XcodeGen.
Problem: Building and maintaining Discord-compatible client logic separately across backend and iOS is costly and inconsistent; OpenCord centralizes protocol/client behavior in shared modules and exposes a stable app-facing API.

# OpenCord Rust Workspace

This repo currently centers on a Rust workspace under `reconstructed/`.
It includes the core Discord client crates, an FFI boundary for Swift/native
consumers, and build/test tooling.

## Quick Start

```bash
cd reconstructed
cargo fmt --all
cargo check --workspace
cargo test --workspace
./scripts/generate-ffi-header.sh

# Optional: run app-facing backend service
cargo run -p discord_service
```

## Repository Layout

```text
opencord/
├── .github/
│   └── workflows/
│       └── reconstructed-rust-ci.yml
└── reconstructed/
    ├── Cargo.toml
    ├── Cargo.lock
    ├── crates/
    │   ├── discord_api_types/
    │   ├── discord_http/
    │   ├── discord_gateway/
    │   ├── discord_auth/
    │   ├── discord_storage/
    │   ├── discord_client/
    │   ├── discord_persistence_pg/
    │   ├── discord_ffi/
    │   ├── discord_service/
    │   └── discord_voice/
    ├── docs/
    │   ├── packet/
    │   └── V1_PARITY.md
    ├── include/
    ├── scripts/
    └── target/            # local build output (generated)
```

### Root

- `.github/workflows/reconstructed-rust-ci.yml`: CI for format, `cargo check`,
  and workspace tests under `reconstructed/`.
- `reconstructed/`: Rust workspace root.

### Workspace Root (`reconstructed/`)

- `Cargo.toml`: workspace members + shared dependency versions.
- `Cargo.lock`: pinned dependency graph.
- `crates/`: all first-party crates in this workspace.
- `docs/packet/`: planning and architecture packet (Rust + Swift integration).
- `docs/V1_PARITY.md`: execution tracking matrix for V1 feature parity.
- `include/`: generated C header output (`discord_ffi.h`).
- `scripts/`: local automation scripts for checks, FFI generation, and scaffolding.
- `target/`: generated build artifacts (ignored in git).

### Crates (`reconstructed/crates/`)

- `discord_api_types`: shared Discord types, payloads, and route definitions.
- `discord_http`: HTTP transport/client with rate-limit retry handling.
- `discord_gateway`: gateway websocket client, event parsing, state machine, and runtime.
- `discord_auth`: token-provider abstraction and in-memory provider.
- `discord_storage`: key-value storage abstraction + gateway session persistence helpers.
- `discord_client`: higher-level orchestration over auth/http/storage/gateway startup.
- `discord_ffi`: C ABI surface for embedding the client from Swift/native code.
- `discord_persistence_pg`: Postgres-backed `KeyValueStore` adapter for durable backend/session state.
- `discord_service`: Axum service exposing `/v1` app-facing API and realtime stream endpoints.
- `discord_voice`: voice websocket runtime + UDP discovery primitives with reconnect handling.

### Scripts (`reconstructed/scripts/`)

- `dev-check.sh`: run `fmt`, `check`, and tests.
- `generate-ffi-header.sh`: generate `include/discord_ffi.h` via `cbindgen`.
- `new-crate.sh <name>`: scaffold a new lib crate under `crates/`.

### iOS Client Package

- Swift package path: `../ios/OpenCordUIKit`
- Runnable app target scaffold: `../ios/OpenCordUIKitApp`
- Validate locally:
  ```bash
  cd ../ios/OpenCordUIKit
  swift test
  ```
- Generate iOS app project:
  ```bash
  cd ../ios/OpenCordUIKitApp
  xcodegen generate
  ```
