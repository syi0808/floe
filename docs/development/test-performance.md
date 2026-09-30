# Test performance

The default Rust final gate remains `CARGO_INCREMENTAL=0 cargo test --workspace --no-fail-fast`.
Use normal incremental `cargo test -p <affected-crate>` during iteration. Do not change
profiles, flags, targets or target directories just to run a different test runner.

## Optional nextest pilot

Install a compatible stable `cargo-nextest` using its [official installation guide](https://nexte.st/docs/installation/).
The pilot was validated with nextest 0.9.146 and Rust 1.93.1 on Apple silicon.

```sh
CARGO_INCREMENTAL=0 cargo nextest run --workspace --no-fail-fast
CARGO_INCREMENTAL=0 cargo test --workspace --doc
```

Nextest does not replace the separate doctest gate. `.config/nextest.toml` limits
native and live-server groups to one test process each and database-heavy tests to
four, within eight overall threads. Retries are disabled; existing ignored tests
remain ignored. Native tests retain their child-process and private-bundle isolation.
Group limits are scheduling controls, not substitutes for fixture/process locks.

Do not assume nextest is faster for this workspace. Compare repeated warmed runs
against Cargo before selecting it for a task. Initial binary discovery and macOS
process-launch costs can dominate; record them separately rather than hiding them
inside a warm-run result. Never change system security settings as a benchmark shortcut.

## Shared cross-language fixtures

Rust and Flutter tests use one builder for immutable Go server and Swift calendar
artifacts. Python 3.9+, Go and the selected Xcode/macOS SDK are required.

```sh
python3 tools/validation/build_test_fixtures.py server
python3 tools/validation/build_test_fixtures.py calendar
python3 tools/validation/test_test_fixtures.py -v
```

The builder prints only the artifact's absolute path on stdout. Compiler output is
sent to stderr. Artifacts live under `target/test-fixtures/`; inputs, build flags,
toolchain identity and relevant SDK/environment inputs select a content-addressed
directory. Go dependency discovery includes repository-owned sources and embedded
assets. Each reuse verifies the output checksum. A per-key file lock and temporary
build directory prevent concurrent callers or failed builds from publishing partial
artifacts. Missing or modified outputs rebuild automatically.

Only compiled artifacts are shared. Server credentials/data and copied native host
bundles remain private to each test. Tests must not modify the cached artifact.
Input changes create a new entry; ordinary validation does not delete old caches.

## Timing tests and measurements

Pure Tokio lease-expiry tests use paused time. HTTP cancellation waits for an
explicit request-received signal rather than guessing readiness with a sleep.
Real external-process readiness polling remains bounded; synchronous deadline-
crossing serialization tests still exercise real elapsed time.

For before/after measurements, record toolchain, test count, command, cache state
and concurrency. Separate compilation (`--no-run --timings`), initial discovery,
warm test execution and fixture preparation. Alternate multiple before/after runs
without competing builds. Preserved test binaries can measure pre-change execution
without changing Cargo's cache configuration, but label these as direct execution,
not Cargo CLI timing. A single cold-to-warm comparison is not a code-only speedup.
