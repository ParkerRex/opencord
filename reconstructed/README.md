# OpenCord Rust Workspace

This repository now contains the Rust client workspace scaffold for:
- `discord_api_types`
- `discord_http`
- `discord_gateway`
- `discord_auth`
- `discord_storage`
- `discord_client`

## Quick Start

```bash
cd reconstructed
cargo fmt --all
cargo check --workspace
cargo test --workspace
```

## Layout

- `Cargo.toml`: workspace definition and shared dependencies.
- `crates/`: all Rust crates.
- `docs/packet/`: planning docs for Rust + upcoming Swift UIKit app.
- `scripts/dev-check.sh`: formatting + check + tests.
