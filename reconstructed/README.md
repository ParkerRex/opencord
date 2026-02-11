# OpenCord Rust Workspace

This repository now contains the Rust client workspace scaffold for:
- `discord_api_types`
- `discord_http`
- `discord_gateway`
- `discord_auth`
- `discord_storage`
- `discord_client`
- `opencord_ffi`

## Quick Start

```bash
cd reconstructed
cargo fmt --all
cargo check --workspace
cargo test --workspace
./scripts/generate-ffi-header.sh
```

## Layout

- `Cargo.toml`: workspace definition and shared dependencies.
- `crates/`: all Rust crates.
- `include/`: generated C header output (`opencord_ffi.h`).
- `docs/packet/`: planning docs for Rust + upcoming Swift UIKit app.
- `scripts/dev-check.sh`: formatting + check + tests.
- `scripts/generate-ffi-header.sh`: generate Swift-consumable C header via `cbindgen`.
