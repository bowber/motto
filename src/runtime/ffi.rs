//! C ABI facade for the motto transport core
//!
//! Exposes a handle-based API that any FFI-capable language can bind to:
//! - TypeScript via napi-rs or node-ffi
//! - Swift via C interop
//! - Kotlin via JNI/JNA
//! - Unity/C# via P/Invoke
//!
//! All functions are `extern "C"` and use opaque handle pointers.
//! Error information is retrieved via `motto_transport_last_error`.
//!
//! Gated behind the `ffi-transport` feature flag.
//!
//! # Thread Safety
//! Each `MottoTransportHandle` owns a dedicated tokio runtime.
//! Functions are safe to call from any thread. The handle must not be
//! shared across threads without external synchronization — create one
//! handle per connection.

#![cfg(feature = "ffi-transport")]
#![allow(private_interfaces)] // Intentional: TransportHandle is opaque behind a raw pointer

use crate::runtime::state::ConnectionState;
use crate::runtime::transport::{TransportConfig, WebSocketClient};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;

/// Opaque handle wrapping a WebSocket client + its own tokio runtime
struct TransportHandle {
    client: WebSocketClient,
    runtime: tokio::runtime::Runtime,
    last_error: Option<CString>,
}

/// Opaque pointer type for FFI consumers
pub type MottoTransportHandle = *mut TransportHandle;

/// Connection state values returned by `motto_transport_state`
#[repr(u8)]
pub enum MottoTransportState {
    Disconnected = 0,
    Connecting = 1,
    Connected = 2,
    Reconnecting = 3,
    Error = 4,
}

impl From<ConnectionState> for MottoTransportState {
    fn from(s: ConnectionState) -> Self {
        match s {
            ConnectionState::Disconnected => MottoTransportState::Disconnected,
            ConnectionState::Connecting => MottoTransportState::Connecting,
            ConnectionState::Connected => MottoTransportState::Connected,
            ConnectionState::Reconnecting => MottoTransportState::Reconnecting,
            ConnectionState::Error => MottoTransportState::Error,
        }
    }
}

/// Callback type for async events (receive, state change)
/// The `data` pointer is only valid for the duration of the callback.
/// `data_len` is the number of bytes pointed to by `data`.
/// `user_data` is the opaque pointer passed to `motto_transport_set_callback`.
pub type MottoTransportCallback = Option<
    unsafe extern "C" fn(data: *const u8, data_len: usize, user_data: *mut std::ffi::c_void),
>;

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

/// Create a new transport handle.
///
/// `url` must be a valid null-terminated C string with a `ws://` or `wss://` scheme.
/// Returns a non-null handle on success, null on failure.
///
/// # Safety
/// `url` must be a valid, null-terminated C string pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn motto_transport_new(url: *const c_char) -> MottoTransportHandle {
    if url.is_null() {
        return ptr::null_mut();
    }

    let url_str = match unsafe { CStr::from_ptr(url) }.to_str() {
        Ok(s) => s.to_owned(),
        Err(_) => return ptr::null_mut(),
    };

    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(_) => return ptr::null_mut(),
    };

    let config = TransportConfig {
        url: url_str,
        ..Default::default()
    };

    let handle = Box::new(TransportHandle {
        client: WebSocketClient::new(config),
        runtime,
        last_error: None,
    });

    Box::into_raw(handle)
}

/// Free a transport handle.
///
/// After calling this, the handle is invalid and must not be used.
///
/// # Safety
/// `handle` must be a valid handle returned by `motto_transport_new`,
/// or null (in which case this is a no-op).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn motto_transport_free(handle: MottoTransportHandle) {
    if !handle.is_null() {
        let mut h = unsafe { Box::from_raw(handle) };
        // Disconnect gracefully before dropping
        h.runtime.block_on(h.client.disconnect());
        // Box drops here, runtime shuts down
    }
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

