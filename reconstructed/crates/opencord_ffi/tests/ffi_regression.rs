use opencord_ffi::{
    OpenCordClient, OpenCordErrorCode, opencord_client_clear_token, opencord_client_new,
    opencord_client_set_token, opencord_string_free,
};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;

fn take_error(error: *mut c_char) -> String {
    assert!(!error.is_null());
    // SAFETY: `error` is expected to be allocated by this crate.
    let text = unsafe { CStr::from_ptr(error).to_str().expect("utf-8 error") }.to_owned();
    // SAFETY: `error` came from this crate.
    unsafe { opencord_string_free(error) };
    text
}

#[test]
fn client_new_rejects_null_base_url() {
    let mut client: *mut OpenCordClient = ptr::null_mut();
    let mut error: *mut c_char = ptr::null_mut();

    // SAFETY: Inputs are valid pointers; `base_url` is intentionally null.
    let result = unsafe { opencord_client_new(ptr::null(), &mut client, &mut error) };
    assert_eq!(result, OpenCordErrorCode::OpenCordNullPointer);
    assert!(client.is_null());

    let error_text = take_error(error);
    assert_eq!(error_text, "base_url cannot be null");
}

#[test]
fn set_token_rejects_null_client_pointer() {
    let token = CString::new("token").expect("literal");
    let mut error: *mut c_char = ptr::null_mut();

    // SAFETY: Inputs are valid pointers; `client` is intentionally null.
    let result = unsafe { opencord_client_set_token(ptr::null_mut(), token.as_ptr(), &mut error) };
    assert_eq!(result, OpenCordErrorCode::OpenCordNullPointer);

    let error_text = take_error(error);
    assert_eq!(error_text, "client cannot be null");
}

#[test]
fn clear_token_rejects_null_client_pointer() {
    let mut error: *mut c_char = ptr::null_mut();

    // SAFETY: Inputs are valid pointers; `client` is intentionally null.
    let result = unsafe { opencord_client_clear_token(ptr::null_mut(), &mut error) };
    assert_eq!(result, OpenCordErrorCode::OpenCordNullPointer);

    let error_text = take_error(error);
    assert_eq!(error_text, "client cannot be null");
}

#[test]
fn string_free_accepts_null() {
    // SAFETY: Null is explicitly accepted by the API.
    unsafe { opencord_string_free(ptr::null_mut()) };
}
