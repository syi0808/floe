# 00: executable baseline and contract freeze

Prerequisite: none. This checkpoint executes the baseline/reproduction work described in [the plan](README.md) before any production fix. It must leave the repository on a green, reviewable state and must not silently implement 01-04.

At plan authoring time, `main` is `c90c5d19819553eeb83d0d2fc9aae4b7cb10a1ca`. The last source-bearing baseline before the plan bundle is `43005508338d7ae38d3247910361c733d7cdfe98`; `c90c5d...` changes plan documentation only. The agent executing this checkpoint must re-read actual HEAD instead of assuming either SHA is still current.

## Purpose and exit state

00 exists to replace static findings with executable evidence and to remove design ambiguity before implementation starts.

It closes only when all of the following are true:

1. current HEAD, worktree state, source delta from the source baseline, dependency policy and available toolchain prerequisites are recorded;
2. R1, R2 and R3 from [01](01-contract-regressions.md) have each been reproduced by a temporary positive regression test through the real owner path, or the finding has been corrected with contrary executable evidence;
3. the exact owner/serialization decisions needed by 01-04 are written into those checkpoint documents so later agents do not invent incompatible contracts independently;
4. touched old tests are classified as delete, move/rewrite or retain at their eventual owner;
5. temporary red reproducer hunks are not committed to `main` by an 00-only task;
6. the plan status row records the actual 00 evidence commit and 01 remains not started.

No production behavior, public API, wire schema, persisted schema or authority policy is intentionally changed in 00. If executable investigation proves a production change is necessary, record it for the owning checkpoint rather than landing it early.

## Current source anchors to recheck

Line numbers below describe `c90c5d...` only and are orientation aids, not stable instructions.

| Finding / decision | Current anchor | Evidence visible at plan authoring |
|---|---|---|
| R1 candidate classification | `crates/modules/context/src/application/remote_sources.rs` around `classify_remote_sources` | Candidate filtering checks Person/revocation/connector compatibility, then treats two grants with equal `GrantSourceBinding` as `Conflict` before exact View/resource matching. |
| R1 fixture topology | same file, inline `remote_view_tests::ViewFixture` | `sources: HashMap<String, SourceFixture>` plus `add_source(connection_id, ...)` encodes one grant per connection, so the product-shaped multi-View state is absent. |
| R1 connector compatibility | `crates/modules/context/src/application/remote_views.rs::remote_view_connector_admissible` | `MAIL_VIEW` is bounded, while `WORK_VIEW | LOGISTICS_VIEW => true`. |
| R2 Manager consumer | `crates/modules/context/src/application/tools.rs::ContextToolService` | Manager remote reads construct `GrantConsumer::builtin(ASSISTANT_CONSUMER)`; `ASSISTANT_CONSUMER == "assistant"`. |
| R2 reviewed product policy | `crates/app/src/first_party_observe.rs::builtin_consumers`, `remote_policies` | Remote Observe policies derive consumers only from built-in Expert readers. Existing test `policy_never_default_grants_extensions_or_assistant_wildcards` explicitly rejects `assistant`. |
| R2 reviewed target | `crates/modules/conversation/src/domain/interaction.rs::{ReviewedBundleMember, InlineObserveTarget, canonical_target_digest}` and App review snapshot/publication | A member binds source revision, expected grant and policy authority. The target carries only the initiating `consumer` string; it does not directly encode the full consumer/policy set that an absent grant would be created with. |
| R3 Rust producer | `crates/experts/builtin/src/schedule/expert.rs::judge` | A successful Schedule draft sets `model_calls: 1` and forwards the actual `view_calls`. |
| R3 Rust validator | `crates/modules/experts/src/registry.rs::validate_result_content` | Current Rust accepts `view_calls` in `1..=8`, `model_calls <= 10`, and requires summary iff `model_calls > 0`; summary-only content is allowed. |
| R3 Dart consumer | `apps/client/lib/features/experts/domain/agent_expert_result.dart::tryParse` | Current parser requires `view_calls == 1`, rejects `model_calls == 1`, requires summary only for `model_calls >= 2`, and rejects empty insights. |
| R3 stale test fixture | `apps/client/test/support/expert_result.dart`, `agent_expert_result_test.dart` | Hand-written fixture uses `model_calls: 2`, `view_calls: 1`; tests classify `view_calls: 2`, one model call and empty insights as invalid. |

If any anchor no longer has this meaning on the execution HEAD, update the finding before reproducing it.

