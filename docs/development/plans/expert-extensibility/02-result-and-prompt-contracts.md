# 02: package-owned prompts/results and generic settlement

Prerequisite: 01 complete. Use [the plan's ownership decisions](README.md). This checkpoint is a coordinated contract cutover across producer, Registry/Task persistence, Actions and client. Do not move a type while leaving old consumers compiling through aliases.

## Existing source anchors

| Surface | Files / symbols to inspect and change |
|---|---|
| Prompt contract | `crates/contracts/agent/src/prompts.rs`: `PromptRole`; `crates/experts/builtin/src/prompts.rs`; owner prompt builders and model serialization callers |
| Result contract | `crates/contracts/agent/src/expert.rs`: `ExpertInput`, `ExpertInsight`, `ExpertFocusProposal`, `ExpertResult`, exports in `lib.rs` |
| Model identity | `crates/contracts/agent/src/expert_model.rs`: `EXPERT_INFERENCE_CONSUMER`; `crates/modules/inference/src/application/service.rs`; provider model profile builders |
| Result validation | `crates/modules/experts/src/registry.rs`: result validators; `dispatch.rs`: A2A result conversion; `lib.rs` re-exports |
| Producer and state draft | `crates/experts/builtin/src/host.rs`, `schedule/expert.rs`, `schedule/dispatch.rs`; all other result-producing Experts |
| Settlement and persistence | `crates/app/src/vault_host/conversation_turn/expert_dispatch/stateful_settlement.rs`; `crates/adapters/vault/src/repositories/task.rs`, `vault/registry.rs`, `vault/expert_actions.rs` |
| Actions boundary | `crates/modules/actions/src/application/expert.rs`, `ports/mod.rs`, `domain/origin.rs`; `crates/adapters/vault/src/repositories/expert_actions.rs` |
| Product caller | `apps/client/lib/features/experts/domain/agent_expert_result.dart`, `features/conversation/application/agent_controller.dart`, proposal gateways/renderers and actual protocol conversion callers |

Paths identify existing baseline anchors. Any replacement files are proposed and must be placed at their semantic owner.

## 02-A: freeze the three result boundaries

Before editing, specify the concrete representation and validation owner for each:

| Boundary | Owns / validates |
|---|---|
| Generic Task report | Exact Task/invocation/assignment/package identity, outcome, bounded text/artifacts, coverage and safe interaction references. Reuse `ExpertReport`, `Artifact` and `TaskSnapshot` where sufficient. |
| Expert domain payload | Package-owned Schedule insights, Work judgment, etc.; declared schema/media identifier, bounded encoding and package validation. Shared Registry does not interpret FocusWindow. |
| Consequential proposal | Actions-owned typed proposal, exact target and evidence references; permission review, durable execution intent and reconciliation remain Actions responsibilities. |

An unknown artifact may be shown as bounded safe text or a supported attachment. It is never interpreted as a command, approval, source-access requirement or authority. Do not add a media-type-to-arbitrary-function execution bus. Real source requirement publication remains trusted host output, not model-supplied JSON.

Keep one public generic result shape rather than parallel report/result/envelope families with the same meaning. Distinguish transport bounds from domain validation; opaque payload does not mean unbounded or unvalidated payload.

## 02-B: remove individual Expert roles from shared contracts

Reduce shared prompt role semantics to the necessary runtime roles (Manager/Expert/Learner), using existing `RoleSpec`/prompt composition where possible. Individual package prompt identity/revision and instructions belong to the package. Update every prompt serializer/renderer, provider-facing consumer and fixture together.

Separate the built-in distribution marker in `experts.builtin` from the generic delegated execution role. Preserve current profile selection constraints: root-only profiles must not become universally usable. Actual source identity and consent lineage are still exact; replacing a string with `expert` does not authorize any package to impersonate any other.

Delete unused `ExpertInput` variants after caller-zero proof. If an input still has legitimate Schedule callers, make it Schedule-private and migrate them. Do not invent new runtime callers to justify keeping it public.

## 02-C: move domain result production and validation

Move Schedule insights and Focus proposal assembly into Schedule and Actions as appropriate. Delete `ExpertInsight`, `ExpertFocusProposal` and their semantics from the generic agent contract and Registry. Other Experts must not be forced to emit a Schedule-shaped result.

Change `AgentRegistry::validate_result_content` and related historical/recorded validators to validate package/assignment/state identity and generic report constraints only. The domain owner validates payload before publication; a trusted bridge passes an Actions proposal to Actions validation, not generic JSON execution.

A historical record is validated under its stored identity and evidence. Do not reinterpret it using a newer package's rules silently. Local obsolete profiles need no compatibility decoder; use an explicitly chosen fresh development profile when stored meaning changes, preserving uncertain external-effect records.

## 02-D: make state settlement domain-neutral

Remove Focus-specific data assembly from `StatefulExpertDraft` and `VaultStatefulExpertSettlement`. Use bounded, owner-validated state/result changes associated with the exact package and assignment. Registry/Task identity checks, expected state revision and atomic terminal Task + Expert state settlement remain mandatory.

Do not trust an extension-provided replacement Registry snapshot as an unrestricted state mutation. Apply a validated change only to the admitted assignment; reject mutation of another installation/assignment. No source or model I/O while a Vault transaction is held.

Replace `evidence_for_source`'s domain string interpretation with explicit Context-produced evidence references wherever the generic boundary currently derives observation identity from `calendar.observe:`. A proposal identifies the actual contributing source and destination; do not pick the first dependency for convenience. Multi-source reports retain every dependency even when one Action targets one source.

Action publication must be idempotent across Task settlement/recovery. Keep existing atomicity where supported; otherwise use the existing durable intent/outbox pattern at the owner rather than a best-effort post-commit callback. No external mutation occurs during result settlement.

## 02-E: cut over Actions and Flutter in the same checkpoint

Actions ports consume their own validated proposal/evidence contract plus generic Task origin, not `floe_agent_contract::ExpertResult` with Schedule insights. Preserve exact Person, package, assignment, invocation, evidence, target authority and state checks in `AgentActionOrigin`/proposal inspection.

Flutter's general result surface consumes generic outcome/text/artifacts and Actions DTOs. It does not enumerate commitment/focus/no-focus as the universal Expert schema or use model/view call counts as display validity. Keep schema, identity, size, privacy and safe-rendering validation. A dedicated Calendar Action UI is legitimate; a Schedule-specific common gateway is not.

Port 01's Rust-generated fixture across the new boundary and delete the previous parser/schema, fixtures and imports after all callers move. No dual parser. A single model call, multiple source reads, summary-only and blocked-domain results remain covered at their relevant boundaries.

## Test disposition

| Existing tests / evidence | Treatment |
|---|---|
| Registry inline FocusWindow content tests | Move domain checks to Schedule/Actions; keep generic identity/state tests in Experts. |
| `apps/client/test/features/experts/agent_expert_result_test.dart` | Replace with generic artifact/result tests; remove obsolete call-count/schema assertions. |
| `apps/client/test/features/actions/agent_proposal_test.dart` | Retain proposal safety using actual Actions DTOs. |
| `crates/app/src/vault_host/tests/proposals.rs`, `expert_actions.rs`, `expert_actions/inspection.rs` | Rewrite fixtures at new owner boundaries; retain approval, evidence, response-loss and uncertain-write cases. |
| Task/Vault settlement tests | Preserve atomic rollback, foreign assignment rejection, duplicate invocation and stale state conflict. |
| Package reasoning tests | Keep Schedule domain behavior under Schedule; other Expert results must not require Focus-shaped fixtures. |

Add negative tests for forged requirement artifacts, unrecognized executable-looking media types, mutation of another assignment and a proposal tied to the wrong contributor. Add a real generic report -> persisted Task -> product DTO -> Dart fixture test.

## Deletion and verification gate

Search production and tests for the old shared role variants, `ExpertInput`, `ExpertInsight`, `ExpertFocusProposal`, Schedule-shaped `ExpertResult`, `StatefulFocusProposal`, `experts.builtin`, old result MIME assumptions and Focus validation in Registry/App. Schedule-local concepts and Actions-owned Calendar semantics are allowed; explain each remaining match. Delete re-exports and dead constructors, not only their callers.

Run the Rust gate and full affected product gate in [06](06-verification.md), plus settlement/recovery tests. Update current runtime/owner documentation to the implemented result boundary; package and binding implementation targets remain future work until their checkpoint.

02 is complete only when generic Task/Registry/settlement/client code no longer interprets Schedule result semantics, and 01's behavior is still covered through the replacement contract.
