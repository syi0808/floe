# Floe Client

## Apple-first development

macOS, iPhone and iPad are the first product targets. Android implementation, parity work
and build/test validation are deferred; existing Android code is not a delivery requirement.

### iPhone and iPad native builds

iOS Xcode builds package the Rust and shared Apple local-model dylibs for the selected
device/simulator architecture; the corresponding Rust target, Xcode 26 SDK and an eligible
Xcode runtime are required. FoundationModels is weak-linked so older supported iOS versions
remain usable without local inference. Generation requires the supported iOS 26 model profile
and an available on-device model; unavailability never triggers a remote-model fallback.

Run `flutter run -d <apple-device> -t tool/mobile_vault_smoke.dart` to directly check
the platform secure-key store, exclusive Vault ownership, and reopening an encrypted Vault.
This uses a separate `mobile-vault-smoke` application-support directory and does not request
Contacts, location, or external provider access. Success prints `MOBILE_VAULT_SMOKE_PASSED`.

iOS uses device-only Keychain items available while unlocked and does not regenerate
a missing key while opening an existing Vault.

Flutter client for Floe's Personal Day experience.

The production UI uses Floe's shared design system: a compact desktop rail becomes a floating bottom navigation at `780px` and below. The time grid, in-flow capture field, context cards, collections, and task detail share the same shell. Empty days retain the same calendar rather than switching to a separate hero layout.

The app bundles Pretendard, Lucide icons, and the reference mascot SVG. `FloeSquircle` uses `figma_squircle` with corner smoothing `0.82`; a zero-width border is truly absent rather than a hairline.

The desktop shell includes icon-only navigation. On macOS, content extends into a transparent, title-free titlebar; standard window controls, resizing, and native window dragging remain available.

## Run on macOS

```sh
flutter pub get
flutter run -d macos
```

Build native FFI and Flutter from the same source snapshot (`cargo build -p floe-ffi`
before native Flutter tests). Pairing uses the schema-2 Connections owner ABI;
protected grant operations use the separate Access owner ABI and load the saved
connection in Rust, not from a Flutter request route. Approved credentials are
persisted and verified before releasing their bounded Rust result.

From the repository root, `tools/validation/check-local-model.sh` runs the Swift
host suite plus current provider-adapter and Inference tests.
`tools/validation/run-local-model-smoke.sh --availability` checks the bundled
FoundationModels transport; supported hosts can also run `--exercise` and
`--exercise-learner`. The smoke example belongs to `floe-app`.

`flutter test integration/local_server_pairing_test.dart` (from `apps/client`)
requires a buildable Go server and the debug FFI dylib. It uses a fresh temporary
profile, final Rust pairing envelopes and memory-only credential persistence,
then checks direct server authorization/revocation. It does not write the shared
server Keychain slot or claim a live protected Access success through that slot.
The separate `cargo test -p floe-provider-adapters --test live_server_access`
test (repository root, macOS with Go) starts another disposable real server,
strictly pairs it using test-owned vault keys, and loads the exact approved
credential through `CurrentSavedConnectionStore::fixed`. It proves protected
Rust Access inspection succeeds, then rejects both fresh and previously prepared
transports after server revocation. This test also runs in the macOS Rust
workspace suite; it never reads or writes the shared saved-connection Keychain slot.
Current validation results and remaining blockers are recorded only in
`docs/refactoring/stage-3/3-d.md`.

To start the local server and macOS client together from the repository root:

```sh
./scripts/run-local.sh
```

Pass Flutter run arguments to target another Apple device, for example
`./scripts/run-local.sh -d <apple-device>`. Stopping either process stops the other.

## Localization

The client defaults to English regardless of the operating system language. App-owned UI strings live in `lib/l10n/app_en.arb`, including accessibility labels, errors, and parameterized/plural messages. User-authored tasks, notes, and imported calendar content are never translated.

Run `flutter gen-l10n` after editing ARB resources; commit the generated `app_localizations*.dart` files with the resources. Add an `app_<locale>.arb` file to support another language and pass the desired locale to `FloeApp(locale: ...)`. Flutter delegates localize built-in controls and dialogs; `intl` formats dates and collection timestamps. The calendar grid intentionally uses 24-hour time. There is no language picker yet.

Native macOS permission descriptions are English in `macos/Runner/Info.plist`; future native translations belong in localized `InfoPlist.strings` files, not Dart ARB resources. Gateway diagnostics also use English, while the UI presents localized recovery messages.

## Preview the product UI

```sh
flutter run -d macos -t lib/main_preview.dart
```

This separate entry point uses an in-memory gateway with September 4 sample content, including overlapping and five-minute events. It uses the **same production widgets**, never opens the production database, and does not persist preview edits. `DayAppearance` supplies optional display metadata (tone, note excerpt, task context) without inventing backend fields. Production uses real items and neutral empty metadata until those fields are connected. Preview event timestamps are explicitly UTC, not simulated IANA timezone conversion.

The Tasks destination retains the working task collection. Open a task to review its detail with the same production widgets used by the app.

### Interaction behavior

- Navigation uses a 180 ms fade/3 px entrance without animating ordinary data updates; reduced motion disables the entrance.
- Icon navigation provides hover, keyboard-focus, tooltips and press feedback. Selecting the current destination also returns from task detail.
- The 24-hour calendar has a 1–12× slider, matching scrollbars, exact duration geometry and overlap lanes. Hour/half-hour guides stay sparse; five-minute events remain accurate, with tooltip and detail access. Zoom and scroll survive navigation.
- Event details use a shared 240 ms entrance / 120 ms exit dialog with backdrop blur, rounded time/source panels and read-only provenance. Reduced motion skips dialog animation. Empty days center their message over the blurred calendar; refreshing keeps the calendar underneath an eight-dot spinner.
- Connect and Settings open a service list; the plain icon/name/description card opens detail, with Back to connections. Counts, status and read-only badges are not shown on list cards.
- Notes supports search, Personal filtering, and Clear filters. New note opens an autofocus editor and saves through the capture/classification gateway, with empty-input prevention, pending protection, and retry feedback. Saving clears filters so the new note is visible; cancelling does not create an item.
- Capture retains the real classification flow, then shows dismissible, screen-reader-announced success feedback only after saving.
- Week/Month and the previous timeline suggestion bubble are not exposed. Unsupported domain actions are not simulated as successful native operations.