/// Connect to the server. Blocks until the WebSocket handshake completes or times out.
///
/// Returns 0 on success, -1 on failure (retrieve error with `motto_transport_last_error`).
///
/// # Safety
/// `handle` must be a valid, non-null handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn motto_transport_connect(handle: MottoTransportHandle) -> i32 {
    let h = unsafe { &mut *handle };
    match h.runtime.block_on(h.client.connect()) {
        Ok(()) => {
            h.last_error = None;
            0
        }
        Err(e) => {
            h.last_error = CString::new(e.to_string()).ok();
            -1
        }
    }
}

/// Disconnect from the server. Safe to call even if already disconnected.
///
/// # Safety
/// `handle` must be a valid, non-null handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn motto_transport_close(handle: MottoTransportHandle) {
    let h = unsafe { &mut *handle };
    h.runtime.block_on(h.client.disconnect());
    h.last_error = None;
}

// ---------------------------------------------------------------------------
// Send / Receive
// ---------------------------------------------------------------------------

/// Send a binary message. The first byte must be the protocol version byte.
///
/// Returns 0 on success, -1 on failure.
///
/// # Safety
/// `handle` must be valid. `data` must point to at least `data_len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn motto_transport_send(
    handle: MottoTransportHandle,
    data: *const u8,
    data_len: usize,
) -> i32 {
    let h = unsafe { &mut *handle };
    if data.is_null() || data_len == 0 {
        h.last_error = CString::new("data pointer is null or length is zero").ok();
        return -1;
    }

    let buf = unsafe { std::slice::from_raw_parts(data, data_len) }.to_vec();
    match h.runtime.block_on(h.client.send(buf)) {
        Ok(()) => {
            h.last_error = None;
            0
        }
        Err(e) => {
            h.last_error = CString::new(e.to_string()).ok();
            -1
        }
    }
}

/// Receive a binary message. Blocks until a message arrives or the connection closes.
///
/// On success, writes the data pointer to `*out_data` and the length to `*out_len`,
/// and returns 0. The caller must free the data with `motto_transport_recv_free`.
///
/// On failure, returns -1 and sets `*out_data` to null.
///
/// # Safety
/// `handle`, `out_data`, and `out_len` must be valid, non-null pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn motto_transport_recv(
    handle: MottoTransportHandle,
    out_data: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    let h = unsafe { &mut *handle };
    match h.runtime.block_on(h.client.receive()) {
        Ok(data) => {
            unsafe { *out_len = data.len() };
            let boxed = data.into_boxed_slice();
            unsafe { *out_data = Box::into_raw(boxed) as *mut u8 };
            h.last_error = None;
            0
        }
        Err(e) => {
            unsafe { *out_data = ptr::null_mut() };
            unsafe { *out_len = 0 };
            h.last_error = CString::new(e.to_string()).ok();
            -1
        }
    }
}

/// Free a buffer returned by `motto_transport_recv`.
///
/// # Safety
/// `data` must be a pointer previously returned by `motto_transport_recv`,
/// and `len` must be the corresponding length. Passing any other pointer is UB.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn motto_transport_recv_free(data: *mut u8, len: usize) {
    if !data.is_null() && len > 0 {
        let _ = unsafe { Box::from_raw(std::slice::from_raw_parts_mut(data, len)) };
    }
}

// ---------------------------------------------------------------------------
// State & Error
// ---------------------------------------------------------------------------

/// Get the current connection state.
///
/// # Safety
/// `handle` must be a valid, non-null handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn motto_transport_state(
    handle: MottoTransportHandle,
) -> MottoTransportState {
    let h = unsafe { &mut *handle };
    let state = h.runtime.block_on(h.client.state());
    state.into()
}

/// Get the last error message as a null-terminated C string.
///
/// Returns null if there is no error. The returned pointer is valid until
/// the next API call on this handle.
///
/// # Safety
/// `handle` must be a valid, non-null handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn motto_transport_last_error(handle: MottoTransportHandle) -> *const c_char {
    let h = unsafe { &*handle };
    match &h.last_error {
        Some(err) => err.as_ptr(),
        None => ptr::null(),
    }
}