## 00-A: freeze actual repository and toolchain baseline

Do not stash, reset, clean or overwrite user work.

Run and record:

```sh
git status --short --branch
git rev-parse HEAD
git log -1 --oneline
git diff --name-status 43005508338d7ae38d3247910361c733d7cdfe98...HEAD
git diff --stat 43005508338d7ae38d3247910361c733d7cdfe98...HEAD
cargo metadata --no-deps --format-version 1 >/tmp/floe-cargo-metadata.json
python3 tools/architecture/check_boundaries.py
```

Then inspect, not recursively:

- root `AGENTS.md`;
- `.agents/skills/architecture-change/SKILL.md`;
- this plan and [01](01-contract-regressions.md);
- `docs/architecture/{README.md,invariants.md,runtime.md,authority-recovery.md}`;
- `tools/architecture/module-dependencies.json`;
- the manifests for Context, App, Experts, built-in Experts and any crate whose edge is under discussion.

Record whether source files changed after `4300550...`. A documentation-only delta does not require re-baselining the source findings; any code/manifests change affecting an anchor does.

Capture the available versions needed to interpret later results:

```sh
rustc --version
cargo --version
flutter --version
```

On macOS, also record `xcodebuild -version` if Flutter reports an Apple toolchain dependency. Do not install SDKs, change signing, reset profiles or modify external accounts.

Before temporary repro patches, run the existing targeted green baseline:

```sh
cargo test -p floe-context remote_view_tests
cargo test -p floe-app first_party_observe::tests
cargo test -p floe-app gmail_views_allow_enables_bundle_atomically_and_resolves
cargo test -p floe-app schedule_settlement_binds_evidence_to_the_exact_grant_policy_dependency
(
  cd apps/client
  flutter test test/features/experts/agent_expert_result_test.dart
)
```

Record exact pass/fail/unavailable output. A pre-existing failure is baseline evidence, not permission to weaken the test.

## 00-B: reproduce R1 without fixing classification

The reproduction must exercise `read_remote_view`, not only call `classify_remote_sources` directly.

### Temporary fixture change

Inside `crates/modules/context/src/application/remote_sources.rs::remote_view_tests`, temporarily make `ViewFixture` capable of representing more than one grant/resource for the same connection. A `Vec<SourceFixture>`, a grant-id/resource keyed map, or another bounded test-only representation is acceptable. Do not change production `RemoteGrantStore`, Access types or classification logic in 00.

The test transport/store lookup must still select the exact grant/binding/resource used by the production read path. Do not collapse two product grants back into one synthetic aggregate grant.

### R1.1 multi-View grant collision

Add a temporary positive regression test with an explicit name such as:

`same_connection_mail_and_logistics_grants_are_not_duplicate_authority`

Arrange one Gmail `GrantSourceBinding` / connection with two active grants:

- `mail.communication:<connection>`;
- `life.logistics:<connection>`.

Both grants should admit the chosen test consumer for their own resource. Invoke `read_remote_view(..., MAIL_VIEW, ...)`.

Desired assertion:

- outcome is `Ready`;
- only the mail grant is acquired;
- transport read count is one;
- logistics grant contributes neither payload nor blocker.

Expected current red evidence at `c90c5d...`: `AgentFailure::Conflict` before provider I/O because duplicate detection compares the shared source binding before resource filtering.

Run:

```sh
cargo test -p floe-context same_connection_mail_and_logistics_grants_are_not_duplicate_authority -- --nocapture
```

Record the actual observed result rather than copying the expectation above.

### R1.2 wildcard compatibility admits an unrelated grant

Add a second temporary positive regression test such as:

`work_view_ignores_unrelated_mail_grants`

Arrange only a Gmail mail/logistics grant set and request `WORK_VIEW`, with no actual work-capable source configured.

Desired assertion:

- unrelated Gmail grants do not become target blockers;
- the result is the normal navigation-only `SelectResource` requirement for work context;
- no remote payload read occurs.

At the authoring baseline, `WORK_VIEW | LOGISTICS_VIEW => true` may cause the unrelated grant to be treated as a known candidate. Record the exact blocker/error actually observed.

Run:

```sh
cargo test -p floe-context work_view_ignores_unrelated_mail_grants -- --nocapture
```

Do not repair `remote_view_connector_admissible` in 00.

### R1 disposition to freeze

Update 01's test table/report notes to state:

