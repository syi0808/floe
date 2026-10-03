# T0 App behavior ledger: coverage and support closure

Baseline: `3f4b407f8079d611224cd7adbef121f9e7e75e8e` on `refactor/architecture-20261002`.

## Scope and evidence

All 335 Rust test registrations in 44 App test-containing paths have narrative coverage: 254 in [the primary ledger](t0-app-behavior-ledger.json), 81 in [the coordinator addendum](t0-app-behavior-ledger-addendum.md). The source index supplies immutable file hashes, declaration anchors and stable A001–A335 IDs; primary rows additionally record lexical complete function spans. [Static reconciliation](t0-app-behavior-ledger-audit.json) found no missing or duplicate IDs and no changed source hashes. The full files, including adjacent production code and helpers, were read: 38,973 lines by the App worker and 8,831 lines in the two coordinator files, totaling 47,804. Supplementary helper/manifest/diagnostic reads are separately hashed in [support evidence](t0-app-behavior-ledger-support.json).

This is extraction, not a claim of passing behavior or approval to delete tests. No compilation, build, tests, formatter, architecture checker, diagnostic execution, credentials, reset, push or deployment was performed. Assertions are historical evidence. D denotes durable safety/property, P product hypothesis, O obsolete representation, H harness/support in the primary JSON; the addendum defines its own equivalent D/H/O/S vocabulary. A mixed row preserves safety motivation while retiring obsolete mechanics.

Each primary case records preconditions, concrete inputs/actions, expected legacy observations and edge/failure/target disposition. Table-driven subcases are enumerated within the row. Assertions embedded in helpers are explained in support fields or the helper graph below. Names are not evidence: several tests claiming successful execution, rejected state, post-publication loss or broad mutation coverage actually only exercise narrower branches; those differences are explicitly recorded.

## Registration coverage

| Source | Registrations | Full-source lines | Narrative |
|---|---:|---:|---|
| crates/app/examples/floe_cli.rs | 11 | 958 | primary JSON |
| crates/app/examples/local_model_smoke/manager_guidance.rs | 11 | 1605 | primary JSON |
| crates/app/examples/vault_keyring_smoke.rs | 3 | 282 | primary JSON |
| crates/app/src/connection_observe.rs | 6 | 523 | primary JSON |
| crates/app/src/connection_services.rs | 4 | 1114 | primary JSON |
| crates/app/src/core.rs | 4 | 378 | primary JSON |
| crates/app/src/day_services.rs | 1 | 497 | primary JSON |
| crates/app/src/events.rs | 3 | 228 | primary JSON |
| crates/app/src/first_party_observe.rs | 11 | 492 | primary JSON |
| crates/app/src/host.rs | 3 | 249 | primary JSON |
| crates/app/src/personal_source_spec.rs | 1 | 149 | primary JSON |
| crates/app/src/services.rs | 4 | 547 | primary JSON |
| crates/app/src/turn_request.rs | 1 | 103 | primary JSON |
| crates/app/src/vault_host/calendar_access.rs | 1 | 623 | primary JSON |
| crates/app/src/vault_host/conversation_turn/engine_ports.rs | 1 | 74 | primary JSON |
| crates/app/src/vault_host/conversation_turn/expert_dispatch/stateful_settlement.rs | 1 | 677 | primary JSON |
| crates/app/src/vault_host/conversation_turn/expert_dispatch.rs | 8 | 2427 | primary JSON |
| crates/app/src/vault_host/conversation_turn/expert_host.rs | 19 | 2451 | primary JSON |
| crates/app/src/vault_host/conversation_turn/interaction_publication.rs | 7 | 1223 | primary JSON |
| crates/app/src/vault_host/conversation_turn.rs | 18 | 3555 | primary JSON |
| crates/app/src/vault_host/interaction_owners.rs | 1 | 1364 | primary JSON |
| crates/app/src/vault_host/interaction_resolution.rs | 7 | 2056 | primary JSON |
| crates/app/src/vault_host/learner_worker.rs | 6 | 616 | primary JSON |
| crates/app/src/vault_host/personal_access.rs | 2 | 802 | primary JSON |
| crates/app/src/vault_host/remote_observe.rs | 10 | 1327 | primary JSON |
| crates/app/src/vault_host/review_snapshot.rs | 3 | 922 | primary JSON |
| crates/app/src/vault_host/tests/calendar_connection_observe.rs | 3 | 532 | primary JSON |
| crates/app/src/vault_host/tests/conversation_flows.rs | 34 | 4546 | coordinator addendum |
| crates/app/src/vault_host/tests/expert_actions/inspection.rs | 6 | 334 | primary JSON |
| crates/app/src/vault_host/tests/expert_actions.rs | 23 | 1986 | primary JSON |
| crates/app/src/vault_host/tests/interaction_resolution.rs | 47 | 4285 | coordinator addendum |
| crates/app/src/vault_host/tests/learner_worker.rs | 2 | 174 | primary JSON |
| crates/app/src/vault_host/tests/local_product.rs | 4 | 303 | primary JSON |
| crates/app/src/vault_host/tests/memory_review.rs | 4 | 195 | primary JSON |
| crates/app/src/vault_host/tests/native_actions.rs | 2 | 407 | primary JSON |
| crates/app/src/vault_host/tests/proposals.rs | 3 | 525 | primary JSON |
| crates/app/src/vault_host/tests/registered_runner.rs | 20 | 2788 | primary JSON |
| crates/app/src/vault_host/tests/remote_product.rs | 4 | 245 | primary JSON |
| crates/app/src/vault_host/tests/root_environment.rs | 3 | 466 | primary JSON |
| crates/app/src/vault_host/tests/synchronization.rs | 3 | 101 | primary JSON |
| crates/app/src/vault_host/tests/vault_registry/generic_install.rs | 2 | 121 | primary JSON |
| crates/app/src/vault_host/tests/vault_registry.rs | 12 | 1043 | primary JSON |
| crates/app/src/vault_host.rs | 10 | 4115 | primary JSON |
| crates/app/tests/connected_calendar.rs | 6 | 396 | primary JSON |

