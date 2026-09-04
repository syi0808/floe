# Floe Client

Flutter client for Floe's Personal Day experience.

The production UI follows the squircle-first HTML reference: a compact desktop rail becomes a floating bottom navigation at `780px` and below. The time grid, in-flow capture field, context cards, collections, and task detail share the same shell. Empty days retain the same calendar rather than switching to a separate hero layout.

The app bundles Pretendard, Lucide icons, and the reference mascot SVG. `FloeSquircle` uses `figma_squircle` with corner smoothing `0.82`; a zero-width border is truly absent rather than a hairline.

The desktop shell now includes the prototype's inset squircle frame and icon-only navigation. On macOS, content extends into a transparent, title-free titlebar; standard window controls, resizing, and native window dragging remain available.

## Run on macOS

```sh
flutter pub get
flutter run -d macos
```

## Compare with the HTML prototype

```sh
flutter run -d macos -t lib/main_preview.dart
```

This separate entry point uses an in-memory gateway with September 4 sample content, including overlapping and five-minute events. It uses the **same production widgets**, never opens the production database, and does not persist preview edits. `DayAppearance` supplies optional display metadata (tone, note excerpt, task context) without inventing backend fields. Production uses real items and neutral empty metadata until those fields are connected. Preview event timestamps are explicitly UTC, not simulated IANA timezone conversion.

The Tasks destination retains the working task collection. Open a task to compare its detail with the HTML Tasks screen. The HTML-only Progress dashboard is not a product destination in this app.

### Interaction parity

- Navigation replays the prototype's 180 ms fade/3 px entrance without animating ordinary data updates; reduced motion disables the entrance.
- Icon navigation provides hover, keyboard-focus, tooltips and press feedback. Selecting the current destination also returns from task detail.
- The 24-hour calendar has a 1–12× slider, matching scrollbars, exact duration geometry and overlap lanes. Hour/half-hour guides stay sparse; five-minute events remain accurate, with tooltip and detail access. Zoom and scroll survive navigation.
- Event details use a shared 240 ms entrance / 120 ms exit dialog with backdrop blur, rounded time/source panels and read-only provenance. Reduced motion skips dialog animation. Empty days center their message over the blurred calendar; refreshing keeps the calendar underneath an eight-dot spinner.
- Connect and Settings open a service list; the plain icon/name/description card opens detail, with Back to connections. Counts, status and read-only badges are not shown on list cards.
- Notes supports search, Personal filtering, and Clear filters. New note opens an autofocus editor and saves through the capture/classification gateway, with empty-input prevention, pending protection, and retry feedback. Saving clears filters so the new note is visible; cancelling does not create an item.
- Capture retains the real classification flow, then shows dismissible, screen-reader-announced success feedback only after saving.
- Week/Month and the previous timeline suggestion bubble are removed, matching the current calendar prototype. Unsupported domain actions are not simulated as successful native operations.

Run `flutter test test/visual_interaction_test.dart test/floe_interaction_test.dart test/widget_test.dart test/visual_capture_test.dart` for interaction and responsive-layout regression coverage.

### Shared button press motion

Use `FloeButton.filled`, `.outlined`, `.text`, or `.icon` for app buttons. Text variants accept an optional `icon` alongside `child`; existing Material `ButtonStyle` values still apply. These wrap the entire Material button in `ScaleTransition` (1 → 0.97 → 1, 120 ms), including its background and border, without animating layout dimensions or rebuilding the child on every animation tick.

For custom controls, use `PressableScale(builder: (states) => InkWell(statesController: states, ...))`. The navigation and Floe anchor use this path with a 0.98 scale. Always connect the supplied state controller to the interactive child so disabled states, keyboard activation, and gesture cancellation follow Flutter's native behavior rather than raw pointer events. Reduced motion suppresses scaling. Checkbox, switch, popup-menu, and platform picker interactions retain their native behavior.

This uses Flutter's paint-transform rendering path, not a separate GPU-acceleration switch. No blanket `RepaintBoundary` or raster-cache hints are added; verify raster performance with a profile build on the target device before adding them.

See `../../docs/design/flutter-visual-parity.md` for the paired screenshots, measured layout contract, and reproduction steps.

```sh
flutter test test/visual_capture_test.dart --dart-define=VISUAL_OUTPUT=build/visual-parity
```

The capture suite loads the bundled fonts and SVG, renders at 1440×1100 and 390×844 at DPR 1, and exports Today, Event Detail, Connections, Notes, and Task Detail. Normal test runs also assert calendar geometry and capture placement without writing images.

The macOS build compiles `floe-ffi`, embeds `libfloe_ffi.dylib`, and starts a
dedicated FFI isolate. `FfiDayGateway` exchanges versioned JSON envelopes with
the Rust core, which owns all Turso reads and writes. Local data is stored under
the app's Application Support directory.

## Connected Calendar (S1)

Connect → macOS Calendar offers **Calendar 연결**, calendar selection, and manual refresh of
the selected date. Permission is requested only after the connection disclosure.
EventKit requires full OS access even for reads; the approved exception does not
enable external writes in Floe. Use **권한 설정** after denial or revocation.

Rust persists the selection, imported provenance, last successful range/time, and
typed failures. Relaunch displays cached data; refresh explicitly to recollect it.
Switching calendars replaces the previous mirror without touching local items.
This path is macOS-only. Fixture integration tests do not access personal calendars.

The native gateway still supports one selected calendar. The prototype's all-calendar inventory,
disconnect/cache deletion, recurrence metadata and original-zone formatting require domain/API
work and are not falsely exposed as implemented. The existing task collection and capture
classification flows remain functional rather than being replaced by static demo content.

Reusable Flutter boundaries and parity rules: [Flutter prototype parity](../../docs/design/flutter-prototype-parity.md).

Live acceptance, timezone/recurrence limitations, and dogfood steps are tracked in
[`docs/validation/s1-calendar.md`](../../docs/validation/s1-calendar.md).

## Validation commands

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
