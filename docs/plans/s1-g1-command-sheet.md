# G1 compiler gate command sheet

This sheet is not an execution record. Execute only after the coordinator supplies the final coherent S1 snapshot and explicitly opens G1. Use a new task workspace, preserve its caches for later fixes/G2, and do not touch the user's existing checkout or profile.

## Inputs and boundaries

- Input: complete committed snapshot, with supplied commit/tree identity and SHA-256 manifest. No `.git`, credentials, runtime databases or user profile is transferred.
- Installed Mac toolchain previously inventoried: Rust/Cargo 1.93.1; Go 1.25.5; Flutter 3.47.2/Dart 3.13.2; Xcode 26.2, Swift 6.2.3 and SDK 26.2. Verify executable locations/current identity once before the gate; do not assume an executable named `go` is the Go compiler.
- Read AGENTS and relevant repository skill instructions. User gate restrictions override generic instructions to run tests.
- No tests, fixture executables, live model/Calendar probes, profile creation, keyring exercises, `flutter build`, Xcode application/archive, codesign or release/package bundling.
- Dependency resolution needed by compilation is allowed at this completed boundary. Preserve resolved lockfile changes; no unrelated upgrades or `clean`.
- Save each exact command, cwd, toolchain, exit status, elapsed time and full log. Return source/lockfile/format diffs separately from generated cache files. A blocked SDK/import phase is not a pass.

## Formatting and production type checking

Run one coordinated formatting pass: `cargo fmt --all` (the slice spans the workspace), `gofmt -w` over the supplied changed Go production paths, and `dart format` over supplied changed Dart production/diagnostic paths. Do not format historical planning/evidence or reconstruct test files. Record formatter diffs.

From repository root:

```
cargo check --workspace --lib --bins
cargo check -p floe-app --example floe_cli --example local_model_smoke --example vault_keyring_smoke
```

From `server`:

```
GOTOOLCHAIN=local go build ./internal/...
```

The Go `cmd` executable link and `go build ./...` remain G2. No `go test`/`go vet` is part of G1.

From `apps/client`, resolve exact existing Flutter dependencies if `.dart_tool/package_config.json` is missing, preserving `pubspec.lock`. Then run `dart analyze` against production `lib` and retained diagnostic `tool` paths. Label this as static analysis, not a Flutter application build. Do not run `flutter test` or a diagnostic program.

## Apple compiler-only phase

Resolve each SDK through `xcrun --sdk <sdk> --show-sdk-path`. Place module caches and emitted `.swiftmodule` files under this task's `.g1/swift/<target>` directory. Record the resolved command before executing it.

Existing source and language/target settings:

| Source group | Language / target |
| --- | --- |
| `apps/client/apple/FloeAppleContacts/Sources/FloeAppleContacts/*.swift` | Swift 5, macOS arm64 minimum 12.0; iOS arm64 minimum 16.0 |
| `apps/client/apple/FloeAppleHealth/Sources/FloeAppleHealth/*.swift` | Swift 5, macOS arm64 minimum 13.0; iOS arm64 minimum 16.0 |
| `apps/client/ios/ScreenTimeGate/Sources/FloeScreenTimeGate/*.swift` | Read the package's exact Swift mode; iOS arm64 minimum 16.0 |
| `apps/client/macos/CalendarActions/EventKitActions.swift` | Existing native build Swift mode, macOS arm64 minimum 12.0, warnings as errors |
| `apps/client/macos/LocalModel/LocalModel.swift` plus `apps/client/apple/FloeAppleHealth/Sources/FloeAppleHealth/HealthPrivacyTransform.swift` | Swift 6, warnings as errors; macOS arm64 minimum 12.0 and iOS arm64 minimum 16.0 |
| `apps/client/macos/Runner/*.swift` and actual generated plugin registration inputs | Swift 5, macOS arm64 minimum 12.0; actual installed FlutterMacOS framework/module paths |
| `apps/client/ios/Runner/*.swift` and actual generated plugin registration inputs | Swift 5, iOS arm64 minimum 16.0; actual Flutter module, Contacts/Health/ScreenTime modules and existing bridging-header inputs |

Use `swiftc -typecheck` for standalone source groups. Where runner imports require a local package module, use compiler-only `swiftc -emit-module -parse-as-library -module-name <exact name> ... -emit-module-path <task module path>` with that same target/SDK, then supply `-I` to runner type checking. This does not link or package an application. Do not fabricate Flutter/plugin shims or omit source files merely to make type checking pass. If the full runner compiler phase requires unavailable generated imports, record the exact unresolved input and check the independent native packages; coordinate the remaining compiler phase rather than substituting a full app build.

Do not install missing x86 simulator targets or launch a simulator at G1. Full configured macOS/iOS/simulator build coverage is G2.

## Result interpretation

G1 is first-vertical compilation only. Complete Day refresh/multi-source mirror, Actions owner/UI convergence and the remaining S2 owners are still pending. No behavior is proven until the new architecture is closed and S3 tests are reconstructed. Fix genuine compiler errors against owner contracts, preserve file ownership, and rerun affected compiler phases only after a coherent correction batch.
