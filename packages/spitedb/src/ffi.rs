//! FFI bindings for SpiteDB using C ABI
//!
//! This module provides `extern "C"` functions for use with Bun's bun:ffi module.
//! All functions use simple C types and handle async operations by blocking.

use std::ffi::{c_char, CStr, CString};
use std::ptr;
use std::sync::Arc;

use serde::Deserialize;
use tokio::runtime::Runtime;

use crate::{AppendCommand, CommandId, EventData, SpiteDB, StreamId, StreamRev, Tenant};

/// Opaque handle to a SpiteDB instance with its runtime
pub struct SpiteDbHandle {
    db: Arc<SpiteDB>,
    runtime: Runtime,
}

/// Result of an append operation (C-compatible struct)
#[repr(C)]
pub struct FfiAppendResult {
    /// 1 if successful, 0 if error
    pub success: u8,
    /// First global position assigned
    pub first_pos: u64,
    /// Last global position assigned
    pub last_pos: u64,
    /// First stream revision assigned
    pub first_rev: u64,
    /// Last stream revision assigned
    pub last_rev: u64,
    /// Error message (null if success, caller must free with spitedb_free_string)
    pub error: *mut c_char,
}

impl Default for FfiAppendResult {
    fn default() -> Self {
        Self {
            success: 0,
            first_pos: 0,
            last_pos: 0,
            first_rev: 0,
            last_rev: 0,
            error: ptr::null_mut(),
        }
    }
}

/// Opens a SpiteDB database at the given path.
///
/// Returns a handle that must be freed with `spitedb_close`.
/// Returns null on error.
#[no_mangle]
pub extern "C" fn spitedb_open(path: *const c_char) -> *mut SpiteDbHandle {
    let path = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return ptr::null_mut(),
    };

    let runtime = match Runtime::new() {
        Ok(rt) => rt,
        Err(_) => return ptr::null_mut(),
    };

    let db = match runtime.block_on(SpiteDB::open(path)) {
        Ok(db) => Arc::new(db),
        Err(_) => return ptr::null_mut(),
    };

    Box::into_raw(Box::new(SpiteDbHandle { db, runtime }))
}

/// Closes a SpiteDB handle and frees resources.
#[no_mangle]
pub extern "C" fn spitedb_close(handle: *mut SpiteDbHandle) {
    if !handle.is_null() {
        unsafe {
            drop(Box::from_raw(handle));
        }
    }
}

/// Appends events to a stream using a JSON payload.
///
/// JSON format: `{ "streamId": "...", "commandId": "...", "expectedRev": -1, "events": [...], "tenant": "..." }`
///
/// If there's an error, the error field will be set (caller must free with spitedb_free_string).
#[no_mangle]
pub extern "C" fn spitedb_append_json(
    handle: *mut SpiteDbHandle,
    json_payload: *const c_char,
) -> FfiAppendResult {
    let mut result = FfiAppendResult::default();

    if handle.is_null() {
        result.error = make_c_string("handle is null");
        return result;
    }

    let handle = unsafe { &*handle };

    let payload = match unsafe { CStr::from_ptr(json_payload) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            result.error = make_c_string("invalid UTF-8 in payload");
            return result;
        }
    };

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct AppendRequest {
        stream_id: String,
        command_id: String,
        expected_rev: i64,
        events: Vec<serde_json::Value>,
        tenant: String,
    }

    let req: AppendRequest = match serde_json::from_str(payload) {
        Ok(r) => r,
        Err(e) => {
            result.error = make_c_string(&format!("invalid JSON: {}", e));
            return result;
        }
    };

    if req.events.is_empty() {
        result.error = make_c_string("events array cannot be empty");
        return result;
    }

    let expected = if req.expected_rev < 0 {
        StreamRev::ANY
    } else {
        StreamRev::from_raw(req.expected_rev as u64)
    };

    let event_data: Vec<EventData> = req
        .events
        .into_iter()
        .map(|v| EventData::new(serde_json::to_vec(&v).unwrap_or_default()))
        .collect();

    let command = AppendCommand::new_with_tenant(
        CommandId::new(req.command_id),
        StreamId::new(req.stream_id),
        Tenant::new(req.tenant),
        expected,
        event_data,
    );

    match handle.runtime.block_on(handle.db.append(command)) {
        Ok(append_result) => {
            result.success = 1;
            result.first_pos = append_result.first_pos.as_u64();
            result.last_pos = append_result.last_pos.as_u64();
            result.first_rev = append_result.first_rev.as_u64();
            result.last_rev = append_result.last_rev.as_u64();
        }
        Err(e) => {
            result.error = make_c_string(&format!("append failed: {}", e));
        }
    }

    result
}

