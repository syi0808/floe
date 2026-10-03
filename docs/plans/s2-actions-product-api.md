# S2 Actions product API freeze

The sole public owner is `floe_actions::ActionsService`. App constructs it once with the admitted `OwnerActor`, `ActionsDependencies { repository, sources, proposals, day, executor, clock }`, calls `activate` before publishing ready handles, and closes/drains it before sealing the Vault. It is nongeneric and owns its bounded jobs. No product route supplies Person, device or executor generation. No product or FFI route selects a repository or chains approval, execution and collection.

## Commands and queries

All methods take `&OwnerActor` first and `&ExecutionScope` last. Results use `Result<_, AgentFailure>`, preserving the shared `VaultLocked` variant only for proven locked storage. Corruption/unavailable are not relabeled locked.

- `destinations(actor, scope) -> Vec<ActionDestinationChoice>`; new query route `actions.destinations`. Choice is `{destination_ref: UUID, label: string}` only. It is bounded to 256 choices and metadata is resolved by the real native adapter. The opaque ref binds current Person/device, exact source fence, physical resource and current native label; it is a compare-only selector, not authority. Unknown/permission-failed inventory is an error, not an empty success or a fabricated option.
- `inspect_authority(actor, scope) -> ActionsAuthority {person_id,revision,calendar_create}`; existing `actions.authority.get`. Product may omit redundant Person. Mode is only `allow|ask|deny` for Calendar Create.
- `set_calendar_create_authority(actor, command_id, mode, expected_revision, scope) -> ActionsAuthority`; `actions.authority.set_calendar_create`. Same command is exact replay; changing intent conflicts. A no-op mode preserves revision and existing approvals while recording the command receipt.
- `submit(actor, command_id, ActionIntent, scope) -> ActionSnapshot`; `actions.submit`.
- `decide(actor, command_id, action_ref, review_ref, decision, expected_revision, scope) -> ActionSnapshot`; `actions.decide`. Decision is `approve|reject|cancel`. Expected revision here is the Action revision. Success means the immutable decision command was recorded; current state may already have progressed beyond Approved. Do not infer command admission from that later state.
- `reconcile(actor, command_id, action_ref, expected_revision, scope) -> ActionSnapshot`; `actions.reconcile`. This only observes exact historical native evidence or resumes idempotent Day collection. It never dispatches another write.
- `inspect(actor, action_ref, scope) -> ActionSnapshot`; `actions.inspect`.
- `list(actor, cursor: Option<UUID>, limit: u16, scope) -> ActionsPage {actions,next_cursor}`; `actions.list`, limit 1–100.

There is no product execute command and no Flutter approval→execute→collect sequence. `next_observation_after_ms` is owner-issued only while an actual owner job is active. Query/route disposal does not cancel that job. Startup converts retained Executing into Unknown(ResponseLost) by exact encrypted CAS without a native call or a claim of quiescence. Pending/Approved work is never autoplayed on startup.

## Exact safe submit values

`ActionIntent` is tagged by `kind`, snake_case and unknown fields are rejected:

- `direct_create { destination_ref: UUID, title: string, schedule: TimedSchedule }`
- `direct_update { event_ref: Day EventId, expected_revision: Day Revision, title: string, schedule: TimedSchedule }`
- `direct_delete { event_ref: Day EventId, expected_revision: Day Revision }`
- `expert_proposal { receipt: TaskExecutionReceiptRef, artifact_id: UUID, destination_ref: UUID, timezone: string }`

TimedSchedule is `{starts_at: RFC3339 UTC, ends_at: RFC3339 UTC, timezone: string}`. Its duration is positive and at most 24 hours; title is bounded to 1,024 bytes and timezone to 128 bytes. Direct Update/Delete contain only the Day Event reference and expected Day revision. Actions privately calls Day's raw `calendar_event`, verifies exact revision, resolves the current configured source/native destination and constructs the immutable effect. FFI does not load the target or reconstruct provider preconditions. Day display `source_ref` is not an authorization capability.

Create and Expert submission resolve only the opaque Actions destination selector. The product never sends physical calendar/provider IDs, connection revision, source authority, original native event, external ID/revision or permission booleans. An Expert submission reloads the exact immutable Task execution receipt and artifact through the Vault adapter and Experts pure validator. Artifact JSON alone does not authorize an Action. Manual Action identity derives from its command; Expert Action identity derives from Person plus full immutable Task receipt plus artifact, so another command cannot replay the retained proposal as a new effect.

## Safe snapshots

`ActionSnapshot` is `{action_ref,revision,origin,effect,review_ref,created_at,expires_at,status,allowed_actions,next_observation_after_ms}`. Origin is `direct|expert`. `review_ref` is the immutable compare-only Actions reference from `domain/record.rs`; echo it exactly. It contains only IDs/digests/revision/expiry, never source resources, grants, credentials or native receipts.

`effect` is `ActionEffectSummary`, tagged by `kind`:

- `create { destination_label,title,schedule }`
- `update { event_ref,expected_revision,destination_label,previous_title,previous_schedule,title,schedule }`
- `delete { event_ref,expected_revision,destination_label,title,schedule }`

Observed original titles (`update.previous_title` and `delete.title`) preserve empty or whitespace-only provider titles. New user-requested titles remain required and validated by Actions.

No raw CalendarEffect, native original record, physical ID or revision is in the product snapshot. The immutable stored effect and native evidence remain inside Actions/Vault/provider boundaries.

`status` is tagged by `state`: `pending_review|approved|rejected|cancelled|expired|executing|blocked {reason}|failed {reason}|unknown {reason}|succeeded {collection}`. Collection is `pending|collected` and never changes the external effect's success into failure. Block reasons are `permission_denied|policy_denied|source_changed|executor_unavailable|schedule_conflict`. Failed reasons are `permission_denied|provider_rejected|provider_unavailable` and require a stored positive native prewrite proof. Unknown reasons are `timeout|response_lost|invalid_receipt|cancelled_after_dispatch|inconclusive_lookup|native_operation_pending|native_receipt_unavailable`. Update matching postcondition or Delete absence is insufficient evidence.

Allowed actions are the closed strings `approve|reject|cancel|reconcile`, projected by Actions and independently revalidated on mutation. Expired pending/approved work projects Expired with no decision controls; inspection does not mutate storage. A late explicit decision may record Expired. Malformed/mismatched replies enable no controls.

## Physical assembly

Use `VaultActionsRepository::new(vault.clone())` for the sole encrypted authority, `TursoStore` only as `ActionSourceReader` for Connections-owned metadata/barriers, `VaultExpertProposalReader::new(task_repository.clone())` for immutable Task proof, the owned `Arc<DayService>`, `NativeCalendarExecutor::new(actor.clone(), core_store.clone())`, and `SystemActionsClock`. The actor and handles come from the same admitted host generation. Do not retain `ActionService`, `ExpertActionService`, `ActionRepository`, `ExpertActionStore`, `CalendarAction`, plain action tables, `NativeCalendar`, fixed development Person or origin-dependent lookup paths.

This freeze is implementation input, not verification evidence. G2 follows complete S2 structural closure; no tests, formatting or compilation have run for these changes.
