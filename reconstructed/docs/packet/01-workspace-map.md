# Workspace Map

## Crates

- `discord_api_types`
- `discord_http`
- `discord_gateway`
- `discord_auth`
- `discord_storage`
- `discord_client`
- `opencord_ffi`

## Responsibility Split

- `discord_api_types`: shared DTOs and basic domain types.
- `discord_http`: REST transport and endpoint wrappers.
- `discord_gateway`: websocket gateway connection/event parsing.
- `discord_auth`: token provider abstractions.
- `discord_storage`: key-value persistence abstraction.
- `discord_client`: façade integrating auth/http/storage/gateway.
- `opencord_ffi`: C ABI boundary for Swift/UIKit integration (`cbindgen` header source).
