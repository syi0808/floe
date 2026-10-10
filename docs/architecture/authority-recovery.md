# Authority and recovery invariants

These are durable architectural safety properties, not a progress checklist.

## Source processing and product-read authority

[ADR 0034](../decisions/0034-gateway-reasoning-and-source-processing-authority.md) supersedes exact model-recipient consent as a required authority. Preserve verified Gateway identity, Access-owned source processing permission, source/grant provenance, key identity, CAS, durable pre-dispatch intent, cancellation direction and uncertain-write recovery. DeviceOnly/GatewayAllowed belongs to the source grant; processing expansion belongs to source/connection review, never model routing. Health additionally requires source-owned local-transform evidence and retains HighlySensitive classification. Neither pairing nor transformation grants source or Action permission.

The current Access boundary uses source processing restrictions and exact live Gateway binding, with no model-recipient consent store or product-supplied routing authority. Admission, handoff, post-I/O and release checks remain mandatory. The [2026-10-02 architecture refactor plan](../plans/2026-10-02-architecture-refactor.md) tracks execution and qualification; this document states owner and recovery invariants.

Prompt, card, Run-frame and environment manifest hashes are diagnostic content identities only. They never establish source freshness, grants, processing permission, Gateway identity or execution permission, and never replace the durable Experts-owned Run environment identity or live Context/Access checks.

## Authority is explicit and owner-scoped

