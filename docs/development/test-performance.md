# Test performance

This document defines the current Rust test-performance and validation policy.
It is a runbook, not a benchmark archive. Detailed experiments and superseded
measurements belong in Git history.

## Rust validation phases

Select doctests by command, not by Cargo's dev/test/release build profile. Keep
doctests enabled in library manifests even when a crate currently has no runnable
doctests.

| Phase | Command | Scope |
| --- | --- | --- |
| Affected-crate iteration | `cargo test -p <affected-crate> --tests` | Unit/integration tests with normal incremental compilation; no doctests |
| Intermediate workspace check | `CARGO_INCREMENTAL=0 cargo test --workspace --tests --no-fail-fast` | Workspace unit/integration tests; no doctests |
| Completed Rust change / final verification | `CARGO_INCREMENTAL=0 cargo test --workspace --no-fail-fast` | Default Cargo coverage, including doctests and default example compilation checks |

Use `cargo check -p <affected-crate>` when fast type feedback is useful, but do
not make a workspace-wide check a prerequisite for the final workspace test.
Validate an affected example explicitly during iteration when it changes.

Run the final gate once after the covered Rust changes are complete. A successful
result can be reused before a production build only when the covered Rust sources,
tests, manifests, lockfile, generated inputs and validation configuration
(toolchain, target, features, profiles and flags) are unchanged. Apple/FFI/release
build checks and stronger task-specific plans remain separate requirements.

## Current Cargo profile policy

The root `Cargo.toml` is authoritative for profile values.

- Floe dev/test code uses line-table debug information so panic backtraces keep
  file/line context without full variable-level debug data.
- Non-workspace dependencies omit debug information in dev/test builds.
- Non-workspace test dependencies use `opt-level = 1`; normal dev dependencies
  remain unoptimized.
- `profile.debugging` exists for opt-in full Floe debug information. Use
  `cargo test --profile debugging -p <affected-crate> --tests` when required.
- Routine validation does not force a custom linker, codegen-unit count,
  compiler wrapper or alternate target directory.

These settings were retained after repository-local measurement. Other compiler,
linker and cache experiments were not adopted as defaults. The previous nextest
pilot was also not adopted; Cargo is the only maintained Rust test runner and no
nextest repository configuration is kept.

Do not change profiles, `RUSTFLAGS`, features, targets or `CARGO_TARGET_DIR`
merely to run routine validation. Each variation can create another artifact set
and undermine cache reuse.

## Integration-test layout

A Rust file directly under a crate's `tests/` directory normally becomes a
standalone integration-test executable. Where several tests share one dependency
closure and do not require process isolation, Floe may use:

```toml
[package]
autotests = false

[[test]]
name = "integration"
path = "tests/integration.rs"
```

Module files then live under the integration harness rather than becoming separate
Cargo targets. Run a module with:

```sh
cargo test -p <crate> --test integration <module>::
```

Keep a standalone executable when isolation is part of the test semantics, such
as native subprocess, live-server or C-ABI boundaries. Do not consolidate tests
solely to reduce a binary count when doing so weakens isolation or ownership.

## Shared cross-language fixtures

Rust and Flutter tests share immutable compiled Go-server and Swift-calendar
fixtures through one content-addressed builder:

```sh
python3 tools/validation/build_test_fixtures.py server
python3 tools/validation/build_test_fixtures.py calendar
python3 tools/validation/test_test_fixtures.py -v
```

Artifacts live under `target/test-fixtures/`. The cache key includes relevant
source inputs, build flags, toolchain identity and SDK/environment inputs. Reuse
verifies the output checksum, concurrent callers share a per-key file lock, and
failed builds do not publish partial artifacts.

Only compiled immutable artifacts are shared. Server credentials/data, test
databases, profiles and copied native host bundles remain private to each test.
Tests must not mutate cached artifacts.

## Deterministic timing tests

Prefer deterministic synchronization over wall-clock polling in unit tests:

- use paused Tokio time when the behavior under test is logical time,
- use explicit events/conditions/channels for mock readiness and completion,
- keep external-process readiness waits bounded when the external boundary itself
  is part of the test,
- do not add production or FFI hooks solely to eliminate a small bounded wait.

Performance work must not weaken assertions, retries, ignored-test policy,
authorization, recovery semantics or test isolation.

## Measuring performance

Use `--timings` on the same Cargo phase being measured, for example:

```sh
CARGO_INCREMENTAL=0 cargo test --workspace --no-run --timings
```

For before/after comparisons:

1. record the source revision, toolchain, target, features, profile, test scope and
   cache state;
2. distinguish compile/link preparation, first execution and warmed execution;
3. compare equivalent artifact states and avoid one cold run versus one warm run;
4. run commands serially and repeat enough times to expose ordinary variance;
5. exclude failed runs from speed claims while retaining them as reliability
   evidence;
6. measure narrow affected-crate and broad-workspace behavior separately;
7. do not change signing, security or user-data settings merely to improve a
   benchmark.

A newly linked test suite can have materially different first-execution cost from
its warmed execution. Treat that as a separate quantity rather than attributing it
to compiler changes without evidence.

## Documentation policy

This file records current policy and durable tooling only. Dated timing tables,
one-off configurations, rejected alternatives and raw benchmark procedures are
historical evidence and should remain in Git history or ignored local validation
artifacts, not in the active documentation hierarchy.
