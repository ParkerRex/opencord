# Swift FFI Contract (Scaffold)

This is the initial C ABI contract exported by `crates/opencord_ffi`.

## Generated Header

- Source config: `crates/opencord_ffi/cbindgen.toml`
- Generated header: `include/opencord_ffi.h`
- Generator script: `scripts/generate-ffi-header.sh`

## Types

- `OpenCordClient`: opaque Rust client handle (use only as a pointer).
- `OpenCordErrorCode`: operation status code.

## Functions

- `opencord_ffi_abi_version() -> uint32_t`
- `opencord_client_new(base_url, out_client, out_error)`
- `opencord_client_free(client)`
- `opencord_client_set_token(client, token, out_error)`
- `opencord_client_clear_token(client, out_error)`
- `opencord_client_token_copy(client, out_token, out_error)`
- `opencord_client_current_user_json(client, out_json, out_error)`
- `opencord_client_gateway_bot_json(client, out_json, out_error)`
- `opencord_string_free(value)`

## Ownership Rules

- Any `char*` returned through an out pointer is allocated by Rust.
- Swift/C must release returned strings with `opencord_string_free`.
- `OpenCordClient*` must be released with `opencord_client_free`.
- Passing null required pointers returns `OpenCordNullPointer`.

## Notes for Swift Wrapper

- Treat non-`OpenCordOk` as throwable Swift errors.
- Convert returned JSON strings to Swift `Decodable` models in the wrapper layer.
- Keep FFI calls on a dedicated queue if you need strict call serialization.