/// Appends events to multiple streams atomically using a JSON payload.
///
/// JSON format: `{ "commands": [{ "streamId": "...", "commandId": "...", "expectedRev": -1, "events": [...] }], "tenant": "..." }`
///
/// Returns a JSON string with results. Caller must free with `spitedb_free_string`.
#[no_mangle]
pub extern "C" fn spitedb_append_batch_json(
    handle: *mut SpiteDbHandle,
    json_payload: *const c_char,
) -> *mut c_char {
    if handle.is_null() {
        return make_error_json("handle is null");
    }

    let handle = unsafe { &*handle };

    let payload = match unsafe { CStr::from_ptr(json_payload) }.to_str() {
        Ok(s) => s,
        Err(_) => return make_error_json("invalid UTF-8 in payload"),
    };

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct BatchCommand {
        stream_id: String,
        command_id: String,
        expected_rev: i64,
        events: Vec<serde_json::Value>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct BatchRequest {
        commands: Vec<BatchCommand>,
        tenant: String,
    }

    let req: BatchRequest = match serde_json::from_str(payload) {
        Ok(r) => r,
        Err(e) => return make_error_json(&format!("invalid JSON: {}", e)),
    };

    let tenant = Tenant::new(req.tenant);

    let commands: Vec<AppendCommand> = req
        .commands
        .into_iter()
        .filter(|cmd| !cmd.events.is_empty())
        .map(|cmd| {
            let expected = if cmd.expected_rev < 0 {
                StreamRev::ANY
            } else {
                StreamRev::from_raw(cmd.expected_rev as u64)
            };

            let event_data: Vec<EventData> = cmd
                .events
                .into_iter()
                .map(|v| EventData::new(serde_json::to_vec(&v).unwrap_or_default()))
                .collect();

            AppendCommand::new_with_tenant(
                CommandId::new(cmd.command_id),
                StreamId::new(cmd.stream_id),
                tenant.clone(),
                expected,
                event_data,
            )
        })
        .collect();

    if commands.is_empty() {
        return make_result_json(&[]);
    }

    match handle.runtime.block_on(handle.db.batch_append(commands)) {
        Ok(results) => {
            let result_data: Vec<_> = results
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "firstPos": r.first_pos.as_u64(),
                        "lastPos": r.last_pos.as_u64(),
                        "firstRev": r.first_rev.as_u64(),
                        "lastRev": r.last_rev.as_u64(),
                    })
                })
                .collect();
            make_result_json(&result_data)
        }
        Err(e) => make_error_json(&format!("batch append failed: {}", e)),
    }
}

/// Reads events from a stream.
///
/// Returns a JSON string with events array. Caller must free with `spitedb_free_string`.
#[no_mangle]
pub extern "C" fn spitedb_read_stream(
    handle: *mut SpiteDbHandle,
    stream_id: *const c_char,
    from_rev: u64,
    limit: u32,
    tenant: *const c_char,
) -> *mut c_char {
    if handle.is_null() {
        return make_error_json("handle is null");
    }

    let handle = unsafe { &*handle };

    let stream_id = match unsafe { CStr::from_ptr(stream_id) }.to_str() {
        Ok(s) => s,
        Err(_) => return make_error_json("invalid UTF-8 in stream_id"),
    };

    let tenant = match unsafe { CStr::from_ptr(tenant) }.to_str() {
        Ok(s) => s,
        Err(_) => return make_error_json("invalid UTF-8 in tenant"),
    };

    match handle.runtime.block_on(handle.db.read_stream(
        &StreamId::new(stream_id),
        &Tenant::new(tenant),
        StreamRev::from_raw(from_rev),
        limit as usize,
    )) {
        Ok(events) => {
            let event_data: Vec<_> = events
                .iter()
                .map(|e| {
                    let data: serde_json::Value =
                        serde_json::from_slice(e.data.as_ref()).unwrap_or(serde_json::Value::Null);
                    serde_json::json!({
                        "globalPos": e.global_pos.as_u64(),
                        "streamId": e.stream_id.as_str(),
                        "streamRev": e.stream_rev.as_u64(),
                        "timestampMs": e.timestamp_ms,
                        "data": data,
                    })
                })
                .collect();
            make_result_json(&event_data)
        }
        Err(e) => make_error_json(&format!("read failed: {}", e)),
    }
}

/// Gets the current revision of a stream.
///
/// Returns the revision, or -1 on error.
#[no_mangle]
pub extern "C" fn spitedb_get_revision(
    handle: *mut SpiteDbHandle,
    stream_id: *const c_char,
    tenant: *const c_char,
) -> i64 {
    if handle.is_null() {
        return -1;
    }

    let handle = unsafe { &*handle };

    let stream_id = match unsafe { CStr::from_ptr(stream_id) }.to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };

    let tenant = match unsafe { CStr::from_ptr(tenant) }.to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };

    match handle.runtime.block_on(
        handle
            .db
            .get_stream_revision(&StreamId::new(stream_id), &Tenant::new(tenant)),
    ) {
        Ok(rev) => rev.as_u64() as i64,
        Err(_) => -1,
    }
}

/// Frees a string returned by FFI functions.
#[no_mangle]
pub extern "C" fn spitedb_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            drop(CString::from_raw(s));
        }
    }
}

// Helper functions

fn make_c_string(msg: &str) -> *mut c_char {
    CString::new(msg)
        .map(|s| s.into_raw())
        .unwrap_or(ptr::null_mut())
}

fn make_error_json(msg: &str) -> *mut c_char {
    let json = serde_json::json!({ "error": msg });
    CString::new(json.to_string())
        .map(|s| s.into_raw())
        .unwrap_or(ptr::null_mut())
}

fn make_result_json(data: &[serde_json::Value]) -> *mut c_char {
    let json = serde_json::json!({ "data": data });
    CString::new(json.to_string())
        .map(|s| s.into_raw())
        .unwrap_or(ptr::null_mut())
}
