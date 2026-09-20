# Stage 2 — Canonical Internal Runtime

**Status: active**

Stage 2 answers: **does the actual internal runtime go through the owners established in Stage 1?**

Stage 1 established ownership. Stage 2 connects General Conversation to those owners, removes internal compatibility ownership from App/Conversation, and converges on one canonical model/tool/delegation execution architecture.

Operating rule:

> minimum sound foundation → vertical canonical path → old-path cutover → integrated hardening.

Detailed work is split into the execution plans linked below. This overview owns the Stage 2 progress checkboxes and current checkpoint.

## Progress

- [x] **2-A — Compile and test ownership restoration** — [execution plan](stage-2/2-a.md)
- [ ] **2-B — General Conversation canonical runtime cutover** — [execution plan](stage-2/2-b.md)
  - [x] **2-B.1 — Runtime contract and recovery foundation** — [final P0 plan](stage-2/2-b1.md)
    - [x] typed model conversation and authorized projection
    - [x] durable validated batches and stable execution/batch/ordinal identities
    - [x] durable model usage, cursor, Tool/Delegation binding and single-journal recovery
    - [x] cross-run pending batch → child resume lineage
    - [x] freeze 2-B.1 after the lineage P0 closes
  - [x] **2-B.2 — Canonical Model vertical slice** — [execution plan](stage-2/2-b2.md) · [final current-authority P0](stage-2/2-b2-current-authority.md)
  - [x] **2-B.3 — Canonical Context projection and Tool vertical slice** — [execution plan](stage-2/2-b3.md)
  - [x] **2-B.4 — App production cutover** — [execution plan](stage-2/2-b4.md)
  - [ ] **2-B.5 — Integrated 2-B hardening**
- [ ] **2-C — Delegation ownership convergence** — [execution plan](stage-2/2-c.md)
- [ ] **2-D — Internal compatibility cleanup** — [execution plan](stage-2/2-d.md)
- [ ] **2-E — Internal public-surface closure** — [execution plan](stage-2/2-e.md)
- [ ] **2-F — Stage 2 final validation** — [execution plan](stage-2/2-f.md)

## Current checkpoint

**Active: 2-B.5 — Integrated 2-B hardening.**

2-B.1 through 2-B.4 are complete. General Conversation pre-turn model/source discovery is removed, ConversationTurnRequest.remote_route is gone, source transport is split from Inference RemoteRoute, the root Expert catalog comes from Experts, eager App Model/policy wiring is removed, and zero-caller General Conversation compatibility is deleted with only LegacyDelegationPort retained until 2-C.

2-B.5 has no execution plan document yet. Do not start 2-B.5 implementation from this overview; wait for its plan.

## Target internal architecture

Model:

~~~text
Conversation
  → Conversation/Context model projection
  → AuthorizedModelProjection
  → Agent Engine
  → InferenceService : ModelPort
  → Access model-dispatch admission/fence
  → prepared Provider transport
~~~

Tool:

~~~text
Engine
  → ContextToolService : ToolPort
  → Context / Access
  → authorized source read
  → ToolResult { coverage, artifacts, issue }
~~~

Delegation during 2-B may temporarily remain:

~~~text
Engine
  → LegacyDelegationPort
  → Experts TaskCoordinator
  → staged endpoint context
~~~

2-C removes that final internal bridge.

## Stage-wide rules

- App is composition, not model/source/access policy.
- Context owns projection/source semantics, not model route.
- Inference owns model profile/route/attempt/usage, not source authorization.
- Access owns exact-recipient and authority fences.
- Provider owns transport and private credential use.
- Experts own Task/A2A semantics.
- Do not introduce permanent `v2`/`v3`/`next` paths or rename legacy bridges into equivalent wrappers.

### Issue triage

**P0 — current slice blocker:** authority/data-release risk, duplicate external effects, budget resurrection/double accounting, stable execution identity corruption, loss of durable pending work followed by a different side-effect plan, or an ownership problem blocking the next vertical cutover.

**P1 — integrated hardening:** rare malformed-state fail-close improvements, extra corrupt-journal validation and error-classification refinement. Handle in 2-B.5.

**P2 — cleanup:** naming, visibility, dead code, aliases, fixture cleanup and API ergonomics. Handle in 2-D/2-E.

## Stage 2 exit gate

- `LegacyModelPort` production callers = 0.
- `LegacyToolPort` production callers = 0.
- `LegacyDelegationPort` = 0 after 2-C.
- production `ModelPort` = `InferenceService`.
- production `ToolPort` = `ContextToolService`.
- canonical model projection is Conversation/Context owned.
- App General Conversation model-selection policy = 0.
- raw bearer/token in Inference public API = 0.
- workspace tests/checks and architecture boundaries are green.

Outer FFI/Flutter/native/server DTO and caller cleanup is Stage 3.
