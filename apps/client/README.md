# Floe Client

Flutter client for Floe's Personal Day experience.

The production UI follows the squircle-first HTML reference: a compact desktop rail becomes a floating bottom navigation at `780px` and below. The time grid, in-flow capture field, context cards, collections, and task detail share the same shell. Empty days retain the same calendar rather than switching to a separate hero layout.

The app bundles Pretendard, Lucide icons, and the reference mascot SVG. `FloeSquircle` uses `figma_squircle` with corner smoothing `0.82`; a zero-width border is truly absent rather than a hairline.

## Run on macOS

```sh
flutter pub get
flutter run -d macos
```

## Compare with the HTML prototype

```sh
flutter run -d macos -t lib/main_preview.dart
```

This separate entry point uses an in-memory gateway with September 3 sample content. It uses the **same production widgets**, never opens the production database, and does not persist preview edits. `DayAppearance` supplies optional display metadata (tone, note excerpt, task context) without inventing backend fields. Production uses real items and neutral empty metadata until those fields are connected.

The Tasks destination retains the working task collection. Open a task to compare its detail with the HTML Tasks screen. The HTML-only Progress dashboard is not a product destination in this app.

See `../../docs/design/flutter-visual-parity.md` for the paired screenshots, measured layout contract, and reproduction steps.

```sh
flutter test test/visual_capture_test.dart --dart-define=VISUAL_OUTPUT=build/visual-parity
```

The capture suite loads the bundled fonts and SVG, renders at 1440×1100 and 390×844 at DPR 1, and exports Today, suggestion, Notes, and Task Detail. Normal test runs also assert calendar geometry and capture placement without writing images.

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
