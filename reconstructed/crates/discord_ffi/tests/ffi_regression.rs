use discord_ffi::{
    DiscordClientHandle, DiscordErrorCode, discord_client_clear_token,
    discord_client_gateway_stream_next_event_json, discord_client_gateway_stream_stop,
    discord_client_new, discord_client_set_token, discord_string_free,
};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;

fn take_error(error: *mut c_char) -> String {
    assert!(!error.is_null());
    // SAFETY: `error` is expected to be allocated by this crate.
    let text = unsafe { CStr::from_ptr(error).to_str().expect("utf-8 error") }.to_owned();
    // SAFETY: `error` came from this crate.
    unsafe { discord_string_free(error) };
    text
}

#[test]
fn client_new_rejects_null_base_url() {
    let mut client: *mut DiscordClientHandle = ptr::null_mut();
    let mut error: *mut c_char = ptr::null_mut();

    // SAFETY: Inputs are valid pointers; `base_url` is intentionally null.
    let result = unsafe { discord_client_new(ptr::null(), &mut client, &mut error) };
    assert_eq!(result, DiscordErrorCode::DiscordNullPointer);
    assert!(client.is_null());

    let error_text = take_error(error);
    assert_eq!(error_text, "base_url cannot be null");
}

#[test]
fn set_token_rejects_null_client_pointer() {
    let token = CString::new("token").expect("literal");
    let mut error: *mut c_char = ptr::null_mut();

    // SAFETY: Inputs are valid pointers; `client` is intentionally null.
    let result = unsafe { discord_client_set_token(ptr::null_mut(), token.as_ptr(), &mut error) };
    assert_eq!(result, DiscordErrorCode::DiscordNullPointer);

    let error_text = take_error(error);
    assert_eq!(error_text, "client cannot be null");
}

#[test]
fn clear_token_rejects_null_client_pointer() {
    let mut error: *mut c_char = ptr::null_mut();

    // SAFETY: Inputs are valid pointers; `client` is intentionally null.
    let result = unsafe { discord_client_clear_token(ptr::null_mut(), &mut error) };
    assert_eq!(result, DiscordErrorCode::DiscordNullPointer);

    let error_text = take_error(error);
    assert_eq!(error_text, "client cannot be null");
}

#[test]
fn string_free_accepts_null() {
    // SAFETY: Null is explicitly accepted by the API.
    unsafe { discord_string_free(ptr::null_mut()) };
}

#[test]
fn gateway_stop_requires_running_stream() {
    let base_url = CString::new("https://discord.com/api/v10").expect("literal");
    let mut client: *mut DiscordClientHandle = ptr::null_mut();
    let mut error: *mut c_char = ptr::null_mut();

    // SAFETY: Inputs are valid C pointers produced by Rust.
    let created = unsafe { discord_client_new(base_url.as_ptr(), &mut client, &mut error) };
    assert_eq!(created, DiscordErrorCode::DiscordOk);

    // SAFETY: Inputs are valid C pointers produced by Rust.
    let stopped = unsafe { discord_client_gateway_stream_stop(client, &mut error) };
    assert_eq!(stopped, DiscordErrorCode::DiscordGatewayNotRunning);
    let message = take_error(error);
    assert_eq!(message, "gateway stream is not running");

    // SAFETY: Pointer was allocated by this crate.
    unsafe { discord_ffi::discord_client_free(client) };
}

#[test]
fn gateway_poll_requires_running_stream() {
    let base_url = CString::new("https://discord.com/api/v10").expect("literal");
    let mut client: *mut DiscordClientHandle = ptr::null_mut();
    let mut error: *mut c_char = ptr::null_mut();
    let mut out_json: *mut c_char = ptr::null_mut();

    // SAFETY: Inputs are valid C pointers produced by Rust.
    let created = unsafe { discord_client_new(base_url.as_ptr(), &mut client, &mut error) };
    assert_eq!(created, DiscordErrorCode::DiscordOk);

    // SAFETY: Inputs are valid C pointers produced by Rust.
    let polled =
        unsafe { discord_client_gateway_stream_next_event_json(client, &mut out_json, &mut error) };
    assert_eq!(polled, DiscordErrorCode::DiscordGatewayNotRunning);
    assert!(out_json.is_null());
    let message = take_error(error);
    assert_eq!(message, "gateway stream is not running");

    // SAFETY: Pointer was allocated by this crate.
    unsafe { discord_ffi::discord_client_free(client) };
}
