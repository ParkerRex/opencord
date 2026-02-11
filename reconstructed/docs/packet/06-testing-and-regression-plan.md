# Testing And Regression Plan

This document tracks high-value automated tests around Discord REST payload parsing, gateway state behavior, and end-to-end connectivity.

## REST Payload Fixtures

Location:
- `crates/discord_api_types/tests/rest_payload_fixtures.rs`
- `crates/discord_api_types/tests/fixtures/rest/*.json`

Coverage:
- `/users/@me` user payload deserialization.
- `/gateway/bot` payload deserialization.
- `/users/@me/guilds` payload deserialization.
- `guild channels` payload deserialization.
- `channel messages` payload deserialization.
- `create message` payload serialization snapshot.

Purpose:
- Catch schema drift early when Discord response shape changes.
- Lock expected field names and optional-field behavior.

## Deterministic Gateway State-Machine Tests

Location:
- `crates/discord_gateway/tests/state_machine_deterministic.rs`

Coverage:
- Heartbeat tick + ACK -> continue heartbeats, no reconnect.
- Heartbeat tick without ACK -> reconnect requested.
- Explicit reconnect event with and without resumable session.
- Invalid resumable session keeps resume path on the next HELLO.

Purpose:
- Ensure reconnect and heartbeat behavior stays deterministic under timer-driven scenarios.

## End-To-End Smoke Test

Location:
- `crates/discord_client/tests/smoke_login_gateway_connect.rs`

Flow:
1. Set login token.
2. Call `/users/@me`.
3. Call `/gateway/bot`.
4. Connect to the returned gateway URL.

Purpose:
- Verify the core path from tokened REST access to websocket connectivity.
- Ensure bearer auth headers are sent on critical REST calls.

## Additional Gateway URL Regression Coverage

Location:
- `crates/discord_client/tests/gateway_client_regression.rs`

Coverage:
- Preserve existing gateway query values from `/gateway/bot`.
- Do not inject websocket query defaults for non-websocket URLs.

Purpose:
- Prevent URL rewrite regressions when gateway endpoint formats vary.

## Suggested Local Commands

```bash
cd reconstructed
cargo test -p discord_api_types -p discord_gateway -p discord_client
cargo test -p discord_client
```