- A Connection describes source/account/execution-owner lifecycle; it does not itself authorize AI use.
- Connections durably owns source identity, resource selection, local configuration CAS, `SourceAuthority` and native subject identity. Native setup creates a Pending source before the permission operation; permission completion is not Observe approval. Reviewed configuration compares actual selected-resource/subject evidence and publishes a successor only after the Access invalidation receipt is durable. Provider revision, local configuration revision and caller device identity are separate fences.
- Day owns derived Calendar cache, per-resource intervals/status and an independent mirror revision. Context resolves current configured sources and performs acquisition through Access product-read admission. Day commits against complete inventory and successful-source fences; import or provider failure cannot rotate source authority. A cached mirror cannot restore configuration, source permission or a removed resource.
- Context Task/Note projections use a read-only evidence port with separate `NativeContextProjectionBudget` and `DayEvidenceAcquisitionBudget` values. Context bounds serialized native views; Day bounds candidate count, aggregate stored-payload bytes before decoding, and the existing projected-Day-item bytes. The Vault query uses exact Person/task/note predicates and a one-row overflow sentinel; budget overflow fails closed without returning a partial complete view. Day owns selection/storage decoding, while Context checks exact Person/record identity and constructs the bounded native projection. This port grants no Day write command.
- An Expert assignment's persisted `SourceSelectionReference` is configuration identity only: connector, connection, execution owner, capability, resource and contract version. It contains no credential, grant, source-authority revision, payload or processing consent. A Task admits the exact Run-pinned selection; current binding/enable drift does not revoke it or redirect it to another source. The Task continues with its pinned references while source/grant/provider/OS authority remains live and may deny. Completed Task replay returns the historical result without reacquiring evidence.
- Access owns source grants, exact consumers/purposes/categories, DeviceOnly/GatewayAllowed processing restrictions, immutable reviews and grant commit/abort receipts. Its model dispatch fence independently admits the exact current Device or verified Gateway target. A digest or prepared plan is correlation, not authority.
- Initial connector permission combines source access and disclosed Gateway processing. When an inline review proves grant absence, Access prepares GatewayAllowed for that trusted View's categories in the same immutable review; permission is granted only by the explicit review decision. Existing DeviceOnly grants keep their restriction and require an explicit connector review to expand processing. Settings defaults a new permission review to GatewayAllowed while retaining an explicit DeviceOnly choice. Health includes only the derived Wellbeing View and still requires its mandatory local transform and live receipt before either reasoning boundary.
- A standing `DataAccessGrant` binds the immutable Person/Connection/Connector/execution-owner source and logical connection/View resources. `GrantAuthority` is the permission/state/scope epoch; Connections owns the separately observed `SourceAuthority`. Native/provider drift stales earlier dependencies. Explicit source configuration invalidates affected non-revoked grants through Access before publishing the new resource set, so a prior logical View grant cannot silently expand.
- A `ContextDependency` records grant permission `resources`, exact observed `source_resources`, `GrantAuthority` and `SourceAuthority` separately. Native Calendar reads the configured full selected set; hosted assistant reads verify the exact signed provider set before generic View admission/read/release. Context rechecks source/permission/provenance outside storage transactions, and Access applies grant/review policy inside its own repository transition. Expert bindings remain configuration identities rather than grants.
- Access classifies the exact Calendar `GrantSourceBinding` for source-review requirements: revoked grants count as absent, paused grants require Observe, active grants retain their exact observed identity/authority, and ambiguous live matches fail closed. Context owns configured-source evidence and projects that result; credential expiry remains a Reconnect requirement.
- Context may acquire/project only evidence authorized for the current Person, source, scope and freshness requirements.
- Inference may choose an approved model route, but route selection cannot enlarge data authority.
- Saved provider credentials remain private to credential/provider boundaries and are not product-wire or Conversation inputs. The narrow exception is newly issued approved-pairing output for secure persistence, never a subsequent request input; token-bearing Debug/diagnostics are redacted.
- Provider adapters load the one private Gateway credential through a verified Person/device and durable pin/credential expectation. Access reloads exact Gateway binding at model admission, consumption and response release. Product Calendar transport additionally retains a generation-bound lease from `ProductGatewayLeaseRegistry`; replacement or retirement cancels that exact generation. Credential, provider identity and local runtime generations cannot substitute for one another. No `allow_external`, `expected_recipient` or saved bearer is product-supplied authority.
- Connections owns Pairing. The product sends one address-bearing Start intent; the owner derives and retains its internal preparation identity from that command, with no public prepare or confirm step. The owner drives cryptographic confirmation through a durable, revision-bound internal command after accepting the exact challenge. Its receipt authorizes signing, not a human comparison; administrator approval remains explicit. Internal Prepare validates the loopback target and stores an immutable command/Person/device/target descriptor and original expiry in one encrypted transaction. Start atomically stores the admitted operation, fixed polling proof, a fresh wrapped enrollment-scoped Ed25519 key and Pending credential expectation. Exact Start replay retains that key; a new operation never reuses an earlier issuer. The owner commits Dispatched before HTTP and never holds a database transaction over I/O. Exact command replay reads the admitted proof instead of regenerating it. Proven cancellation before dispatch or terminal remote rejection/expiry restores the recorded predecessor; a potentially dispatched operation with no handle remains RepairRequired rather than issuing Start to manufacture a cancellation handle.
- The safe Gateway address display is reconstructed from the owning pairing’s canonical setup on read; it is not a second persisted address or a transport/authority input. Gateway proofs, enrollment material and bearer credentials live only in encrypted Vault rows. The native Keychain stores the Vault root key, using ordinary login/file Keychain on macOS and the platform-specific iOS provider. There is no second Gateway Keychain record, backend fallback, or credential namespace migration. The provider adapter receives read-only private material and cannot publish credential authority. The final transaction checks exact operation/revision/generation, confirmation and signing receipt, Pending expectation and pin CAS before publishing bearer, pin and Paired together. A cancellation or Forget race records remote-revocation evidence without activating the credential. Forget commits the owner receipt and retired authority together; historical evidence remains encrypted, without an automatic remote revocation request.
- Enrollment, AssistantView and product authorization sign inside the same Immediate transaction that proves their exact live Pending or Committed authority and writes the receipt. Retained historical keys are recovery evidence, not signing authority. Private-key wrapping binds Person, device, operation and public issuer with canonical length-prefixed AAD. Cancel/reject/expiry may restore Unpaired while historical keys remain. Startup validates only the current Pending/Committed key; unrelated retired keys cannot prevent local-only startup.
- Credential reads observe expectation, operation, pin and private material in one transaction. Missing or inconsistent material under Pending/Committed authority is never valid Primary absence. File Keychain does not provide the Data Protection ThisDeviceOnly guarantee. No private proof, bearer or key is serialized into a product DTO or Debug output. Source previews and signed authorizations still enforce exact producer, source, revisions, resources and key identity; pairing or model availability grants neither Observe nor Calendar Operation authority.
- An admitted integration pins its Gateway credential generation. A coherent Unpaired/Forgotten credential read cannot restore that generation and is an authority failure, while storage/transport uncertainty remains retryable. Connections records RepairRequired and removes launch/poll actions without claiming that an uncertain remote effect was cancelled.
- Revocation prevents later admission or release. It cannot retroactively recall data already transmitted or a provider effect already accepted.
- Source pause, revocation and drift constrain later source/model admission without mutating Expert Registry configuration. Grant invalidation applies the Calendar Operations owner transition to pending/approved records while preserving executing/unknown outcome uncertainty. Connections retains source-operation fences and a reservation watermark after completion, so a fast completed Observe/Pause operation is not mistaken for an unchanged reservation history.
- Connection Observe is an explicit Connections command over an Access-owned immutable review. Product carries the stored review reference/decision and exact source revision, not replacement authority fields. Access reloads the descriptor and revalidates actor, expiry, policy, grants and source evidence before mutation. Pause changes active Observe grants only; disconnect revokes before source cleanup; configure invalidates affected grant availability. None of these operations mutates Registry bindings or creates Action permission.
- Native review inspects the selected Calendar/Contacts set and actual subject without acquiring event/content payload. Context reads only the current configured set under the relevant purpose and limits. Hosted assistant View previews bind the provider’s canonical resources; the separate lossless product mirror protocol has its own range, page and total bounds. No missing resource, partial page or overflow is silently treated as complete coverage.
- Context identifies a remote View target by the current Person, connector, connection, requested View and that connection's exact canonical resource before checking grant permissions or detecting duplicate live authority. Unrelated grants do not become candidates; a known target with a consumer, category, operation, purpose, processing or review mismatch remains a blocker. The signed producer preview and binding path rechecks capability and source authority before payload I/O.
- Gateway processing expansion is a source review over the exact source/grant expectations, logical permissions, physical resources and reviewed processing categories. Mandatory Health transform evidence remains bound to the actual outstanding host operation, device/process, source subject, output digest and expiry. It is required for Device and Gateway reasoning and never lowers HighlySensitive classification.
- Trusted shipped manifest/consumer pairs are explicit composition inputs to `ContextTrustedConsumerCatalog`. It preserves their typed namespace; identifier equality cannot upgrade an extension to builtin. Access derives the applicable View policy independently of Registry assignment or binding state. The root Manager has no standing domain source consumer, and binding changes cannot create or expand a grant.
- Access owns review policy identity and immutable source descriptors. A review reference is compare-only; inspection/replay returns the stored descriptor, while apply checks current policy and authority. Projection review origin binds the complete source requirement group, actual projection operation and target digest. No App-calculated digest, query-time source mutation or client echo can replace owner authority.

## Source review and two-store recovery

Connections reserves an exact source operation in its durable store before Access changes encrypted grants. Access persists one commit receipt or abort tombstone bound to operation, command/intent, caller device, reservation identity/generation and exact source expectation. These stores are reconciled, not treated as an atomic transaction. A missing/uncertain receipt is not success; recovery reads the immutable proof before deciding whether a reservation can complete or abort. Uncertain remote cleanup retains the fence and a repair-required state. Abort uses the immutable operation identity rather than requiring fresh external/physical observations; its unique encrypted tombstone excludes a later commit for that operation.