Flutter automated tests cover controller loading, data processing and native gateway integration only. Design, layout and interaction are reviewed manually in the preview; widget, geometry and visual-capture regression suites are not maintained.

### Shared button press motion

Use `FloeButton.filled`, `.outlined`, `.text`, or `.icon` for app buttons. Text variants accept an optional `icon` alongside `child`; existing Material `ButtonStyle` values still apply. These wrap the entire Material button in `ScaleTransition` (1 → 0.97 → 1, 120 ms), including its background and border, without animating layout dimensions or rebuilding the child on every animation tick.

Use `FloeLoading.run` for asynchronous UI operations so loading feedback remains visible for at least 500 ms. Set `loading` on `FloeButton` for a size-preserving button spinner, or wrap an existing section in `FloeLoadingOverlay` to block interaction and place feedback over the current layout without inserting or removing content. Set `blockInteraction: false` only when the underlying controls must remain available during background work.

### Design-system catalog

Run `flutter run -d macos -t lib/main_design_system.dart` to inspect shared colors,
button sizes and states, field alignment, and selection controls. Reusable controls use
semantic state colors from `FloeColor`/`FloeStates`, spacing from `FloeSpace`, and
36px compact, 44px standard, or 48px field metrics from `FloeControlSize`. Add new
reusable states to the catalog and its golden before using them in a feature screen.

For custom controls, use `PressableScale(builder: (states) => InkWell(statesController: states, ...))`. The navigation and Floe anchor use this path with a 0.98 scale. Always connect the supplied state controller to the interactive child so disabled states, keyboard activation, and gesture cancellation follow Flutter's native behavior rather than raw pointer events. Reduced motion suppresses scaling. Checkbox, switch, popup-menu, and platform picker interactions retain their native behavior.

This uses Flutter's paint-transform rendering path, not a separate GPU-acceleration switch. No blanket `RepaintBoundary` or raster-cache hints are added; verify raster performance with a profile build on the target device before adding them.

Use `flutter run -d macos -t lib/main_preview.dart` to review design and interaction manually at desktop and narrow window sizes.

### Design feedback mode

Both normal macOS debug runs and the preview entry point include a review overlay. Press
`Command-Shift-F`, choose **Inspect**, and click any rendered element to attach a numbered
comment. Existing pins can be reopened, edited, or deleted. The toolbar copies all feedback
as Markdown or structured JSON. Each pin records a stable selector, key/text/tooltip/semantics,
the Floe component and page path, render-object creator chain, nearby labels, scroll offsets,
local and normalized geometry, viewport details, and a cropped PNG around the selected element.
Screenshots are written under the system temporary `floe-design-feedback` directory and linked
from exports. There is no persistent review button, release builds do not mount the overlay,
and feedback resets with the running process.

Debug builds also keep a privacy-filtered ring buffer of recent application and Agent
diagnostics. Press `Command-Shift-D`, or use the bug icon in the review overlay, to export a
JSON bundle and copy its path. The bundle contains failure types, correlation IDs, timings,
and stack traces, but does not record prompts, model responses, calendar content, or person IDs.

To build the Rust bridge and capture combined Flutter/Rust structured logs in one terminal:

```sh
./scripts/run-agent-debug.sh
```

Run this command from the repository root. Logs are written under `.floe-debug/` and can be
filtered with `FLOE_LOG`, for example `FLOE_LOG=debug ./scripts/run-agent-debug.sh`.

The macOS build compiles `floe-ffi`, embeds `libfloe_ffi.dylib`, and starts a
dedicated FFI isolate. `FfiDayGateway` exchanges versioned JSON envelopes with
the Rust core, which owns all Turso reads and writes. Local data is stored under
the app's Application Support directory.

## Connected Calendar (S1)

Connect → macOS Calendar offers **Connect Calendar**, calendar selection, and manual refresh of
the selected date. Permission is requested only after the connection disclosure.
EventKit requires full OS access even for reads; the approved exception does not
enable external writes in Floe. Use **권한 설정** after denial or revocation.

Rust persists the selection, imported provenance, last successful range/time, and
typed failures. Relaunch displays cached data; refresh explicitly to recollect it.
Switching calendars replaces the previous mirror without touching local items.
This path is macOS-only. Fixture integration tests do not access personal calendars.

The native gateway still supports one selected calendar. All-calendar inventory,
disconnect/cache deletion, recurrence metadata and original-zone formatting require domain/API
work and are not falsely exposed as implemented. The existing task collection and capture
classification flows remain functional rather than being replaced by static demo content.

Historical S1 acceptance snapshots have been removed from the active documentation tree.
Current Apple product-boundary validation belongs to [Stage 3 end-to-end validation](../../docs/refactoring/stage-3/3-f.md).

## Validation commands

In a connected, unlocked conversation, `/focus` requests a 60-minute focus proposal for today.
It requires exactly one reviewed EventKit calendar. Open the resulting proposal to review it;
the command never dispatches a calendar write. Approval and execution use the unlocked encrypted
vault, and an expired source observation requires a fresh proposal rather than replaying an old one.
Other platforms and ambiguous calendar selections return an explicit source/capability error.

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
