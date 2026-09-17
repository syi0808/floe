//! Bundled-dylib driver: OS handle acquisition, symbol lookup, the call itself,
//! the response byte bound and the provider-owned memory release.
//!
//! No business meaning lives here. Callers map [`NativeCallError`] onto their
//! own failure vocabulary.

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeCallError {
    /// The bundled library, or one of its entry points, is not available.
    Unavailable,
    /// The request could not be encoded for the C boundary.
    InvalidRequest,
    /// The call returned no response.
    NoResponse,
    /// The response exceeded the caller's byte bound.
    ResponseTooLarge,
    /// Another call already holds the single-call gate.
    Busy,
}

/// How the bundled library is addressed and called.
pub struct NativeLibrary {
    /// Path relative to the bundle directory, e.g. `Frameworks/libfloe_eventkit.dylib`.
    pub relative_path: &'static str,
    /// Exported entry point returning a NUL-terminated JSON document.
    pub invoke_symbol: &'static CStr,
    /// Exported release for the returned buffer.
    pub release_symbol: &'static CStr,
    /// Number of parent hops from the executable to the bundle directory.
    pub bundle_parents: usize,
}

type InvokeBytes = unsafe extern "C" fn(*const u8, usize) -> *mut c_char;
type InvokeCString = unsafe extern "C" fn(*const c_char) -> *mut c_char;
type Release = unsafe extern "C" fn(*mut c_char);

#[cfg(any(target_os = "macos", target_os = "ios"))]
unsafe extern "C" {
    fn dlopen(path: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> c_int;
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn resolve(
    library: &NativeLibrary,
) -> Result<(*mut c_void, *mut c_void, *mut c_void), NativeCallError> {
    let executable = std::env::current_exe().map_err(|_| NativeCallError::Unavailable)?;
    let mut directory = executable
        .parent()
        .ok_or(NativeCallError::Unavailable)?
        .to_path_buf();
    for _ in 0..library.bundle_parents {
        directory = directory
            .parent()
            .ok_or(NativeCallError::Unavailable)?
            .to_path_buf();
    }
    let path = CString::new(
        directory
            .join(library.relative_path)
            .to_string_lossy()
            .as_bytes(),
    )
    .map_err(|_| NativeCallError::Unavailable)?;
    unsafe {
        let handle = dlopen(path.as_ptr(), 2);
        if handle.is_null() {
            return Err(NativeCallError::Unavailable);
        }
        let invoke = dlsym(handle, library.invoke_symbol.as_ptr());
        let release = dlsym(handle, library.release_symbol.as_ptr());
        if invoke.is_null() || release.is_null() {
            dlclose(handle);
            return Err(NativeCallError::Unavailable);
        }
        Ok((handle, invoke, release))
    }
}

/// A resolved entry point taking a length-delimited request buffer.
pub struct ByteCall {
    functions: OnceLock<Result<(usize, usize), NativeCallError>>,
    library: NativeLibrary,
}

impl ByteCall {
    pub const fn new(library: NativeLibrary) -> Self {
        Self {
            functions: OnceLock::new(),
            library,
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    pub fn call(
        &self,
        _request: &[u8],
        _max_response_bytes: usize,
    ) -> Result<Vec<u8>, NativeCallError> {
        Err(NativeCallError::Unavailable)
    }

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    pub fn call(
        &self,
        request: &[u8],
        max_response_bytes: usize,
    ) -> Result<Vec<u8>, NativeCallError> {
        let (invoke, release) = *self
            .functions
            .get_or_init(|| {
                resolve(&self.library)
                    .map(|(_, invoke, release)| (invoke as usize, release as usize))
            })
            .as_ref()
            .map_err(|failure| *failure)?;
        unsafe {
            let invoke = std::mem::transmute::<usize, InvokeBytes>(invoke);
            let release = std::mem::transmute::<usize, Release>(release);
            let output = invoke(request.as_ptr(), request.len());
            if output.is_null() {
                return Err(NativeCallError::NoResponse);
            }
            let bytes = CStr::from_ptr(output).to_bytes();
            let reply = if bytes.len() <= max_response_bytes {
                Ok(bytes.to_vec())
            } else {
                Err(NativeCallError::ResponseTooLarge)
            };
            release(output);
            reply
        }
    }
}

/// A resolved entry point taking a NUL-terminated request, serialized behind a
/// single-call gate because the provider is not re-entrant.
pub struct GatedStringCall {
    functions: OnceLock<Result<(usize, usize), NativeCallError>>,
    gate: std::sync::Mutex<()>,
    library: NativeLibrary,
}

impl GatedStringCall {
    pub const fn new(library: NativeLibrary) -> Self {
        Self {
            functions: OnceLock::new(),
            gate: std::sync::Mutex::new(()),
            library,
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    pub fn call(
        &self,
        _request: &str,
        _max_response_bytes: Option<usize>,
    ) -> Result<Vec<u8>, NativeCallError> {
        Err(NativeCallError::Unavailable)
    }

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    pub fn call(
        &self,
        request: &str,
        max_response_bytes: Option<usize>,
    ) -> Result<Vec<u8>, NativeCallError> {
        let _guard = self.gate.try_lock().map_err(|_| NativeCallError::Busy)?;
        let input = CString::new(request).map_err(|_| NativeCallError::InvalidRequest)?;
        let (invoke, release) = *self
            .functions
            .get_or_init(|| {
                resolve(&self.library)
                    .map(|(_, invoke, release)| (invoke as usize, release as usize))
            })
            .as_ref()
            .map_err(|failure| *failure)?;
        unsafe {
            let invoke = std::mem::transmute::<usize, InvokeCString>(invoke);
            let release = std::mem::transmute::<usize, Release>(release);
            let output = invoke(input.as_ptr());
            if output.is_null() {
                return Err(NativeCallError::NoResponse);
            }
            let bytes = if let Some(limit) = max_response_bytes {
                let mut length = 0;
                while length <= limit && *output.add(length) != 0 {
                    length += 1;
                }
                if length > limit {
                    release(output);
                    return Err(NativeCallError::ResponseTooLarge);
                }
                std::slice::from_raw_parts(output.cast::<u8>(), length)
            } else {
                CStr::from_ptr(output).to_bytes()
            };
            let reply = bytes.to_vec();
            release(output);
            Ok(reply)
        }
    }
}

/// The macOS/iOS bundle hop count for a library next to the executable.
pub const BUNDLE_SIBLING: usize = 0;
/// The macOS bundle hop count for `Contents/Frameworks` from `Contents/MacOS`.
pub const MACOS_BUNDLE_ROOT: usize = 1;
