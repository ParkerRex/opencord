# Integration Sequencing (Low-Conflict Plan)

This document defines merge order while multiple agents are working in parallel.

## Merge Order

1. `discord_api_types` model and route shape changes.
2. `discord_http` route clients and rate-limit behavior.
3. `discord_gateway` runtime/state-machine updates.
4. `discord_storage` session persistence updates.
5. `discord_client` session manager orchestration.
6. `discord_ffi` bridge updates.

## Why This Order

- `discord_http` depends on stable route types.
- `discord_client` depends on HTTP + gateway + storage contracts.
- FFI should track a stable client facade, not mid-refactor internals.

## Integration Branch Rules

- Prefer additive API evolution while integration is active.
- Avoid renaming public symbols unless absolutely required.
- Keep protocol shape changes in dedicated commits with migration notes.

## Required Pre-Merge Checks

- `cargo fmt --all`
- `cargo check --workspace`
- `cargo test --workspace`
- Ensure no generated artifacts (`target/`, generated headers outside `include/`) are staged.