- the one-grant-per-connection `ViewFixture` topology is **move/rewrite** in 01;
- true duplicate-authority tests, paused contributor, foreign Person/source drift, provenance and no-truncated-aggregate assertions are **retain**;
- the wildcard connector compatibility list is production behavior to replace in 01, not a fixture workaround.

## 00-C: reproduce R2 through product policy and Manager tool service

Use `microsoft.mail` for the primary R2 reproduction so Gmail's R1 collision cannot mask the consumer mismatch.

Build the temporary integration test in App using existing product builders and owner APIs. Reuse the remote Observe/interaction harness where practical; do not insert an `assistant` grant by hand.

Suggested test name:

`manager_mail_read_requires_assistant_in_reviewed_product_policy`

The flow must be:

```text
first_party_observe::remote_policies("microsoft.mail")
  -> existing reviewed Observe activation / grant owner path
  -> persisted live grant
  -> ContextToolService
  -> mail.communication.read
```

The test first proves the product-issued grant's consumer set comes from the policy, then invokes the Manager tool under its actual `assistant` identity.

Desired assertion after the future fix: `Ready` under a reviewed policy that explicitly included `assistant`.

Expected current red evidence: the grant does not admit `assistant`, so the tool returns a review-required/blocked outcome rather than reading payload. Record the concrete `SourceReadOutcome`/requirement and verify provider payload I/O is not performed under the wrong consumer.

Run the actual chosen test with a focused command such as:

```sh
cargo test -p floe-app manager_mail_read_requires_assistant_in_reviewed_product_policy -- --nocapture
```

Also run the existing policy test and record why its current assertion is stale for Manager-readable remote Views:

```sh
cargo test -p floe-app policy_never_default_grants_extensions_or_assistant_wildcards -- --nocapture
```

This existing test is not deleted wholesale. Split its semantics in 01:

- **retain**: arbitrary extensions/non-first-party consumers are not auto-granted;
- **rewrite**: `assistant` is present only for the exact remote capabilities actually advertised as Manager direct-read tools;
- **retain**: no wildcard consumer admission.

### Freeze the review-binding decision

Before 01 implementation, inspect:

- `ReviewedBundleMember`;
- `InlineObserveTarget`;
- `canonical_target_digest`;
- `review_snapshot.rs`;
- `interaction_publication.rs`;
- `interaction_resolution.rs`;
- remote/native grant review APIs.

At the authoring baseline, an absent future grant has no `policy_authority`, and the reviewed target does not directly bind the full consumer set that resolution will create. 00 must therefore write one exact representation into 01 before 01 starts.

The preferred semantic contract is:

- App/product policy computes a canonical, bounded **per-member reviewed policy fingerprint** from the exact policy that will be applied at mutation time;
- the fingerprint covers at least the sorted consumer set and every other policy field whose change could widen the reviewed grant (View/member identity is already separately bound, but categories, purpose and processing mode must not be allowed to drift silently);
- Conversation stores only the opaque bounded fingerprint in the immutable reviewed target and includes it in `canonical_target_digest`;
- resolution recomputes from current owner/product policy and rejects/supersedes if it differs;
- the fingerprint is review identity, not Access authority and not a persisted standing permission.

If current owner APIs provide an equivalent stronger identity already, document and reuse it instead. Do not add two overlapping digests.

## 00-D: reproduce R3 across the real Rust result contract and Dart parser

Do not change Dart acceptance rules yet.

### Rust side

Use the existing App settlement test `schedule_settlement_binds_evidence_to_the_exact_grant_policy_dependency` as the production serializer anchor. Temporarily add assertions that the settled `ExpertResult` has:

- `model_calls == 1`;
- a nonempty summary;
- the actual `view_calls`;
- serialization through the same `serde_json::to_string`/settlement path used in production.

Print or otherwise capture the exact serialized result bytes for the reproduction run; do not hand-author the JSON shape.

Also confirm `AgentRegistry::validate_result_content` accepts:

- `model_calls == 1` with summary;
- `view_calls` in the current valid bounded range;
- empty insights when summary is nonempty.

If any of these assumptions fails in executable Rust, correct R3 before touching Dart.

Run:

```sh
cargo test -p floe-app schedule_settlement_binds_evidence_to_the_exact_grant_policy_dependency -- --nocapture
cargo test -p floe-experts result_content_requires_matching_non_nil_evidence_identity -- --nocapture
```

### Dart side

