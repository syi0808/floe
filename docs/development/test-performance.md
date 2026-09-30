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

### Second pilot measurements (2026-09-30)

These are local measurements of revision `605f16f7`, not a portable performance
guarantee or a replacement for the default final gate. The host was an Apple silicon
M2 Pro Mac14,9 with 10 logical CPUs, 16 GiB RAM, macOS 26.5.2, Rust 1.93.1 and nextest
0.9.146. The workspace ran 1,233 unit/integration tests across 38 nextest binaries,
with two existing ignored tests. The initial comparison rows below passed, but an
additional throughput run failed as described below. The separate workspace
doctest command passed, with zero runnable doctests at this revision.

The experiment kept `CARGO_INCREMENTAL=0`, Cargo profiles, features, target and
`target/` unchanged for workspace comparisons. Builds and benchmarks ran serially.
Cargo and nextest baseline runs were interleaved; priority variants reversed order
on the second repetition. Output went to files, with nextest using
`--status-level all --final-status-level fail`. No cache cleaning, compiler/linker
changes, security exemptions or fixture-isolation changes were made.

| Warm command / scheduling variant | Runs | Wall seconds | Median |
| --- | ---: | --- | ---: |
| Cargo workspace final gate, including doctests | 3 | 66.241, 76.155, 70.851 | 70.851 |
| Nextest default: 8 overall, 4 database | 3 | 49.868, 50.615, 50.205 | 50.205 |
| Nextest: 4 overall, 4 database | 1 | 55.569 | — |
| Nextest: 10 overall, 4 database | 1 | 50.573 | — |
| Nextest: 8 overall, 8 database | 1 | 38.957 | — |
| Nextest: 10 overall, 10 database | 3 | 37.932, 38.545, 37.838 | 37.932 |
| Nextest: 8 overall, 4 database, early cleanup/native priorities | 3 | 51.413, 50.040, 50.218 | 50.218 |
| Nextest: 10 overall, 10 database, early cleanup/native priorities | 3 | 34.257, 32.065, 31.769 | 32.065 |
| Separate Cargo workspace doctest gate | 4 | 19.199, 27.689, 19.717, 24.080 | 21.899 |

Nextest rows exclude doctests; Cargo's final-gate row includes them. Adding the
separate phase medians estimates 72.104 seconds for the default nextest complete
gate versus 53.964 seconds for the tuned variant. These sums are estimates, not
measurements of an uninterrupted command pair. Do not describe the 50-to-32-second
runner improvement as an equivalent improvement to the complete gate. Moreover,
the 32-second configuration did not pass its additional confirmation run.

That additional run took 35.941 seconds and finished with 1,232 passed, one failed
and two skipped. The failure was
`vault_host::tests::native_actions::native_executor_uses_rust_ledger_and_lookup_only_after_response_loss`:
the child host received `Some(Conflict)` instead of `None` during vault initialization
at `crates/app/src/vault_host/tests/native_actions.rs:124`. The benchmark stopped
there rather than retrying it or counting it as a successful complete gate. Three
subsequent isolated runs of that test passed (3.644, 2.894, 2.907 seconds). This
does not establish whether the failure is an existing race, resource contention,
or another cause; do not classify it as a harmless baseline flake.

A follow-up experiment reserved all nextest slots for that native-action test,
then ran the long cleanup test early and retained the other native-group limits.
Every follow-up unit/integration phase passed all 1,233 runnable tests, and each
was immediately followed by a passing doctest phase without retries:

| Follow-up phase | Run 1 | Run 2 | Run 3 | Median |
| --- | ---: | ---: | ---: | ---: |
| Nextest, 10 overall / 10 database, exclusive native action | 37.207 | 37.195 | 34.940 | 37.195 |
| Immediately following doctest gate | 34.250 | 23.540 | 27.988 | 27.988 |
| Sum of consecutive phase wall times | 71.457 | 60.735 | 62.928 | 62.928 |

The consecutive phase sum excludes only trivial Python orchestration between
commands. Its median is about 11% below the earlier Cargo complete-gate median,
not the 36% suggested by comparing nextest's 50-to-32-second phases alone. The
ranges overlap, desktop/security load was not controlled, and three successful
follow-ups do not prove the failure is fixed. Keep this as an experiment, not a
default-gate replacement.

