# Expert access and conversation interaction convergence

- **Status:** execution plan; Checkpoints 01–05 complete, Checkpoint 06 active, 06-A next.
- **Original baseline:** `0615b343bb752da85487a1a728927f0e3affdf5b` (historical).
- **Current Checkpoint 06 source baseline:** `main` at `c137b35bf44ea12b1d2f5aa57b2f86aa6d0f1761`.
- **Completed code baseline:** Checkpoint 05 code at `27299093263fb4998e59f800f663c5e604f22ac3`, verification/docs closure at `c137b35bf44ea12b1d2f5aa57b2f86aa6d0f1761`.
- **Scope:** Expert runtime, source access, connection permission UX, Conversation-owned interaction, linked resume, and final deletion/document convergence.
- **Product priority:** macOS/Apple. iOS device validation is deferred to active iOS development; Android parity is not part of this work.

This directory is the one temporary execution plan for this refactor. Current architecture documents own durable topology; child plans own sequencing and evidence. Do not treat old code anchors inside completed checkpoints as instructions to recreate deleted paths.

## 1. Current state and reading order

Checkpoints 01 through 05 are complete. Do not reimplement them to start Checkpoint 06.

| Checkpoint | State | Entry document |
|---|---|---|
| 01 — contracts and recoverable outcomes | Complete | [01](01-contracts-and-interaction-foundation.md) |
| 02 — ordinary built-in Schedule runtime | Complete | [02](02-calendar-source-and-schedule-convergence.md) |
| 03 — Registry/Access separation, including 03-E | Complete | [03](03-expert-registry-and-access-authority.md) |
| 04 — connection permission product model | Complete | [04](04-connector-permission-product-model.md) |
| 05 — durable interaction and linked resume | Complete | [05 index](05-conversation-and-flutter-interaction.md) |
| 06 — final deletion, archive and documentation convergence | Active; 06-A next | [06 index](06-deletion-verification-and-doc-convergence.md) |

For current implementation, read `AGENTS.md`, the architecture-change skill, the current architecture documents, the [06 index](06-deletion-verification-and-doc-convergence.md), and only the active child:

1. [06-A — recorded provenance and budget-continuation convergence](06-a-recorded-provenance-and-continuation.md) — **next**.
2. [06-B — legacy model transport and saved-consent deletion](06-b-legacy-inference-and-consent-deletion.md).
3. [06-C — residual public surface and durable docs convergence](06-c-residual-surface-and-doc-convergence.md).
4. [06-D — final verification and execution-plan archive](06-d-final-verification-and-plan-archive.md).

Historical detail remains in the completed checkpoint documents. The Checkpoint 03 index links 03-A through 03-E; the Checkpoint 04 index links 04-A through 04-F and their completion SHAs. No parallel status ledger is needed.

## 2. Why this work began

Schedule formerly bypassed the common built-in endpoint and coupled Calendar acquisition/authorization to Calendar-specific Registry setup, grant mappings and Flutter settings. UI Active did not imply runtime admission, and missing permission could terminate a request with no usable recovery path.

That infrastructure split is gone. The remaining product objective is that a blocked source becomes an honest Manager/Expert observation, a durable user interaction, and a fresh linked Run after verified resolution.

## 3. Final topology

```text
Manager -> ordinary BuiltinExpertEndpoint / Context tool
        -> Context source acquisition -> Access -> provider/native boundary

recoverable source or eligible processing-consent blocker
        -> Conversation-owned interaction (trusted App composition)
        -> settled Tool/Task/model observation + safe reference
        -> assistant limitation + interaction message
        -> original Run Completed

explicit user decision
        -> fresh owner validation + reviewed CAS
        -> owner mutation/reconciliation
        -> Conversation resolution
        -> one linked fresh Run
```

| Concern | Semantic owner |
|---|---|
| Expert package/install/assignment/private state | Experts / Registry |
| Connection/account/resource/system authority | Connections and real provider/native owners |
| Observe grant and consumer-policy authority | Access / DataAccessGrant |
| Acquisition, merge and each source dependency | Context |
| Exact model recipient consent and dispatch/release fencing | Access, composed with Inference/provider route facts |
| Route selection, model attempt/budget/transport policy | Inference |
| Session, interaction lifecycle, resolution linkage, resume admission | Conversation |
| Cross-owner orchestration and canonical first-party product policy | App |
| Presentation and explicit user intent | Flutter |
| External actions and uncertain-effect recovery | Actions |

A Conversation interaction is neither a DataAccessGrant nor a model dispatch permit. A successful resolution never authorizes reuse of old source bytes.

## 4. Invariants carried into Checkpoint 06

- Registry remains source-independent. No `SourceGrants`, Calendar-Expert setup, `experts.calendar.*` compatibility path or `calendar.expert` proxy may return.
- Use with Floe is a projection of current source plus grant bundle, not a persisted enable bit. Startup/inspect does not mint grants.
- First-party consumers are App policy. Flutter/model output cannot select consumers, scope, purpose or processing restrictions.
- Resolve compares current owners with the exact target and scope the user reviewed. Reloading current state is not permission to substitute a new expected authority.
- Each contributing source retains its own DataAccessGrant, grant/policy/source authority and ContextDependency. Multi-source merge never synthesizes aggregate authority.
- Unknown, unavailable and denied data are not empty successful evidence. History, compaction, proposal inspection and dispatch/release retain dependency fencing.
- Missing eligible recipient consent is recoverable; source LocalOnly, forbidden data classes, forged identity, corruption and arbitrary policy denials are not permission buttons.
- Observe, Act and external processing remain separate. Resolving an interaction never executes or blindly retries an external action.
- No Run lease/deadline remains alive for human waiting. Interaction resume is not budget continuation and does not inherit a pending batch or old successful evidence.
- Queries, previews, observer timeouts and screen disposal do not cancel Runs or mutate authority.
- No Vault transaction spans model, provider, OAuth or native I/O.

## 5. Local data and compatibility

Floe is pre-stable. Replace in-scope internal contracts and callers together; delete obsolete variants rather than adding v2/legacy wrappers or migration-only optional state. Fixed schema numbers are not proof of old-profile compatibility.

Use only an explicitly selected isolated development profile for changed persistence acceptance. Old unsupported schemas fail closed. Never auto-delete a database/key after an access error, reset the normal operator profile, delete provider data, or discard uncertain external-operation records.

## 6. Completion and reporting

Each 06 child closes its named transitional surface and focused regressions before the next child starts. Passing a broad suite is not proof that caller-zero, provenance reauthorization, request-scoped transport consent, or deletion gates are satisfied.

Checkpoint 06 owns the last architectural convergence plus the archive of this temporary plan. 06-D deletes this entire directory from the active documentation tree only after code, current architecture, product/ADR docs, residual searches and full applicable gates are green. Git history is the archive; no replacement status ledger is created.