## Harness roots, targets and syntax boundaries

- Cargo has no explicit `[[test]]`, `[[example]]`, custom harness, App test feature or doctest-disable target. Cargo auto-discovers `tests/connected_calendar.rs` and three top-level example targets: `floe_cli`, `local_model_smoke`, `vault_keyring_smoke`. Nested example source files are modules, not independent targets.
- `local_model_smoke.rs:11,14` explicitly loads `learner.rs` and `manager_guidance.rs` with `#[path]`. These are live diagnostic modules. Only manager-guidance's cfg(test) block is test infrastructure; the production `run` function follows that block and must remain.
- `lib.rs` gates `vault_host` and several services under Unix. The `vault_host.rs` cfg(test) root loads 18 external files, including the helper-only `expert_evidence.rs` and `expert_registry_host.rs`; `native_actions.rs` is additionally macOS-only. Its real EventKit test is ignored and explicitly authorization-gated. No test execution is implied by capturing that behavior.
- Mixed-file interior test blocks require exact item removal: `services.rs` has production result/event types after its tests; `calendar_access.rs` has all production Observe functions after its test block; `engine_ports.rs` has the production validator after tests; `vault_host.rs` has production `stored_vault_state` after the test module. `expert_host.rs` also has a standalone cfg(test) function before production source/model code.
- Test support outside ordinary modules includes `conversation_turn.rs::conversation_context` and `optional_task_views`; `review_snapshot.rs::NoCaptureSnapshots` and `fixtures`; `stateful_settlement.rs::RejectStatefulSettlement` plus its test-only reexport; `worker.rs::WorkerOperation` and `WorkerResult.events`; `lib.rs` test-only WorkerOperation reexport; `vault_host.rs` test-only Worker::request, Job.finished, constructor initialization and notification, test-only events field emission and imports. These fragments are not whole-file deletion candidates.
- No executable Rust doctest was found across App Rust files. No rstest/proptest/quickcheck/generated registration mechanism or separate test feature was found. Ordinary docs/comments remain production documentation.

