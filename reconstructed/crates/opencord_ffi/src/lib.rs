use discord_auth::MemoryTokenProvider;
use discord_client::DiscordClient;
use discord_http::DiscordHttpClient;
use discord_storage::MemoryStore;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;
use tokio::runtime::{Builder, Runtime};
use url::Url;

/// Increment when changing the C ABI surface in a breaking way.
pub const OPENCORD_FFI_ABI_VERSION: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenCordErrorCode {
    OpenCordOk = 0,
    OpenCordNullPointer = 1,
    OpenCordInvalidUtf8 = 2,
    OpenCordInvalidUrl = 3,
    OpenCordRuntimeInitError = 4,
    OpenCordClientError = 5,
    OpenCordSerializationError = 6,
}

#[repr(C)]
pub struct OpenCordClient {
    _private: [u8; 0],
}

struct OpenCordClientInner {
    runtime: Runtime,
    client: DiscordClient<MemoryTokenProvider, MemoryStore>,
}

fn clear_out_string(out: *mut *mut c_char) {
    if out.is_null() {
        return;
    }

    // SAFETY: The caller provided `out`; when non-null, writing a null pointer is valid.
    unsafe {
        *out = ptr::null_mut();
    }
}

fn set_out_error(out_error: *mut *mut c_char, message: &str) {
    if out_error.is_null() {
        return;
    }

    let sanitized = message.replace('\0', "?");
    let c_message = CString::new(sanitized)
        .unwrap_or_else(|_| CString::new("error message could not be encoded").expect("literal"));

    // SAFETY: `out_error` is non-null and points to writable memory by API contract.
    unsafe {
        *out_error = c_message.into_raw();
    }
}

fn fail(
    code: OpenCordErrorCode,
    out_error: *mut *mut c_char,
    message: impl AsRef<str>,
) -> OpenCordErrorCode {
    set_out_error(out_error, message.as_ref());
    code
}

unsafe fn read_c_string(value: *const c_char, name: &str) -> Result<String, OpenCordErrorCode> {
    if value.is_null() {
        return Err(OpenCordErrorCode::OpenCordNullPointer);
    }

    // SAFETY: `value` is checked non-null; validity is still caller's responsibility.
    let c_str = unsafe { CStr::from_ptr(value) };
    match c_str.to_str() {
        Ok(v) => Ok(v.to_owned()),
        Err(_) => {
            let _ = name;
            Err(OpenCordErrorCode::OpenCordInvalidUtf8)
        }
    }
}

unsafe fn client_from_ptr<'a>(
    client: *mut OpenCordClient,
) -> Result<&'a OpenCordClientInner, OpenCordErrorCode> {
    if client.is_null() {
        return Err(OpenCordErrorCode::OpenCordNullPointer);
    }

    // SAFETY: The pointer was created from `Box<OpenCordClientInner>` in `opencord_client_new`.
    Ok(unsafe { &*client.cast::<OpenCordClientInner>() })
}

fn write_out_value(out: *mut *mut c_char, value: &str) -> Result<(), OpenCordErrorCode> {
    if out.is_null() {
        return Err(OpenCordErrorCode::OpenCordNullPointer);
    }

    let sanitized = value.replace('\0', "?");
    let c_value =
        CString::new(sanitized).map_err(|_| OpenCordErrorCode::OpenCordSerializationError)?;

    // SAFETY: `out` is non-null and points to writable memory by API contract.
    unsafe {
        *out = c_value.into_raw();
    }

    Ok(())
}

