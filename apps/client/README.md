# Floe Client

During the active architecture refactor, the [staged verification policy](../../docs/plans/2026-10-02-architecture-refactor.md#8-verification-policy-and-final-evidence)
governs execution. T0 removes the documented legacy suites using static inspection only;
the run and opt-in diagnostic instructions below are not T0 verification commands.
Replacement behavioral tests are deferred until S3, after structural closure.

## Apple-first development

macOS, iPhone and iPad are the first product targets. Android implementation, parity work
and build/test validation are deferred; existing Android code is not a delivery requirement.

### iPhone and iPad native builds

iOS Xcode builds package the Rust and shared Apple local-model dylibs for the selected
device/simulator architecture; the corresponding Rust target, Xcode 26 SDK and an eligible
Xcode runtime are required. FoundationModels is weak-linked so older supported iOS versions
remain usable without local inference. Local generation requires iOS 26 and an available on-device model. The common Rust
planner owns Gateway-primary/local-fallback selection; a denied or failed call cannot
trigger a client-selected fallback. Health additionally requires its independent
source-local privacy transform before any reasoning.

The opt-in `tool/mobile_vault_smoke.dart` OS-keyring diagnostic uses a Release/Profile
client/native build (not the isolated Debug file-key profile) and requires explicit Dart defines:
`FLOE_VAULT_SMOKE_EXERCISE=true`, `FLOE_VAULT_SMOKE_DATABASE`,
`FLOE_VAULT_SMOKE_PERSON_ID` and `FLOE_VAULT_SMOKE_DEVICE_ID`. The database must
already exist at `<application support>/mobile-vault-smoke/people/<Person UUID>/floe.db`,
and that diagnostic root must contain the matching existing `local_device_id`.
No profile, directory or identity is created or reset. By default the Vault must also
exist; `FLOE_VAULT_SMOKE_CREATE_VAULT=true` explicitly permits its first creation only.
The exercise retains exclusive-owner rejection, lock and reopen validation, and never
requests Contacts, Health or external-provider access. Run it only at an authorized
Apple validation gate; it has not been executed during S1 caller migration.

iOS Release/Profile uses device-only Keychain items available while unlocked and does not regenerate
a missing key while opening an existing Vault.

Flutter client for Floe's Personal Day experience.

The production UI uses Floe's shared design system: a compact desktop rail becomes a floating bottom navigation at `780px` and below. The time grid, in-flow capture field, context cards, collections, and task detail share the same shell. Empty days retain the same calendar rather than switching to a separate hero layout.

The app bundles Pretendard, Lucide icons, and the reference mascot SVG. `FloeSquircle` uses `figma_squircle` with corner smoothing `0.82`; a zero-width border is truly absent rather than a hairline.

The desktop shell includes icon-only navigation. On macOS, content extends into a transparent, title-free titlebar; standard window controls, resizing, and native window dragging remain available.

## Run on macOS

Apple builds run separate Rust and Swift-library phases. Cargo remains responsible for
Rust input discovery (including newly added files and build-script inputs); it still runs
on each Xcode build, but unchanged crates are not compiled. Swift compilation and dylib
copy/install-name/signing use content fingerprints under Xcode's `DERIVED_FILE_DIR`.
Unchanged native artifacts are not rewritten on Dart-only or no-change builds. Each
fingerprint includes source content, build scripts, compiler/SDK identity, target/compiler
arguments and signing identity, and verifies the existing output content. Missing or
modified outputs rebuild; failed compilation/signing never commits a fresh fingerprint.
The lightweight phases deliberately retain `alwaysOutOfDate` rather than an incomplete
Rust source list that could silently skip required builds. All bundled dylibs are declared
as Xcode phase outputs. Deleting Xcode derived data simply forces a native rebuild.

```sh
flutter pub get
flutter run -d macos
```

Build native FFI and Flutter from the same source snapshot when the staged production-build
gate permits it. Local Vault, Conversation sessions, Experts, Access,
Knowledge, Connections, Day, Actions and Context use owner-prefixed typed intents
on the existing `command_v2/query_v2` AppWire. Rust derives Person/device authority
from the verified product profile; there is no generic AgentVault/fixture ABI or
unverified host fallback. Pairing, source review and management launch use ordinary Connections intents on
the same AppWire. Flutter receives opaque references and safe owner snapshots;
Gateway credentials, enrollment challenges and HTTP calls stay inside Rust owners
and adapters. A connected pairing is shown only after owner commit and readback.
The former Flutter HTTP client, credential channel and special remote ABIs are gone.

Startup admits one internal installation without a profile-selection screen. Debug builds
use isolated development custody; Release/Profile builds require OS keyring. Rust and
Flutter verify the selected custody profile before any installation is opened.

### Development storage

`./scripts/run-local.sh` from the repository root builds the Debug client and a Gateway
with the explicit `floe_dev` Go tag. The client keeps its own data under
`<application support>/development-storage/client`; the Gateway uses
`<user config>/FloeServerDevelopment` and loopback port `18431`. The default Debug
pairing address matches that port. Existing normal client/Gateway data, identities,
Keychain items and pairings are neither imported nor reset. The new profile initially
needs its own explicit setup/pairing. Calendar/Contacts/Health permissions still apply.

The Agent Vault stays AES-256-GCM encrypted in development; only key custody changes
to private development files. Gateway development credentials likewise use private files,
not OS protection. iOS Debug Contacts handle keys use a separate `FloeDevelopmentNative`
application-support directory. The visible `DEV DATA` banner identifies the Debug profile.
Use synthetic data; file keys and development credentials are not protected like Keychain.
No OS-store error, environment variable or missing key activates a weaker fallback.

Rust development builds select `--no-default-features --features development-storage`.
The default `os-keyring` feature and development feature are mutually exclusive; the
development feature rejects non-debug-assertion compilation. Apple embedding scripts
choose the profile from the Xcode configuration and reject development native custody
in Release/Profile. A `FLOE_CORE_LIBRARY_PATH` override must still match the Flutter build.
The Gateway's ordinary build excludes its development file-store implementation entirely.
`run-local.sh --release` or `--profile` selects ordinary Gateway custody and port `8431`.

Production encryption coverage is an open gate: the Agent Vault is encrypted, but the
host Day/source store and some Gateway private files are not currently covered by that
Vault encryption. OS file permissions are not encryption. See the development-profile
and encryption-coverage section of the [restoration plan](../../docs/plans/2026-10-04-flutter-ux-restoration.md)
before treating a production build as qualified for personal data.

Native Calendar, Attention and Personal pumps keep host-issued registration refs
inside native infrastructure. A separate callback isolate owns one independently acquired
native lane sharing the verified Rust host; the product handle never crosses isolates.
Shutdown disposes registrations before freeing the callback lane and closing the product
core. Repeated close, late callback, startup abandonment and shutdown-during-permission
races are required S3 behavioral coverage; current review is static only. Catalog inspection reads bounded resource metadata only;
Calendar and Contacts catalog calls never request OS permission. Only an explicit
Connections integration start can admit a `request_permission` acquisition. Health
reports that the OS request completed without claiming hidden read permission.
Closing a screen detaches its observer; it does not
cancel a Run or an owner operation. Conversation source-review blocks are explicit
`blocked` / `not_produced` states, and eligible review completion is resumed by the
durable Rust owner without a resolved-card Continue command.

From the repository root, `tools/validation/run-local-model-smoke.sh --availability` checks the bundled
FoundationModels transport; supported hosts can also run `--exercise` and
`--exercise-learner`. The smoke example belongs to `floe-app`.

`tools/validation/run-vault-keyring-smoke.sh --probe` checks access to the macOS
login Keychain without creating a key; `--exercise` uses only a fresh disposable
Vault root and exact validation-owned key cleanup.

The retained [Calendar diagnostics and exact recovery tools](../../tools/validation/calendar/README.md)
cover the production C ABI host and inspection of an explicitly identified disposable
Calendar operation. Any live Calendar access or cleanup requires approval for the exact
target and operation; it never authorizes TCC reset or unrelated Calendar edits.

The legacy client, native-package and Runner test suites have been removed in T0.
Their [behavior ledger](../../docs/plans/t0-client-behavior-ledger.md) preserves the
observed safety, failure and recovery cases for review. Shared JSON fixtures remain
available. The old shared fixture builder was also removed in T0; any replacement
follows the [S3 reconstruction policy](../../docs/development/test-performance.md#shared-cross-language-fixtures).
Private profiles and copied native host bundles must remain isolated. The ledger
does not claim the old behavior passed or that a replacement suite exists.

To start the local server and macOS client together from the repository root:

```sh
./scripts/run-local.sh
```

Pass Flutter run arguments to target another Apple device, for example
`./scripts/run-local.sh -d <apple-device>`. Stopping either process stops the other.

The app supplies `AppWireDayGateway` for Day reads, local mutations and explicit
owner-backed refresh, and `CalendarActionFacade` for the Actions flow. Rust owns
acquisition, authority, durable operation identity and reconciliation. Dart does
not publish source evidence, import mirrors or run a separate acquisition policy.
This is the current S2 source composition; G2 build and S3 behavioral validation
remain separate gates.

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
- Connections and Gateway settings render owner-issued setup, pairing, source and integration snapshots. Resource and processing changes use immutable reviewed references; provider sign-in opens a bounded Gateway management action.
- Notes supports search, Personal filtering, and Clear filters. New note opens an autofocus editor and saves through the capture/classification gateway, with empty-input prevention, pending protection, and retry feedback. Saving clears filters so the new note is visible; cancelling does not create an item.
- Capture retains the real classification flow, then shows dismissible, screen-reader-announced success feedback only after saving.
- Week/Month and the previous timeline suggestion bubble are not exposed. Unsupported domain actions are not simulated as successful native operations.

The product preview remains available for manual design, layout and interaction review.
Legacy controller, native gateway and widget suites were removed in T0; their behavior
is recorded in the client ledger. New behavioral proof follows S2 structural closure.

### Shared button press motion

Use `FloeButton.filled`, `.outlined`, `.text`, or `.icon` for app buttons. Text variants accept an optional `icon` alongside `child`; existing Material `ButtonStyle` values still apply. These wrap the entire Material button in `ScaleTransition` (1 → 0.97 → 1, 120 ms), including its background and border, without animating layout dimensions or rebuilding the child on every animation tick.

Use `FloeLoading.run` for asynchronous UI operations so loading feedback remains visible for at least 500 ms. Set `loading` on `FloeButton` for a size-preserving button spinner, or wrap an existing section in `FloeLoadingOverlay` to block interaction and place feedback over the current layout without inserting or removing content. Set `blockInteraction: false` only when the underlying controls must remain available during background work.

### Design-system catalog

Run `flutter run -d macos -t lib/main_design_system.dart` to inspect shared colors,
button sizes and states, field alignment, and selection controls. Reusable controls use
semantic state colors from `FloeColor`/`FloeStates`, spacing from `FloeSpace`, and
36px compact, 44px standard, or 48px field metrics from `FloeControlSize`. Add new
reusable states to the catalog before using them in a feature screen. New behavioral
tests follow the staged verification policy after structural closure.

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
dedicated product FFI isolate and native callback isolate. `AppWireDayGateway` exchanges admitted Day command/query envelopes with
the Rust core, which owns all Turso reads and writes. Local data is stored under
the app's Application Support directory.

## Connected Calendar

Connections owns reviewed source selection and Observe authority. Catalogs carry
actual bounded EventKit labels and handles; the owner validates selection, source
revision and the current native subject before granting access. Permission requests,
resource configuration and Observe remain separate explicit steps.

Day refresh uses the distinct product Calendar read permit and retains mirror
coverage and resource failures from the owner. Calendar Actions use typed preview,
approval, execution and exact receipt recovery through the Actions owner. An
assistant Observe grant and a product refresh permit are separate authorities.
Legacy raw SourceConnection mutation/query adapters are removed from the running
client.

## Validation commands

Calendar proposal and Action diagnostics must use the current owner-backed Day and
Actions contracts at their authorized gate. No retained diagnostic command is
evidence that the current S2 product flow has passed validation.

Follow the [architecture refactor verification gates](../../docs/plans/2026-10-02-architecture-refactor.md#8-verification-policy-and-final-evidence):

- T0 permits static residual inspection, complete diff review and `git diff --check` only.
  Do not run a formatter, compiler, analyzer, build, test, package resolver or architecture checker.
- At the completed S1 boundary, G1 permits the recorded production compiler/type-check
  phase using the required SDK. It does not permit a Flutter application build or tests.
- After S2 structural closure, G2 requires the full production compilation/build gate,
  including same-snapshot FFI, Flutter and affected Apple targets.
- Only after S2, S3 restores test dependencies and targets for new behavioral tests.
  G3 then includes `flutter analyze`, `flutter test` and final Apple/FFI builds.

The direct `flutter_test` dependency and old native test targets are absent at T0.
Dependency resolution follows its authorized gate; retained lockfile entries are
not evidence of a runnable old or new suite. `flutter_lints` remains in use.
