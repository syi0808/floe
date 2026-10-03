# S2 native Calendar effect wire freeze

This is the exact same-snapshot schema-1 C JSON contract for the Actions owner and the bundled EventKit adapter. Replace the old `preflight`/`create`/`lookup` Action branches; keep independent `capabilities`, `view_access`, and `observe` reads. No real Calendar operation is authorized during implementation. No tests/builds/formatting until G2.

The exported symbols remain `floe_eventkit_action` and `floe_eventkit_free`. Responses retain the C envelope `{ "data": <typed value> }` on parsed Action results. Malformed outer requests may return the existing bounded `{ "error": "..." }`; Rust treats dispatch transport/malformed errors as Unknown, never proof of non-application. Exact Action request top-level keys are listed below; reject unknown fields. All request identities/effects come from the admitted Rust Actions handle and its prepared adapter, never Flutter-authored native JSON.

## Exact shared values

Use the serde representation in `crates/modules/actions/src/domain/record.rs` as canonical for these structures:

- `EffectIdentity { execution_id, effect_digest, person_id, device_id, executor_generation, connection_id, calendar_id }`. UUIDs and IDs are strings; compare UUID values canonically rather than relying on Foundation's uppercase UUID spelling, and emit lowercase UUID strings. The Create marker uses the exact lowercase canonical Person/execution strings. Digest is an array of exactly 32 integers 0–255, executor generation is positive.
- `CalendarEffect` tagged `kind`: `create { destination,title,schedule }`, `update { destination,target,title,schedule }`, or `delete { destination,target }`. `destination` includes provider `event_kit`, connection_id, positive connection_revision, calendar_id and calendar_name. `target.original` is the exact reviewed Day Event and contains the original Calendar source/external ID/revision. No optional delete boolean or inferred origin.
- `ActionSourceFence { connection_id,revision,authority,execution_owner,resources,native_subject_fingerprint }`. Source authority is a compare-only value; native checks exact selected resources and recomputes native subject fingerprint. EventKit's sole execution owner is `apple:<admitted device_id>`; no alternate prefix is accepted. Exact source revision and target are retained from Rust admission, not invented by native.
- `ExecutionIntent { action_id,person_id,device_id,execution_id,effect_digest,effect,source,authorization,executor_generation,prepared_at }`. Authorization is the Rust-owned immutable tagged value. Native does not derive permission from an arbitrary boolean. The adapter checks its exact identity against the consumed preparation and checks current OS/source/target preconditions before writing.
- All timestamps use RFC3339 UTC. Timed writes preserve the exact requested timezone and second precision. Reject unsupported recurrence, detached instances, attendees and unsupported target shape before invoking SDK writes, as existing normal behavior requires.

Calendar external revision is tagged `{ "kind":"provider_opaque", "value":"..." }` or `{ "kind":"observation_fingerprint", "sha256":"<64 lowercase hex characters>" }`, exactly as owned by Day's CalendarExternalRevision serde codec. There is no array or second decoder. EventKit computes SHA256 over sorted-key JSON containing the exact raw title (including empty or whitespace-only titles), externally tagged `Timed`/`AllDay` schedule, and fractional UTC lastModifiedDate (or an empty string). The transport schedule remains tagged `kind: timed|all_day`; the fingerprint input is separate and identical for read and write preconditions. Recompute this exact ObservationFingerprint before update/delete; older mismatched fingerprints require a fresh read. Never label it provider opaque or claim atomic provider CAS. Keep the existing native check-to-write race limitation explicit. New native read/acknowledgement records must carry the actual computed revision. Gateway fingerprints do not grant write capability.

## Prepare

Before product create selection, metadata-only destination inspection uses exact request keys:

`{schema_version:1,operation:"action_destinations",person_id:UUID,device_id:string,executor_generation:positive integer,source:ActionSourceFence,deadline:timestamp}`

It never prompts or reads Event payloads, and never writes. Check current full Calendar permission and recompute the exact source subject/resources before and after collecting native Calendar metadata. Result is `{schema_version:1,person_id,device_id,executor_generation,source:ActionSourceFence,resources:[{calendar_id,calendar_name,can_modify}]}`. Return one unique resource for every requested configured calendar, with actual display name and actual `allowsContentModifications && !isSubscribed`; no invented selected-as-complete catalog. Source and identity echo exactly the admitted values after the physical fingerprint comparison. The complete request and reply each remain bounded to 65,536 bytes; oversized or inaccessible inventory is an explicit error, never truncated success. Rust resolves a private opaque selector from this exact current metadata and sends only selector plus safe label to the product. Update/Delete product inputs carry only Day EventId+expected Day revision; Actions privately loads the raw target and current native destination. No product JSON supplies provider/calendar IDs, source revision or original native preconditions.

