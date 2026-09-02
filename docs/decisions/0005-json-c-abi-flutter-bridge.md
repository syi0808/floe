# ADR 0005: Connect Flutter to Rust through a versioned JSON/C ABI

- **Date:** 2026-09-03
- **Status:** accepted

## Context

The Personal Day UI and Rust/Turso core were independently usable, but the
Flutter client still depended on `FakeDayGateway`. The integration boundary
must remain coarse-grained, versioned, safe across ownership boundaries, and
must not expose database access to Dart.

## Decision

Use a small C ABI exported by the `floe-ffi` dynamic library. Requests and
responses remain JSON documents defined by `floe-protocol`.

The ABI exposes an opaque core handle, day snapshot loading, command execution,
protocol version discovery, response string release, and handle release. Rust
allocates response strings and Dart always returns them through
`floe_string_free`.

`FfiDayGateway` owns a dedicated Dart isolate. That isolate loads the dynamic
library, owns the native handle, serializes all calls, and releases the handle
on shutdown. The UI isolate communicates only through sendable JSON strings and
Dart domain models.

On macOS, Xcode builds `floe-ffi`, copies `libfloe_ffi.dylib` into the app's
Frameworks directory, gives it an `@rpath` install name, and signs it. Turso data
is stored in the sandboxed Application Support directory under a stable local
Person UUID.

## Consequences

- Rust remains the sole owner of validation, commands, projection, and storage.
- Protocol and snapshot schema versions are checked on both sides.
- UI work does not block on synchronous native storage calls.
- C strings and the opaque handle have explicit, tested ownership rules.
- The current packaging implementation targets macOS. Other platforms need
  platform-specific static or dynamic library packaging without changing the
  JSON protocol.

## Validation

- Rust C ABI tests cover every command variant, typed boundary failures,
  revision conflicts, string release, and reopen persistence.
- Flutter integration tests cover Event, Task, and Note decoding, task reopen,
  deletion, isolate lifecycle, and Turso persistence.
- The release app is checked for the embedded signed library and exported ABI
  symbols.