Review preparation and inspection do not advance source authority. Access retains immutable whole-source reviews, including exact reviewed grant absence. Applying a review changes only its admitted bundle. Pure descriptor replay does not extend expiry or grant new authority; a fresh apply must pass current actor, source, policy and grant checks.

Connections projects each immutable reviewed View separately: current and requested processing retain the exact DeviceOnly or GatewayAllowed category scope, and absent permission is explicit. Mixed Views and Metadata/Content/Derived expansions cannot collapse into an any/all boolean. Sensitivity labels remain a separate disclosure and cannot grant processing permission. A resource-configuration review shows its current per-View scopes with no requested Observe permission; changing resources invalidates prior authority and requires a new access review. Settings and inline reviews render this same owner projection.

## Day commands, cache and product read recovery

Direct Day refresh uses Access `ProductCalendarReadPermit`, not an assistant Observe grant. The permit binds verified Person/device, the exact configured source/resources, current OS/provider permission, purpose, durable refresh/read identities, UTC range, limits, expiry and original cancellation/deadline. Metadata validation alone mints no permit. Native product acquisition can run with the agent Vault locked under current OS permission; Gateway acquisition requires the available encrypted signer/pin and the retained private credential generation. Missing encrypted capability remains a typed failure, never absence or fallback.

The private product wire has distinct `day_calendar_admission` and `day_calendar_release` tags and fixed `day_refresh` / `calendar.mirror` semantics. Every page binds the complete source/resource/range/query and limits. Release signs the verified staged-result hash; transport checks exact released bytes against it before decoding. Product permits and receipts have no conversion into assistant dependencies, model permission or Action approval. Go enforces Rust-authorized claims without becoming a second client permission policy owner.

Day refresh admission persists the Person/device, command identity, normalized intent, exact mirror presence/revision and executor generation. It owns bounded background work and never reruns interrupted acquisition automatically. Complete inventory or successful-source fence drift rejects the whole mirror commit and leaves the previous mirror unchanged. Ordinary resource failure preserves that resource’s previous complete interval while other stable successful resources may commit. Consumption includes discarded pages; unknown consumption ends the refresh rather than being treated as zero.

The derived cache retains the latest complete interval per resource. Complete empty replaces that resource’s cache; failure does not clear it. Newly cached or reinserted events use the next durable mirror revision for Day CAS, preventing EventId/revision reuse after eviction. ProviderOpaque and ObservationFingerprint external revisions retain their actual meaning. Manual records, durable Calendar Operations evidence and historical command results are not cache-retention targets.

A fresh Day snapshot performs bounded metadata-only inspection through Context: current source identity/resources, source-operation fences and actual available permission evidence. Changed, removed, fenced or unverified sources cannot appear Current; a requested date outside cached coverage is not complete-empty. The query neither prompts nor acquires provider events, changes grants/identity, persists new cache status or starts refresh. Owner clock controls freshness. An immutable command replay instead returns the original stored snapshot without fresh inspection; new manual mutation receipts conservatively mark unverified cached Calendar state stale.

Manual mutation and refresh share a Person/command UUID namespace. The full mutation, including occurred_at and expected_revision, participates in intent identity; display-only day.now and executor generation do not. A manual mutation applies Day’s pure transition, row writes, bounded safe snapshot and exact receipt in one local transaction. The selected result is bounded without materializing unrelated history, and overdue counts include all selected displayed tasks. At 4,096 retained Day commands per Person, new admission fails while existing exact replay remains available; there is no receipt eviction permitting re-execution. Manual input is bounded to 1 MiB and its receipt to 4 MiB snapshot + 2 MiB command/capture + 64 KiB framing.

New manual mutations and Action collection commits hold Day’s admission/drain guard and compare the active executor generation. A nonserializable `DayWriteFence` also binds the admitted actor and original owner/caller cancellation and deadline through the final commit check. Historical receipt replay is first, and a new scope cannot revive an old write. Action collection binds the exact execution, receipt digest, source revision/authority and resource; repeated collection rejoins its receipt and never repeats an external effect.

## Product command disposition and receipt recovery

`floe_kernel::CommandFailure` is the semantic owner's command result. `NotAdmitted` means this delivery did not begin owner admission; it does not settle an earlier uncertain delivery. `NotApplied` requires owner evidence that the exact command has no committed effect, such as an absent receipt/occupant checked before mutable preconditions and a confirmed precommit rejection or rollback. An error named `Conflict`, by itself, never proves this. `Admitted` means the owner has a durable command receipt even if later projection failed. `Indeterminate` preserves uncertainty when lookup, commit, rollback or post-admission evidence cannot establish the result; a reused ID with a changed body also remains uncertain.

The affected command paths keep that authority at their owners: Conversation checks its durable receipt and command-family occupancy before current Session or continuation validation; Day checks mutation and refresh receipts in their shared Person/command namespace before applying a fresh mutation or reading the current mirror; Calendar Operations checks its receipt before revision-based authority changes and serializes one command ID across submit, decision, reconciliation and authority changes before replay lookup or fresh evidence validation; Experts checks Registry and Binding Review admissions before revision changes; and Knowledge checks the Memory decision receipt before validating candidate state. The Calendar Operations repository still rechecks receipt and command-family occupancy inside each durable transaction. Session Start retains its existing typed start-failure contract, and Connections keeps its existing command disposition. A positive receipt is replayed with its original command ID after a lost acknowledgement; no new intent may take over an occupied ID.

The stateless App router maps these typed owner variants to the product outcome contract, and FFI preserves the disposition in AppWire. Neither layer infers command state from `AgentFailure` or storage error codes. A client may discard an ID after `NotApplied`, or `NotAdmitted` when it knows this was the first submission. After any possible prior submission it retains and retries the same ID; a later `NotAdmitted` does not clear that uncertainty.

## Conversation Session admission and observation