#[unsafe(no_mangle)]
pub extern "C" fn opencord_ffi_abi_version() -> u32 {
    OPENCORD_FFI_ABI_VERSION
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn opencord_string_free(value: *mut c_char) {
    if value.is_null() {
        return;
    }

    // SAFETY: Pointer must come from this crate via `CString::into_raw`.
    unsafe {
        drop(CString::from_raw(value));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn opencord_client_new(
    base_url: *const c_char,
    out_client: *mut *mut OpenCordClient,
    out_error: *mut *mut c_char,
) -> OpenCordErrorCode {
    clear_out_string(out_error);

    if out_client.is_null() {
        return fail(
            OpenCordErrorCode::OpenCordNullPointer,
            out_error,
            "out_client cannot be null",
        );
    }

    // SAFETY: `out_client` is non-null and writable by API contract.
    unsafe {
        *out_client = ptr::null_mut();
    }

    // SAFETY: FFI caller must provide a valid C string.
    let base_url = match unsafe { read_c_string(base_url, "base_url") } {
        Ok(v) => v,
        Err(OpenCordErrorCode::OpenCordNullPointer) => {
            return fail(
                OpenCordErrorCode::OpenCordNullPointer,
                out_error,
                "base_url cannot be null",
            );
        }
        Err(_) => {
            return fail(
                OpenCordErrorCode::OpenCordInvalidUtf8,
                out_error,
                "base_url must be valid UTF-8",
            );
        }
    };

    let parsed_base_url = match Url::parse(&base_url) {
        Ok(url) => url,
        Err(error) => {
            return fail(
                OpenCordErrorCode::OpenCordInvalidUrl,
                out_error,
                format!("invalid base_url: {error}"),
            );
        }
    };

    let runtime = match Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(error) => {
            return fail(
                OpenCordErrorCode::OpenCordRuntimeInitError,
                out_error,
                format!("failed to initialize runtime: {error}"),
            );
        }
    };

    let client = DiscordClient::new(
        DiscordHttpClient::new(parsed_base_url),
        MemoryTokenProvider::new(),
        MemoryStore::new(),
    );
    let raw_inner = Box::into_raw(Box::new(OpenCordClientInner { runtime, client }));

    // SAFETY: `out_client` is non-null and writable by API contract.
    unsafe {
        *out_client = raw_inner.cast::<OpenCordClient>();
    }

    OpenCordErrorCode::OpenCordOk
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn opencord_client_free(client: *mut OpenCordClient) {
    if client.is_null() {
        return;
    }

    // SAFETY: Pointer must come from `opencord_client_new` and be freed exactly once.
    unsafe {
        drop(Box::from_raw(client.cast::<OpenCordClientInner>()));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn opencord_client_set_token(
    client: *mut OpenCordClient,
    token: *const c_char,
    out_error: *mut *mut c_char,
) -> OpenCordErrorCode {
    clear_out_string(out_error);

    // SAFETY: FFI caller must provide a valid client pointer from this crate.
    let client = match unsafe { client_from_ptr(client) } {
        Ok(v) => v,
        Err(_) => {
            return fail(
                OpenCordErrorCode::OpenCordNullPointer,
                out_error,
                "client cannot be null",
            );
        }
    };

    // SAFETY: FFI caller must provide a valid C string.
    let token = match unsafe { read_c_string(token, "token") } {
        Ok(v) => v,
        Err(OpenCordErrorCode::OpenCordNullPointer) => {
            return fail(
                OpenCordErrorCode::OpenCordNullPointer,
                out_error,
                "token cannot be null",
            );
        }
        Err(_) => {
            return fail(
                OpenCordErrorCode::OpenCordInvalidUtf8,
                out_error,
                "token must be valid UTF-8",
            );
        }
    };

    match client.runtime.block_on(client.client.set_token(token)) {
        Ok(()) => OpenCordErrorCode::OpenCordOk,
        Err(error) => fail(
            OpenCordErrorCode::OpenCordClientError,
            out_error,
            format!("set_token failed: {error}"),
        ),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn opencord_client_clear_token(
    client: *mut OpenCordClient,
    out_error: *mut *mut c_char,
) -> OpenCordErrorCode {
    clear_out_string(out_error);

    // SAFETY: FFI caller must provide a valid client pointer from this crate.
    let client = match unsafe { client_from_ptr(client) } {
        Ok(v) => v,
        Err(_) => {
            return fail(
                OpenCordErrorCode::OpenCordNullPointer,
                out_error,
                "client cannot be null",
            );
        }
    };

    match client.runtime.block_on(client.client.clear_token()) {
        Ok(()) => OpenCordErrorCode::OpenCordOk,
        Err(error) => fail(
            OpenCordErrorCode::OpenCordClientError,
            out_error,
            format!("clear_token failed: {error}"),
        ),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn opencord_client_token_copy(
    client: *mut OpenCordClient,
    out_token: *mut *mut c_char,
    out_error: *mut *mut c_char,
) -> OpenCordErrorCode {
    clear_out_string(out_error);
    clear_out_string(out_token);

    // SAFETY: FFI caller must provide a valid client pointer from this crate.
    let client = match unsafe { client_from_ptr(client) } {
        Ok(v) => v,
        Err(_) => {
            return fail(
                OpenCordErrorCode::OpenCordNullPointer,
                out_error,
                "client cannot be null",
            );
        }
    };

    let token = match client.runtime.block_on(client.client.token()) {
        Ok(token) => token,
        Err(error) => {
            return fail(
                OpenCordErrorCode::OpenCordClientError,
                out_error,
                format!("token fetch failed: {error}"),
            );
        }
    };

    match write_out_value(out_token, &token) {
        Ok(()) => OpenCordErrorCode::OpenCordOk,
        Err(OpenCordErrorCode::OpenCordNullPointer) => fail(
            OpenCordErrorCode::OpenCordNullPointer,
            out_error,
            "out_token cannot be null",
        ),
        Err(_) => fail(
            OpenCordErrorCode::OpenCordSerializationError,
            out_error,
            "token could not be encoded as a C string",
        ),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn opencord_client_current_user_json(
    client: *mut OpenCordClient,
    out_json: *mut *mut c_char,
    out_error: *mut *mut c_char,
) -> OpenCordErrorCode {
    clear_out_string(out_error);
    clear_out_string(out_json);

    // SAFETY: FFI caller must provide a valid client pointer from this crate.
    let client = match unsafe { client_from_ptr(client) } {
        Ok(v) => v,
        Err(_) => {
            return fail(
                OpenCordErrorCode::OpenCordNullPointer,
                out_error,
                "client cannot be null",
            );
        }
    };

    let user = match client.runtime.block_on(client.client.current_user()) {
        Ok(user) => user,
        Err(error) => {
            return fail(
                OpenCordErrorCode::OpenCordClientError,
                out_error,
                format!("current_user failed: {error}"),
            );
        }
    };

    let user_json = match serde_json::to_string(&user) {
        Ok(payload) => payload,
        Err(error) => {
            return fail(
                OpenCordErrorCode::OpenCordSerializationError,
                out_error,
                format!("failed to serialize user: {error}"),
            );
        }
    };

    match write_out_value(out_json, &user_json) {
        Ok(()) => OpenCordErrorCode::OpenCordOk,
        Err(OpenCordErrorCode::OpenCordNullPointer) => fail(
            OpenCordErrorCode::OpenCordNullPointer,
            out_error,
            "out_json cannot be null",
        ),
        Err(_) => fail(
            OpenCordErrorCode::OpenCordSerializationError,
            out_error,
            "serialized user could not be encoded as a C string",
        ),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn opencord_client_gateway_bot_json(
    client: *mut OpenCordClient,
    out_json: *mut *mut c_char,
    out_error: *mut *mut c_char,
) -> OpenCordErrorCode {
    clear_out_string(out_error);
    clear_out_string(out_json);

    // SAFETY: FFI caller must provide a valid client pointer from this crate.
    let client = match unsafe { client_from_ptr(client) } {
        Ok(v) => v,
        Err(_) => {
            return fail(
                OpenCordErrorCode::OpenCordNullPointer,
                out_error,
                "client cannot be null",
            );
        }
    };

    let gateway = match client.runtime.block_on(client.client.gateway_bot()) {
        Ok(gateway) => gateway,
        Err(error) => {
            return fail(
                OpenCordErrorCode::OpenCordClientError,
                out_error,
                format!("gateway_bot failed: {error}"),
            );
        }
    };

    let gateway_json = match serde_json::to_string(&gateway) {
        Ok(payload) => payload,
        Err(error) => {
            return fail(
                OpenCordErrorCode::OpenCordSerializationError,
                out_error,
                format!("failed to serialize gateway payload: {error}"),
            );
        }
    };

    match write_out_value(out_json, &gateway_json) {
        Ok(()) => OpenCordErrorCode::OpenCordOk,
        Err(OpenCordErrorCode::OpenCordNullPointer) => fail(
            OpenCordErrorCode::OpenCordNullPointer,
            out_error,
            "out_json cannot be null",
        ),
        Err(_) => fail(
            OpenCordErrorCode::OpenCordSerializationError,
            out_error,
            "serialized gateway payload could not be encoded as a C string",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    fn take_error(error: *mut c_char) -> String {
        assert!(!error.is_null());
        // SAFETY: `error` is a valid pointer allocated by this crate.
        let text = unsafe { CStr::from_ptr(error).to_str().expect("utf-8 error") }.to_owned();
        // SAFETY: `error` was allocated by this crate.
        unsafe { opencord_string_free(error) };
        text
    }

    #[test]
    fn abi_version_is_set() {
        assert_eq!(opencord_ffi_abi_version(), OPENCORD_FFI_ABI_VERSION);
    }

    #[test]
    fn client_new_set_and_clear_token() {
        let base_url = CString::new("https://discord.com/api/v10").expect("literal");
        let token = CString::new("test-token").expect("literal");
        let mut client: *mut OpenCordClient = ptr::null_mut();
        let mut error: *mut c_char = ptr::null_mut();
        let mut out_token: *mut c_char = ptr::null_mut();

        // SAFETY: Inputs are valid C pointers produced by Rust.
        let created = unsafe { opencord_client_new(base_url.as_ptr(), &mut client, &mut error) };
        assert_eq!(created, OpenCordErrorCode::OpenCordOk);
        assert!(error.is_null());
        assert!(!client.is_null());

        // SAFETY: Inputs are valid C pointers produced by Rust.
        let set = unsafe { opencord_client_set_token(client, token.as_ptr(), &mut error) };
        assert_eq!(set, OpenCordErrorCode::OpenCordOk);
        assert!(error.is_null());

        // SAFETY: Inputs are valid C pointers produced by Rust.
        let read = unsafe { opencord_client_token_copy(client, &mut out_token, &mut error) };
        assert_eq!(read, OpenCordErrorCode::OpenCordOk);
        assert!(error.is_null());
        assert!(!out_token.is_null());

        // SAFETY: `out_token` is valid and null-terminated from Rust.
        let token_text = unsafe { CStr::from_ptr(out_token).to_str().expect("utf-8 token") };
        assert_eq!(token_text, "test-token");

        // SAFETY: Pointer was allocated by this crate.
        unsafe { opencord_string_free(out_token) };

        // SAFETY: Inputs are valid C pointers produced by Rust.
        let cleared = unsafe { opencord_client_clear_token(client, &mut error) };
        assert_eq!(cleared, OpenCordErrorCode::OpenCordOk);
        assert!(error.is_null());

        // SAFETY: Pointer was allocated by this crate.
        unsafe { opencord_client_free(client) };
    }

    #[test]
    fn client_new_rejects_invalid_base_url() {
        let base_url = CString::new("not a url").expect("literal");
        let mut client: *mut OpenCordClient = ptr::null_mut();
        let mut error: *mut c_char = ptr::null_mut();

        // SAFETY: Inputs are valid C pointers produced by Rust.
        let result = unsafe { opencord_client_new(base_url.as_ptr(), &mut client, &mut error) };
        assert_eq!(result, OpenCordErrorCode::OpenCordInvalidUrl);
        assert!(client.is_null());

        let error_text = take_error(error);
        assert!(error_text.contains("invalid base_url"));
    }

    #[test]
    fn client_new_requires_out_client_pointer() {
        let base_url = CString::new("https://discord.com/api/v10").expect("literal");
        let mut error: *mut c_char = ptr::null_mut();

        // SAFETY: Inputs are valid C pointers produced by Rust.
        let result = unsafe { opencord_client_new(base_url.as_ptr(), ptr::null_mut(), &mut error) };
        assert_eq!(result, OpenCordErrorCode::OpenCordNullPointer);

        let error_text = take_error(error);
        assert_eq!(error_text, "out_client cannot be null");
    }

    #[test]
    fn set_token_rejects_invalid_utf8() {
        let base_url = CString::new("https://discord.com/api/v10").expect("literal");
        let mut client: *mut OpenCordClient = ptr::null_mut();
        let mut error: *mut c_char = ptr::null_mut();
        let invalid = CString::new(vec![0xff]).expect("single byte");

        // SAFETY: Inputs are valid C pointers produced by Rust.
        let created = unsafe { opencord_client_new(base_url.as_ptr(), &mut client, &mut error) };
        assert_eq!(created, OpenCordErrorCode::OpenCordOk);
        assert!(error.is_null());

        // SAFETY: Inputs are valid C pointers produced by Rust.
        let result = unsafe { opencord_client_set_token(client, invalid.as_ptr(), &mut error) };
        assert_eq!(result, OpenCordErrorCode::OpenCordInvalidUtf8);

        let error_text = take_error(error);
        assert_eq!(error_text, "token must be valid UTF-8");

        // SAFETY: Pointer was allocated by this crate.
        unsafe { opencord_client_free(client) };
    }

    #[test]
    fn token_copy_requires_output_pointer() {
        let base_url = CString::new("https://discord.com/api/v10").expect("literal");
        let token = CString::new("test-token").expect("literal");
        let mut client: *mut OpenCordClient = ptr::null_mut();
        let mut error: *mut c_char = ptr::null_mut();

        // SAFETY: Inputs are valid C pointers produced by Rust.
        let created = unsafe { opencord_client_new(base_url.as_ptr(), &mut client, &mut error) };
        assert_eq!(created, OpenCordErrorCode::OpenCordOk);

        // SAFETY: Inputs are valid C pointers produced by Rust.
        let set = unsafe { opencord_client_set_token(client, token.as_ptr(), &mut error) };
        assert_eq!(set, OpenCordErrorCode::OpenCordOk);

        // SAFETY: Inputs are valid C pointers produced by Rust.
        let result = unsafe { opencord_client_token_copy(client, ptr::null_mut(), &mut error) };
        assert_eq!(result, OpenCordErrorCode::OpenCordNullPointer);

        let error_text = take_error(error);
        assert_eq!(error_text, "out_token cannot be null");

        // SAFETY: Pointer was allocated by this crate.
        unsafe { opencord_client_free(client) };
    }
}
