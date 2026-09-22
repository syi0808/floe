//! C ABI boundary: raw pointers, JSON envelopes, DTO conversion, memory
//! release and the panic barrier.
//!
//! No product judgment lives here. Every request is parsed into the app's own
//! command and handed to `floe-app`.

mod abi;
mod app_wire;
mod bridge;
mod context_wire;
pub mod conversion;
mod day_wire;
mod diagnostics;
mod remote_wire;

pub use abi::*;
pub use bridge::FloeHandle;

use std::{
    ffi::{CStr, CString, c_char},
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

use floe_protocol::wire::{WireResult, invalid};
use floe_protocol::*;
use serde::Serialize;
use serde_json::Value;

fn c_input<'a>(value: *const c_char, field: &'static str) -> WireResult<&'a str> {
    if value.is_null() {
        return Err(invalid(field, "must not be null"));
    }
    unsafe { CStr::from_ptr(value) }
        .to_str()
        .map_err(|value| invalid(field, value.to_string()))
}

fn c_output(value: impl Serialize) -> *mut c_char {
    let encoded = serde_json::to_string(&value).unwrap_or_else(|_| {
        "{\"schema_version\":1,\"status\":\"error\",\"error\":{\"code\":\"internal\",\"message\":\"response serialization failed\"}}".into()
    });
    CString::new(encoded)
        .expect("JSON cannot contain NUL")
        .into_raw()
}

fn handle<'a>(value: *mut FloeHandle) -> WireResult<&'a FloeHandle> {
    unsafe { value.as_ref() }.ok_or_else(|| invalid("handle", "must not be null"))
}
