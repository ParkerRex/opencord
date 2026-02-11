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
    │   └── discord_ffi/
    ├── docs/
    │   └── packet/
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

### Scripts (`reconstructed/scripts/`)

- `dev-check.sh`: run `fmt`, `check`, and tests.
- `generate-ffi-header.sh`: generate `include/discord_ffi.h` via `cbindgen`.
- `new-crate.sh <name>`: scaffold a new lib crate under `crates/`.
