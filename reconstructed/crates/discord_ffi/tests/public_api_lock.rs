use discord_ffi::{
    DISCORD_FFI_ABI_VERSION, DiscordClientHandle, DiscordErrorCode, discord_ffi_abi_version,
};

#[test]
fn ffi_public_api_lock() {
    assert_eq!(discord_ffi_abi_version(), DISCORD_FFI_ABI_VERSION);

    let _ = DiscordErrorCode::DiscordOk;
    let _ = DiscordErrorCode::DiscordGatewayNotRunning;

    let _client_ptr: *mut DiscordClientHandle = std::ptr::null_mut();
}
