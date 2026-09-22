---
name: code-change-verification
description: Use after Floe code, tests, build behavior, public/FFI contracts, native integration, server behavior, persistence semantics or architecture policy changes. Select verification by changed surface and active-plan requirements. Do not invoke for a documentation-only edit unless that edit changes executable scripts, generated artifacts or validation policy that itself needs testing.
---

# Code change verification

Verification is surface-driven. Run narrow checks that give fast feedback first, then the broader gate required by the affected boundary or active execution plan. Do not substitute a small targeted test for a required full gate.

The current active Stage/execution plan may require additional exact commands, repetitions, real-device/provider acceptance or residual searches. Those requirements take precedence.

## 1. Establish changed surfaces

Inspect the diff and classify the change across:

- Rust owner/runtime/contracts,
- architecture/dependency policy,
- FFI/protocol/AppHost boundary,
- Flutter client,
- Apple/native integration,
- Go server,
- persistence/credentials/external side effects.

Do not run Android parity work unless explicitly requested; Apple remains the product priority.

## 2. Fast local checks

Use the nearest package/component tests first when they can identify failures quickly. Then run the broad gate appropriate to the changed surface.

Keep assertions intact. A failing safety/recovery test is not a reason to weaken the test.

## 3. Rust and architecture gate

For Rust workspace or shared-contract/runtime changes, the default broad gate is:

~~~sh
cargo check --workspace --lib
cargo test --workspace --no-fail-fast
python3 tools/architecture/check_boundaries.py
git diff --check
~~~

Use targeted cargo tests before the full workspace gate when useful.

Repository-wide cargo fmt --check has historically contained unrelated baseline drift; do not introduce formatting churn merely to make an unrelated whole-repo format gate clean. Format changed Rust code appropriately and follow any stronger current plan requirement.

Any change to dependency policy must include tools/architecture/check_boundaries.py.

## 4. FFI / App / Flutter boundary

When Rust changes cross the application/FFI boundary or affect Flutter-visible behavior, include:

~~~sh
cargo build -p floe-ffi
cd apps/client
flutter analyze
flutter test
flutter build macos
~~~

If bindings/generated files have a repository-specific generation/check command, run the current command from the nearest README or active execution plan and verify generated output matches the source snapshot.

Do not treat a Rust-only test as proof of a changed Flutter/native contract.

## 5. Apple/native boundary

For macOS/iOS native bridge, EventKit, Contacts, Health, Screen Time, Keychain or platform lifecycle changes:

- run the repository's relevant native validation scripts/tests,
- run the affected Xcode/Swift tests when available,
- run the Flutter macOS build when the product bundle crosses the native bridge,
- record unavailable simulator/device/signing/platform-runtime prerequisites as unverified rather than claiming coverage.

Do not install platforms, modify signing accounts, reset credentials or pair devices unless explicitly authorized.

## 6. Go server

For server changes, from server/ use the repository baseline:

~~~sh
go test -race ./...
go vet ./...
~~~

When credential/keychain integration changes and the environment supports it, also run the current credential-specific validation documented by server/README.md.

Format changed Go files with gofmt.

## 7. Persistence, authority and external side effects

When persistence meaning, credentials, authorization, recovery or external writes change, verification must cover semantic failure cases, not only happy paths.

As applicable verify:

- rejected/foreign identity,
- consent/policy denial,
- provenance and key identity,
- compare-and-swap conflicts,
- reopen/restart behavior,
- durable pre-dispatch intent,
- cancellation direction,
- response-loss/uncertain external-write recovery,
- no duplicate external side effect after recovery,
- no secret/token leakage in debug/error/trace paths.

Use fresh isolated development profiles or test stores. Never delete uncertain external-operation records or unrelated user data as a test setup shortcut.

## 8. Architecture completion checks

For architecture-affecting changes, tests are only one part of the gate. Also confirm:

- one canonical owner/path remains,
- old caller routes are deleted,
- public/FFI surface did not grow without a durable reason,
- migration-only optional state is absent,
- residual searches for removed symbols/branches are clean or explicitly explained,
- current architecture docs match the code.

Use the architecture-change skill's residual-audit section.

## 9. Report evidence, not confidence

Final verification reporting should state:

- exact commands/checks run,
- pass/fail/unavailable result,
- relevant test counts only when actually observed,
- known pre-existing warnings/flakes distinguished from new failures,
- any surface not verified and the concrete reason.

Do not claim "fully validated" when a required platform/provider/device gate was unavailable.