Add a temporary positive parser case that consumes the Rust-produced shape (with only deterministic identity adaptation if the test harness requires fixed call/person IDs; do not change semantic fields) and expects `AgentExpertResult.tryParse` to succeed.

At minimum create separate red assertions for:

1. one model call + summary;
2. a currently Rust-valid `view_calls > 1`;
3. summary-only result with empty insights.

Run:

```sh
(
  cd apps/client
  flutter test test/features/experts/agent_expert_result_test.dart
)
```

Expected current evidence is parser rejection at the checks around `modelCalls == 1`, `view_calls == 1`, and `insights.isEmpty`. Record which assertions actually fail.

Freeze 01's cross-language fixture decision:

- the tracked fixture added in 01 is generated by a Rust production serializer path, not duplicated by hand in Dart;
- generation/check instructions live next to the fixture or in the owning test;
- 01 fixes the current parser against that fixture;
- 02 ports the fixture to the generic report/artifact boundary and deletes the Schedule-shaped common parser without a dual decoder.

## 00-E: freeze owner, dependency and serialization decisions for 01-04

00 must leave no unresolved choice that would make two later agents invent different contracts. Update the relevant checkpoint document with the concrete choice and the inspected symbols that justify it.

### 01 decisions

Freeze:

- exact authority-target identity for R1 before duplicate detection: source/producer identity + View/capability + canonical resource, with consumer/purpose/category/operation validation performed after the target is identified;
- Manager remote direct-read consumer is exactly `assistant`, but only for capabilities present in the Manager tool catalogue;
- product review must explicitly include that consumer before Access admits it;
- reviewed product-policy fingerprint semantics from 00-C;
- no read-time grant mutation and no fallback source selection.

### 02 decisions

Inspect existing `ExpertReport`, `Artifact`, `TaskSnapshot`, Actions proposal/origin ports and the current settlement path. Record one final representation for:

1. generic Task report;
2. package-owned domain artifact/result;
3. Actions-owned consequential proposal.

Prefer existing types when they already carry the required identity/bounds. Do not introduce parallel `ExpertResultV2`/envelope families.

The decision must state exactly where validation occurs, which identity/coverage fields cross each boundary, and how an Actions proposal references the exact contributing evidence.

### 03 decisions

Freeze exact assignment/definition identity used by Directory publication and Task admission. A package ID alone and "first assignment" lookup are insufficient.

Inspect whether built-in bundle registration should depend directly on the generic Experts registration API. Current dependency policy allows App -> built-in and App -> Experts, but not built-in -> Experts.

Use this rule:

- if the registration descriptor/API is semantically owned by Experts and adding `builtin -> experts` is acyclic, record that explicit dependency change for 03;
- do not move an Experts-owned descriptor into `agent_contract` merely to evade the checker;
- if a genuinely pure cross-owner value exists, document why it is pure before placing it in a contract crate.

The dependency-policy edit itself belongs to 03, not 00.

### 04 decisions

Freeze the bounded source-reference and Task pin semantics.

The source reference must identify a selected owner object/resource without copying:

- credentials/bearers;
- current grant or grant authority;
- current source authority;
- current provider health;
- processing consent.

Record the exact fields chosen from existing Context/Connections value contracts. Avoid a magic "all current/future sources" selector.

Task admission must bind:

- exact assignment identity;
- manifest/definition revision;
- binding-set revision;
- canonical selected target set or immutable revision-addressable snapshot;
- one canonical selection digest.

Same-Task continuation/crash recovery keeps the admitted selection. A linked resume performs fresh admission. A settings change may fence later use of an old active selection but never reroutes that Task to a new source.

## 00-F: classify existing tests before later deletion

Record these dispositions in the owning checkpoint tables/report notes rather than creating a second global ledger.

| Existing coverage / assumption | 00 classification | Owning checkpoint |
|---|---|---|
| `remote_view_tests::ViewFixture` one grant per connection | Move/rewrite to multi-grant product-shaped fixture | 01 |
| Exact duplicate grants conflict before I/O | Retain | 01 |
| Paused contributor blocks complete aggregate | Retain | 01 |
| Foreign Person/source incarnation, provenance, consumer/category/purpose denial | Retain | 01/05 |
| `first_party_observe::policy_never_default_grants_extensions_or_assistant_wildcards` | Split: retain extension/wildcard denial, rewrite Manager `assistant` assumption | 01 |
| Gmail bundle interaction/CAS/drift/crash tests | Retain or rewrite only to bind the new reviewed policy fingerprint | 01/05 |
| Dart assertions that one model call, multiple view reads or summary-only result are invalid | Delete/replace with serializer interoperability assertions | 01 |
| Generic Registry FocusWindow semantic validation | Move package/Actions semantics; retain generic identity/state validation | 02 |
| Schedule settlement evidence, exact assignment, atomicity and stale-state checks | Retain/move to new generic settlement owner | 02/05 |
| Actions approval, target authority and uncertain-write recovery | Retain | 02/05 |
| Durable interaction origin, immutable target, CAS, exact-command rejoin, linked resume | Retain | 01/04/05 |