Artifact preparation with `cargo test --workspace --no-run --timings` took 63
seconds using an existing cache, not a clean build. Initial Cargo execution took
152.106 seconds and was excluded from warm statistics. Initial nextest execution,
after Cargo had exercised the binaries and fixtures, took 50.106 seconds and is
not a comparable cold measurement. `syspolicyd` and XProtect were active during
initial execution; this is consistent with possible security-scanning overhead,
not proof of its contribution. The [official macOS guidance](https://nexte.st/docs/installation/macos/)
describes this issue; the experiment did not alter system security settings.

Warm nextest binary-only and full discovery commands took 0.754 and 0.470 seconds
respectively, each including Cargo startup/cache checks. These are single noisy
samples, not a subtraction-based estimate of discovery cost. Workspace Cargo
build checks reported roughly 0.24–0.27 seconds. Reusing binary metadata cannot
explain or remove the tens-of-seconds scheduling bottleneck in this warm case.

The default nextest run reported 888 of 1,233 tests below 50 ms. For the affected
`floe-context` suite (146 tests), warmed Cargo execution took about 0.30 seconds
versus about 0.50 seconds for nextest. The first narrow Cargo run rebuilt its
package-specific dependency feature set (12.81 seconds compiling, 16.632 seconds
total) and was excluded. This comparison also used `CARGO_INCREMENTAL=0` for
consistency; ordinary affected-crate iteration still uses normal incremental Cargo.
Nextest's [process-per-test design](https://nexte.st/docs/design/why-process-per-test/)
is not automatically a speed advantage for tiny tests.

#### Reproducing the tuned experiment

Use a temporary copy, not a second maintained repository configuration:

```sh
mkdir -p target/validation
cp .config/nextest.toml target/validation/nextest-throughput.toml
```

In that copy only, change `database-heavy = { max-threads = 4 }` to
`database-heavy = { max-threads = 10 }`, and append the follow-up settings:

```toml
[[profile.default.overrides]]
filter = 'package(floe-app) & test(=vault_host::tests::native_actions::native_executor_uses_rust_ledger_and_lookup_only_after_response_loss)'
priority = 100
threads-required = 'num-test-threads'

[[profile.default.overrides]]
filter = 'package(floe-vault) & test(=vault::context_cleanup::tests::drain_resumes_after_byte_budget_before_row_budget)'
priority = 90

[[profile.default.overrides]]
filter = 'binary(native_calendar) | package(floe-native) | binary(c_abi)'
priority = 80
```

Run both phases consecutively and time their combined wall clock:

```sh
CARGO_INCREMENTAL=0 cargo nextest run --workspace --no-fail-fast -j 10 \
  --config-file target/validation/nextest-throughput.toml
CARGO_INCREMENTAL=0 cargo test --workspace --doc
```

The [priorities](https://nexte.st/docs/configuration/test-priorities/) start the
exclusive native action first and long cleanup/native pipeline next; they introduce
no test dependencies. Reserving all slots uses
[`threads-required`](https://nexte.st/docs/configuration/threads-required/), not a
timeout, retry or weaker assertion. Native and live-server groups remain capped at one, fixture
locks/private stores remain intact, retries remain zero and ignored tests stay
ignored. Only the database scheduling throttle is widened; native-action scheduling
is more restrictive. For the initial priority experiment, cleanup had priority 100
and the entire existing native filter had priority 90, without exclusive slots.

The evidence supports further opt-in throughput experiments on this host, not a
universal 10-process default or a fix to the native failure. Increasing overall
threads alone did not overcome the shared
four-process database cap; priorities alone did not improve that configuration.
Widening the database cap helped, and priorities then reduced the remaining tail.
Retain Cargo for fast affected suites. Before promoting a tuned policy, repeat on
other Apple hardware with realistic memory/desktop load, measure the complete
doctest-inclusive gate, and check failures and memory pressure as well as speed.
The repository's default nextest configuration and final Cargo policy are unchanged.

Raw local logs, temporary configurations, timing output and one-off measurement
scripts were retained under `target/validation/nextest-study/`; they are ignored
benchmark artifacts, not permanent validation tooling.

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
App unit-test fixtures wait for job completion and blocked mock/key-store signals
with bounded condition variables. Mock TCP listeners use bounded OS readiness
waiting; stopping an observing server wakes its listener explicitly. Job completion
notification exists only under `cfg(test)`; product APIs and cancellation semantics
are unchanged. Tests still obtain results through the admitted query/poll paths.
Real external-process readiness polling remains bounded; synchronous deadline-
crossing serialization tests still exercise real elapsed time.

### Polling fixture comparison (2026-09-30)

The follow-up removed ten Rust test `sleep()` call sites and two busy `yield_now()`
loops. Six sleep sites remain intentionally: real deadline-crossing serialization
and publication, external server startup, FFI observation, worker teardown/reopen,
and the simulated attention-host polling boundary. Removing those would require
changing what is exercised or adding otherwise unnecessary runtime/FFI hooks.

Before and after App test binaries were rebuilt with the same
`CARGO_INCREMENTAL=0 cargo test --workspace --no-run --message-format=json`
command, in the same `target/`, with Rust 1.93.1 and the existing line-table test
profile on the M2 Pro host described above. Cargo's artifact JSON confirmed matching
App profiles/features. Both executables were preserved beside the Cargo binaries,
warmed, and run alternately three times without competing builds. These are direct
executable measurements, excluding compilation and Cargo/doctest startup, not
complete-gate measurements.

| Identical test scope | Before seconds | After seconds | Median before → after |
| --- | --- | --- | --- |
| 292 existing App unit tests | 15.451, 17.811, 15.205 | 15.541, 15.131, 15.124 | 15.451 → 15.131 |
| 34 conversation-flow tests | 4.515, 4.729, 4.422 | 4.601, 4.981, 4.547 | 4.515 → 4.601 |

A further three alternating conversation-flow runs measured child-process user
plus system CPU time: before 28.898, 28.934, 28.563 seconds versus after 28.917,
28.807, 28.730 seconds (medians 28.898 → 28.807). Aggregate CPU time includes all
test threads and is not wall time. Neither this nor the wall results demonstrates
a material speedup: App wall time improved about 2%, while conversation wall time
in the main comparison increased about 2%, within the observed variation.

The 292-test comparison excluded native-action tests, two existing extension-runner
tests sharing one global counter, and the three new synchronization-helper tests
from both binaries. A preliminary before run failed in the native subprocess;
the shared-counter failure was reproduced in the rebuilt before binary in five
independent trials. No assertions, ignored-test policy or final gates were weakened
to make measurements pass. The three new helper tests passed; the affected-crate
run encountered the counter failure; the complete
`CARGO_INCREMENTAL=0 cargo test --workspace --no-fail-fast --timings` gate included
all those tests and passed, as did architecture boundary validation and diff checks.

Only the `compare-verified-*` measurements under `target/validation/polling-study/`
are used in the table. Preliminary comparisons using a stale executable were
discarded; compiler-reported artifact paths and test listings were checked before
the verified comparison. Raw logs and one-off benchmark scripts remain ignored
local artifacts. This change replaces timing guesses with explicit fixture events;
it is not evidence that polling was the dominant test-runtime bottleneck.

For before/after measurements, record toolchain, test count, command, cache state
and concurrency. Separate compilation (`--no-run --timings`), initial discovery,
warm test execution and fixture preparation. Alternate multiple before/after runs
without competing builds. Preserved test binaries can measure pre-change execution
without changing Cargo's cache configuration, but label these as direct execution,
not Cargo CLI timing. A single cold-to-warm comparison is not a code-only speedup.

## Third-stage dependency measurements (2026-09-30)

The dependency-graph experiment compares baseline `64f771c7` with the manifest
cleanup in this change set, on the same M2 Pro / 10 logical CPU / 16 GiB host,
macOS 26.5.2 and Rust 1.93.1. No Rust implementation, test assertion, test-group,
profile, linker, feature, target or fixture-isolation changes were made.

### Changes and retained dependencies

Source and Cargo metadata audits identified 13 unused direct dependency
declarations. Removed dependencies include Builtin's direct Agent Runtime and
unused Conversation/Inference/Kernel/tempfile test wiring, Access's unused Context
test wiring, App's unused Protocol test wiring, Conversation's unused jsonschema,
Day's unused serde_json, Experts' unused chrono/tempfile and FFI's unused
base64/Tokio. Fifteen redundant dev-dependency declarations already supplied by
normal dependencies were also removed.

Five test-only dependencies moved to dev-dependencies: App's base64,
Conversation's chrono, Day's Tokio and Vault's Inference/Tokio. Real heavy
regressions remain: Experts' delegation tests still drive Agent Runtime; provider
authorization tests still use Vault; App's durable/native tests retain their
storage, signing and private fixtures. No new test crate or compatibility path
was introduced. Current ownership is documented in the architecture module map.

The shared Tokio runtime/sync/time/macros, UUID serde/v4/v5, chrono clock/serde,
serde_json raw_value and reqwest JSON/Rustls features have real callers and remain
enabled. Upstream Turso also enables chrono and tracing-subscriber defaults;
removing only Floe's defaults would not remove those workspace features. No
speculative per-crate feature fragmentation was added.

The Apple graph has 11 names with multiple versions: allocator-api2, getrandom,
itertools, rand, rand_core, rustc-hash, rustix, shlex, strum, strum_macros and syn.
These come from still-needed external dependency families, including Turso,
jsonschema, crypto, bindgen and tempfile. They were audited but not forcibly
unified, patched or upgraded as part of an unused-dependency cleanup. Both the
workspace's resolved package set and its duplicate versions remain unchanged.

| Apple package scope, including normal/build/dev edges | Before | After |
| --- | ---: | ---: |
| Entire workspace | 304 | 304 |
| floe-experts-builtin | 104 | 46 |
| floe-access | 44 | 39 |
| Internal production dependency edges | 105 | 103 |

Package counts are unique package/version pairs from `cargo tree --target
aarch64-apple-darwin --edges normal,build,dev`, including workspace packages.
They are not counts of compiler invocations. The default tree's repeated entries
and same-version host/target feature variants are deduplicated for these counts.

### Measurements

All timed Rust commands used `CARGO_INCREMENTAL=0`, the existing `target/`,
unchanged profiles/flags/features and serial execution. External dependency and
Go/Swift fixture caches were reused. No cache cleaning or separate target
directory was used. These are **dependency-warm workspace-source rebuilds**, not
clean/cold builds: immediately before each compile measurement, only the
modification times of all workspace `src/lib.rs` files were touched. File contents
were unchanged; Cargo rebuilt the workspace packages reachable from the command.

Compile commands were `cargo test --workspace --no-run --timings` and
`cargo test -p floe-experts-builtin --no-run --timings`. Execution commands were
the matching `cargo test --workspace --no-fail-fast` and
`cargo test -p floe-experts-builtin --no-fail-fast`, including doctests.
`/usr/bin/time -p` measured whole-command wall time with output redirected to logs.

| Command/scenario | Before wall seconds | After wall seconds | Median before → after |
| --- | --- | --- | --- |
| Workspace source rebuild | 35.01, 34.35, 32.92 | 34.01, 33.40, 33.54 | 34.35 → 33.54 |
| Workspace tests, existing binaries | 79.12, 67.09, 67.18 | 66.05, 66.56, 68.25 | 67.18 → 66.56 |
| Builtin reachable-source rebuild | 9.45, 9.46, 9.35 | 7.44, 7.41, 7.50 | 9.45 → 7.44 |
| Builtin warmed tests | 0.96, 0.97, 0.97 | 0.93, 0.93, 0.96 | 0.97 → 0.93 |

Builtin trials alternated exact baseline/optimized manifest and lockfile snapshots
three times. Each phase first prepared its package-specific dependency feature
set, then timed the source rebuild, exercised the test binaries untimed and timed
the warmed test command. The final optimized manifests/lockfile were restored.
Its compile logs show 15 reachable workspace package compilations before versus
11 after. All six measured test runs passed the same 56 tests across two binaries.

Workspace trials used three consecutive baseline runs followed by three
consecutive optimized runs, not an interleaved comparison. The baseline binaries
were already present and previously exercised; newly fingerprinted optimized
binaries were exercised in a separate 148.73-second preparation run, excluded
from warm results. The first baseline observation was 79.12 seconds and remains
visible rather than silently discarded. Desktop/security load was uncontrolled.
Do not compare these initial/preparation observations as cold-build results.

The reproducible narrow rebuild improvement is about **21%** on this host.
Workspace compile and execution median differences are only about 2.4% and 0.9%,
respectively, with overlapping ranges; they do **not** establish a meaningful
workspace-wide speedup. The full workspace still needs the removed dependency
families through other owners, and the test runtime code is unchanged. The narrow
test difference of 40 ms is also too small to claim a runtime improvement.

Every measured workspace run passed the same 1,233 runnable tests across 38
binaries, with two existing ignored tests, and passed the separate Cargo doctest
phases with zero runnable doctests. Native child-process result lines were not
double-counted. Normal-incremental affected-package tests also passed (971 tests,
two ignored), and the architecture checker passed with 22 nodes, 103 production
edges and no errors/warnings. Existing App dead-code and Vault test-field warnings
remained; assertions, retries and ignored status were not changed.

`flutter build macos --debug` also passed with the optimized manifests and bundled
the same-source Rust core. Xcode reported stale DerivedData file warnings; no
DerivedData cleanup or signing-account change was made. iOS/device execution and
Flutter analyze/full widget tests were not run: this change removes unused Rust
dependency declarations without changing the app wire or runtime implementation.

Raw logs, before/after Cargo timing HTML, dependency-tree snapshots and metadata
are local ignored artifacts under `target/validation/dependency-study/`, not new
permanent validation tooling. Source rebuilds do not establish the size of a
clean-build improvement; cold dependency compilation was not measured.
