# 01: fix current producer/consumer contract regressions

Prerequisite: checkpoint 00 in [the plan](README.md), completed by evidence commit `b9617d578a2bf8e442e6d2cc26593e7cb4165cfd` and status commit `ed885f5867f018f9ee6f7006abab508532cf0aec`. This checkpoint repairs the three now-executably-confirmed regressions before structural changes. Checkpoint 00 already captured the pre-fix red evidence; 01 turns those cases into permanent green regression coverage. No security check is weakened to make the implementation pass.

## Confirmed failures and current anchors

| Finding | Existing files / symbols | Confirmed pre-fix behavior |
|---|---|---|
| R1: View grant classification | `crates/modules/context/src/application/remote_sources.rs`: `classify_remote_sources`, `read_remote_view`; `remote_views.rs`: `remote_view_connector_admissible`; `crates/app/src/first_party_observe.rs`: `remote_policies`; `vault_host/remote_observe.rs`: bundle activation | A Gmail connection legitimately has mail and logistics grants. Candidate collection and duplicate-source checks happen before exact View/resource filtering. Work/Logistics also accept any connector as a candidate. |
| R2: Manager consumer | `crates/modules/context/src/application/tools.rs`: `ContextToolService`, `ASSISTANT_CONSUMER`; `crates/app/src/first_party_observe.rs`; `vault_host/remote_observe.rs`, `review_snapshot.rs`, `interaction_owners.rs` | Manager reads as `assistant`, but remote product Observe policy names built-in Expert consumers only. Re-enabling the same bundle need not satisfy the original read. |
| R3: result contract | `crates/experts/builtin/src/schedule/expert.rs`: `judge`; `crates/app/src/vault_host/conversation_turn/expert_dispatch/stateful_settlement.rs`; `crates/modules/experts/src/registry.rs`: `validate_result_content`; `apps/client/lib/features/experts/domain/agent_expert_result.dart`: `tryParse` | Schedule emits `model_calls=1`; Flutter rejects it. Rust allows bounded multiple view calls and summary-only results which the old parser also rejects. |

## Contract frozen by checkpoint 00

- Context identifies a remote authority target by exact producer/source identity, requested capability/View, and canonical resource before duplicate detection. Different View grants on one connection are not duplicates. Once a target is identified, Access/Context validate the exact consumer, read operation, Assistant purpose, categories, processing restriction and scope; a mismatched target remains a blocker, never a fallback. Reads neither mutate grants nor select an alternative source automatically.
- The Manager direct-read consumer is exactly `assistant`. App product policy includes it only for remote Views actually exposed by `manager_tool_descriptors` (`mail.communication`, `work.context`, `life.logistics`), never by a wildcard or arbitrary extension rule. The reviewed mutation and subsequent exact requirement resolution must agree on that consumer.
- App computes one canonical bounded per-`ReviewedBundleMember` policy fingerprint from the policy **actually applied** by `remote_policies`/grant mutation: sorted exact consumers, category set, purpose and processing restriction, plus member/View identity already bound by the target. Conversation stores that opaque fingerprint in the immutable member and includes it in `canonical_target_digest`; at resolution App recomputes from current product/owner policy and supersedes on mismatch before mutation. The fingerprint is review identity, not Access grant authority. Reuse the existing target digest; do not add a second target authority/digest. Current `policy_authority` binds a live grant revision only, and is `None` for absent grants, so it cannot serve this purpose.
- The tracked 01 interoperability fixture is emitted by the existing Rust production `serde_json::to_string(&ExpertResult)` settlement path; Dart consumes the same serialized bytes/schema, with only deterministic identity adaptation in tests. Keep generator/check instructions beside that fixture. In 02, regenerate/relocate it at the generic Task report/artifact boundary and delete the Schedule-shaped parser rather than keeping two decoders.

## Executable sequence and commit gates

Plan-authoring parent is `ed885f5867f018f9ee6f7006abab508532cf0aec`. The coding agent must still re-read actual `HEAD`, `origin/main`, and the worktree before editing. If source changed after that parent, recheck the affected symbols and preserve unrelated user work.

The preferred implementation order is:

