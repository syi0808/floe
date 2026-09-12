# Remaining runtime integration contracts

Implementation instructions for the next Luna/high assignments. These are acceptance requirements,
not claims that the production paths already meet them. No backward compatibility is required.

## Ordering and shared files

Finish and review P3b/P4c Calendar lineage and P4d cleanup before generalizing their resolver.
Finish explicit producer review and positive FFI enrollment before remote route adoption.
Assign one implementation owner to `vault_host/conversation_turn.rs` at a time; other agents submit
narrow coordinated changes. Do not run broad formatters or selectively stage zero-context patches.

## P6a personal projections

The actual acquisition boundary is `PersonalViewSource` in
`crates/floe-ffi/src/vault_host/conversation_turn.rs`, not Core's `native_context.rs` (which serves
Floe-owned tasks/notes). Its current local-to-server fallback is not authority selection.

1. Introduce an explicit selected-source descriptor owned by the host: Person, exact connection,
   connector, device/execution owner and source authority. Resolve the active encrypted grant and
   current consumer policy before reading `LocalContextStore`. Publishing a valid DTO does not grant
   Assistant access. Source replacement invalidates old consent even when `view_id` is unchanged.
2. Preserve domain validators in `crates/floe-agent/src/personal_context.rs`: People/Feasibility at
   most five minutes, Attention two minutes, Wellbeing thirty minutes, and 32 KiB projection limit.
   Use process-local monotonic validity and fresh device presence; wall-clock rollback or restart
   cannot restore an old observation. Retain derived-only Wellbeing and coarse Attention data.
3. Acquire only the exact selected source. Remove automatic local-to-server substitution from
   `people_view`, `attention_view`, and `wellbeing_view`; a separate selected remote source needs
   P5 owner admission/release. No wildcard resources or default consent from OS availability.
4. Record immutable dependencies through the same governed turn accumulator as Calendar before a
   tool result is released. Resolve the union before subsequent model egress and final output.
   The actual model recipient must satisfy processing scope; `allow_external` is not a recipient.
5. Calendar's optional Feasibility/Wellbeing enrichment in `expert_dispatch/schedule.rs` must use the
   same admission and lineage. An ungoverned optional source must not be labeled independent.
   Omission must remain distinguishable from an empty successful source observation.
6. Add explicit review/pause UI against host-returned descriptors. Do not prompt for OS permissions
   during background acquisition or silently activate grants on publication or connection refresh.

Required synthetic tests: admitted positive read for each domain; wrong Person/device/source,
permission loss, expired presence, rollback, source replacement, scope expansion, local failure
without remote fallback, and revoke between acquisition/model/final output. Retained user history
and an unrelated independent conversation must survive every source-local failure.

## P6b action admission

Extend existing `calendar_action.rs`, `action_authority.rs`, `agent_action.rs` and
`agent_vault/expert_actions.rs`; do not create a second attempt state machine.

1. Separate historical proposal inspection from publication and dispatch. A syntactically valid
   `calendar.lease:*` handle, stored receipt or displayed approval is not current source authority.
2. At preparation bind immutable proposal evidence, Person/source/target, normalized payload digest,
   target-specific provider version/precondition, current action policy and explicit approval.
   A Read grant never implies Write. Unrelated provider item changes must not cancel valid approval.
3. Serialize current authorization and cancellation/revocation with durable dispatch admission.
   No provider call inside a long database lock. Persist the attempt identifier before sending it;
   provider idempotency/reconciliation must use that same identifier.
4. Crash/timeout after send remains unknown, not failed-and-retryable. Reconcile the same attempt,
   never blindly create another action. Revocation after admitted dispatch cannot recall bytes or
   undo an already successful provider mutation; retain an honest uncertain outcome.
5. Keep the P7 failure envelope source-local: review for missing consent, reconciliation for unknown
   execution, and no automatic action replay. Ordinary conversation CAS conflicts remain separate.

Required barrier/fault tests: revoke/cancel before and after dispatch admission; target version change
versus unrelated item change; payload/target tampering; restart after provider success before local
settlement; duplicate completion; reconciliation without duplicate effects; and expired proposal
dependencies that remain inspectable but cannot authorize publication.

## P7 final gate

Only mark complete after all rows in the acceptance ledger have production positive and negative
tests or are explicitly excluded by the user. Unsupported implementations are not environment-only
gates. Real OS/provider tests remain separately consented; do not reset a user database or mutate a
live provider to obtain acceptance evidence. Run final suites on a stable tree, with no concurrent
build deleting artifacts used by rustdoc or the Flutter FFI loader.