## Shared helper consumer graph

| Support node | Actual consumers / behavior | Disposition |
|---|---|---|
| `vault_host.rs::tests::{Keys,TestConnections,wait,perform,perform_vault_lifecycle,remote_caller,saved_remote_connection,read_http_request,write_http_response}` | External Vault-host suites access parent symbols with `super::*`; Keys controls blocked/unavailable reads, perform uses admitted lifecycle/remote identity and exact result release. HTTP helpers bound headers and serve synthetic data. | Test-only, remove only with all external suites and test root; never remove production Worker merely because tests use it. |
| `tests/synchronization.rs::{TestSignal,wait_for_job,accept_before}` | Parent Keys/worker tests, Conversation HTTP/race suites, proposal stop tests and local_product waits. Signal preserves early notification; waits are bounded; listener handles EINTR/WouldBlock. | Test-only closed group after every consumer removal. |
| `tests/expert_evidence.rs::{delegation_message,record_proposal_task,record_proposal_task_with_artifacts}` | Expert Actions, proposals and Vault registry suites. Constructs exact admitted Task, Working CAS, stateful settlement with contributor coverage, then old A2A message projection. | Test-only; durable Task provenance is recorded in consuming cases; old conversion retires. |
| `tests/expert_registry_host.rs::{generic_manifest,TestExpertRegistryHost}` | Expert Actions, proposals, Vault registry and generic_install. Installs generic package, resolves exact Person/package/assignment and increments private completion state; foreign Person exposes no cards. | Test-only, no real product plugin registration. |
| `tests/expert_actions.rs::{Fixture,GovernedFocus,Provider,Keys}` | 23 parent and six inspection tests. Supplies committed exact evidence and simulates provider receipts/key loss; no real external writes. | Remove with inspection child and parent suites only. |
| `tests/vault_registry.rs::Fixture` | Registry suite and generic_install child; stages Task/private-state transactions and synthetic corruption. | Test-only. |
| `review_snapshot.rs::fixtures` / `NoCaptureSnapshots` | Snapshot tests, interaction publication, expert dispatch and deep interaction-resolution suites. Stable native evidence or explicit capture failure. | Test-only shared types; production HostReviewSnapshots is separate. |
| `stateful_settlement.rs::RejectStatefulSettlement` | conversation_turn and expert_dispatch inline suites via test-only reexport. Always denies stateful settlement. | Test-only; production real settlement remains until owner cutover. |
| `registered_runner.rs::example_manifest` | Local registered-runner tests and coordinator's Conversation-flow supplied-extension scenario. | Test-only; captures common package path rather than product package content. |

All helper candidates also have source declarations in the support JSON. That mechanical list does not replace the narrative or pretend to be compiler-resolved name analysis. Generic method names alone are not deletion evidence.

## Manifest dependency reconciliation

| App dev dependency | Actual consumers | T0 disposition |
|---|---|---|
| `base64` | Signed enrollment/remote Observe/interaction fixtures and registered-runner producer pin, all cfg(test) | May remove from App manifest only after those complete test groups retire and static residual confirms no remaining App consumer. |
| `ring` | Same signed synthetic Ed25519 fixtures | Same conditional removal. |
| `turso` | `tests/vault_registry.rs` direct corruption of temporary encrypted registry | Test-only direct dependency; remove after that suite. Production storage is through `floe-vault`, which is a normal dependency and must remain. |
| `tempfile` | Many tests, but also live `vault_keyring_smoke.rs:184` and `local_model_smoke/learner.rs:200` | KEEP for real diagnostics. |
| `keyring-core` | Live macOS keyring smoke entry construction, probe and exact cleanup | KEEP. |
| `zeroize` | Live keyring smoke secret buffer and current manager diagnostic connection-file reader | KEEP; later removal of credential export does not remove keyring's use. |
| Unix `libc` | Test synchronization poll, plus live keyring ownership checks and manager diagnostic O_NOFOLLOW/O_NONBLOCK/effective UID checks | KEEP for retained diagnostics. |
| Apple `apple-native-keyring-store` with `keychain` | Live keyring smoke | KEEP; no platform build run here. |