Session Start owns creation and its positive receipt in one guarded Vault transaction.
Before replay or refusal, it validates the Conversation and Interactions schemas and
checks one consistent command-ID occupancy view across all command families. A
replayed Start requires the Start receipt and sole SessionStart occupant to agree;
inconsistent or multiply occupied IDs fail closed. Only a genuinely absent ID can
receive a new Start, subject to the 4,096 retained Start-receipt capacity. Occupant
identities are immutable and receipts are not evicted; changing either policy requires
revisiting that proof. An arbitrary Conflict, rollback failure or unreadable row is
not proof of no effect. Pre-admission refusal is NotAdmitted; interrupted/post-BEGIN
work and integrity failure while validating a positive receipt are Indeterminate.
After a valid Started or Replayed receipt, failure projecting its Session is Admitted.
Only NotAdmitted(StorageBusy) permits three owner-scoped retries, sleeping 25, 50
and 100 ms while preserving the same command ID. The physical writer guard stays
armed until transaction completion;
interruption retires the store's availability. This is not a claim that every
Conversation write uses it.

External Product and Runtime control command envelopes require UUID-v4 nonces so
they cannot collide with internally derived command identities. NativeHost command
envelopes and other query references retain their existing contracts; Runtime
preparation-history references also require UUID-v4. The generic CommandId type
retains its existing contract. Conversation stored family format 10 removes the
obsolete Session recovery-command table; older family
formats fail closed and require an explicitly selected fresh development profile,
not an automatic reset, key replacement or migration.

Resume, Get and history queries never settle or replay commands. The app-lifetime
client gateway retains an uncertain Start's exact identity and exposes explicit
settlement separately from fresh Start. A later NotAdmitted response cannot erase
an earlier uncertain submission. Reload may adopt a settled Start; explicit New
first settles the old request without adopting it and then creates a fresh one.
Unresolved settlement stops the operation rather than silently resuming another
Session or replacing the pending identity.

There is no separate business Session Recover command. The executor-owned recovery
driver reconciles Runs; the screen re-reads the exact Session and observes its Run.
Client adoption epochs and monotonic Session revisions prevent obsolete responses
from replacing current state. Stopping an observation or disposing a screen does
not cancel a Run. Already handed-off bounded turn admission settles with its original
identity; a replacement load waits for this handshake before reading. A user Stop
is a separate explicit owner command: for a newly submitted Start, unresolved
admission cannot claim cancellation, so Cancel waits for a positive Start receipt.
An already-admitted Run can be cancelled directly. If the observation stopped, an
explicit Stop already requested is honored after that receipt, the shared read model
is sealed before releasing the activity slot, and the receipt is never adopted into
a newer epoch. Re-observation can Stop the Session's activeTurn before the first Run
event arrives. A cancellation transport failure does not abandon an admitted Run's
active observation; the Stop caller gets the failure. For a Run owned by an active
turn admission, an explicit retry retains that Cancel identity. A Stop for a Run
that is no longer known active sends no command and restores the UI's non-stopping
state. Detached query completions cannot publish stale client state.

## Provenance and coverage travel with evidence

A Calendar grant may authorize several consumers. Each observation dependency
records the exact admitted consumer, operation and purpose for that read, while
retaining the original grant identity, authority, resources, categories and
processing restriction. Context validates membership in the full grant scope;
it must not infer the caller by requiring a singleton scope or selecting its
first consumer. Live Access reauthorization still checks the exact dependency.

Each new model intent carries a conservative ceiling bounded by the remaining
root ledger, budget partition and ancestor lease quotas, including settled and
pending charges. This ceiling snapshot is not a live reservation. The owner
journal still rejects oversubscribed intents, and dispatch atomically reserves
within both the persisted ceiling and the then-current allowance. Unresolved
attempts remain conservatively charged. A configured per-attempt cap is not the
remaining budget and must not be reused after earlier positive-cost attempts.

Source-backed model/tool inputs retain enough identity to determine:

- Person and source/producer;
- authority/grant dependency;
- observation/projection revision;
- coverage and freshness;
- consumer/purpose/data class;
- exact source processing restrictions and, for Health, independently verified local-transform provenance.

A broader follow-up request cannot silently reuse evidence whose coverage is too narrow. Unknown or unavailable evidence is not represented as an empty successful observation.

When one logical view reads multiple connected sources, each source retains its own grant, `GrantAuthority`, `SourceAuthority` and `ContextDependency`. Context may merge bounded payloads, but it cannot manufacture an aggregate grant or dependency or drop a contributing dependency from model coverage.

Expert package artifacts retain exact internal coverage in the journaled final payload, TaskSnapshot and immutable TaskExecutionReceipt. Product projections expose bounded text and safe artifact metadata without internal authority. An artifact's narrower direct contributor does not erase the Task's inherited or other settled dependencies. Unknown coverage cannot become a Completed or Blocked Task report, and package JSON cannot establish Task provenance or user-review authority.

## Validated pending work is durable work

The common Agent Runtime validates a complete model-produced batch before executing its steps. A package finalizer returns the validated text and artifacts once, before canonical batch/output acknowledgement. The final size and provenance checks cover the transformed payload. Recovery consumes the acknowledged payload and exact pinned batch; it never reruns package transformation or asks a model to replace durable pending work.

`ValidatedModelBatch.projection_coverage` records the exact source provenance of pending work. Conversation reauthorizes that stored coverage through the current `DependencyResolver` immediately before pending execution and again before terminal output release. Stale or Unknown dependencies suppress the stored step or answer; neither message shape nor Expert/capability identity can reconstruct missing provenance.

Stable identities bind:

- execution;
- validated batch;
- step ordinal;
- Tool intent/result;
- Delegation Task intent/result;
- preamble where journal ordering requires it.

Journal corruption or mismatched durable identity fails closed as storage/recovery failure rather than being corrected by a model.

### Cross-run continuation

A continuation child does not supersede a parent's pending batch merely by existing. The child must durably claim the **exact same validated batch and starting cursor** before takeover.

