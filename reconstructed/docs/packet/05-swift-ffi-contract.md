# Swift FFI Contract (Scaffold)

This is the initial C ABI contract exported by `crates/discord_ffi`.

## Generated Header

- Source config: `crates/discord_ffi/cbindgen.toml`
- Generated header: `include/discord_ffi.h`
- Generator script: `scripts/generate-ffi-header.sh`

## Types

- `DiscordClientHandle`: opaque Rust client handle (pointer only).
- `DiscordErrorCode`: operation status code.

## Exposed C ABI

- `discord_ffi_abi_version() -> uint32_t`
- `discord_client_new(base_url, out_client, out_error)`
- `discord_client_free(client)`
- `discord_client_set_token(client, token, out_error)`
- `discord_client_clear_token(client, out_error)`
- `discord_client_current_user_json(client, out_json, out_error)`
- `discord_client_list_guilds_json(client, out_json, out_error)`
- `discord_client_list_channels_json(client, guild_id, out_json, out_error)`
- `discord_client_list_messages_json(client, channel_id, limit, out_json, out_error)`
- `discord_client_gateway_stream_start(client, intents, out_error)`
- `discord_client_gateway_stream_stop(client, out_error)`
- `discord_client_gateway_stream_next_event_json(client, out_json, out_error)`
- `discord_string_free(value)`

## Ownership Rules

- Any `char*` returned through an out pointer is allocated by Rust.
- Swift must release returned strings with `discord_string_free`.
- `DiscordClientHandle*` must be released with `discord_client_free`.
- Passing null required pointers returns `DiscordNullPointer`.

## Gateway Event Contract

- `discord_client_gateway_stream_start` launches the runtime stream task.
- `discord_client_gateway_stream_next_event_json` is non-blocking.
- If no event is available, `DiscordOk` is returned with `out_json == NULL`.
- Event payload format is JSON with top-level `type`:
  - `gateway_event` (contains nested `event` object)
  - `reconnect_scheduled` (includes `attempt`, `resumable`, `delay_ms`)
  - `shutdown`

## Swift Wrapper Contracts

Define a thin Swift wrapper layer around C ABI for memory safety and typed models.

```swift
public protocol DiscordFFIClient {
    func setToken(_ token: String) throws
    func currentUser() throws -> UserDTO
    func listGuilds() throws -> [GuildDTO]
    func listChannels(guildID: String) throws -> [ChannelDTO]
    func listMessages(channelID: String, limit: UInt8?) throws -> [MessageDTO]
    func startGateway(intents: UInt64) throws
    func stopGateway() throws
    func pollGatewayEvent() throws -> GatewayRuntimeEventDTO?
}
```

Wrapper requirements:

- Convert non-`DiscordOk` error codes into Swift `Error`.
- Decode JSON payloads into `Decodable` DTOs.
- Ensure every Rust-allocated C string is freed via `discord_string_free`.
- Ensure `discord_client_free` runs exactly once for each created client handle.
