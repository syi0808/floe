# Floe Client

Flutter client for Floe's Personal Day experience.

The production UI follows the squircle-first HTML reference: a compact desktop rail becomes a floating bottom navigation below `780px`, the calendar uses a real time-grid with a timeline-anchored Floe suggestion, and Tasks, Notes, and Task Detail share the same adaptive shell.

## Run on macOS

```sh
flutter pub get
flutter run -d macos
```

The macOS build compiles `floe-ffi`, embeds `libfloe_ffi.dylib`, and starts a
dedicated FFI isolate. `FfiDayGateway` exchanges versioned JSON envelopes with
the Rust core, which owns all Turso reads and writes. Local data is stored under
the app's Application Support directory.

## Validate

```sh
flutter analyze
flutter test
flutter build macos
```

The native gateway integration test needs a debug library before `flutter test`:

```sh
cd ../..
cargo build -p floe-ffi
cd apps/client
flutter test
```
