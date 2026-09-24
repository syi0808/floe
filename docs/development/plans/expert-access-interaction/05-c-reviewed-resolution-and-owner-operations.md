# 05-C — reviewed resolution and owner-operation recovery

- **Status:** complete at `890c6207aa7a3c14c1a0921b6dfc0e588f418c4c`.
- **Exit:** delayed/duplicate/foreign decisions cannot widen authority or repeat owner mutations.

## 1. Reuse these boundaries

- `crates/app/src/local_access_services.rs:20-171`: native reviewed Calendar operations and Personal/Contacts owner commands.
- `crates/app/src/remote_services.rs:137-200`: RemoteAccessCommand::ConnectionObserve and owner operation result.
- `crates/app/src/connection_observe.rs:1-133`: effective bundle projection.
- `crates/app/src/first_party_observe.rs:1-128`: fixed policy and actual consumers.
- `crates/app/src/vault_host/` connection-observe/native/remote handlers: locate actual worker methods and atomic grant-set implementation.
- `crates/adapters/vault/src/vault/calendar_grants.rs`, `calendar_grant_policy.rs`, remote grant-set operations: exact owner CAS/persistence.
- New Conversation interaction service/repository from 05-A, composed through App.

Inspect `local_operations.rs` and remote operation storage before assuming operation receipts survive process restart. A returned operation id is not proof of durable idempotency.

## 2. Reviewed target is immutable

The card must represent the mutation actually performed. Inline EnableObserve invokes the connection's canonical first-party bundle operation, not a new grant for whichever consumer the LLM names.

At creation/review, capture the exact source identity, selected resources, affected views/categories, connection/source revision, expected grant set (including no-grant expectation) and policy authority. Display a safe explanation of the whole affected bundle in UI. Re-read owners immediately before mutation and compare with that captured descriptor.

Native subject drift, remote account/producer rebind, changed resource selection, new bundle member, policy expansion or changed requesting consumer invalidates old approval. Do not use a fresh source read to manufacture replacement expected values. Transition the old review to Superseded and require a newly presented review; no silent approval retry with the new target.

An out-of-band change that already satisfies the same bounded requirement may resolve via explicit refresh after exact admission checks. Never rewrite that external operation as a grant mutation performed by this interaction.

## 3. Fix the canonical owner API where necessary

The current remote ConnectionObserve command has connector/connection/resource/enabled/disconnecting fields but no reviewed bundle expectation. Add a typed expected snapshot/digest backed by the actual owner revisions, and validate all members in the existing atomic transaction. Migrate both Connections UI and interaction callers to this one safe command.

Do not bolt CAS onto only the chat wrapper while the canonical operation refreshes scope or grants underneath it. Likewise, retain native reviewed source/fingerprint/grant expectations all the way to Vault.

Projection status Active alone is insufficient. Verify the original requested read's consumer, purpose, resources, source identity and current system/provider access. If owner APIs cannot prove those facts without a full read, add a bounded authorization/preview probe, not a hidden data-read grant or fabricated dependency.

## 4. Decision execution protocol

1. Validate current CallerContext Person/Session/device, interaction state/revision and expiry.
2. Under a short Conversation transaction, persist decision digest and stable owner operation id; claim Resolving.
3. Outside that transaction, obtain fresh native/provider evidence and compare the immutable reviewed target.
4. Invoke the same Access/Connections operation used by connection detail with exact expected state and operation identity.
5. After owner success, revalidate the requirement and persist semantic resolution plus owner receipt/reference.
6. Leave follow-up admission to 05-E; Resolved is not a source-read receipt.

Owner mutation and decision completion may share a transaction when both are local and owner contracts support it. Never use nested write transactions or hold one over I/O.

For response loss between steps 4 and 5, recover by stable owner operation/digest and current owner receipt. If the existing owner operation is only in memory, add the narrow durable idempotency receipt at that actual mutation boundary or a shared transaction commit hook. Do not implement blind re-enable loops. Ambiguous outcomes remain reconcilable/non-resolved until proven; they must not be marked denied or silently retried.

## 5. Action matrix

| Requirement | User action | Owner behavior |
|---|---|---|
| EnableObserve, exact unchanged target | Allow / Not now | Reviewed connection bundle mutation or denial |
| ReviewChangedSource | Review details | Present fresh owner review; old descriptor cannot authorize new source |
| RequestSystemPermission | Native request / open system settings | UI invokes supported native class; backend refresh verifies actual access |
| Reconnect | Open owning connection | Existing OAuth/provider workflow; backend validates outcome, no bearer in Conversation |
| SelectResource | Open owning connection | Existing resource selection; confirm new review as needed |
| ApproveProcessingRecipient | Review exact recipient | 05-D Access consent operation only |

Navigation is an allowlisted typed action resolved by the app router. A tool/model-provided URL cannot be executed. Wrong-device native permission requests are non-actionable on that device.

Showing an OS dialog, opening settings, returning from OAuth or a Flutter `success=true` does not resolve an interaction. Explicit refresh must query current owner truth. A source rename can update safe presentation without changing security scope; account/resource/source authority changes cannot.

## 6. Query and cancellation semantics

Get/list/inspect are read-only. `refresh` is an explicit reconciliation command: it may settle an already satisfied requirement or replace a stale review, but never enable a grant without an approved decision.

Deny does not mutate Observe/Act/recipient state and does not resume. Cancel interaction prevents further decision/continuation; it is not cancellation of a completed Run and not revocation of independently approved source access.

Screen disposal and observation timeout do not cancel owner operations. Resolving+response-loss remains pending reconciliation with stable identity.

## 7. Required regression cases

- missing native grant → Allow → exact default grant, unrelated connection and Act unchanged;
- expected absence → concurrent grant appears → conflict/review, not silent adoption;
- stale native subject or remote producer/source revision → no mutation;
- Gmail reviewed mail+logistics bundle → one atomic operation; new/duplicate member → rejected;
- double Allow same command → same receipt; different decision/digest → conflict;
- grant committed then process crash → reopen resolves without second authority advance;
- native OS denied / OAuth failed / resources unselected → still actionable repair, never false Resolved;
- source enabled externally for exact scope → explicit refresh resolves, inspect does not;
- foreign Session/Person/device → rejected;
- corrupt/missing policy and invalid source signature → no recoverable approval mutation.

Run focused App/Conversation/Access/Vault and owner operation tests. Record failure injection points and final authority revisions, not just success states. Exit requires a demonstrated recovery story for every mutation path that can return before Conversation completion.
