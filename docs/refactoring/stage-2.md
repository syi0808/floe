# Stage 2 — Canonical Internal Runtime

**Status: complete · frozen**

Stage 2 answers: **does the actual internal runtime go through the owners established in Stage 1?**

Stage 1 established ownership. Stage 2 connects General Conversation to those owners, removes internal compatibility ownership from App/Conversation, and converges on one canonical model/tool/delegation execution architecture.

Operating rule:

> minimum sound foundation → vertical canonical path → old-path cutover → integrated hardening.

Detailed work is split into the execution plans linked below. This overview owns the Stage 2 progress checkboxes and current checkpoint.

## Progress

- [x] **2-A — Compile and test ownership restoration** — [execution plan](stage-2/2-a.md)
- [x] **2-B — General Conversation canonical runtime cutover** — [execution plan](stage-2/2-b.md)
  - [x] **2-B.1 — Runtime contract and recovery foundation** — [final P0 plan](stage-2/2-b1.md)
    - [x] typed model conversation and authorized projection
    - [x] durable validated batches and stable execution/batch/ordinal identities
    - [x] durable model usage, cursor, Tool/Delegation binding and single-journal recovery
    - [x] cross-run pending batch → child resume lineage
    - [x] freeze 2-B.1 after the lineage P0 closes
  - [x] **2-B.2 — Canonical Model vertical slice** — [execution plan](stage-2/2-b2.md) · [final current-authority P0](stage-2/2-b2-current-authority.md)
  - [x] **2-B.3 — Canonical Context projection and Tool vertical slice** — [execution plan](stage-2/2-b3.md)
  - [x] **2-B.4 — App production cutover** — [execution plan](stage-2/2-b4.md)
  - [x] **2-B.5 — Integrated 2-B hardening** — [execution plan](stage-2/2-b5.md) · [credential-seam P1](stage-2/2-b5-credential-seam.md) · [final credential-read P0](stage-2/2-b5-credential-read.md)
- [x] **2-C — Delegation ownership convergence** — [execution plan](stage-2/2-c.md)
- [x] **2-D — Internal compatibility cleanup** — [execution plan](stage-2/2-d.md)
- [x] **2-E — Internal public-surface closure** — [execution plan](stage-2/2-e.md)
- [x] **2-F — Stage 2 final validation** — [execution plan](stage-2/2-f.md)

## Closure

Stage 2 is closed. 2-F proved the combined 2-A through 2-E result is one sound
canonical internal runtime: the General Conversation root goes through
`ConversationModelProjection` / `InferenceService` / `ContextToolService` /
`TaskCoordinator` with recovery/authority/accounting invariants preserved, zero
internal legacy ports, and the 2-E public-surface closure intact.

There is no active Stage 2 checkpoint. Continue with
[Stage 3](stage-3.md); the next checkpoint is
[3-A — Remaining Root and Domain Caller Convergence](stage-3/3-a.md).

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

Delegation after 2-C:

~~~text
Engine
  → Experts TaskCoordinator : DelegationPort
  → Directory endpoint resolution
  → EndpointInvocation { DelegationRequest + explicit execution context }
~~~

2-C removed the final internal bridge (LegacyDelegationPort and staged endpoint context).

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