Search both inline modules and test trees for every touched assumption. A falling test count is acceptable only when the obsolete semantic assertion has executable replacement coverage at the final owner.

## 00-G: clean temporary repros, record evidence and close only 00

An 00-only task must not leave expected-red tests committed to `main`.

After capturing each failure:

1. save the exact command and salient observed assertion/error in the 00 completion evidence;
2. update 01-04 with any corrected finding and the frozen decisions from 00-E;
3. restore **only the temporary code/test hunks created by 00**. Use selective restore/editing; never discard pre-existing user changes;
4. verify the final diff contains only the authoritative plan-document updates expected for 00;
5. run:

```sh
python3 tools/architecture/check_boundaries.py
git diff --check
git status --short
```

If the 00 documentation edit accidentally changes scripts, generated files or executable policy, run the corresponding code-change-verification gate; otherwise do not claim code verification from a docs-only final diff.

Update only the 00 row in [README.md](README.md):

- status: `Complete` only after R1-R3 executable evidence/ corrections and owner decisions are recorded;
- completion evidence: the actual 00 commit SHA(s) and a concise reference to the recorded red commands/findings.

Do not mark 01 started or complete. Do not implement the fixes in the same 00-only authorization.

## Required 00 completion report

Report exactly:

1. start HEAD, source baseline comparison and whether unrelated user work existed;
2. baseline targeted tests / architecture checker and actual outcomes;
3. R1.1, R1.2, R2 and R3 temporary test names, commands and observed red result, or corrected finding with contrary evidence;
4. owner/serialization/dependency decisions frozen into 01-04;
5. old-test disposition changes;
6. final diff scope proving temporary red patches were removed;
7. resulting documentation commit SHA(s);
8. any prerequisite unavailable and the concrete reason;
9. the next executable checkpoint: 01, without starting it.

A report that only repeats the static hypotheses does not complete 00.

## Executed checkpoint 00 evidence (2026-09-25)

### Repository and prerequisites

At start, `git status --short --branch` was `## main...origin/main` with no user changes. Local `HEAD`, `git log -1`, and `git ls-remote origin refs/heads/main` all identified `f3476e845d3b2b112882a7047136cce991e2574e` (`docs: detail expert extensibility checkpoint 00`). Both `git diff --name-status 43005508338d7ae38d3247910361c733d7cdfe98...HEAD` and `git diff --stat` reported only `docs/README.md` plus the eight Expert-extensibility plan Markdown files: nine documentation files, 1,144 insertions, no source/manifests changed. `cargo metadata --no-deps --format-version 1` passed. `python3 tools/architecture/check_boundaries.py` passed in final mode (22 nodes, 99 edges, no errors or warnings). Toolchains: `rustc 1.93.1`, `cargo 1.93.1`, Flutter `3.47.2` stable / Dart `3.13.2`, Xcode `26.2` (build `17C52`). No prerequisite was unavailable or installed during this checkpoint.

Before temporary patches, all requested targeted baseline commands passed: `cargo test -p floe-context remote_view_tests` (4 inline tests passed), `cargo test -p floe-app first_party_observe::tests` (3 passed), `cargo test -p floe-app gmail_views_allow_enables_bundle_atomically_and_resolves` (1 passed), `cargo test -p floe-app schedule_settlement_binds_evidence_to_the_exact_grant_policy_dependency` (1 passed), and `(cd apps/client && flutter test test/features/experts/agent_expert_result_test.dart)` (3 passed). The Cargo invocations also ran filtered integration-test binaries; their zero-test results were not failures.

### Temporary executable findings

