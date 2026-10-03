# S2 Action recovery boundary proposal

Read-only design during the pending G1 gate. No Action implementation or live Calendar operation is performed by this document.

## Accepted foundation

Keep the approved single encrypted ActionsRepository, immutable Create/Update/Delete effect variants, exact ExecutionId, predispatch Executing transaction, owner-driven decide/dispatch, and idempotent Day collection ticket. A locked Vault exposes unavailable Action operations. Do not introduce a plaintext recovery ledger, origin-based store lookup, automatic replay, or a second external write key.

## Current-source review of Claude Korean F1

The current EventKit `lookup` branch (`apps/client/macos/CalendarActions/EventKitActions.swift`) explicitly rejects a delete mutation, and accepts an update only when the exact current event matches the requested result. Its shared error shape does not distinguish a failure before invoking the write from a failure during/after it. Rust timeout does not prove that the native thread stopped. These are confirmed implementation limits, not evidence that an operation did or did not happen.

The accepted contract already requires inconclusive lookup to remain Unknown. Therefore inability to prove every deletion after a lost acknowledgement must not be “fixed” by labeling NotFound as success/failure, treating the previous value as proof of no effect, dropping Delete/Update support, or deleting the uncertain record. A terminal answer for every possible external failure is not available from the current evidence.

## Required typed adapter outcomes

Define effect-specific native requests and a closed result union owned by Actions:

- `Committed { execution_id, effect_digest, receipt }`: a matching authoritative native write acknowledgement and effect receipt. Validate exact Person, source/calendar, external identity and effect fields.
- `NotApplied { execution_id, effect_digest, proof }`: positive trusted adapter evidence that this exact invocation never crossed the external write boundary. A proof records its prewrite failure class and exact admission identity; timeout alone cannot construct it.
- `Unknown { execution_id, effect_digest, reason }`: a write may have crossed the boundary, an acknowledgement was lost, or receipt identity cannot be validated.

The native adapter must track the dispatch boundary explicitly. Validation, current permission, source/target identity, expiry, cancellation and expected revision are checked before `save`/`remove`; a definite rejection before that boundary may return NotApplied. Once the external write method is invoked, an exception or cancellation remains Unknown unless the provider supplies authoritative non-application evidence. Do not broadly classify all SDK errors as proof.

Native requests carry an admitted Person/executor identity supplied by the Actions owner. Remove the fixed development Person guard, but do not replace it with caller-controlled JSON authority. The App/native lane binds the request to the same verified host and exact outstanding effect admission.

## Bounded lookup by effect

- Create: inspect the existing exact operation marker and expected source/window. A unique identity- and content-matching observation may establish the recorded effect. Zero, multiple, foreign or mismatched matches remain inconclusive.
- Update: inspect the exact reviewed external target identity and expected resulting effect. Preserve the distinction between a committed write receipt and a later observed matching postcondition. If the current receipt contract requires causal acknowledgement, a matching state alone is insufficient; choose the typed evidence semantics before implementation. The old value alone never proves non-application, because later changes or delayed execution may exist.
- Delete: ordinary absence is not a NotAppliedProof. Success after response loss needs an authoritative tombstone/operation receipt for the exact target. If EventKit cannot supply one, retain Unknown and show a truthful bounded recovery result. No hidden repeated deletion is allowed.

Before claiming a negative result, account for the native invocation's lifetime. A Rust deadline cannot make a still-running native write disappear. A repeated observation may be allowed after the original native operation is quiescent, but quiescence alone does not establish a provider outcome.

## Ownership and future evidence

Actions decides state transitions from typed evidence; the native adapter only reports bounded physical evidence. Vault stores the exact intent/outcome/collection transaction. FFI and Flutter copy safe owner status/actions; no approval→execute→collect orchestration survives in Flutter.

S3 must reconstruct lost-acknowledgement, delayed native completion, create marker collision, update changed-back, delete absent/permission-redacted, lock-after-dispatch, process restart and repeated-lookup cases. No tests are added before S2 structural closure. Actual EventKit behavior and supported authoritative evidence must be verified against the installed Apple SDK when designing the final adapter; this proposal does not assert undocumented SDK guarantees.

## Coordinator recommendation for implementation

Use conservative causal evidence for terminal success. A matching Update postcondition without an operation-specific receipt does not by itself establish that this execution committed; keep Unknown. Native Update/Delete remain fully supported on a normal matching write acknowledgement. Their existence is not conditional on guaranteeing recovery after every possible process crash.

Add bounded, non-authoritative physical outcome readback inside the native adapter: after an actual SDK invocation finishes, retain its exact Committed/NotApplied/Unknown result by execution ID + effect digest + admitted Person/executor identity until acknowledgement or a fixed bounded retention expiry. Actions can consult this readback before fresh provider lookup. It may recover a lost Rust/FFI acknowledgement or late native completion in the same host process. This cache never grants approval, decides Action state, dispatches on lookup, becomes a plaintext durable action journal, or claims absence after eviction/restart. The encrypted Actions record remains authoritative. A missing native receipt means unknown evidence, not NotApplied.

Recovery is not a new dispatch. Original action/approval expiry must not erase a known committed result or prevent readback of its exact historical receipt. Fresh physical lookup still requires current lookup permission and bounded scope, but cannot create a new write authorization. Source/approval revocation after handoff preserves the immutable effect uncertainty and may later settle an authentic historical receipt.

The default user-visible behavior is retained Unknown plus truthful safe observation/reconciliation options. No new “acknowledge as failed/succeeded,” hidden cleanup, or user-dismissal transition is introduced by this refactor. If a future product wants an acknowledgment/archival feature for permanently inconclusive effects, its label and consequences must be explicitly decided; acknowledgment must never unlock automatic replay or fabricate outcome proof.