Request exact keys:

`{schema_version:1,operation:"action_preflight",identity:EffectIdentity,effect:CalendarEffect,source:ActionSourceFence,authorization_expires_at:timestamp,local_events:[Event],deadline:timestamp}`

The native adapter validates shape, Person UUID, admitted device/positive generation, exact identity-to-effect/source binding, source subject/resource inventory, OS permission, calendar identity/write capability, target identity/recomputed revision, writable precision/timezone and schedule conflicts. It never invokes `save` or `remove`. Remove the fixed development Person equality guard. Preserve actual identity checks; merely accepting arbitrary caller Person JSON is not a replacement for Rust admission.

Result is the closed union:

- `{schema_version:1,status:"ready",identity:EffectIdentity,host_epoch:UUID,preparation_id:UUID,native_subject_fingerprint:string,expires_at:timestamp}`
- `{schema_version:1,status:"blocked",identity:EffectIdentity,reason:"permission_denied"|"policy_denied"|"source_changed"|"executor_unavailable"|"schedule_conflict"}`

The native preparation is a bounded one-time in-process entry retaining exact request identity/effect/source/local events and authorization expiry. Its expiry is at most 30 seconds and no later than authorization expiry. A native process `host_epoch` is generated once. Reject exhaustion without a write; never evict a live write to make space. Rust retains the ready value inside a consumed-once PreparedCalendarEffect.

## Dispatch

Request exact keys:

`{schema_version:1,operation:"action_dispatch",admission:ExecutionIntent,preparation_id:UUID,host_epoch:UUID,deadline:timestamp}`

The encrypted Actions transaction already committed immutable Executing before this request. Native atomically consumes the exact matching unexpired preparation once, binds its invocation to the original execution ID/effect digest/Person/device/executor generation and retains an in-flight cache entry before invoking any SDK write. Repeat same invocation can only return recorded/in-flight evidence; it must never write again. Changed binding returns Unknown. No new external operation/idempotency key is introduced: EventKit Create marker stays exactly `floe://calendar-action/<person_id>/<execution_id>`.

Recheck permission, source subject/resources, target identity/revision, expiry, deadline and write capability immediately before the SDK call. A positive rejection before crossing the boundary returns NotApplied with the exact identity and preparation ID as invocation_id. Once `save` or `remove` is invoked, thrown SDK errors, timeout, cancellation, missing/mismatched post-write result or later permission loss are Unknown unless an actual authoritative receipt already exists. Do not classify broad SDK error classes as NotApplied. A Rust timeout never establishes native quiescence.

Typed outcome JSON is exactly `CalendarEffectOutcome`:

- `{status:"committed",receipt:{identity,effect,evidence,committed_at}}`
- `{status:"not_applied",proof:{identity,host_epoch,invocation_id,reason,rejected_at}}`
- `{status:"unknown",identity,reason}`

Committed effect tagged `kind` is `created { event:CalendarWriteResult }`, `updated { target:CalendarTarget,event:CalendarWriteResult }`, or `deleted { target:CalendarTarget }`. `CalendarWriteResult` is `{external_id,external_revision,title,schedule,can_modify}`. Native acknowledgement evidence is `{kind:"native_acknowledgement",host_epoch,receipt_id:UUID}`. Normal create/update/delete after a matching successful SDK acknowledgement remain supported. Delete receipt copies the exact original target only after successful `remove`; ordinary absence is not a receipt.

NotApplied reason is `permission_denied|provider_rejected|provider_unavailable|source_changed|cancelled|timeout`. Unknown reason is `timeout|response_lost|invalid_receipt|cancelled_after_dispatch|inconclusive_lookup|native_operation_pending|native_receipt_unavailable`. All physical outcomes bind the original EffectIdentity and preserve the operation even if the original authorization expires later.

## Readback and lookup

Readback request exact keys:

`{schema_version:1,operation:"action_readback",admission:ExecutionIntent,deadline:timestamp}`

