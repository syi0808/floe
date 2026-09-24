# 05-A — contracts and durable Conversation interactions

- **Status:** next; not implemented by this plan.
- **Baseline:** `00017e668e71e34be3d3ea5772aa1612b15c5d6c`.
- **Depends on:** 05 index and completed CP4.
- **Exit:** durable origin/review/decision semantics proven without UI or provider mutation.

## 1. Edit map

| Existing anchor | Required edit |
|---|---|
| `crates/contracts/context/src/source_access.rs:1-158` | Retain owner-produced reason/target, validate bounded identifiers; explicitly represent multiple concrete blockers when necessary rather than a fake aggregate source |
| `crates/contracts/agent/src/interaction.rs:1-44` | Pure reference remains; align kind/status with lifecycle, add ProcessingRecipient kind; remove the ambiguous Allowed spelling rather than retain an alias |
| `crates/contracts/agent/src/message.rs` and `expert_model.rs` | Validate safe interaction artifacts/observations; reference shape is not authorization |
| `crates/modules/conversation/src/domain/mod.rs`, `lib.rs`, ports | Add owner interaction domain/repository contracts, narrowly exported |
| `crates/modules/conversation/src/turn/session.rs:123-252` | First-class immutable Interaction message and exhaustive turn/history validation |
| `crates/adapters/vault/src/repositories/conversation.rs` | Implement owner repository contracts using existing Vault transaction conventions |
| `crates/adapters/vault/src/vault/conversations.rs`, session storage/schema | Store interactions, decisions and publication linkage beside Conversation; no separate authority database |

Suggested new owner files are `conversation/src/domain/interaction.rs`, `application/interactions.rs`, `ports/interactions.rs` and a Vault interaction storage module. Names may follow current layout; semantic ownership may not move.

## 2. Record and trust model

A ConversationInteraction contains:

```text
id, person_id, session_id
origin_run_id, origin_turn_id
origin = Tool(call_id) | Task(task_id, capability_call_id?) | Model(attempt_id)
requirement (owner-produced semantic request)
reviewed_target (immutable owner-supplied descriptor and expected state)
state, revision, created_at, expires_at
resolution / decision reference when applicable
```

Reviewed target must distinguish exact connection/device/source, selected resource set, affected capability bundle, actual requesting consumer/purpose, source revision/authority, grant expectation including expected absence, and policy authority where applicable. Recipient review has the different bounded scope in 05-D. Use typed variants so navigation-only requirements cannot accidentally acquire inline-mutation fields.

Do not persist credentials, tokens, source payloads, full model prompts or provider error strings. Resources/account labels can themselves be private: durable/UI owner projection may display them, but model-safe artifacts must contain only opaque ids and generic source labels.

Validate Person against Session, Run against Session, and Task/call/attempt against the admitted journal identity. A Task endpoint cannot forge another Task's interaction. Actual consumer must match the source call; a consumer absent from canonical product policy is not repaired by minting a broader grant.

## 3. Lifecycle, commands and replay

Canonical states:

```text
Pending
Resolving { decision_id, owner_operation_id }
Resolved { semantic resolution receipt }
Denied | Cancelled | Superseded | Expired
```

Persist decision intent before mutation: interaction id/revision, decision kind, reviewed descriptor digest, stable command/operation identity and principal. This is coordination evidence, not Access authority. The same command id with a different digest conflicts.

A identical retry rejoins the recorded decision before validating a now-advanced revision. Different decisions race through CAS: one wins; the other conflicts. Never release a claimed decision and blindly perform the mutation again after a timeout.

Pending expiry is checked with owner time. Read-only get may project expired/non-actionable state without writing; an explicit command/recovery can persist Expired. Denied/Cancelled/Superseded/Expired are not resolved approvals. During Resolving, a cancellation can prevent continuation but cannot pretend an already committed grant mutation did not occur.

## 4. Publication must be crash-safe

Derive stable identity from admitted origin + canonical target/requirement digest, or persist a generated id in the durable origin result before exposure. Canonicalization sorts set-valued scope fields and includes exact authority/expected absence; never include display text in the security digest.

Use unique constraints for origin+digest, message reference publication and decision command identity. Publishing a settled Tool/Task result and interaction linkage must be transactional where they share the owner store, or use a durable prepared record reconciled by the canonical journal before making the reference actionable. A process-local vector or global run-id stash is not sufficient.

Crash cases:

1. record staged, origin result not settled: no actionable orphan card; replay settles or cancels the staged publication;
2. result settled, message projection interrupted: replay publishes the same reference exactly once;
3. duplicate journal projection: no duplicate interaction/message;
4. corrupt origin/digest: hard storage/recovery failure, never fresh regeneration;
5. original Run cancelled before publication: no still-actionable orphan approval.

## 5. Messages, coverage and retention

Add `AgentMessage::Interaction { turn_id, interaction_id, kind }` or an equivalent safe immutable reference; authoritative status is loaded from the interaction row. If the message includes a status snapshot, it is explicitly historical.

Update `turn_id`, validation, serialization, archive/recovery mapping, transcript conversion, compaction and Session size accounting. A bare metadata-only interaction message is source-independent. This does not make a surrounding source-derived assistant answer or completed Task independent.

Compaction may move original user intent to durable archive; resume must retain an owner-validated way to retrieve it. If the archive/Session is explicitly removed, dependent interactions become non-actionable and cannot synthesize user intent. Do not make compaction silently destroy a pending interaction's target or origin.

## 6. Tests and exit gate

Add tests in Conversation domain/repository and Vault fixtures for foreign Person/Session/Task/call rejection, nil/invalid refs, expected-absence binding, stale revision, conflicting decision digest, idempotent duplicate, expiry, descriptor size/count bounds, and reopen of every state.

Add publication crash/replay and compaction/origin retrieval tests before calling storage complete. Verify raw requirement/fingerprint/labels never appear in model-safe artifact serialization; forged syntactically valid refs still fail trusted lookup.

Run focused `floe-agent-contract`, `floe-context-contract`, `floe-conversation`, `floe-vault` tests using actual manifest names, workspace library check, architecture check and diff whitespace check. Update current Conversation ownership docs and record the durable interaction/resume decision in one ADR (allocate its number from the current ADR index, do not create a second progress document).

Do not add Flutter commands or source mutation here. The storage semantics must work independently first.
