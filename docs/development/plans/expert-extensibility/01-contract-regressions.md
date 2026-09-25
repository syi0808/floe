# 01: fix current producer/consumer contract regressions

Prerequisite: checkpoint 00 in [the plan](README.md). This checkpoint repairs current behavior before structural changes. Findings were statically traced at the plan baseline; first make each executable. No security check is weakened to force a reproduction.

## Source anchors and current failure hypotheses

| Finding | Existing files / symbols | Current mismatch to reproduce |
|---|---|---|
| R1: View grant classification | `crates/modules/context/src/application/remote_sources.rs`: `classify_remote_sources`, `read_remote_view`; `remote_views.rs`: `remote_view_connector_admissible`; `crates/app/src/first_party_observe.rs`: `remote_policies`; `vault_host/remote_observe.rs`: bundle activation | A Gmail connection legitimately has mail and logistics grants. Candidate collection and duplicate-source checks happen before exact View/resource filtering. Work/Logistics also accept any connector as a candidate. |
| R2: Manager consumer | `crates/modules/context/src/application/tools.rs`: `ContextToolService`, `ASSISTANT_CONSUMER`; `crates/app/src/first_party_observe.rs`; `vault_host/remote_observe.rs`, `review_snapshot.rs`, `interaction_owners.rs` | Manager reads as `assistant`, but remote product Observe policy names built-in Expert consumers only. Re-enabling the same bundle need not satisfy the original read. |
| R3: result contract | `crates/experts/builtin/src/schedule/expert.rs`: `judge`; `crates/app/src/vault_host/conversation_turn/expert_dispatch/stateful_settlement.rs`; `crates/modules/experts/src/registry.rs`: `validate_result_content`; `apps/client/lib/features/experts/domain/agent_expert_result.dart`: `tryParse` | Schedule emits `model_calls=1`; Flutter rejects it. Rust allows bounded multiple view calls and summary-only results which the old parser also rejects. |

## Contract frozen by checkpoint 00

- Context identifies a remote authority target by exact producer/source identity, requested capability/View, and canonical resource before duplicate detection. Different View grants on one connection are not duplicates. Once a target is identified, Access/Context validate the exact consumer, read operation, Assistant purpose, categories, processing restriction and scope; a mismatched target remains a blocker, never a fallback. Reads neither mutate grants nor select an alternative source automatically.
- The Manager direct-read consumer is exactly `assistant`. App product policy includes it only for remote Views actually exposed by `manager_tool_descriptors` (`mail.communication`, `work.context`, `life.logistics`), never by a wildcard or arbitrary extension rule. The reviewed mutation and subsequent exact requirement resolution must agree on that consumer.
- App computes one canonical bounded per-`ReviewedBundleMember` policy fingerprint from the policy **actually applied** by `remote_policies`/grant mutation: sorted exact consumers, category set, purpose and processing restriction, plus member/View identity already bound by the target. Conversation stores that opaque fingerprint in the immutable member and includes it in `canonical_target_digest`; at resolution App recomputes from current product/owner policy and supersedes on mismatch before mutation. The fingerprint is review identity, not Access grant authority. Reuse the existing target digest; do not add a second target authority/digest. Current `policy_authority` binds a live grant revision only, and is `None` for absent grants, so it cannot serve this purpose.
- The tracked 01 interoperability fixture is emitted by the existing Rust production `serde_json::to_string(&ExpertResult)` settlement path; Dart consumes the same serialized bytes/schema, with only deterministic identity adaptation in tests. Keep generator/check instructions beside that fixture. In 02, regenerate/relocate it at the generic Task report/artifact boundary and delete the Schedule-shaped parser rather than keeping two decoders.

## 01-A: reproduce with product-shaped fixtures

Start each case independently so one defect cannot mask another. Use Microsoft's single mail View for R2 to isolate it from Gmail's multi-View collision.

Replace the test fixture assumption `connection_id -> exactly one grant` with a fixture capable of holding multiple View/resource grants per connection. Prefer the actual product bundle builder and owner activation APIs. Do not fix the fixture by inserting `assistant` directly where product policy would not issue it.

For R3, export a deterministic fixture through the current Rust producer/serializer and consume the exact bytes from the Dart test. Freeze IDs/clock and include generator/check instructions. A hand-written Dart approximation is not a cross-boundary contract test. Choose one shared tracked fixture location, record it in the completion report, and retain it through 02's schema cutover.

## 01-B: classify the exact authority target

Change `classify_remote_sources` and its callers in this order:

1. Identify the requested View's exact resource and compatible source capability. Distinguish unrelated grants from a known target whose current permission is missing or stale.
2. Exclude grants for other Views/resources before duplicate detection. Same connection is not the duplicate key. Canonical source/producer identity, capability/View and resource scope define the candidate authority target; reject true ambiguous authority there.
3. Check consumer, operation, purpose, categories, scope, review status and authority on the target. Do not discard consumer-mismatched candidates as if the source did not exist. Do not accept wrong categories merely because a fixture does.
4. Preserve typed blockers for targets that need review and hard denials for corrupt/foreign/ambiguous authority. Classify all intended contributors before returning a complete aggregate.
5. Replace `WORK_VIEW | LOGISTICS_VIEW => true` with capability facts from the actual source/connector owner. Inspect the descriptor contract and source grant resource mapping; do not copy the App connector switch into a second competing Context list. If the current descriptor lacks required information, narrow the owner API rather than inventing wildcard compatibility.

This is an immediate repair to current multi-source semantics. Explicit assignment target selection is 04, not a reason to defer R1. Do not introduce a parallel next reader.

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

### Regression cases

Use actual product policies and `ContextToolService` in one test path. Cover mail, Work and Logistics, a consumer still excluded after a mutation, an unreviewed extension, and review -> resolve -> linked resume. The successful resume must no longer produce the identical consumer mismatch. Changes in producer, scope, policy or consumer set during review remain rejected.

## 01-D: align the existing result contract without dropping validation

Bring the current Dart parser into agreement with the current Rust contract for positive model calls with a summary, bounded view-call counts and empty insights with a nonempty summary. Keep byte limits, identity, data-class policy, interval and evidence/proposal consistency checks. Reject a summary unsupported by the declared execution outcome and malformed/private fields as required by the current contract.

Update `apps/client/test/features/experts/agent_expert_result_test.dart`, `apps/client/test/features/actions/agent_proposal_test.dart`, and `apps/client/test/support/expert_result.dart` according to the generated fixture. Remove obsolete assertions that `view_calls=2` or summary-only results are necessarily invalid. Add a real one-call Schedule result case, not just permissive parser unit tests. Verify the live `agent_controller.dart` call site and proposal display/inspection path.

The temporary old result contract is removed in 02. The repaired current path and its cross-boundary evidence stay valid until the direct cutover; do not add old/new parsing branches.

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

For R1-R3 report the failing test before the fix, the exact producer/consumer after it, deleted obsolete assertions, and any finding revised by execution. Record the actual shared fixture path/generator so 02 and later checkpoints can reuse it.
