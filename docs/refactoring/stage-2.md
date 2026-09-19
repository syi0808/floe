# Stage 2 — Canonical Internal Runtime

**Status: active**

Stage 2 answers: **does the actual internal runtime go through the owners established in Stage 1?**

Stage 1 established ownership. Stage 2 connects General Conversation to those owners, removes internal compatibility ownership from App/Conversation, and converges on one canonical model/tool/delegation execution architecture.

Operating rule:

> minimum sound foundation → vertical canonical path → old-path cutover → integrated hardening.

See [Stage 1](stage-1.md) for ownership and [Stage 3](stage-3.md) for product-boundary/FFI/Flutter cutover.

## Target internal path

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

Delegation may remain the one explicit compatibility bridge until 2-C:

~~~text
Engine
  → LegacyDelegationPort
  → Experts TaskCoordinator
  → staged endpoint context
~~~

## Current checkpoint

Stage 2-A is complete and Stage 2-B is active.

2-B.1 has established typed model conversation, authorized projection, durable validated batches, stable execution/batch/ordinal identities, bounded correction, recovery cursors, durable model-usage recovery and exact single-journal Tool/Delegation binding.

The remaining P0 item before freezing 2-B.1 is **cross-run continuation lineage**: a child run may take over a parent's pending validated batch only by durably re-journaling the exact batch and starting cursor. A child that crashes before takeover must not silently discard the parent's pending batch.

After that fix, **2-B.1 freezes**. Further non-P0 recovery hardening moves to 2-B.5.

## 2-A — Compile and test ownership restoration

**Status: done**

- Restore workspace compilation after Stage 1 moves.
- Put tests with semantic owners.
- Remove stale reverse dev-dependency pressure.
- Close migration regressions without preserving old ownership through wrappers.

2-A is not reopened for later runtime changes.

## 2-B — General Conversation canonical runtime cutover

Objective:

> remove `LegacyModelPort` and `LegacyToolPort` from the production General Conversation path.

### 2-B.1 — Runtime contract and recovery foundation

Required foundation:

- typed `ModelConversation`;
- immutable `AuthorizedModelProjection`;
- whole-batch validation before the first side effect;
- durable `ValidatedModelBatch` before Tool/Delegation execution;
- stable identity from execution + batch + ordinal + kind;
- durable cursor and call/result recovery;
- pending validated batch resumes without model re-call;
- Engine-owned invalid-output correction, never a fake User message;
- model attempt usage cannot disappear across restart/continuation.

#### Freeze rule

After cross-run pending/resume lineage is closed and focused regressions are green, 2-B.1 is frozen.

Reopen only if a finding can cause:

1. duplicate external side effects;
2. unauthorized data release;
3. loss of durable pending work followed by generation of a different side-effect plan.

Other recovery hardening goes to 2-B.5.

### 2-B.2 — Canonical Model vertical slice

Treat Access dispatch, Inference ownership and Provider adaptation as one vertical cutover.

Target:

~~~text
Engine
  → InferenceService : ModelPort
  → Access DispatchPermit / DispatchFence
  → Prepared Provider Transport
~~~

Access owns exact recipient, purpose/consumer binding, projection/coverage/data-class binding, revocation/lease fencing, pre-handoff consume and post-response authority revalidation.

Inference owns non-secret model-profile observation, explicit/Auto profile selection, route/recipient planning, attempt lifecycle, model usage/budget and transport retry/fallback. The same caller-provided attempt ID follows the entire attempt.

Provider owns HTTP/native transport, wire conversion, timeout/body/redirect/no-proxy safeguards, private credential use and provider identity/response validation. Provider does not own Access or Context policy.

2-B.2 production exit:

- General Conversation no longer calls `LegacyModelPort`.
- Canonical production `ModelPort` is `InferenceService`.
- Inference public API carries no raw bearer/token.
- Model discovery is separated from source connector catalog.
- App no longer chooses Foundation vs Server for General Conversation.

### 2-B.3 — Canonical Context projection and Tool vertical slice

Projection:

~~~text
Conversation durable transcript
  → Conversation history projection
  → Context assembly
  → AuthorizedModelProjection