This invariant is part of the stable architecture: a child may take over only after durably binding the exact validated batch and starting cursor.

Conversation durably records the Experts-owned environment revision/digest on every Run, separately from canonical user intent. Pending-batch Continue requires the source and destination Run environment identities to match before any stored step executes; without pending work a new Run may sample new configuration. Journal batch revisions must match the admitted Run, including resumed re-records and finalization. Executor activation interrupts Working Runs on reopen while preserving their original environment identities; it does not reconstruct endpoints or resume the same Run under current Directory state. Registry configuration changes apply only to future Runs, not as authority over admitted Tasks.

ADR 0034 separately requires durable post-review automatic resume: a terminal interaction group with at least one Resolved member records the need for one fresh linked child and recovers it idempotently across crash/reopen and lost acknowledgements. This is an accepted requirement, not a claim that the existing best-effort trigger is already durable. Removing the routine resolved-card Continue action must follow that recovery cutover; it does not remove the exact-batch continuation machinery above.

## Model attempts and budgets

`ModelPort::prepare` returns one owned prepared call and immutable plan before Context projects source data. A source-review outcome precedes attempt allocation and ModelIntent. Validation correction may reuse that same prepared object and plan; no failure after possible handoff permits automatic transport retry or fallback.

The owning Run, Task or Learner claim has the sole canonical `JournalEvent` sequence. ModelIntent binds attempt ID, actual plan/projection, optional owning Task and a conservative reservation ceiling. Inference records a pending dispatch fact before handoff, then produces exactly one terminal immutable attempt receipt on settlement, over-budget response or drop. The Engine acknowledges ModelResult before releasing its in-memory receipt. A proven pre-handoff failure may settle with zero charge; missing or uncertain post-handoff evidence retains conservative charge and unknown flags.

Usage remains monotonic through cancellation, finalization and recovery. Charged unknown tokens/cost stay separate from observed usage, including unknown dimensions with a zero charged amount. `TaskModelAccounting` retains exact unknown-token and unknown-cost attempt counts. Attempt references/tombstones are bounded; capacity exhaustion rejects admission before another attempt. Journal capacity separately reserves room for terminal intent settlement.

### Task settlement and parent recovery

Each Task pins execution/generation, original request identity, admitted selection and allowance. Every append atomically advances its durable journal head. Task terminal state, exact output/coverage, accounting receipt and any assignment-local private-state transition commit together. A missing tail or mismatched head fails closed. Source retention capabilities survive until settlement is acknowledged.

For Experts, Task execution also owns an isolated transcript reservation and immutable history pin. Admission reserves the exact assignment conversation and pins its current Core prefix in the same Vault transaction as `Submitted`; Vault persists an immutable owner admission receipt, separate from the unchanged Task record, binding the original request digest to the complete host-input/pin commitment. Lost admission acknowledgement readback returns this stored receipt. Duplicate Task replay resolves it before mutable registry/head state; Working CAS, history reads and terminal receipt verification require the same receipt and fail closed on missing or mismatched input evidence. The existing `Submitted -> Working` CAS appends the exact delegated input with Host provenance and a Core evidence link, then opens the mapped `TaskExecution` recorder atomically. Task journal events, typed/Core contributions, terminal receipt and recorder settlement compose at the Task owner transaction boundary. A terminal Task, including `Blocked`, never reopens. A never-started Submitted recovery releases only its exact proven reservation. Uncertain execution retains transcript custody until authentic settlement or a generation fence proves the prior writer cannot act; timeout and observer cancellation are not proof.

The Expert history resolver reads only the immutable pinned prefix in one Vault snapshot. It accounts for message count and cumulative serialized, reference, typed-payload and coverage bytes before hydration, then Context rechecks each historical dependency against live source/processing policy. Revoked or Unknown derived history is omitted, and the same prefix is never silently replaced with today's head on exact replay.

The parent records DelegationIntent before handoff and later records the authenticated immutable Task receipt as DelegationResult. It never copies Task ModelResults into the parent journal. Aggregation includes a distinct Task execution once, even when a continuation replays its receipt; changed evidence for the same execution is corruption. Observer timeout or cancellation after delegation cannot manufacture an Unadmitted/Rejected zero-charge result. Only owner-verified absence of Task admission supports that result.

Task activation precedes Conversation executor activation. Conversation advances its fence and starts one retained, actor-scoped recovery driver with bounded pages/scopes; valid backlogs cannot fail Ready merely by exceeding one page. Exact-origin lookup and transactional child admission remain separate from discovery. Queries never trigger recovery.

A parent failure with unresolved delegated evidence stores an immutable pending-terminal intent and retains the active Session. The driver observes the original Task, authenticates the actual receipt and appends it exactly once before finalizing the original failure atomically. Orphaned Working Runs require an admission barrier and proof that no retained driver remains. Existing terminal Runs can gain late evidence without changing their terminal meaning. Accounting is recomputed from authenticated continuation lineage with Task deduplication; the Session is updated only under the exact still-current terminal revision and no-active-turn CAS. A newer Session cannot be overwritten by late work. Stored Run metadata and its journal are read in one transaction.

A continuation first reconciles unresolved delegation from actual stored Task evidence. A Working Task remains pending; recovery never reruns its endpoint. Ancestor adoption authenticates the original request and exact batch/cursor lineage without rewriting the Task's original parent. A recovered Blocked Task retains replay/accounting and its pending cursor, is omitted from provider history, and blocks the child before any new dispatch so Conversation can publish the authenticated review group.

### Learner recovery and memory review

A Learner claim has an immutable Person/device/budget binding and one authenticated canonical journal. Fresh claims are admitted only from Queued or explicitly Deferred work. Expired Running recovery preserves the same claim for a genuinely empty journal or known acknowledged Output. Incomplete, unresolved or unknown execution becomes Failed with an Interrupted issue and cannot generate a fresh model call.