Readback does not dispatch or require original approval expiry to remain current. It compares the complete stored historical intent/identity against the native cache. Exact completed entry returns its exact outcome. In-flight entry returns Unknown `native_operation_pending`. Missing/evicted/restarted entry returns Unknown `native_receipt_unavailable`. Cache mismatch returns Unknown `invalid_receipt`. Keep final physical outcome for 15 minutes after completion with a maximum of 128 retained entries; evict only completed expired entries or the oldest completed entry when bounded capacity requires it. In-flight entries are never evicted and consume capacity. This is non-authoritative, process-local physical evidence, not a plaintext durable Action journal. The encrypted Actions record remains the only approval/execution authority.

The readback branch must acquire only the cache lock before any global EventKit write lock. Rust uses an independent readback gate to the same loaded dylib image. A still-running SDK call must therefore remain observable as `native_operation_pending`; readback cannot queue behind that write or wait for its completion. Other EventKit operations may retain their serial write/read gate.

Fresh Create lookup request exact keys:

`{schema_version:1,operation:"action_lookup",admission:ExecutionIntent,deadline:timestamp}`

Rust calls lookup only for Create after readback reports unavailable and after current local source/operation barrier checks. Native requires current read permission, exact source subject/resources and bounded original source/time window. It never writes. A unique marker matching original Person/execution/calendar and exact expected content can return a committed Created observation, with evidence `{kind:"unique_create_marker",observed_at:timestamp,marker:original marker}`. The receipt's committed_at is the observation time and never claims an SDK acknowledgement. Multiple/foreign/mismatched/zero markers remain Unknown. Update/Delete fresh postconditions are always inconclusive without an operation-specific causal cache receipt; do not return success from a matching update or absence after delete.

No `acknowledge_as_success`, `acknowledge_as_failure`, retry write, cancellation-as-nonapplication, cleanup deletion or automatic replay route is introduced. The user-visible default for permanently insufficient evidence remains Unknown with explicit bounded reconciliation.

## Implementation package

Swift owns only the EventKit Action branches/cache and exact wire adaptation in `apps/client/macos/CalendarActions/EventKitActions.swift`; if a separate native test/support file is genuinely necessary report it before adding. Do not edit Rust, FFI, Flutter, iOS unrelated Calendar read paths, provider data or credentials. Preserve independently owned source read operations. Return an exact patch and a concise source review; no checks or live native operations. The Rust Actions owner implements the paired adapter and validates every returned identity/receipt.

## Inherited source reservation fence

The Rust owner authenticates the full terminal Task coverage, not only the proposal's direct Calendar contributor. For every dependency it captures the current `SourceConnection` plus Connections' `SourceReservationFence`: explicit `NeverReserved`, or the latest retained immutable operation ID, reservation ID and reservation generation, together with the current fence. New reservations insert their operation and active fence atomically; command replay does not create a new stamp. Completed reservations remain observable, including Observe/Pause operations that do not change source revision. SQLite insertion ordering is adapter-private and is never a product authority.

The ordering is: authenticate Task receipt → capture each source between equal clear reservation stamps → native preflight with source checks before/after → recheck all retained sources/stamps → encrypted transaction revalidates full Task grants and commits `Executing` → the live Rust prepared capability rechecks every source/stamp immediately before native handoff. These retained snapshots never enter the native request or product DTOs. Before `Executing`, a mismatch blocks the Action. After `Executing`, the unique live capability can return positive `NotApplied(source_changed)` only while it proves `action_native` has not been invoked, using its exact existing host epoch/preparation ID. Recovery after a crash cannot reconstruct that non-invocation proof.

Source metadata, the encrypted Vault and EventKit do not share a transaction. The final source check→SDK call race remains after durable `Executing`; a reservation beginning in that interval cannot retroactively turn a dispatched effect into a prewrite rejection. Such work retains the normal causal acknowledgement/Unknown recovery semantics and is never blindly retried.

A live prepared Rust capability returns positive `NotApplied(cancelled)` or `NotApplied(timeout)` when cancellation, deadline or preparation expiry is observed before native work is scheduled. Cancellation or timeout while validating source metadata has the same precise reason, not `source_changed`. Once `spawn_blocking` may have started the native call, cancellation/timeout remains Unknown unless a causal native receipt proves an outcome. Executing alone never proves invocation or non-invocation.
