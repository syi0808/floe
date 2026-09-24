# Checkpoint 05 — Conversation-owned interaction and linked resume

- **Status:** active execution plan; no implementation completion claimed.
- **Source baseline:** `00017e668e71e34be3d3ea5772aa1612b15c5d6c`.
- **Prerequisite:** Checkpoints 01–04 complete, including 03-E CAS and 04 atomic multi-view grants.
- **Next execution:** 05-A. Checkpoint 06 is blocked until 05-G passes.

This index replaces the earlier single-file sketch. Child plans are the authoritative execution sequence. Anchors below refer to the source baseline, not the planning commit that contains these files. Line ranges are inspected reading windows; symbols are authoritative as lines move.

## 1. Read and execute in order

| Slice | Goal | Exit gate |
|---|---|---|
| [05-A](05-a-contracts-and-durable-interactions.md) | Typed contracts, durable interaction/review/decision records | Origin/replay/CAS/reopen tests |
| [05-B](05-b-source-outcomes-and-publication.md) | Preserve source blockers through trusted publication and Manager/Expert results | Direct/delegated blocked Run completes with durable reference |
| [05-C](05-c-reviewed-resolution-and-owner-operations.md) | Resolve through canonical source/Access owners | Stale approval and response-loss recovery cannot widen/repeat mutations |
| [05-D](05-d-processing-consent-and-inference.md) | Access-owned contextual recipient consent and typed pre-dispatch blockage | No unapproved transmission; first-model blockage still produces a usable turn |
| [05-E](05-e-linked-resume-and-recovery.md) | Exactly one linked fresh Run admission | Resolution/admission/restart races and action-safety tests |
| [05-F](05-f-protocol-and-flutter.md) | Owner wire, snapshot/event resync, inline cards and native recovery | Real App route → card → decision → same Session follow-up |
| [05-G](05-g-verification-and-convergence.md) | Cross-owner crash/failure matrix, deletion and docs | All named behaviors plus applicable full gates |

Read `AGENTS.md`, `.agents/skills/architecture-change/SKILL.md`, `docs/architecture/README.md`, `invariants.md` and `authority-recovery.md` first. Read other owner documents only as needed. Finish code work with the code-change-verification skill. Do not re-run completed checkpoint implementation plans as tasks.

## 2. Verified baseline gaps

| Inspected path/window | Current behavior to change |
|---|---|
| `crates/contracts/context/src/source_access.rs:1-158` | Six requirement kinds; optional source/connection/authority; inline flag is not enough to authorize a mutation |
| `crates/contracts/agent/src/interaction.rs:1-44` | Pure ref exists; status uses `Allowed`; it is not durable Conversation state |
| `crates/experts/builtin/src/shared.rs:24-39` | `optional_calendar_views` loses requirement details by mapping NeedsUserAction to Denied |
| `crates/modules/context/src/application/tools.rs:1-310` | Direct tools still return Result<ToolResult, AgentFailure>; no trusted interaction publication boundary |
| `crates/modules/context/src/application/remote_sources.rs:223-327` | Multi-source acquisition returns per-source AuthorizedSourceBinding; preserve this when adding typed blockers |
| `crates/app/src/vault_host/conversation_turn/expert_dispatch.rs:95-230` | Common endpoint already binds Session/device/parent Run and wires Context/Inference; use this trust boundary |
| `crates/modules/conversation/src/turn/session.rs:123-252` | No Interaction message; current message/event/history matches require coordinated updates |
| `crates/modules/conversation/src/domain/mod.rs:101-290` | Working/terminal Run states; TurnMode only New/Continue; receipt linkage is continuation/retry only |
| `crates/modules/conversation/src/domain/intent.rs:32-169` | Canonical digest and validation encode only New/Continue; resume cannot be an extra Flutter text message |
| `crates/modules/conversation/src/application/coordinator.rs:61-290` | Admission, session revision, transcript, budget continuation and EngineResumeState are coupled |
| `crates/adapters/vault/src/repositories/conversation.rs:1-280` | Owner repository maps admission and transcript to Vault; archive projection also needs interaction handling |
| `crates/app/src/local_access_services.rs:20-171` | Native Calendar has reviewed expectations and owner operations; reuse rather than recreate a Calendar Expert API |
| `crates/app/src/remote_services.rs:137-200` | ConnectionObserve command has no caller-reviewed bundle expectation; add it to the canonical owner path, not a chat-only mutation bypass |
| `crates/app/src/connection_observe.rs:1-133` | Derived bundle projection; not an authority receipt or proof of source read admission |
| `crates/app/src/first_party_observe.rs:1-128` | App owns real first-party consumers; native Calendar LocalOnly and remote paired-source restrictions remain authoritative |
| `crates/modules/access/src/application/model_dispatch.rs:26-231` | Recipient, dependency and processing checks return AgentFailure; LocalOnly and forbidden-class denials must not become consent approvals |
| `crates/modules/inference/src/application/service.rs:76-285` | Only Result<ModelResponse, AgentFailure>; pre-reserve admission, handoff and release fences; no typed recoverable model outcome |
| `crates/adapters/providers/src/control/recipient_authority.rs:1-71` | SavedConnectionRecipientAuthority reloads legacy saved consent fields; not a contextual consent store |
| `apps/client/lib/features/conversation/application/conversation_runtime_gateway.dart:77-270` | Standard Run polling and command recovery exist; interaction updates after Run completion need separate snapshot/refresh support |