| Finding | Exact command and temporary test | Observed result on unfixed production code |
|---|---|
| R1.1 | `cargo test -p floe-context same_connection_mail_and_logistics_grants_are_not_duplicate_authority -- --nocapture` | Red (exit 101): `read_mail(...).unwrap()` received `Err(Conflict)` before payload I/O. The temporary multi-grant fixture held two active, correctly resource-scoped grants on the same Person/Gmail connection and one source binding. |
| R1.2 | `cargo test -p floe-context work_view_ignores_unrelated_mail_grants -- --nocapture` | Red (exit 101): blocker reason was `ReviewChangedSource`, not expected navigation-only `SelectResource`; payload read count remained zero. This isolated the wildcard connector classification with one unrelated Gmail mail grant, without letting the separate duplicate collision mask it. |
| R2 | `cargo test -p floe-app manager_mail_read_requires_assistant_in_reviewed_product_policy -- --nocapture` | Red (exit 101): `NeedsUserAction` with one `floe.source.mail` / `microsoft.mail` / `mail.communication:<connection>` `ReviewChangedSource` blocker for exact `Builtin("assistant")` read. The test activated a real `remote_policies("microsoft.mail")` reviewed bundle through `resolve_interaction`/owner mutation, asserted the persisted grant consumers equal that product policy, then invoked `ContextToolService` through `read_remote_view`; provider payload I/O count was zero before the red readiness assertion. No fake assistant grant was installed. |
| R3 producer | `cargo test -p floe-app schedule_settlement_binds_evidence_to_the_exact_grant_policy_dependency -- --nocapture` | Green temporary instrumentation: actual settled `ExpertResult` had `model_calls=1`, nonempty summary, `view_calls=1`; `serde_json::to_string(&result)` equaled the production `output.data` bytes printed below. |
| R3 validator | `cargo test -p floe-experts result_content_requires_matching_non_nil_evidence_identity -- --nocapture` | Green temporary assertions: `validate_result_content` accepted the one-model-call fixture, the same valid fixture with `view_calls=2`, and empty insights/proposals with nonempty summary. |
| R3 Dart | `(cd apps/client && flutter test test/features/experts/agent_expert_result_test.dart)` | Red: three temporary positive cases returned `null`: the exact Rust-serialized one-call result, a Rust-valid `model_calls=2`/`view_calls=2` variant, and a Rust-valid `model_calls=2`/summary-only variant. The first is rejected by `modelCalls == 1`, the second by `view_calls != 1`, and the third by `insights.isEmpty`; all pre-existing negative tests still passed. |

The production settlement serializer emitted these bytes (run-specific UUIDs/clock; copied verbatim into the temporary Dart test before removal):

```json
{"schema_version":1,"invocation_id":"535289f1-56a9-4745-b65c-2a3b1a582903","instance_id":"d1892fb6-c005-425c-8d56-036ef0560e59","person_id":"b8698a9e-8464-4277-ba14-4a219ff90a63","assignment_id":"359f5419-20e0-4b7e-81d5-3dbe0a6038c4","package":{"kind":"expert","id":"floe.builtin.schedule","version":"1.0.0"},"evidence_id":"8e9e65f7-0c76-49ac-b103-268ad329cbfc","source_handle":"calendar.observe:8e9e65f7-0c76-49ac-b103-268ad329cbfc","data_class":"personal","expires_at_unix_ms":1790344120619,"insights":[{"kind":"focus_window","starts_at_unix_ms":1800000000000,"ends_at_unix_ms":1800001800000}],"action_proposals":[{"starts_at_unix_ms":1800000000000,"ends_at_unix_ms":1800001800000,"evidence_id":"8e9e65f7-0c76-49ac-b103-268ad329cbfc"}],"summary":"One focus window","model_calls":1,"state_revision":1,"view_calls":1}
```

The R1/R2/R3 production hypotheses held. R1.2's exact blocker and R2's exact `SourceReadOutcome` above replace the earlier tentative wording; no contrary finding required changing the target design. The existing `policy_never_default_grants_extensions_or_assistant_wildcards` test passed unchanged under `cargo test -p floe-app policy_never_default_grants_extensions_or_assistant_wildcards -- --nocapture`; its extension/wildcard denials remain valid while its blanket assistant assumption is rewritten in 01.

All temporary Context/App/Experts/Dart test and instrumentation hunks were then removed selectively. The final worktree diff comprised only this evidence document and the frozen contracts/test disposition in checkpoint documents 01-04; no production or expected-red test code remained. `python3 tools/architecture/check_boundaries.py` again passed (22 nodes, 99 edges, no errors/warnings), and `git diff --check` passed. No toolchain prerequisite was unavailable. Checkpoint 01 was not started.