| Substep | Scope | Commit rule |
|---|---|---|
| 01-A | Product-shaped permanent fixtures and the deterministic Rust -> Dart fixture seam | Do not commit an expected-red state. Fixture changes may land with the first owning fix if they cannot stay green independently. |
| 01-B | Exact remote authority-target classification in Context | One green Context-focused commit after permanent R1 coverage passes. No App/Flutter policy work mixed into it. |
| 01-C | Manager `assistant` policy alignment plus reviewed policy fingerprint through App/Conversation/protocol/FFI/client review round-trip | One coherent authority/review commit, or C1 contract + C2 behavior only if every intermediate commit compiles and no review can widen scope. |
| 01-D | Production-serialized Expert result fixture and Dart parser alignment | One Rust/Flutter contract commit with no dual parser or hand-written semantic duplicate. |
| 01-E | Residual audit, broad gates, current architecture/status convergence | Completion/docs commit only after all required product gates are green. |

A substep commit must not leave a caller compiled against a stale contract, a review target that omits policy identity, an expected-red test, or an old/new compatibility branch. Checkpoint 01 remains `Not started` in the status table until production implementation begins; it becomes `Complete` only in 01-E.

### Required start checks

Before changing production code:

```sh
git status --short --branch
git rev-parse HEAD
git log -1 --oneline
git diff --name-status ed885f5867f018f9ee6f7006abab508532cf0aec...HEAD
python3 tools/architecture/check_boundaries.py

cargo test -p floe-context remote_view_tests
cargo test -p floe-app first_party_observe::tests
cargo test -p floe-app gmail_views_allow_enables_bundle_atomically_and_resolves
cargo test -p floe-app schedule_settlement_binds_evidence_to_the_exact_grant_policy_dependency
(
  cd apps/client
  flutter test test/features/experts/agent_expert_result_test.dart
)
```

Checkpoint 00's red evidence is authoritative pre-fix evidence. Do not spend a standalone commit recreating temporary failing tests; restore those cases as permanent tests adjacent to each fix and prove they become green.

## 01-A: reproduce with product-shaped fixtures

Start each case independently so one defect cannot mask another. Use Microsoft's single mail View for R2 to isolate it from Gmail's multi-View collision.

Replace the test fixture assumption `connection_id -> exactly one grant` with a fixture capable of holding multiple View/resource grants per connection. Prefer the actual product bundle builder and owner activation APIs. Do not fix the fixture by inserting `assistant` directly where product policy would not issue it.

For R3, export a deterministic fixture through the current Rust producer/serializer and consume the exact bytes from the Dart test. Freeze or normalize only identity/clock fields after the result has passed the real settlement path; do not hand-author semantic fields. Use one repository-level fixture, preferably `fixtures/expert-result/schedule-v1.json`, with generator/check instructions beside it (for example `fixtures/expert-result/README.md`). The normal Rust test must compare the production-serialized/identity-normalized bytes with the tracked fixture; an ignored or explicitly opted-in test/helper may regenerate it. Dart reads that same file from the repository root. Retain it through 02's schema cutover.

## 01-B: classify the exact authority target

Change `classify_remote_sources` and its callers in this order:

1. Identify the requested View's exact resource and compatible source capability. Distinguish unrelated grants from a known target whose current permission is missing or stale.
2. Exclude grants for other Views/resources before duplicate detection. Same connection is not the duplicate key. Canonical source/producer identity, capability/View and resource scope define the candidate authority target; reject true ambiguous authority there.
3. Check consumer, operation, purpose, categories, scope, review status and authority on the target. Do not discard consumer-mismatched candidates as if the source did not exist. Do not accept wrong categories merely because a fixture does.
4. Preserve typed blockers for targets that need review and hard denials for corrupt/foreign/ambiguous authority. Classify all intended contributors before returning a complete aggregate.
5. Replace `WORK_VIEW | LOGISTICS_VIEW => true` with capability facts from the actual source/connector owner. Inspect the descriptor contract and source grant resource mapping; do not copy the App connector switch into a second competing Context list. If the current descriptor lacks required information, narrow the owner API rather than inventing wildcard compatibility.

This is an immediate repair to current multi-source semantics. Explicit assignment target selection is 04, not a reason to defer R1. Do not introduce a parallel next reader.

### 01-B exact implementation path

Current anchors on the checkpoint-00 completion HEAD are `classify_remote_sources` around `crates/modules/context/src/application/remote_sources.rs:298`, `read_remote_view` around line 450, and the one-grant-per-connection `ViewFixture` around line 1354. Treat these as orientation only and search symbols on the execution HEAD.

