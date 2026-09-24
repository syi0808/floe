# 05-E — linked fresh Run admission and recovery

- **Status:** planned; depends on 05-A through 05-D.
- **Exit:** a resolved origin admits at most one automatic child Run, with fresh authorization and no repeated external effects.

## 1. Edit map

| Baseline anchor | Required change |
|---|---|
| `conversation/src/domain/mod.rs:101-290` | Explicit InteractionResumeRef/TurnMode branch and receipt lineage, separate from continuation/retry |
| `conversation/src/domain/intent.rs:32-169` | Normalize/validate/digest resume linkage and reject mixed modes |
| `conversation/src/application/admission.rs`, `coordinator.rs:61-290` | Owner-resolved original intent, atomic linked admission and fresh execution setup |
| `conversation/src/application/recovery.rs`, `finalization.rs` | Resume history and blocked attempt handling; no old pending-batch takeover |
| `vault/src/repositories/conversation.rs:201-280` | Owner-domain ↔ Vault mode/receipt/transcript conversion |
| `vault/src/vault/conversations.rs` (`admit_conversation_turn`) and tests | Unique resume slot, origin linkage, Session CAS and command receipt in one transaction |
| `app/src/vault_host/conversation_turn.rs`, App Run admission/query/worker paths | Dispatch/reconcile linked Run through existing durable runner |

Search every `TurnMode`, continuation receipt field, request digest encoder, run serialization, archive projection and `EngineResumeState`. Compile all same-snapshot callers; do not treat an unknown resume mode as New or Continue.

## 2. Resume semantics

```text
ResumeInteraction {
  validated origin Run / interaction group reference
  resolved revisions/digest
}
```

The backend derives original user intent from the origin's canonical admission/archive. Flutter does not resend original text, choose a parent Run or inject `continue` as a second user message.

Keep original profile preference unless an explicit new review authorizes a changed profile. Recipient consent is still checked against the actual selected candidate. In model context, represent resumption as a host-owned bounded event/reference, not forged User or Tool success. Do not replay untrusted old source output into privileged instructions.

The new Run gets fresh scope, budget ledger and deadline under ordinary limits. Do not inherit budget-continuation level, executor generation, old model attempt id, pending batch/cursor, transport or source receipts. Normal historical evidence can be used only through the existing fresh dependency/coverage reauthorization path.

## 3. One automatic resume slot per origin Run

Deduplication must not depend on the Flutter command id alone. Use a durable unique origin-resume linkage shared by all interactions from that origin.

Automatic admission conditions:

- origin Run is Completed and belongs to current Person/Session;
- all relevant cards are terminal, no Resolving/Pending card remains;
- at least one requirement is Resolved;
- no child is already admitted;
- no newer user turn has superseded the original request;
- source/recipient resolution is still currently valid;
- existing action-effect reconciliation does not block a safe fresh continuation.

A denied requirement remains unavailable and must not be silently approved or immediately re-prompted in a loop. The fresh Manager may proceed with resolved/remaining authorized information while respecting the denied scope. Bound automatic lineage depth and report an honest limitation at the cap, not endless resume or budget resurrection.

## 4. Atomic admission, not a check-then-start pair

In one short Vault transaction, validate Session/origin state, claim the unique resume slot, insert the canonical child command/intent and Run receipt, and link it to the resolved interaction group. Every decision path uses this admission primitive.

A stable resume command identity can derive from the origin slot. Persist its canonical digest/receipt once. Retries first look up the already admitted command and rejoin it before comparing current Session revisions. Never reuse a command id with newly invented text/profile/digest.

Crash after interaction resolution but before child admission is recoverable: explicit refresh/resume or the standard durable recovery dispatcher can admit from the recorded resolved group. Crash after admission but before response/scheduling rejoins the same Run and existing durable execution recovery. A process-local flag, polling controller or launch callback is not the exactly-once boundary.

Do not enqueue a second mutable resume intent whose revision can drift independently from the canonical Conversation command.

## 5. Newer Session activity and multiple cards

If another user turn has begun since the origin finished, suppress automatic restart. Show Continue original request; its explicit command uses the current reviewed Session revision and still claims the same origin slot. Do not cancel the newer Run or insert the old request ahead of it.

Two concurrent Allow/refresh commands resolving different cards may race to admit the group. One transaction wins; the other receives the existing receipt. A later repeat click on any card rejoins, never starts another Run.

A failed admitted child is still the consumed automatic slot. Recovery/retry uses normal explicit child Run semantics, not re-resolving the original card to obtain unlimited retries.

## 6. Fresh authority and action safety

After admission and before every read/model handoff, reauthorize current source/grant/policy/recipient. Revocation between resolution and Run B is an ordinary fresh blocker, never permission inherited from Resolved.

Inspect the origin's settled proposals/action references and uncertain operations through their canonical owners. Resume does not execute an ActionProposal merely because source access is now enabled. Do not blindly regenerate or retry an already committed/uncertain external write.

Where the new Manager can propose the same domain action again, enforce stable effect identity/deduplication at the Actions owner or prohibit new dispatch until existing effects are reconciled. Prompt text alone is not an effect fence. An uncertain provider result stays uncertain/reconcilable. Keep existing `/focus` current evidence validation.

## 7. Required tests

1. Allow → one linked Run with original intent, no duplicate User message, fresh read and dependency.
2. simultaneous duplicate Allow and simultaneous resolution of two cards → one child Run/command.
3. crash before mutation, after owner mutation, after resolution, after admission, after runner dispatch and after response loss → correct single mutation/admission/receipt.
4. restart during Pending/Resolving/Resolved/admitted-child states.
5. deny all cards → no child; mixed resolved+denied → one child without denied-source approval loop.
6. source/policy/recipient revoke after resolution → fresh denial, no stale data release.
7. later user turn → no automatic stale restart; explicit Continue obeys current Session CAS.
8. archive compaction preserves origin retrieval; deleted/unavailable origin cannot fabricate text.
9. prior proposal/confirmed write/uncertain write → no duplicate external effect.
10. existing budget continuation still claims exact validated batch/cursor; interaction resume never does.

Run Conversation/Vault/App/runtime/action suites and inspect durable rows/receipt ids in tests. Exit requires concurrency/failure injection, not merely a sequential happy-path UI test.