Staging proves the exact proposal against that claim's stored Output and head. Its immutable stage receipt retains the complete original request even though the candidate key deliberately omits mutable payload/time/revision fields; changed same-key requests conflict. Staging never activates memory. User decisions have immutable command receipts and apply the Knowledge-owned review plan atomically. Public review and decision projections expose safe content, validity and actual acknowledgement identity without storage hashes, idempotency keys or raw provenance.

## External writes and uncertain outcomes

Consequential external effects require an admitted manual instruction or exact Access review/standing authority, current target/source checks, and a durable immutable pre-dispatch intent. Calendar Operations normalizes manual Calendar effects and Expert proposals into one immutable operation record in the existing encrypted Vault. Access owns the operation subject, policy, review reference and decision receipt; Vault composes these owner transitions with operation admission and dispatch-intent CAS in the same immediate transaction. Missing, locked or corrupt encrypted state never selects an alternate plaintext repository or silently resets policy.

Expert proposals currently carry absolute UTC millisecond instants for non-recurring focus blocks. Calendar Operations owns their UTC schedule representation; product callers do not supply a timezone for those hidden artifacts. The pure proposal preview described below authenticates Task evidence for a new effect and can inspect the exact already-admitted operation. Querying or displaying a proposal does not admit an operation.

An Expert proposal names an exact immutable `TaskExecutionReceiptRef` and artifact. The Vault adapter loads the actual committed Task receipt and record in the Calendar Operations admission transaction, and Experts' pure validator proves Person/device, terminal Task, admission, selected sources and exact artifact bytes. Calendar Operations validates its proposal schema, captured Calendar contributor and requested effect. Current Registry binding or enable state is separate from historical provenance. The current source/grant and Access authority still gate first dispatch. One admitted proposal artifact can name only one effect identity; a new command cannot replay it as another write. Conversation publishes an OperationApproval interaction beside that existing operation in the same Vault transaction. Its durable decision is forwarded to Access with the persisted timestamp; the exact Access receipt is checked against the operation before Conversation records resolution and requests a linked fresh Run. Resume observes the existing operation and does not submit the write again.

The native EventKit boundary retains a preparation bound to the exact effect, source, Person/device, host generation and expiry. After encrypted `Executing` commits, one consumed capability crosses the write boundary. EventKit's recomputed `ObservationFingerprint` is an explicit native prewrite condition; it is never relabeled a provider version and does not claim atomic compare-and-write. Exact target, permission and resource checks still have the native check-to-write race. Gateway mirror fingerprints do not grant write capability.

A known native acknowledgement settles a committed effect even if a later observation becomes unavailable. A positive prewrite rejection proves `NotApplied`; timeout, cancellation after dispatch, malformed receipt or lost response leaves `Unknown`. The bounded native cache retains immutable invocation identity and never redispatches a cached invocation. Explicit recovery reads causal receipts first. Only Create may use a unique matching execution marker as bounded positive recovery evidence; matching Update fields or absent Delete targets do not prove who performed the effect. Insufficient evidence remains uncertain, and startup performs no external replay.

A succeeded effect and its pending Day collection ticket commit together. Day records the exact execution ID, receipt digest and normalized collection intent with its projection change, checking current source revision, incarnation and resource membership. Repeated collection rejoins that receipt. Collection failure leaves the Calendar operation succeeded with collection pending; one native acknowledgement does not mark whole-calendar coverage fresh.

## Lifecycle and Connections command recovery

App owns one serial Runtime preparation queue and current readiness. `runtime.readiness` is a pure query. `runtime.prepare` carries an immutable UUID-v4 operation identity; Rust chooses create or open from current physical presence, treats a healthy generation as a no-op, and fences/drains a sealed generation before replacement. Admission distinguishes rejection from uncertainty. Every terminal outcome is archived immutably in the existing encrypted product receipt table before the explicit idempotent ACK can release its in-memory entry. Lookup and archive-write failures retry without repeating the physical operation. Replaying or joining the exact operation preserves Person, device and runtime-epoch checks, and never retires a newer generation. Current readiness and historical operation result are separate AppWire projections.

Connections failure resolution follows the command's actual first durable admission. Product commands use the Connections command journal in Agent Vault; source apply/pause/disconnect use the source-operation journal in the separate host product database. Both stores are encrypted. Each negative receipt is serialized with that journal's admission and fences late insertion. Access review preparation shares the encrypted rejection fence and counts as admission even before the product review record exists. Absence in the wrong journal never proves NotApplied.

Integration cancellation atomically commits its exact-intent receipt and the target operation’s cancellation fence in the same encrypted transaction. The registered owner job records its snapshot receipt; startup rejoins pending intents. Cancellation never issues a remote Begin merely to obtain a handle: an absent observation preserves uncertainty. Integration and catalog drivers live until completion, owner shutdown or a terminal authority failure, using bounded observation rounds. Source operations do not expose a Cancel action; their existing Access receipt reconciliation remains authoritative. A revision advancing during uncertain delivery cannot make the same admitted cancellation become a fresh rejected command.

Fresh Observe admission checks Access-owned review expiry, consumer policy and current grant expectations before acquiring a source reservation. The same grant-expectation predicate is used by transactional commit, including stale Present authority and a previously Absent grant that now exists. Recovery reads immutable Access receipts first. If no receipt exists and the review is no longer applicable, Access atomically records an abort or returns the winning committed receipt; only that causal result releases or completes the source fence. A proved aborted Allow command resolves as NotApplied, so exact client retries do not remain uncertain indefinitely.

Integration observations retain cancellation direction, do not re-offer Cancel or authorization launch after cancellation, and avoid writes for unchanged snapshots. Equivalent nonexpired launch descriptors retain their stored expiry; a provider observation timestamp is not a business revision. Transient credential-read failures retain retryable classifications rather than masquerading as changed identity. A terminal authority failure is persisted as a non-polling RepairRequired state. Catalog refresh wake generations are checked while releasing the active-job lease, so a concurrent refresh request either keeps the job alive or starts its successor.

