use discord_api_types::{GetChannelMessagesQuery, GetCurrentUserGuildsQuery};
use discord_auth::MemoryTokenProvider;
use discord_client::DiscordClient;
use discord_gateway::{
    GatewayEvent, GatewayRuntimeEvent, GatewayRuntimeOptions, GatewayStateMachineConfig,
};
use discord_http::DiscordHttpClient;
use discord_storage::MemoryStore;
use serde_json::{Value, json};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;
use std::sync::Mutex;
use tokio::runtime::{Builder, Runtime};
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use url::Url;

/// Increment when changing the C ABI surface in a breaking way.
pub const DISCORD_FFI_ABI_VERSION: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscordErrorCode {
    DiscordOk = 0,
    DiscordNullPointer = 1,
    DiscordInvalidUtf8 = 2,
    DiscordInvalidUrl = 3,
    DiscordRuntimeInitError = 4,
    DiscordClientError = 5,
    DiscordSerializationError = 6,
    DiscordGatewayAlreadyRunning = 7,
    DiscordGatewayNotRunning = 8,
}

#[repr(C)]
pub struct DiscordClientHandle {
    _private: [u8; 0],
}

struct GatewayStreamHandle {
    shutdown_tx: watch::Sender<bool>,
    task: JoinHandle<()>,
    events_rx: Mutex<mpsc::UnboundedReceiver<String>>,
}

struct DiscordClientInner {
    runtime: Runtime,
    client: DiscordClient<MemoryTokenProvider, MemoryStore>,
    gateway_stream: Mutex<Option<GatewayStreamHandle>>,
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
    code: DiscordErrorCode,
    out_error: *mut *mut c_char,
    message: impl AsRef<str>,
) -> DiscordErrorCode {
    set_out_error(out_error, message.as_ref());
    code
}

unsafe fn read_c_string(value: *const c_char) -> Result<String, DiscordErrorCode> {
    if value.is_null() {
        return Err(DiscordErrorCode::DiscordNullPointer);
    }

    // SAFETY: `value` is checked non-null; validity is caller-controlled by FFI contract.
    let c_str = unsafe { CStr::from_ptr(value) };
    match c_str.to_str() {
        Ok(v) => Ok(v.to_owned()),
        Err(_) => Err(DiscordErrorCode::DiscordInvalidUtf8),
    }
}

unsafe fn client_from_ptr<'a>(
    client: *mut DiscordClientHandle,
) -> Result<&'a DiscordClientInner, DiscordErrorCode> {
    if client.is_null() {
        return Err(DiscordErrorCode::DiscordNullPointer);
    }

    // SAFETY: The pointer was created from `Box<DiscordClientInner>` in `discord_client_new`.
    Ok(unsafe { &*client.cast::<DiscordClientInner>() })
}

fn write_out_value(out: *mut *mut c_char, value: &str) -> Result<(), DiscordErrorCode> {
    if out.is_null() {
        return Err(DiscordErrorCode::DiscordNullPointer);
    }

    let sanitized = value.replace('\0', "?");
    let c_value =
        CString::new(sanitized).map_err(|_| DiscordErrorCode::DiscordSerializationError)?;

    // SAFETY: `out` is non-null and points to writable memory by API contract.
    unsafe {
        *out = c_value.into_raw();
    }

    Ok(())
}

fn gateway_event_to_json(event: &GatewayEvent) -> Value {
    match event {
        GatewayEvent::Dispatch {
            event_type,
            sequence,
            data,
        } => json!({
            "kind": "dispatch",
            "event_type": event_type,
            "sequence": sequence,
            "data": data,
        }),
        GatewayEvent::HeartbeatRequest => json!({"kind": "heartbeat_request"}),
        GatewayEvent::Hello(payload) => json!({
            "kind": "hello",
            "heartbeat_interval_ms": payload.heartbeat_interval,
        }),
        GatewayEvent::HeartbeatAck => json!({"kind": "heartbeat_ack"}),
        GatewayEvent::Reconnect => json!({"kind": "reconnect"}),
        GatewayEvent::InvalidSession { resumable } => json!({
            "kind": "invalid_session",
            "resumable": resumable,
        }),
        GatewayEvent::NonTextFrame => json!({"kind": "non_text_frame"}),
        GatewayEvent::Unknown(data) => json!({
            "kind": "unknown",
            "data": data,
        }),
    }
}