Implement the target/admission split explicitly:

1. Validate that the requested ID is a supported remote View.
2. For each non-revoked grant belonging to the current Person, compute that grant's canonical requested resource from **its own connection**: `remote_view_resource(view_id, grant.source().connection_id())`.
3. A grant names the requested logical target when its resource set contains that canonical resource. This target test intentionally does **not** filter on consumer, purpose, categories, operation, review flag or processing restriction; those mismatches must remain visible blockers instead of becoming “no source”.
4. Group target grants by the logical producer-owned target: Person + connector + connection + requested View + canonical resource. A second live grant for that same logical target is ambiguous authority and fails closed before payload I/O, even if grant IDs or source-authority revisions differ. Different resources on one connection (for example Gmail mail vs logistics) are different targets and never collide.
5. Only after target grouping, run the full admission predicate. The current remote product grant shape is one exact resource, one expected data category from `remote_view_data_category(view_id)`, Read, Assistant purpose, the exact consumer, and `ProcessingRestriction::LocalOnly`. Extra/wrong category, operation, purpose, resource, processing or consumer is not silently accepted. Paused/review-required target grants become typed blockers.
6. If no grant names the requested resource, return the existing navigation-only `SelectResource`. Unrelated grants do not become blockers.
7. Preserve the “classify all contributors before read” rule: any selected target blocker prevents a truncated `Ready` aggregate.

Do **not** replace `WORK_VIEW | LOGISTICS_VIEW => true` with another App-maintained connector switch. The product already proves the real connector/View pair when it creates a scoped grant, and `read_one_remote_source` revalidates the exact query through the pinned producer's signed `view_source_preview` plus `verify_view_source_preview` / `admit_remote_view_source` / `admit_remote_view_binding`. Therefore the Context candidate classifier should key on the exact grant resource, not a static connector whitelist. After callers are migrated, delete `remote_view_connector_admissible` and the currently uncalled `remote_view_grant_resource` export if the residual search confirms no legitimate owner still needs them. Keep `is_remote_view`, resource parsing/validation and signed producer admission.

Rewrite the inline `ViewFixture` so one connection can hold multiple grants without losing the exact grant/resource/source-authority lookup behavior. Prefer a collection keyed by exact member/authority identity, not a map that overwrites by connection ID.

Targeted green gate for 01-B:

```sh
cargo test -p floe-context remote_view_tests
cargo test -p floe-context same_connection_mail_and_logistics_grants_are_not_duplicate_authority
cargo test -p floe-context work_view_ignores_unrelated_mail_grants
python3 tools/architecture/check_boundaries.py
git diff --check
```

Before the 01-B commit, search for `remote_view_connector_admissible`, `remote_view_grant_resource`, the old `HashMap<String, SourceFixture>` topology, and any duplicate check that still uses source binding alone. Every residual match must be justified or removed.

### Regression cases

| Case | Expected assertion |
|---|---|
| One Gmail connection with active mail and logistics grants | Each View is readable independently; no false duplicate authority. |
| Work read with unrelated Calendar/native/Mail grants | Unrelated grants neither read nor block that View. |
| Same exact target with conflicting live grants | Fails closed; no read and no arbitrarily chosen winner. |
| Correct target, wrong consumer/category/purpose | Not Ready; no payload acquired under the wrong grant. |
| Current intended multi-source set with a paused contributor | Typed blocker; no truncated aggregate claimed complete. |
| Foreign Person or changed source incarnation | Rejected without adopting another grant. |

## 01-C: make advertised Manager tools usable under reviewed policy

Keep the existing direct Manager tools; do not remove them as a shortcut. Align the exact `assistant` consumer for supported direct-read capabilities with product review, activation and resolver behavior. Preserve per-Expert consumer checks and the distinction between a source consumer and an Inference execution role.

Change the first-party default policy and the reviewed mutation together. A changed consumer set must be part of the newly reviewed target, not silently appended to an existing grant during read or resume. Validate whether current reviewed-target records bind the full consumer set; add a bounded consumer-set identity/digest where needed before enabling it. An older review must not approve a newly added consumer.

After an owner mutation, resolve the original requirement only if its exact consumer/resource/operation is now satisfied. `Observe == on` or successful settings navigation is insufficient. Reuse existing durable interaction and linked-resume semantics; do not create a second recovery path.