The app-lifetime Connections controller retains unresolved exact requests separately by action and target. It prevents a changed decision from borrowing an unresolved command identity, while allowing unrelated targets and explicit repair actions. Each pending request has its own replay control keyed by the canonical reference value. Runtime preparation uncertainty retains the same preparation UUID until its durable result is acknowledged. A pure readiness reobserve cannot allocate a new preparation or change the physical Vault state; a fresh UUID is allowed only after the prior operation settled and the Runtime owner exposes Retry.

## Cancellation

Cancellation flows from the owning Run/Task/execution scope. Query, preview, observer timeout, route refresh or screen disposal are not implicit cancellation of durable work.

Do not hold a global Vault transaction across model or provider I/O.

## Related decisions

- [ADR 0016 — Native Agent model protocol](../decisions/0016-native-agent-model-protocol.md)
- [ADR 0018 — Manager–Expert A2A delegation](../decisions/0018-manager-expert-a2a-delegation.md)
- [ADR 0024 — Device context collection and convergence](../decisions/0024-device-context-collection-and-convergence.md)
- [ADR 0025 — Person-owned connections](../decisions/0025-person-owned-connections.md)
- [ADR 0027 — Connection authority and observation](../decisions/0027-connection-authority-and-observation.md) (accepted; standing Observe semantics amended by ADR 0031)
- [ADR 0031 — Connection-owned source scope and logical standing Observe](../decisions/0031-connection-owned-source-scope-and-logical-observe.md)
- [ADR 0034 — Gateway reasoning and source-owned processing authority](../decisions/0034-gateway-reasoning-and-source-processing-authority.md)


Pairing command resolution is durable: public Start and Cancel, and the internal key-possession confirmation step, atomically commit the exact immutable intent with either its safe success snapshot or a terminal rejection. `NotApplied` means that stored whole-command decision; it is not inferred from a Conflict error or from a missing read response. Replays cannot turn that rejection into later execution. Attempt-local `NotAdmitted` remains distinct and cannot erase prior uncertainty. A monotonic admission generation survives restoration of a prior credential expectation. Cancelling is a nonterminal workflow phase; late exact remote responses are retained without reversing local Cancel/Forget or publishing retired authority.


Product-record Connections commands resolve uncertainty by checking their exact admitted record and committing a negative receipt when absent. Both first insertion and atomic Forget honor that fence. Preparatory native source construction is pure; the admitted NativeSetup operation materializes it before any OS prompt. Source-operation journals are not inferred absent from a missing product record. Enrollment receipt readback contains only identity/digest metadata, never a newly minted owner signature. RevocationPending and Forgotten pairing snapshots are terminal and cannot advertise live Gateway authority.


Source-operation command rejection is resolved inside the host product database source journal, against the exact source reservation identity. It never uses absence from the separate Agent Vault Connections command journal as evidence. The source reservation transaction checks the journal's immutable rejection fence before its first write. Existing Reserved/Committed/RepairRequired source operations remain admitted and fenced, even if a separate grant receipt is unreadable. Thus rejection can settle a genuinely unadmitted source request without treating a missing cross-store receipt as an abort.

### Human enrollment deadline

Gateway Trust defines a five-minute maximum for the human enrollment ceremony, shared by Pairing challenge issuance and the final Trust activation check. This deadline covers comparing the code displayed in Floe with the dashboard and explicit administrator approval in the dashboard. The client has no separate human confirmation action. Its admitted Start intent drives an exact, durable key-possession proof step after accepting the signed challenge. The dashboard never treats that cryptographic proof as evidence that a human compared codes. The original signed deadline never renews on confirmation, refresh or same-operation replay. Expired requests cannot activate; a new request requires an explicit user intent. Source admission/release proof lifetimes are separate and unchanged. Dashboard phase guidance and remaining time are presentation only; server and Vault checks remain authoritative.

Proposal preview is a pure Calendar Operations read. It returns the owner-derived title/schedule and eligible destinations for an unadmitted proposal, or the already-admitted operation snapshot after exact actor/receipt/artifact verification. Inspecting an existing operation never redispatches it and does not depend on its original grant remaining active. In addition to authenticated Task evidence, temporal bounds and current source fences, it checks current grant coverage through a read-only repository transaction using the same coverage validator as admission. This observation grants no write permission and does not reserve a later submission; admission and dispatch retain their atomic current-authority checks.

At the EventKit boundary, receipt and target comparison use Foundation-normalized timezone identifiers, not raw alias spelling or offsets sampled at a single instant. After exact UTC endpoints and normalized zone identity match, the physical receipt retains the admitted display-zone spelling so its immutable effect digest remains stable. External revision fingerprints retain the actual native timezone observation. Create-marker recovery uses the same translation; it never rewrites stored intent or resubmits an uncertain write.

Agent Vault database statements and Connections source-operation storage expose
contention as a distinct StorageBusy outcome. Connections command
settlement retains the exact request and resolves its durable outcome on retry; it
never writes a negative receipt from contention. Already-known admission remains
admitted. A Busy observation does not retire the Vault; semantic Conflict and
commit/rollback uncertainty retain their separate recovery behavior.

Assistant authorization receipts permit exact live replay after a lost acknowledgement.
The operation/challenge, canonical digest, full expectation and expiry must match;
current enrollment, grant, producer proof and expiry checks run again before the
deterministic signature is returned. A receipt is not historical authorization.

The product-store source journal serializes its mutation paths through a writer
guard held until SQL outcome and availability are settled. A failed commit/rollback
or a dropped, armed SQL future latches the store before releasing that guard. New
connections and negative-journal writes then fail until explicit reopen; absence
after an uncertain commit is never used to manufacture NotApplied. Readiness does
not automatically delete data or replace a key. Assistant admissions permit one
release receipt; exact live replay of that receipt remains permitted.

