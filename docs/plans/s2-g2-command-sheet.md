# S2 / G2 production build gate

Prepared command sheet only. The coordinator must explicitly declare all S2 source closure complete and supply one exact published commit before either executor runs this gate. Current preparation is not a G2 start or pass. Use that same source commit in both environments. No tests, live diagnostics, profile/key creation or reset, provider/Calendar operations, device registration or deployment.

## Input and evidence

Fetch the designated public commit into the existing validation checkout, preserving unrelated work and the prior gate logs. Record `git rev-parse HEAD`, toolchain/SDK versions, command, working directory, exit code, elapsed time and complete output for every phase. Keep caches and standard Cargo target directories; do not clean or vary optimization/linker flags. Do not make semantic fixes in the validation executor. Return compiler findings together as a coherent batch.

Formatting and resolver changes are permitted at this gate. Return their exact patches and separate lockfile diff. Do not upgrade dependencies intentionally. The verified input is the designated commit plus those recorded formatting/lockfile patches; later coordinator integration must reproduce that tree before reusing a pass. No application launch is required for this build gate.

## Saved cloud: Rust, Go and dependency policy

Use the already prepared official toolchains via `/workspace/.floe-cloud-validation/env.sh` (Rust 1.93.1, Go 1.25.5). Confirm the environment file and versions in this executor before use. Do not mistake the unrelated desktop `/usr/bin/go` application for the Go compiler.

1. At repository root, run `cargo fmt --all` once. S2 spans the Rust workspace. Capture the formatting patch before semantic corrections.
2. Format changed, existing Go production files with `gofmt -w`. Enumerate paths with `git diff --name-only --diff-filter=ACMR -z 229b468fd440e3c0d4b31fa7d0e976806313dcf4 <G2_COMMIT> -- 'server/**/*.go'`; use an argument array/null-delimited consumer, not whitespace splitting. Do not include deleted legacy tests.
3. From repository root: `cargo build --workspace --lib --bins --examples --keep-going`. This builds production libraries, binaries, FFI and the three retained App examples; it does not run the examples or compile restored tests.
4. From `server`: `GOTOOLCHAIN=local go build ./...`. Create the task-local `.g2/artifacts` directory, then capture the tracked executable link with `GOTOOLCHAIN=local go build -o ../.g2/artifacts/floe-server ./cmd/floe-server`. The existing main package is `server/cmd/floe-server/main.go`; the second command reuses the same build cache to retain its output artifact.
5. From repository root: `python3 tools/architecture/check_boundaries.py`, then `git diff --check`.

Do not run `cargo test`, `go test`, `go vet` or the retained smoke programs during G2. Native Apple-only branches are covered by the Mac lane, not inferred from Linux success.

## Mac: Flutter, same-snapshot FFI and Apple application targets

Reuse the existing G1 checkout/toolchain caches when safe, after exact source checkout. Rust 1.93.1, Flutter 3.47.2/Dart 3.13.2 and Xcode 26.2 were previously verified; verify actual current paths/SDKs again. Use supported permission review for SDK/cache writes when needed; never change credentials, signing accounts, security settings or use a denied alternate download route.

1. Format changed existing Dart production/diagnostic paths once with the installed Dart formatter, with analytics suppressed. Select paths from the G1 closure to the designated commit. Do not format retained historical fixtures or regenerate tests. If the installed Apple toolchain includes `swift-format`, format changed Swift production paths once with its documented in-place command and capture that patch; report its absence honestly rather than installing a substitute silently.
2. In `apps/client`, run `flutter pub get` using the existing lockfile and SDK. Capture any lockfile changes; no intentional upgrades or test dependency restoration.
3. Run `flutter analyze --no-pub --no-fatal-infos`. Report errors, warnings and informational style diagnostics separately. The existing informational baseline is not a reason for broad unrelated churn.
4. Run `flutter build macos --debug --no-pub`.
5. Run `flutter build ios --simulator --debug --no-pub` for the host-supported simulator architecture. Do not launch a simulator. Report an actually missing target/SDK and its exact attempted architecture; do not infer runtime availability from a prior CoreSimulatorService error.
6. Run `flutter build ios --debug --no-codesign --no-pub` for the device SDK. No signing-account configuration or device registration. If an existing project requirement blocks unsigned build, report that precise requirement rather than weakening target contents.

The tracked Xcode phases invoke `apps/client/{macos,ios}/build_rust.sh` and `build_native.sh`, which build and embed same-snapshot FFI and Contacts/Health/Calendar/LocalModel/ScreenTime native inputs as configured. Do not replace these phases with stubs or prebuilt old libraries. Confirm the build logs use the designated source paths, record the resulting app and bundled library paths/hashes, and inspect the expected C ABI exports. Codesigning/install-name edits can change embedded bytes; record the actual build phase provenance instead of falsely asserting unmodified source-library byte equality.

Return all attempted target results separately. A successful macOS build does not imply iOS/device/simulator success. Missing prerequisites are blocked coverage, not passes. No app packaging/distribution beyond these local build artifacts, tests or live provider operations.

## Completion rule

G2 passes only after the complete structural source builds and the required dependency/FFI/Apple boundaries are covered. Record an external prerequisite block and have the parent coordinate it; a blocked target is not a pass and does not by itself authorize S3. Any reduced gate requires an explicit changed user instruction. Fix real errors against owner contracts in one batch, then rerun affected phases; do not repeat unchanged passing gates blindly. Only after this boundary may S3 reconstruct the new architecture's behavioral tests.