### 01-C product-policy derivation

The Manager catalogue owner is Context. Do not duplicate the three direct remote Views in App. Refactor the private Manager tool specs in `crates/modules/context/src/application/tools.rs` so `manager_tool_descriptors()` and a small exported query such as `manager_direct_remote_view(view_id)` derive from the same remote-tool specification. App may ask that query while constructing `FirstPartyObservePolicy`.

In `crates/app/src/first_party_observe.rs`:

- keep built-in Expert consumers exactly as today;
- append `GrantConsumer::builtin(ASSISTANT_CONSUMER)` only when Context says that policy View is an advertised Manager direct-read View;
- sort/deduplicate the final consumer set;
- do not add `assistant` to Calendar or unrelated/native policies merely because the Manager exists;
- split the old `policy_never_default_grants_extensions_or_assistant_wildcards` assertion: retain arbitrary-extension and wildcard denial, but positively assert `assistant` only on mail/work/logistics remote policies exposed by Manager tools.

### 01-C canonical reviewed policy fingerprint

Implement one App-owned canonical fingerprint helper for `FirstPartyObservePolicy`. The input is the policy that will actually be applied by `review_bundle` / grant activation:

- schema/domain tag for the fingerprint algorithm;
- exact `view_id`;
- sorted exact consumer identifiers;
- sorted exact category identifiers;
- exact purpose;
- exact `SourceProcessingPolicy`.

Use SHA-256 (already an App dependency) and a bounded deterministic representation such as 64 lowercase hex characters. The function must reject/avoid noncanonical sets before hashing. Do not include live grant authority, source authority, provider fingerprint, recipient, credentials or connection health; those already have separate review/authority owners.

Bind this fingerprint through **both** review paths:

1. **Connection Observe review -> enable round-trip**
   - add the field to `RemoteObserveMemberExpectation`;
   - add it to `ConnectionObserveMemberDto` and both FFI conversion directions;
   - validate exact fingerprint shape in protocol and App expectation validation;
   - emit it from `review_member` / remote calendar review;
   - in `enable_bundle`, recompute from current `remote_policies(connector)` and compare every member **before** any owner mutation. A mismatch returns review-required/conflict semantics and never upgrades an old review.
   - update `apps/client/lib/features/connections/application/remote_access_gateway.dart` / `ConnectionObserveBundle` models and connection-panel fixtures so the client transparently echoes the reviewed fingerprint; it never calculates one.

2. **Durable Conversation interaction**
   - add the opaque field to `review_snapshot::SnapshotMember`, `conversation::ReviewedBundleMember`, and `interaction_resolution::LiveMember`;
   - publication copies it into the immutable reviewed target;
   - `ReviewedBundleMember::validate` enforces the bounded fingerprint form;
   - extend `canonical_target_digest`'s existing member serialization with this field. Do not create another target digest or authority;
   - the owner live-state read recomputes the current App policy fingerprint and comparison reports policy drift before mutation;
   - stale fingerprint during refresh creates/supersedes the reviewed target under the existing durable interaction lifecycle; no in-place widening.

The explicit Connection Observe expectation is wire-visible and the durable interaction is persisted. Floe is pre-stable, so do a direct contract cutover: update all protocol/FFI/client/test fixtures in the same 01-C change. Do not add an optional legacy fingerprint, default value, dual decoder or migration-only branch merely to read an old local profile.

### 01-C exact-satisfaction and safety tests

Permanent tests must cover:

- the checkpoint-00 Microsoft Mail path: product policy -> reviewed activation -> persisted grant -> `ContextToolService` as exact `assistant` -> `Ready`;
- Gmail policy carries `assistant` on both Manager-readable mail/logistics members while keeping built-in consumers;
- work connectors carry `assistant` only for `work.context`;
- Calendar and unadvertised/unknown extension consumers do not gain `assistant`;
- an older Connection Observe bundle whose policy fingerprint no longer matches current policy is rejected before mutation;
- an older durable interaction whose fingerprint no longer matches is superseded/rejected, including reviewed absence where `policy_authority == None`;
- producer/source/grant/policy-authority drift tests continue to fail closed;
- successful resolution checks the original exact consumer/resource/operation and linked resume no longer repeats the same Manager consumer mismatch;
- crash/rejoin/CAS semantics remain intact.