~~~

Context does not choose model route. Inference does not call a concrete Context implementation.

Tool:

~~~text
Engine
  → ContextToolService : ToolPort
  → Context / Access
~~~

Exit:

- Context-owned `ToolDescriptor` catalog is the Manager execution truth.
- `ToolResult.coverage` is returned directly.
- App capability IDs no longer implement business dispatch.
- Tool/source availability is independent from model route.
- `TransitionalModelProjection` and `LegacyToolPort` have zero production callers.

### 2-B.4 — App production cutover

App becomes composition-only for General Conversation.

Remove production ownership of:

- `LegacyModelPort` and `LegacyToolPort`;
- Foundation/Server model branching;
- `RootModel` / `GovernedModel` routing policy;
- App-side model consent and route selection;
- `ConversationCapabilities` business dispatch;
- compatibility conversions used only by the old General Conversation path.

Remove pre-turn model-route resolution. Conversation admits user intent; Inference resolves the model attempt later.

Expected wiring:

~~~text
ConversationService
  projection = canonical Conversation/Context projection
  model      = InferenceService
  tools      = ContextToolService
  delegation = LegacyDelegationPort   # until 2-C
  validator  = ManagerPayloadValidator
~~~

### 2-B.5 — Integrated 2-B hardening

This is the deliberate deep-review point.

Review together:

- Security: exact recipient, revoke-before-handoff, revoke-during-call/post-response rejection, history/source reauthorization, provenance/coverage/freshness, secret boundaries.
- Recovery: validated batch before effects, pending-batch resume, cross-run lineage, stable Tool/Task identity, cursor/result pairing.
- Accounting: one attempt ID, one model usage owner, no double settlement, finalization reserve.
- Ownership: App business policy absent, Inference has no concrete Context dependency, Provider does not own Access policy.

## 2-C — Delegation ownership convergence

Remove `LegacyDelegationPort`, staged run-id endpoint context and App stage/clear.

Target:

~~~text
Engine
  → DelegationPort
  → Experts-owned Task lifecycle
  → explicit endpoint request
~~~

Expert input becomes explicit contract data rather than hidden mutable staging state.

## 2-D — Internal compatibility cleanup

After production caller cutover, remove old internal APIs such as old Conversation model contracts, `CapabilityHost`, stale aliases, route compatibility and fixture-only entry points no longer needed by product callers.

Rule: **cut callers first, delete compatibility second**.

## 2-E — Internal public-surface closure

Reduce exports to actual owner contracts. Remove obsolete re-exports, transition constructors, compatibility aliases and accidental test-only production surface.

Outer product DTO cleanup belongs to Stage 3.

## 2-F — Stage 2 final validation

~~~sh
cargo check --workspace --lib
cargo test --workspace --no-fail-fast
python3 tools/architecture/check_boundaries.py
git diff --check
~~~

Stage 2 exit requires:

- `LegacyModelPort` production callers: 0.
- `LegacyToolPort` production callers: 0.
- `LegacyDelegationPort` removed after 2-C.
- Production `ModelPort` = InferenceService.
- Production `ToolPort` = ContextToolService.
- Canonical projection owned by Conversation/Context.
- App General Conversation model-selection policy: 0.
- Raw bearer/token in Inference public API: 0.
- Workspace and architecture checks green.

## Issue triage

### P0 — current slice blocker

Fix immediately only for authority/data-release risk, duplicate external effects, budget resurrection/double accounting, stable execution identity corruption, loss of durable pending work followed by re-planning, or ownership direction that blocks the next canonical cutover.

### P1 — 2-B.5 integrated hardening

Additional corrupt-journal detection, rare malformed-state fail-close improvements, error-classification refinement and defensive validation that do not change authority/side-effect safety.

### P2 — 2-D / 2-E cleanup

Naming, visibility, dead code, compatibility aliases, fixture cleanup and API ergonomics.

## Stage 2 boundary

Stage 2 succeeds when Floe's **internal General Conversation runtime** follows one canonical owner architecture.

It does not require every outer FFI/Flutter/native/server caller or old product DTO to be deleted. Those are Stage 3 concerns.