fn runtime_event_to_json(event: &GatewayRuntimeEvent) -> Value {
    match event {
        GatewayRuntimeEvent::GatewayEvent(event) => json!({
            "type": "gateway_event",
            "event": gateway_event_to_json(event),
        }),
        GatewayRuntimeEvent::ReconnectScheduled {
            attempt,
            resumable,
            delay,
        } => json!({
            "type": "reconnect_scheduled",
            "attempt": attempt,
            "resumable": resumable,
            "delay_ms": delay.as_millis() as u64,
        }),
        GatewayRuntimeEvent::Shutdown => json!({"type": "shutdown"}),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn discord_ffi_abi_version() -> u32 {
    DISCORD_FFI_ABI_VERSION
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn discord_string_free(value: *mut c_char) {
    if value.is_null() {
        return;
    }

    // SAFETY: Pointer must come from this crate via `CString::into_raw`.
    unsafe {
        drop(CString::from_raw(value));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn discord_client_new(
    base_url: *const c_char,
    out_client: *mut *mut DiscordClientHandle,
    out_error: *mut *mut c_char,
) -> DiscordErrorCode {
    clear_out_string(out_error);

    if out_client.is_null() {
        return fail(
            DiscordErrorCode::DiscordNullPointer,
            out_error,
            "out_client cannot be null",
        );
    }

    // SAFETY: `out_client` is non-null and writable by API contract.
    unsafe {
        *out_client = ptr::null_mut();
    }

    // SAFETY: FFI caller must provide a valid C string.
    let base_url = match unsafe { read_c_string(base_url) } {
        Ok(v) => v,
        Err(DiscordErrorCode::DiscordNullPointer) => {
            return fail(
                DiscordErrorCode::DiscordNullPointer,
                out_error,
                "base_url cannot be null",
            );
        }
        Err(_) => {
            return fail(
                DiscordErrorCode::DiscordInvalidUtf8,
                out_error,
                "base_url must be valid UTF-8",
            );
        }
    };

    let parsed_base_url = match Url::parse(&base_url) {
        Ok(url) => url,
        Err(error) => {
            return fail(
                DiscordErrorCode::DiscordInvalidUrl,
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
                DiscordErrorCode::DiscordRuntimeInitError,
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

    let raw_inner = Box::into_raw(Box::new(DiscordClientInner {
        runtime,
        client,
        gateway_stream: Mutex::new(None),
    }));

    // SAFETY: `out_client` is non-null and writable by API contract.
    unsafe {
        *out_client = raw_inner.cast::<DiscordClientHandle>();
    }

    DiscordErrorCode::DiscordOk
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn discord_client_free(client: *mut DiscordClientHandle) {
    if client.is_null() {
        return;
    }

    // SAFETY: Pointer must come from `discord_client_new` and be freed exactly once.
    unsafe {
        drop(Box::from_raw(client.cast::<DiscordClientInner>()));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn discord_client_set_token(
    client: *mut DiscordClientHandle,
    token: *const c_char,
    out_error: *mut *mut c_char,
) -> DiscordErrorCode {
    clear_out_string(out_error);

    // SAFETY: FFI caller must provide a valid client pointer from this crate.
    let client = match unsafe { client_from_ptr(client) } {
        Ok(v) => v,
        Err(_) => {
            return fail(
                DiscordErrorCode::DiscordNullPointer,
                out_error,
                "client cannot be null",
            );
        }
    };

    // SAFETY: FFI caller must provide a valid C string.
    let token = match unsafe { read_c_string(token) } {
        Ok(v) => v,
        Err(DiscordErrorCode::DiscordNullPointer) => {
            return fail(
                DiscordErrorCode::DiscordNullPointer,
                out_error,
                "token cannot be null",
            );
        }
        Err(_) => {
            return fail(
                DiscordErrorCode::DiscordInvalidUtf8,
                out_error,
                "token must be valid UTF-8",
            );
        }
    };

    match client.runtime.block_on(client.client.set_token(token)) {
        Ok(()) => DiscordErrorCode::DiscordOk,
        Err(error) => fail(
            DiscordErrorCode::DiscordClientError,
            out_error,
            format!("set_token failed: {error}"),
        ),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn discord_client_clear_token(
    client: *mut DiscordClientHandle,
    out_error: *mut *mut c_char,
) -> DiscordErrorCode {
    clear_out_string(out_error);

    // SAFETY: FFI caller must provide a valid client pointer from this crate.
    let client = match unsafe { client_from_ptr(client) } {
        Ok(v) => v,
        Err(_) => {
            return fail(
                DiscordErrorCode::DiscordNullPointer,
                out_error,
                "client cannot be null",
            );
        }
    };

    match client.runtime.block_on(client.client.clear_token()) {
        Ok(()) => DiscordErrorCode::DiscordOk,
        Err(error) => fail(
            DiscordErrorCode::DiscordClientError,
            out_error,
            format!("clear_token failed: {error}"),
        ),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn discord_client_current_user_json(
    client: *mut DiscordClientHandle,
    out_json: *mut *mut c_char,
    out_error: *mut *mut c_char,
) -> DiscordErrorCode {
    clear_out_string(out_error);
    clear_out_string(out_json);

    // SAFETY: FFI caller must provide a valid client pointer from this crate.
    let client = match unsafe { client_from_ptr(client) } {
        Ok(v) => v,
        Err(_) => {
            return fail(
                DiscordErrorCode::DiscordNullPointer,
                out_error,
                "client cannot be null",
            );
        }
    };

    let user = match client.runtime.block_on(client.client.current_user()) {
        Ok(user) => user,
        Err(error) => {
            return fail(
                DiscordErrorCode::DiscordClientError,
                out_error,
                format!("current_user failed: {error}"),
            );
        }
    };

    let user_json = match serde_json::to_string(&user) {
        Ok(payload) => payload,
        Err(error) => {
            return fail(
                DiscordErrorCode::DiscordSerializationError,
                out_error,
                format!("failed to serialize current user: {error}"),
            );
        }
    };

    match write_out_value(out_json, &user_json) {
        Ok(()) => DiscordErrorCode::DiscordOk,
        Err(DiscordErrorCode::DiscordNullPointer) => fail(
            DiscordErrorCode::DiscordNullPointer,
            out_error,
            "out_json cannot be null",
        ),
        Err(_) => fail(
            DiscordErrorCode::DiscordSerializationError,
            out_error,
            "serialized current user could not be encoded as a C string",
        ),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn discord_client_list_guilds_json(
    client: *mut DiscordClientHandle,
    out_json: *mut *mut c_char,
    out_error: *mut *mut c_char,
) -> DiscordErrorCode {
    clear_out_string(out_error);
    clear_out_string(out_json);

    // SAFETY: FFI caller must provide a valid client pointer from this crate.
    let client = match unsafe { client_from_ptr(client) } {
        Ok(v) => v,
        Err(_) => {
            return fail(
                DiscordErrorCode::DiscordNullPointer,
                out_error,
                "client cannot be null",
            );
        }
    };

    let token = match client.runtime.block_on(client.client.token()) {
        Ok(token) => token,
        Err(error) => {
            return fail(
                DiscordErrorCode::DiscordClientError,
                out_error,
                format!("guild list token fetch failed: {error}"),
            );
        }
    };

    let guilds = match client.runtime.block_on(
        client
            .client
            .http()
            .get_current_user_guilds(&token, GetCurrentUserGuildsQuery::default()),
    ) {
        Ok(guilds) => guilds,
        Err(error) => {
            return fail(
                DiscordErrorCode::DiscordClientError,
                out_error,
                format!("list guilds failed: {error}"),
            );
        }
    };

    let guilds_json = match serde_json::to_string(&guilds) {
        Ok(payload) => payload,
        Err(error) => {
            return fail(
                DiscordErrorCode::DiscordSerializationError,
                out_error,
                format!("failed to serialize guild list: {error}"),
            );
        }
    };

    match write_out_value(out_json, &guilds_json) {
        Ok(()) => DiscordErrorCode::DiscordOk,
        Err(DiscordErrorCode::DiscordNullPointer) => fail(
            DiscordErrorCode::DiscordNullPointer,
            out_error,
            "out_json cannot be null",
        ),
        Err(_) => fail(
            DiscordErrorCode::DiscordSerializationError,
            out_error,
            "serialized guild list could not be encoded as a C string",
        ),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn discord_client_list_channels_json(
    client: *mut DiscordClientHandle,
    guild_id: *const c_char,
    out_json: *mut *mut c_char,
    out_error: *mut *mut c_char,
) -> DiscordErrorCode {
    clear_out_string(out_error);
    clear_out_string(out_json);

    // SAFETY: FFI caller must provide a valid client pointer from this crate.
    let client = match unsafe { client_from_ptr(client) } {
        Ok(v) => v,
        Err(_) => {
            return fail(
                DiscordErrorCode::DiscordNullPointer,
                out_error,
                "client cannot be null",
            );
        }
    };

    // SAFETY: FFI caller must provide a valid C string.
    let guild_id = match unsafe { read_c_string(guild_id) } {
        Ok(v) => v,
        Err(DiscordErrorCode::DiscordNullPointer) => {
            return fail(
                DiscordErrorCode::DiscordNullPointer,
                out_error,
                "guild_id cannot be null",
            );
        }
        Err(_) => {
            return fail(
                DiscordErrorCode::DiscordInvalidUtf8,
                out_error,
                "guild_id must be valid UTF-8",
            );
        }
    };

    let token = match client.runtime.block_on(client.client.token()) {
        Ok(token) => token,
        Err(error) => {
            return fail(
                DiscordErrorCode::DiscordClientError,
                out_error,
                format!("channel list token fetch failed: {error}"),
            );
        }
    };

    let channels = match client
        .runtime
        .block_on(client.client.http().get_guild_channels(guild_id, &token))
    {
        Ok(channels) => channels,
        Err(error) => {
            return fail(
                DiscordErrorCode::DiscordClientError,
                out_error,
                format!("list channels failed: {error}"),
            );
        }
    };

    let channels_json = match serde_json::to_string(&channels) {
        Ok(payload) => payload,
        Err(error) => {
            return fail(
                DiscordErrorCode::DiscordSerializationError,
                out_error,
                format!("failed to serialize channel list: {error}"),
            );
        }
    };

    match write_out_value(out_json, &channels_json) {
        Ok(()) => DiscordErrorCode::DiscordOk,
        Err(DiscordErrorCode::DiscordNullPointer) => fail(
            DiscordErrorCode::DiscordNullPointer,
            out_error,
            "out_json cannot be null",
        ),
        Err(_) => fail(
            DiscordErrorCode::DiscordSerializationError,
            out_error,
            "serialized channel list could not be encoded as a C string",
        ),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn discord_client_list_messages_json(
    client: *mut DiscordClientHandle,
    channel_id: *const c_char,
    limit: u8,
    out_json: *mut *mut c_char,
    out_error: *mut *mut c_char,
) -> DiscordErrorCode {
    clear_out_string(out_error);
    clear_out_string(out_json);

    // SAFETY: FFI caller must provide a valid client pointer from this crate.
    let client = match unsafe { client_from_ptr(client) } {
        Ok(v) => v,
        Err(_) => {
            return fail(
                DiscordErrorCode::DiscordNullPointer,
                out_error,
                "client cannot be null",
            );
        }
    };

    // SAFETY: FFI caller must provide a valid C string.
    let channel_id = match unsafe { read_c_string(channel_id) } {
        Ok(v) => v,
        Err(DiscordErrorCode::DiscordNullPointer) => {
            return fail(
                DiscordErrorCode::DiscordNullPointer,
                out_error,
                "channel_id cannot be null",
            );
        }
        Err(_) => {
            return fail(
                DiscordErrorCode::DiscordInvalidUtf8,
                out_error,
                "channel_id must be valid UTF-8",
            );
        }
    };

    let token = match client.runtime.block_on(client.client.token()) {
        Ok(token) => token,
        Err(error) => {
            return fail(
                DiscordErrorCode::DiscordClientError,
                out_error,
                format!("message list token fetch failed: {error}"),
            );
        }
    };

    let query = GetChannelMessagesQuery {
        limit: if limit == 0 { None } else { Some(limit) },
        ..GetChannelMessagesQuery::default()
    };

    let messages = match client.runtime.block_on(
        client
            .client
            .http()
            .get_channel_messages(channel_id, query, &token),
    ) {
        Ok(messages) => messages,
        Err(error) => {
            return fail(
                DiscordErrorCode::DiscordClientError,
                out_error,
                format!("list messages failed: {error}"),
            );
        }
    };

    let messages_json = match serde_json::to_string(&messages) {
        Ok(payload) => payload,
        Err(error) => {
            return fail(
                DiscordErrorCode::DiscordSerializationError,
                out_error,
                format!("failed to serialize message list: {error}"),
            );
        }
    };

    match write_out_value(out_json, &messages_json) {
        Ok(()) => DiscordErrorCode::DiscordOk,
        Err(DiscordErrorCode::DiscordNullPointer) => fail(
            DiscordErrorCode::DiscordNullPointer,
            out_error,
            "out_json cannot be null",
        ),
        Err(_) => fail(
            DiscordErrorCode::DiscordSerializationError,
            out_error,
            "serialized message list could not be encoded as a C string",
        ),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn discord_client_gateway_stream_start(
    client: *mut DiscordClientHandle,
    intents: u64,
    out_error: *mut *mut c_char,
) -> DiscordErrorCode {
    clear_out_string(out_error);

    // SAFETY: FFI caller must provide a valid client pointer from this crate.
    let client = match unsafe { client_from_ptr(client) } {
        Ok(v) => v,
        Err(_) => {
            return fail(
                DiscordErrorCode::DiscordNullPointer,
                out_error,
                "client cannot be null",
            );
        }
    };

    let mut stream_handle_guard = match client.gateway_stream.lock() {
        Ok(guard) => guard,
        Err(_) => {
            return fail(
                DiscordErrorCode::DiscordClientError,
                out_error,
                "gateway stream lock is poisoned",
            );
        }
    };

    if stream_handle_guard.is_some() {
        return fail(
            DiscordErrorCode::DiscordGatewayAlreadyRunning,
            out_error,
            "gateway stream already running",
        );
    }

    let token = match client.runtime.block_on(client.client.token()) {
        Ok(token) => token,
        Err(error) => {
            return fail(
                DiscordErrorCode::DiscordClientError,
                out_error,
                format!("gateway stream token fetch failed: {error}"),
            );
        }
    };

    let gateway_client = match client.runtime.block_on(client.client.gateway_client()) {
        Ok(gateway_client) => gateway_client,
        Err(error) => {
            return fail(
                DiscordErrorCode::DiscordClientError,
                out_error,
                format!("gateway stream client initialization failed: {error}"),
            );
        }
    };

    let state_machine = GatewayStateMachineConfig::new(token, intents);
    let runtime_options = GatewayRuntimeOptions::default();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let (events_tx, events_rx) = mpsc::unbounded_channel::<String>();

    let stream_task = client.runtime.spawn(async move {
        let _ = gateway_client
            .run_with_shutdown(state_machine, runtime_options, shutdown_rx, move |event| {
                let payload = runtime_event_to_json(&event);
                let serialized = serde_json::to_string(&payload).unwrap_or_else(|error| {
                    json!({
                        "type": "serialization_error",
                        "error": format!("failed to serialize gateway runtime event: {error}"),
                    })
                    .to_string()
                });
                let _ = events_tx.send(serialized);
            })
            .await;
    });

    *stream_handle_guard = Some(GatewayStreamHandle {
        shutdown_tx,
        task: stream_task,
        events_rx: Mutex::new(events_rx),
    });

    DiscordErrorCode::DiscordOk
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn discord_client_gateway_stream_stop(
    client: *mut DiscordClientHandle,
    out_error: *mut *mut c_char,
) -> DiscordErrorCode {
    clear_out_string(out_error);

    // SAFETY: FFI caller must provide a valid client pointer from this crate.
    let client = match unsafe { client_from_ptr(client) } {
        Ok(v) => v,
        Err(_) => {
            return fail(
                DiscordErrorCode::DiscordNullPointer,
                out_error,
                "client cannot be null",
            );
        }
    };

    let stream_handle = {
        let mut guard = match client.gateway_stream.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return fail(
                    DiscordErrorCode::DiscordClientError,
                    out_error,
                    "gateway stream lock is poisoned",
                );
            }
        };

        match guard.take() {
            Some(handle) => handle,
            None => {
                return fail(
                    DiscordErrorCode::DiscordGatewayNotRunning,
                    out_error,
                    "gateway stream is not running",
                );
            }
        }
    };

    let _ = stream_handle.shutdown_tx.send(true);
    let _ = client.runtime.block_on(stream_handle.task);

    DiscordErrorCode::DiscordOk
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn discord_client_gateway_stream_next_event_json(
    client: *mut DiscordClientHandle,
    out_json: *mut *mut c_char,
    out_error: *mut *mut c_char,
) -> DiscordErrorCode {
    clear_out_string(out_error);
    clear_out_string(out_json);

    // SAFETY: FFI caller must provide a valid client pointer from this crate.
    let client = match unsafe { client_from_ptr(client) } {
        Ok(v) => v,
        Err(_) => {
            return fail(
                DiscordErrorCode::DiscordNullPointer,
                out_error,
                "client cannot be null",
            );
        }
    };

    let guard = match client.gateway_stream.lock() {
        Ok(guard) => guard,
        Err(_) => {
            return fail(
                DiscordErrorCode::DiscordClientError,
                out_error,
                "gateway stream lock is poisoned",
            );
        }
    };

    let stream_handle = match guard.as_ref() {
        Some(handle) => handle,
        None => {
            return fail(
                DiscordErrorCode::DiscordGatewayNotRunning,
                out_error,
                "gateway stream is not running",
            );
        }
    };

    let mut events_rx = match stream_handle.events_rx.lock() {
        Ok(events_rx) => events_rx,
        Err(_) => {
            return fail(
                DiscordErrorCode::DiscordClientError,
                out_error,
                "gateway events lock is poisoned",
            );
        }
    };

    let next_event = match events_rx.try_recv() {
        Ok(event) => event,
        Err(TryRecvError::Empty) => return DiscordErrorCode::DiscordOk,
        Err(TryRecvError::Disconnected) => {
            return fail(
                DiscordErrorCode::DiscordGatewayNotRunning,
                out_error,
                "gateway stream channel disconnected",
            );
        }
    };

    match write_out_value(out_json, &next_event) {
        Ok(()) => DiscordErrorCode::DiscordOk,
        Err(DiscordErrorCode::DiscordNullPointer) => fail(
            DiscordErrorCode::DiscordNullPointer,
            out_error,
            "out_json cannot be null",
        ),
        Err(_) => fail(
            DiscordErrorCode::DiscordSerializationError,
            out_error,
            "gateway event could not be encoded as a C string",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn take_error(error: *mut c_char) -> String {
        assert!(!error.is_null());
        // SAFETY: `error` is a valid pointer allocated by this crate.
        let text = unsafe { CStr::from_ptr(error).to_str().expect("utf-8 error") }.to_owned();
        // SAFETY: `error` was allocated by this crate.
        unsafe { discord_string_free(error) };
        text
    }

    #[test]
    fn abi_version_is_set() {
        assert_eq!(discord_ffi_abi_version(), DISCORD_FFI_ABI_VERSION);
    }

    #[test]
    fn client_new_set_and_clear_token() {
        let base_url = CString::new("https://discord.com/api/v10").expect("literal");
        let token = CString::new("test-token").expect("literal");
        let mut client: *mut DiscordClientHandle = ptr::null_mut();
        let mut error: *mut c_char = ptr::null_mut();

        // SAFETY: Inputs are valid C pointers produced by Rust.
        let created = unsafe { discord_client_new(base_url.as_ptr(), &mut client, &mut error) };
        assert_eq!(created, DiscordErrorCode::DiscordOk);
        assert!(error.is_null());
        assert!(!client.is_null());

        // SAFETY: Inputs are valid C pointers produced by Rust.
        let set = unsafe { discord_client_set_token(client, token.as_ptr(), &mut error) };
        assert_eq!(set, DiscordErrorCode::DiscordOk);
        assert!(error.is_null());

        // SAFETY: Inputs are valid C pointers produced by Rust.
        let cleared = unsafe { discord_client_clear_token(client, &mut error) };
        assert_eq!(cleared, DiscordErrorCode::DiscordOk);
        assert!(error.is_null());

        // SAFETY: Pointer was allocated by this crate.
        unsafe { discord_client_free(client) };
    }

    #[test]
    fn client_new_rejects_invalid_base_url() {
        let base_url = CString::new("not a url").expect("literal");
        let mut client: *mut DiscordClientHandle = ptr::null_mut();
        let mut error: *mut c_char = ptr::null_mut();

        // SAFETY: Inputs are valid C pointers produced by Rust.
        let result = unsafe { discord_client_new(base_url.as_ptr(), &mut client, &mut error) };
        assert_eq!(result, DiscordErrorCode::DiscordInvalidUrl);
        assert!(client.is_null());

        let error_text = take_error(error);
        assert!(error_text.contains("invalid base_url"));
    }

    #[test]
    fn client_new_requires_out_client_pointer() {
        let base_url = CString::new("https://discord.com/api/v10").expect("literal");
        let mut error: *mut c_char = ptr::null_mut();

        // SAFETY: Inputs are valid C pointers produced by Rust.
        let result = unsafe { discord_client_new(base_url.as_ptr(), ptr::null_mut(), &mut error) };
        assert_eq!(result, DiscordErrorCode::DiscordNullPointer);

        let error_text = take_error(error);
        assert_eq!(error_text, "out_client cannot be null");
    }

    #[test]
    fn set_token_rejects_invalid_utf8() {
        let base_url = CString::new("https://discord.com/api/v10").expect("literal");
        let mut client: *mut DiscordClientHandle = ptr::null_mut();
        let mut error: *mut c_char = ptr::null_mut();
        let invalid = CString::new(vec![0xff]).expect("single byte");

        // SAFETY: Inputs are valid C pointers produced by Rust.
        let created = unsafe { discord_client_new(base_url.as_ptr(), &mut client, &mut error) };
        assert_eq!(created, DiscordErrorCode::DiscordOk);
        assert!(error.is_null());

        // SAFETY: Inputs are valid C pointers produced by Rust.
        let result = unsafe { discord_client_set_token(client, invalid.as_ptr(), &mut error) };
        assert_eq!(result, DiscordErrorCode::DiscordInvalidUtf8);

        let error_text = take_error(error);
        assert_eq!(error_text, "token must be valid UTF-8");

        // SAFETY: Pointer was allocated by this crate.
        unsafe { discord_client_free(client) };
    }

    #[test]
    fn gateway_start_requires_token() {
        let base_url = CString::new("https://discord.com/api/v10").expect("literal");
        let mut client: *mut DiscordClientHandle = ptr::null_mut();
        let mut error: *mut c_char = ptr::null_mut();

        // SAFETY: Inputs are valid C pointers produced by Rust.
        let created = unsafe { discord_client_new(base_url.as_ptr(), &mut client, &mut error) };
        assert_eq!(created, DiscordErrorCode::DiscordOk);

        // SAFETY: Inputs are valid C pointers produced by Rust.
        let started = unsafe { discord_client_gateway_stream_start(client, 513, &mut error) };
        assert_eq!(started, DiscordErrorCode::DiscordClientError);

        let error_text = take_error(error);
        assert!(error_text.contains("gateway stream token fetch failed"));

        // SAFETY: Pointer was allocated by this crate.
        unsafe { discord_client_free(client) };
    }

    #[test]
    fn runtime_event_serialization_contains_event_type() {
        let event = GatewayRuntimeEvent::ReconnectScheduled {
            attempt: 2,
            resumable: true,
            delay: std::time::Duration::from_millis(1200),
        };

        let payload = runtime_event_to_json(&event);
        assert_eq!(payload["type"], "reconnect_scheduled");
        assert_eq!(payload["attempt"], 2);
        assert_eq!(payload["delay_ms"], 1200);
    }
}