These gaps are implementation work for 05, not proof that tests were run during plan authoring. Code exploration for a slice must extend these anchors to all callers, manifests and tests before editing.

## 3. Fixed final behavior

```text
source/tool/Expert -> owner-produced recoverable requirement
  -> trusted App/Conversation create-or-replay
  -> settled safe Tool/Task reference
  -> Manager limitation + immutable Interaction message
  -> original Run Completed

person decision -> Conversation decision CAS
  -> existing source/Access owner operation with reviewed target binding
  -> fresh outcome/reconciliation -> interaction Resolved
  -> one origin-linked fresh Run -> new tools/reads/dependencies
```

If the selected model cannot be called because eligible recipient consent is missing, no unauthorized model is called to explain the denial. Conversation emits a deterministic, source-independent limitation plus the same durable interaction. This is a distinct typed expected completion, not a forged ModelResponse or a generic catch of PolicyDenied.

Manager natural explanation remains the ordinary path when an approved model is available. An optional blocker may coexist with useful authorized evidence; coverage of that evidence must not be changed to Independent just because a safe card is attached.

## 4. Owner and authority boundaries

Conversation owns interaction identity, origin, immutable reviewed descriptor, lifecycle, decision intent and resume linkage. Access owns actual Observe and recipient authority. Connections/provider/native owners own connection/resource/system changes. Inference selects real routes and supplies non-secret recipient facts. App composes owners. Flutter only presents the backend projection and sends an explicit decision.

Pure contracts may carry references and non-secret descriptors; they do not make Context, Experts or Inference depend on Conversation persistence. Introduce only a real trusted publication/owner-operation boundary, not forwarding-only compatibility layers.

An LLM-produced id, summary, reason, inline flag or JSON artifact never authorizes creation/resolution. Validate the admitted Run/Task/call, actual consumer and owner-observed target. A syntactically valid reference to another Session is still forbidden.

## 5. Deliberate lifecycle choices

- Original Run and Task do not wait for the person. Keep RunState free of `WaitingForUser`.
- Use a Conversation state machine: Pending → Resolving → Resolved; Pending can also become Denied/Cancelled/Superseded/Expired. Resolving is justified by durable owner-operation recovery, not a long-lived Run.
- Resolve/refresh are explicit commands. Get/list/inspect are read-only and never create grants, decisions or Runs.
- No scope is silently refreshed underneath an old Allow button. Material drift invalidates that review and requires a newly presented descriptor/revision.
- Default inline Observe approval is connection-level, including the current first-party bundle. The card discloses all affected source capabilities/resources; do not claim to approve only Calendar if the operation affects a larger bundle.
- An unresolved target with insufficient current identity has navigation/review actions only, not inline enable.
- Recipient consent is Access-owned, exact, time-bounded and limited to the reviewed intent lineage/scope. It never changes source ProcessingRestriction, Act, or saved global allow flags.
- Budget continuation and interaction resume are different domain modes. New resume execution recomputes context and preserves effect safety; it does not claim old batch takeover.

## 6. Limits and multi-interaction policy

Start with explicit tested limits: at most 8 actionable interactions per Run, bounded descriptors, bounded list pagination and a 24-hour maximum pending-review lifetime. Use owner clock injection in tests. Overflow is an honest bounded limitation, never dropping requirements while claiming completion of data acquisition.

Deduplicate only identical origin and reviewed target/scope, never different accounts. Publication identity is based on the admitted invocation and canonical requirement, not projection time or a fresh UUID on replay.

One origin Run has one automatic resume slot. For several cards, automatic resume waits until all relevant cards are terminal and at least one was resolved; Not now/deny is a valid terminal decision. No auto-resume occurs for all-denied groups. Later changed requirements can be surfaced by the new Run. A bounded lineage limit prevents automatic loops; reaching it requires a new explicit user request, not a new budget-continuation level.

If the user has since started another turn, do not silently restart an old request. Expose an explicit Continue original request action after safe current-session review. See 05-E for the atomic admission rule.

## 7. Non-goals and stop conditions

Do not restore Calendar-Expert setup/wire/UI, Registry source authority, a second Use-with-Floe bit, global model consent, arbitrary Flutter grant fields, hidden provider fallback on policy denial, or an aggregate source dependency.

Do not turn every PolicyDenied into NeedsUserAction. Corrupt storage, forged/foreign authority, unknown provenance, prohibited classes, LocalOnly-to-external and mismatched approved recipients remain fail-closed. A source policy review, if later needed, is a separate explicit Access decision, not hidden inside recipient consent.

Do not execute Action proposals from an interaction, retry uncertain external writes, or hold a transaction during provider/model/native I/O. Keep paired-server credentials inside existing current-connection adapters.

Stop and amend the active child with the concrete failing scenario if a contract lacks required provenance, reviewed CAS, operation replay or effect deduplication. Preserve completed CP3/4 semantics; fix the narrow owner contract rather than adding an alternate chat permission system.

## 8. Completion discipline

Each semantic slice commits its code, caller cutover, focused regression evidence, residual deletion and current architecture update together. Short compile breaks within a slice are acceptable; obsolete compatibility interfaces at the slice exit are not.

05-G is the final acceptance source. Record actual executed commands separately from agent-reported prior results. Do not certify macOS, live credentials or device tests that were not run. Only after all acceptance rows pass may the README/index say 05 complete and 06 next.
