# Remaining runtime integration contracts

Implementation instructions for the next Luna/high assignments. These are acceptance requirements,
not claims that the production paths already meet them. No backward compatibility is required.

## Ordering and shared files

Finish and review P3b/P4c Calendar lineage and P4d cleanup before generalizing their resolver.
Finish explicit producer review and positive FFI enrollment before remote route adoption.
Assign one implementation owner to `vault_host/conversation_turn.rs` at a time; other agents submit
narrow coordinated changes. Do not run broad formatters or selectively stage zero-context patches.

## P4c live history resolution

The current `CalendarLeaseRegistry` accounts for quota only; the actual lease entries belong to one
`CalendarTimelineViews` invocation. A persisted `ContextDependency` is not proof that an observation
is still live. Do not authorize historical output merely because its wall-clock expiry is in the
future or because a newly acquired view has the same resource IDs.

1. Retain bounded host-owned observation evidence, keyed by process/observation identity, until its
   monotonic expiry. It must match the full immutable dependency and reviewed subject, not just a
   serialized UUID. This evidence is distinct from the invocation-bound tool lease: it cannot grant
   a different invocation permission to reuse the tool handle or read its payload. Prefer metadata
   rather than a second raw-source cache; enforce count/byte quotas and expiry cleanup.
2. Resolve a historical dependency through that live evidence, current encrypted source/grant and
   consumer policy, actual model recipient and current host presence/subject. Revalidate the union
   before every later model request and final dependent commit. Missing old-process evidence denies
   historical derived context while retaining the user-visible history and independent turns.
3. Use `GovernedDependencyResolver` and the existing sidecar/accumulator. Replace Calendar's text
   heuristic only when positive current-process history and negative restart/revoke/expiry tests
   exercise the real adapter. A fresh acquisition may generate new output but must not restamp old
   summaries or model responses as newly authorized observations.
4. Keep opaque provider replay disabled until individual replay entries have verifiable turn/source
   coverage. Metadata-only expert private state needs no fictitious source-payload resolver.

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

### Audited action integration gaps

`calendar_action.rs::execute_calendar_action` currently transitions to `Executing` before preflight,
then separately reads action policy, mirror revision and the entire local event list. The final
provider call is not serialized with policy/source revocation. Equality of the entire event list and
`connection_revision` also rejects unrelated item updates. These checks are not dispatch admission.
`AgentActionOrigin` does not yet carry an exact connection/source authority, normalized approved
payload digest or target precondition. Existing `execution_id` and unknown-result reconciliation
must be retained, not replaced with another retry state machine.

The action projection and `ActionAuthority` currently live in the Core store; grant authority lives
in the encrypted agent vault. A transaction in either database cannot atomically check the other.
Before implementation, explicitly choose the authoritative dispatch owner and make every relevant
pause/revoke/cancel/policy mutation participate in its fence. Do not describe two independent
transactions as atomic. Keep provider preflight/mutation outside that fence. A crash between durable
admission and delivery must remain reconcilable using the original execution identifier.

`agent_vault/expert_actions.rs` currently denies publication of all `calendar.lease:*` evidence.
This is an intentional temporary safety gate, not P6b adoption. Historical inspection can retain
that evidence without granting a new live lease. Replace the gate only when the actual current
dependency resolver and dispatch admission are wired end to end.

## P7 final gate

Only mark complete after all rows in the acceptance ledger have production positive and negative
tests or are explicitly excluded by the user. Unsupported implementations are not environment-only
gates. Real OS/provider tests remain separately consented; do not reset a user database or mutate a
live provider to obtain acceptance evidence. Run final suites on a stable tree, with no concurrent
build deleting artifacts used by rustdoc or the Flutter FFI loader.