The same physical admission/drop mechanism also fences the Vault product journal,
review admission, Gateway credential mutations and authorization receipt writers.
All writes that can establish the product command's durable admission share its
negative writer's mutex through the terminal SQL/latch step. Store-specific access
checks and finishers retain their own failure types; the shared guard owns only
admission and uncertain-drop retirement. Source operation background recovery
(configure and reconcile, including review application) retries only StorageBusy
with bounded backoff under the existing deadline and cancellation. Configure reloads
the current durable product record; reconciliation reuses the same reservation and
revalidates current evidence, expiry and any existing Access receipt. Authority,
integrity and uncertain-write failures are not blindly retried. Sustained contention
can repeat evidence reads; this is not a provider-write retry policy.

A non-Busy BEGIN failure conservatively retires the affected store until reopen,
as does an abandoned armed BEGIN. A determinate Busy BEGIN leaves it usable.
Read-only integrity errors retain their typed failure but do not all retire the
entire Vault generation; callers must not infer a global latch from the error alone.

Source presentation changes use a single local settlement transaction in the
Connections source journal. Native observation happens before this transaction and
never publishes a presentation reservation fence. Exact command replay is checked
first; then an unchanged source identity/scope may receive its label-only successor
and terminal record atomically. A rejection is recorded in that same journal without
changing the source, so it cannot overtake an uncertain positive commit. An active
authority reservation is a semantic fence and rejects local settlement before either
outcome is written; it is not classified as SQL Busy.

No Access commit/abort receipt is manufactured. Standing Observe and SourceAuthority
remain unchanged for labels; account identity changes advance SourceAuthority and
follow the authority-changing workflow. Local revision and journal watermark changes
still invalidate in-flight exact source snapshots and Calendar Operations. Completion projects the recorded
historical successor rather than requiring it to remain the latest source forever.
Known presentation rejection settles the product command as NotApplied. Foreground
and recovery share one per-command drive lease. Other foreground callers observe
the existing result without driving again; interruption releases the lease before
handing unfinished work to recovery. Completion reloads a winning terminal product
record. Local commit has no separate user-visible cancellation stage.

Authority configuration rejected before reservation uses the source journal's
negative receipt to exclude future admission, then settles product NotApplied.
Replays consult that receipt before re-observing mutable evidence. A Completed
authority operation never invalidates grants again merely to render its result;
current summary reads may still require Access availability. Owner shutdown closes
new settlement and handoff admission.

Foreground ownership registers a directional child cancellation scope: owner shutdown
may cancel the drive without cancelling the caller or an enclosing Run, while caller
cancellation still reaches the drive. After native observation the owner and scope
are re-admitted before reservation or Access invalidation. Before reservation, AccessReviewRequired from the original stored-source
observation is definitive subject drift and uses the guarded negative receipt;
a candidate-selection error is not positive mismatch evidence; transport, storage, cancellation and other permission failures
remain pending rather than becoming invented negative outcomes.

### Settled configuration after grant invalidation

Configure has one dedicated finalization path, scoped to source storage, native
candidate evidence and the Connections lifetime. The immutable reviewed product
record binds its selected resources and captured candidate fingerprint. Before
reservation, a fresh candidate mismatch gets the existing no-effect rejection.
After a canonical InvalidateSource receipt, all live grants remain paused and require
review. A matching candidate may complete under the original stored-source CAS;
obsolete deselected native resources are not probed again. Other source operation
kinds retain their original finalization checks.

A stable positive candidate mismatch after invalidation settles as
ConfigurationRejectedAfterInvalidation, with the original source unchanged and the
source fence released atomically with the journal outcome. Generic native errors,
malformed/unstable evidence, cancellation, a changed stored source or mismatched
receipt do not qualify. The receipt ID/digest, operation, review and exact expectation
remain checked; no grant is restored and no source is implicitly disconnected.
Terminal replay reconstructs its product acknowledgement without reacquiring native
evidence. Repeating an identical repair observation does not advance its revision.

The configure command returns Configured or NotSavedReviewRequired as settled
business outcomes. The latter is not a no-effect NotApplied error: Access invalidation
already happened. Flutter settles only that exact pending command and closes the
obsolete review, showing historical non-save feedback rather than reporting Save
success. A later source review is a new explicit intent, not an automatic retry.


Reviewed selection references retain their incoming order only for command intent
correlation. Connections resolves the references once into a unique handle-sorted
candidate resource list. Admission, pre-reservation recheck and finalization pass
that candidate with the same original source/revision to the evidence port. The
finalizer checks the normalized operation against the caller's reviewed reservation
template. A Configure operation cannot move from Reserved directly to
RepairRequired; its committed receipt digest must first be pinned. A canonical Access
abort while Reserved settles the source journal without mutation. An expired review
that has not reserved uses the guarded renew-review rejection before any effect;
expiry is checked again after native reads immediately before reservation.

An evidence error that persists after invalidation can still leave a fenced command:
there is no synthesized negative native proof or automatic disconnect/reset. In
particular, removing selected calendars or denying OS access is not equivalent to a
successful candidate inspection with a different fingerprint. Recovery of that
condition requires a separately designed owner contract. Reserved operations with no
Access receipt can still invalidate before the next candidate inspection; an extra
pre-effect inspection/abort policy is not currently implemented.

Review dismissal is client presentation, not durable consumption of the server-side
review record. Same-phase repair idempotence applies to source operations generally;
authority observation is fenced by operation/reservation identity and generation,
not by an increment on every repeated repair observation. Flutter's historical
non-save feedback is keyed by source, distinguishes initial configuration from
reconfiguration, and clears after that source's later successful configure, sharing
review/pause or disconnect. Wire validation and source correlation failures after a
possibly committed command remain Indeterminate and preserve its command identity.