Do not remove normal production dependencies because a test also imports them. Cargo.lock is generated resolved state and was not regenerated. There is no separate manifest target to remove for the auto-discovered integration test; removing its source is the target removal, after authorization.

## Fixture and diagnostic consumer graph

| Asset / entrypoint | Consumer graph and behavior | Disposition |
|---|---|---|
| `fixtures/manager-guidance/corpus.json` | Live include_str in manager_guidance; 22 cases, five generic capability cards and five synthetic follow-up results. The support JSON records every case's history/catalog/input/shape and semantic rubric. `docs/development/debug-cli.md` describes live use. | KEEP. Shape tests do not establish truth, source correctness or real Expert execution. |
| `fixtures/expert-report/delegation-v1.json` | Rust stateful-settlement test producer/comparison; Dart `apps/client/test/features/conversation/agent_delegation_fixture_test.dart` consumer. Environment opt-in prints normalized fixture rather than comparing. | KEEP until coordinated cross-language consumer retirement/regeneration; App alone cannot declare orphaned. |
| `crates/app/tests/fixtures/LocalModelFixture.swift` | Full source read: exports model invoke/free C ABI; availability returns available; first start Hello, second old delegate call to Schedule, later starts calendar-clear answer; other operations released; returned C buffer must be freed. Repository exact path/name search outside planning found no consumer. | H/O orphaned synthetic native fixture candidate, separately audit before deletion. Do not confuse with retained local-model diagnostic or actual platform model host. |
| `tools/validation/build_test_fixtures.py calendar` → provider `NativeCalendarFixture.swift` → dylib | App native_actions, providers native_calendar tests and Flutter native calendar integration all consume shared builder/fixture. Builder cache/toolchain behavior belongs root-tool ledger. | App test removal alone is not sufficient to remove shared builder/input. Parent coordinates all consumers. No builder or compiler was run. |
| `floe_cli.rs` and `scripts/floe-cli.sh` | Real existing-profile CLI; help/inspect/admit/rejoin/explicit cancel; no profile/key creation on open failure; safe traces. | KEEP diagnostic, remove only captured inline tests in T0; migrate its production calls later. |
| `local_model_smoke.rs` / `learner.rs` | Real opt-in synthetic model/optional-memory/learner/expiry/manager diagnostics. Learner uses disposable encrypted Vault and synthetic key map, queues one review, verifies exact optional expiry and keeps candidate unapproved. | KEEP diagnostics; production duplicate learner model host changes later. No live run performed. |
| `vault_keyring_smoke.rs` | Real macOS probe/exercise/cleanup with exact root/mode/owner/marker checks, encrypted CAS/reopen/key-loss fail-closed and verified exact key absence. Three inline cleanup-path tests are separate. | KEEP; running credential create/delete or cleanup needs its own authorization. |
| Ignored real EventKit response-loss test | Reads explicitly supplied private validation root/calendar.json, creates one real disposable event and records proposal/unknown/recovery JSON. Test ends saying exact external cleanup remains. | Capture only. Never run or reset it as part of T0; no successful cleanup is asserted. |

## Removal gate and target reassessment

The coordinator must authorize a covered removal batch after reviewing narrative completeness and consumer closure. Then remove exact syntactic test items and paired test-only imports/fields/targets, inspect the complete diff and allowed static residuals, and preserve adjacent production. This ledger grants no permission to compile, format, rewrite tests, reset data or run diagnostics now.

High-signal legacy mismatches captured explicitly: paused grants surviving disconnect; active status after source expansion; optional missing live source authority comparison; fake live-equivalence used to resolve uncertain owner operations; exact recipient/profile selection; forced local-only Learner or remote-only Experts; separate approve/execute Action flow; dual action stores; and tests whose names overclaim executed branches. These must not automatically become new S3 acceptance requirements. Durable identity,source permission,provenance,CAS,intent-before-dispatch,cancellation direction and uncertain-write recovery remain mandatory.