Targeted gate:

```sh
cargo test -p floe-app first_party_observe::tests
cargo test -p floe-app manager_mail_read_requires_assistant_in_reviewed_product_policy
cargo test -p floe-app gmail_views_allow_enables_bundle_atomically_and_resolves
cargo test -p floe-app interaction_resolution
cargo test -p floe-protocol remote_wire
cargo test -p floe-ffi
cargo build -p floe-ffi
(
  cd apps/client
  flutter test test/features/connections/server_connector_panel_test.dart
)
python3 tools/architecture/check_boundaries.py
git diff --check
```

If a Cargo test filter does not match a test target on the execution HEAD, run the containing package/test target and report the actual command; do not count a zero-test filter as coverage.

### Regression cases

Use actual product policies and `ContextToolService` in one test path. Cover mail, Work and Logistics, a consumer still excluded after a mutation, an unreviewed extension, and review -> resolve -> linked resume. The successful resume must no longer produce the identical consumer mismatch. Changes in producer, scope, policy or consumer set during review remain rejected.

## 01-D: align the existing result contract without dropping validation

Bring the current Dart parser into agreement with the current Rust contract for positive model calls with a summary, bounded view-call counts and empty insights with a nonempty summary. Keep byte limits, identity, data-class policy, interval and evidence/proposal consistency checks. Reject a summary unsupported by the declared execution outcome and malformed/private fields as required by the current contract.

Update `apps/client/test/features/experts/agent_expert_result_test.dart`, `apps/client/test/features/actions/agent_proposal_test.dart`, and `apps/client/test/support/expert_result.dart` according to the generated fixture. Remove obsolete assertions that `view_calls=2` or summary-only results are necessarily invalid. Add a real one-call Schedule result case, not just permissive parser unit tests. Verify the live `agent_controller.dart` call site and proposal display/inspection path.

The temporary old result contract is removed in 02. The repaired current path and its cross-boundary evidence stay valid until the direct cutover; do not add old/new parsing branches.

### 01-D production fixture and parser cutover

Use `schedule_settlement_binds_evidence_to_the_exact_grant_policy_dependency` as the serializer anchor. Factor only test code needed to obtain one deterministic contract sample:

1. drive `VaultStatefulExpertSettlement::settle` with the normal Schedule draft (`model_calls=1`) and real dependency/registry path;
2. take `BuiltinExpertOutput.data`, which is the production `serde_json::to_string(&ExpertResult)` bytes;
3. normalize only nondeterministic identity/time fields after production settlement (for example UUID/person/assignment/evidence/source-handle/expiry values) to fixed valid test values, then serialize canonically once;
4. compare those bytes with the tracked repository fixture. The regeneration helper may be an ignored or explicit-env test utility; the ordinary test is read-only and fails on drift.

The Dart test must read the same tracked JSON file, not recreate its semantic fields in `apps/client/test/support/expert_result.dart`. Keep small mutation helpers for negative cases if useful, but delete the old hand-written “canonical” fixture once all callers use the tracked Rust fixture.

Align `AgentExpertResult.tryParse` to the **current Rust validator**, not to arbitrary permissiveness:

- `0 <= model_calls <= 10`;
- summary presence follows the Rust contract: nonempty summary iff positive model calls (including exactly one);
- bounded `view_calls` matches the Rust valid range rather than exactly one;
- empty insights are allowed when the result is otherwise valid (for example summary-only), while insight count/type/interval validation remains bounded when present;
- keep schema version, invocation/person identity, output byte limit, allowed data class, instance/assignment/evidence/source-handle bounds, state revision, expiry, proposal count/evidence identity and proposal-to-focus-window consistency checks;
- keep unknown/private-field rejection.

Update `agent_expert_result_test.dart`, `agent_proposal_test.dart`, and any live controller/display test that consumes the parser. Add positive tests for the tracked one-call Schedule fixture, a Rust-valid multi-view variant and a Rust-valid summary-only variant. Retain negative tests for `model_calls > 10`, invalid summary/call relation, out-of-range view counts, malformed/private fields and forged proposal/evidence identity.

Targeted gate:

```sh
cargo test -p floe-app schedule_settlement_binds_evidence_to_the_exact_grant_policy_dependency
cargo test -p floe-experts result_content_requires_matching_non_nil_evidence_identity
(
  cd apps/client
  flutter test test/features/experts/agent_expert_result_test.dart
  flutter test test/features/actions/agent_proposal_test.dart
)
git diff --check
```

### 01-E: checkpoint-wide verification and convergence

After B/C/D are individually green, run the affected broad gates required by the verification skill because 01 changes Rust authority/runtime behavior and a Rust/Flutter-visible wire/result contract:

```sh
cargo check --workspace --lib
cargo test --workspace --no-fail-fast
python3 tools/architecture/check_boundaries.py
cargo build -p floe-ffi
(
  cd apps/client
  flutter analyze
  flutter test
  flutter build macos
)
git diff --check
```

Go server behavior is not intended to change in 01. Do not edit server routes/descriptors to make Context tests pass. If implementation unexpectedly changes Go source/protocol behavior, add `go test -race ./...` and `go vet ./...` from `server/` and explain why the boundary expanded.

Run residual searches for:

```text
remote_view_connector_admissible
remote_view_grant_resource
HashMap<String, SourceFixture>
policy_never_default_grants_extensions_or_assistant_wildcards
ReviewedBundleMember {
RemoteObserveMemberExpectation {
ConnectionObserveMemberDto {
modelCalls == 1
view_calls == 1
view_calls != 1
insights.isEmpty
expertResultFixture
```

Each remaining match must be current semantics, test mutation code, historical plan evidence, or an explicitly documented 02 deletion target. There must be no product reader that still treats unrelated grants as candidate authority and no reviewed mutation path that can change the exact consumer policy without changing reviewed identity.

Update current architecture docs only where 01 changed current truth: the exact remote-target classification and reviewed policy identity belong in authority/runtime documentation if those documents describe this path. Do not document 02-04 target architecture as implemented. Then update only the 01 row in [README.md](README.md) with `Complete` and the actual production/verification commit SHAs.

## Test disposition and deletion gate

| Existing assumption | Disposition |
|---|---|
| One remote grant per connection | Replace fixture topology; delete the assumption, retain admission tests. |
| True duplicate target authority and paused contributor/no truncated aggregate | Retain exact fail-closed assertions while rewriting the fixture. |
| Tests supply permissions product never creates | Replace with owner-issued grants. |
| `policy_never_default_grants_extensions_or_assistant_wildcards` | Split: retain extension and wildcard denial; rewrite only its blanket `assistant` denial for advertised direct-read Views. |
| General result display requires two model calls or exactly one View read | Delete obsolete assertions; retain budget validation at the execution owner. |
| Dart one-call/multi-view/summary-only invalid cases | Delete/replace with the Rust-serialized positive fixture and retain malformed/private-field rejection. |
| Gmail interaction, CAS, producer drift and crash coverage | Retain; rewrite only reviewed-target fixtures to bind the policy fingerprint. |
| Grant drift, provenance, foreign Person/source, category/purpose denial and Action approval | Retain or rewrite at the same semantic owner, never relax. |

01 closes with all three reproductions fixed or explicitly corrected by evidence, product-shaped fixtures, no grant wildcard, no truncated complete read and no hidden fallback. Run targeted Context/App/Expert/Rust fixture and Dart tests, then the Rust and affected product gates in [06](06-verification.md). Record exact commands/results. No claim about native or live provider success comes from a fake transport test.

## Report additions

For R1-R3 report:

1. start HEAD/worktree and exact 01-B/01-C/01-D/01-E commit SHAs;
2. checkpoint-00 red evidence that each permanent test replaces;
3. exact target-classification algorithm after 01-B and deleted static/dead compatibility helpers;
4. Manager remote View derivation and exact post-01-C consumer sets;
5. policy fingerprint algorithm/domain tag, every storage/wire field carrying it, and evidence that old review identity cannot authorize widened policy;
6. exact Rust-generated fixture path, regeneration/check command and Dart consumers;
7. obsolete assertions/fixtures/types deleted or rewritten, plus retained safety tests;
8. exact targeted and broad commands with observed results (zero-test filtered runs are not evidence);
9. residual-search results and any match intentionally deferred to 02;
10. architecture/status docs changed and the next checkpoint, 02, without starting it.

For R1-R3 also preserve the original failing result from checkpoint 00 in the report so the regression is traceable from red evidence to the final green owner path.
