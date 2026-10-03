# Authority and recovery invariants

These are durable architectural safety properties, not a progress checklist.

## Accepted processing authority and current implementation

[ADR 0034](../decisions/0034-gateway-reasoning-and-source-processing-authority.md) supersedes exact model-recipient consent as a required authority. Preserve verified Gateway identity, Access-owned source processing permission, source/grant provenance, key identity, CAS, durable pre-dispatch intent, cancellation direction and uncertain-write recovery. DeviceOnly/GatewayAllowed belongs to the source grant; processing expansion belongs to source/connection review, never model routing. Health additionally requires source-owned local-transform evidence and retains HighlySensitive classification. Neither pairing nor transformation grants source or Action permission.

The authority descriptions below that mention recipient consent, recipient lineage, allow_external or expected_recipient describe the current pre-cutover implementation, not invariants to retain. Their replacements are accepted but not yet implemented here. The [convergence plan](../development/plans/reasoning-source-processing-convergence.md) owns their ordered removal; update current implementation descriptions as those code cutovers land. Existing live admission/handoff/post-I/O/release checks must not be removed merely because their recipient-specific representation is being retired.

Prompt, card, Run-frame and environment manifest hashes are diagnostic content identities only. They never establish source freshness, grants, processing permission, Gateway identity or execution permission, and never replace the durable Experts-owned Run environment identity or live Context/Access checks.

## Authority is explicit and owner-scoped

- A Connection describes source/account/execution-owner lifecycle; it does not itself authorize AI use.
- Connections durably owns each Calendar, Contacts, Attention and Wellbeing `SourceConnection`'s current resource set, local configuration CAS revision, `SourceAuthority` and trusted native subject fingerprint. Callers submit only an expected local revision. Reviewed native creation starts Ready at revision 1; a combined resource/subject edit advances source authority exactly once. A provider/server revision cannot substitute for the local CAS revision. There is no standing native source side table in Vault.
- Day stores only Calendar events, provenance, freshness and per-source sync failures under an independent mirror revision. App checks current Connections resources before Day import. Successful or partial sync and transient provider failure cannot rotate source authority; an old Day mirror cannot restore a removed resource or disconnected source. Context, Access and Actions reload Connections for admission and continuity, including after provider I/O.
- An Expert assignment's persisted `SourceSelectionReference` is configuration identity only: connector, connection, execution owner, capability, resource and contract version. It contains no credential, grant, source-authority revision, payload or processing consent. A Task admits the exact Run-pinned selection; current binding/enable drift does not revoke it or redirect it to another source. The Task continues with its pinned references while source/grant/recipient/provider/OS authority remains live and may deny. Completed Task replay returns the historical result without reacquiring evidence.
- Access owns grants, exact recipients, purposes, processing restrictions, revocation and admission/release fences, including the contextual recipient-consent store: an approval grants one exact reviewed dispatch, never a standing recipient allow.
- A standing `DataAccessGrant` binds the stable Person/Connection/Connector/execution-owner source and immutable source identity, not a current `SourceAuthority`. `GrantAuthority` is its only permission/state/scope epoch; source epoch changes leave the grant intact but stale earlier dependencies. Standing personal grants name `people.identity:<connection>`, `attention.coarse:<connection>` or `wellbeing.derived:<connection>` rather than physical resources. A remote View grant is looked up directly by exact stable source and logical resource, without a mapping table. Connections owns all standing source authority.
- A `ContextDependency` records `GrantAuthority`, grant permission `resources`, exact observed `source_resources`, and `SourceAuthority` separately. Native and hosted Calendar permissions each name one logical connection View. Native Context acquires every current Calendar ID from Connections; hosted Context verifies a generic signed source preview carrying the exact current server resource set, then reads through generic View admission. Both record physical IDs only in `source_resources` and stale prior dependencies when source authority or that set changes. Resource edits do not mutate the standing grant or Expert binding. Current-source reauthorization compares source authority and physical resources outside the Vault grant transaction; Vault validates only the current grant and its `GrantAuthority` inside the transaction. The hosted Observe product intent names the connection, never a provider leaf.
- Context may acquire/project only evidence authorized for the current Person, source, scope and freshness requirements.
- Inference may choose an approved model route, but route selection cannot enlarge data authority.
- Saved provider credentials remain private to credential/provider boundaries and are not product-wire or Conversation inputs. The narrow exception is newly issued approved-pairing output for secure persistence, never a subsequent request input; token-bearing Debug/diagnostics are redacted.
- Root, built-in Expert and Schedule composition share one host-scoped current-connection store. Provider adapters bind loaded credentials to the verified person/device; the saved pairing contains no recipient approval. Exact-recipient authority reloads current state at admission, handoff and post-response revalidation. Only a consumed Access fence produces the prepared transport target. External server requests then carry request-scoped `allow_external=true` and the exact `expected_recipient`; local requests carry `allow_external=false` and no external recipient. A prepared transport or availability observation never substitutes for those checks.
- Pairing setup accepts only a bounded loopback endpoint and pairing evidence. Person/device come from AppHost's verified `CallerContext`, never a product route bundle; Connections/key-holder validation binds the exact pending pairing ID, signed challenge and owner issuer. Remote authority and generic View grant services prepare transports from the same current store through provider-owned exact person/device admission. Access still validates producer/source/revision/provider/recipient/grant evidence; model consent and source catalogs are not pairing/grant request fields.
- Revocation prevents later admission or release. It cannot retroactively recall data already transmitted or a provider effect already accepted.
- Source revocation, pause and drift affect Access authority and later dependency admission only. They never mutate Expert Registry state; a Registry revision change is never required to block, and never sufficient to admit, a source read.
- Connection-level **Use with Floe** is a read-only projection plus explicit App intent, never persisted authorization state. Native Calendar, hosted Views, Contacts, Attention and Wellbeing use the same `ConnectionObserve` product wire. Review reloads current Connections source, policy and grants to produce a compare-only expectation; enable echoes that exact expectation, not Calendar IDs, Contacts handles or provider leaves. Explicit connection completion may review the App-derived first-party Observe bundle; startup inspection may not. Off pauses Observe, disconnect revokes Observe before source deletion, and neither operation changes Act or model-recipient authority. Source edits are separate Connections commands and never implicitly expand a grant.
- Native Calendar review probes the full current Connections Calendar set as compare-only evidence, and Context acquisition reads that full set without a fixed calendar-count cap. Hosted Calendar's generic signed View preview binds the server connection's canonical current `calendar_ids`; generic admission/read/release and a bounded composite cursor read every current provider leaf through one connection-level runtime. The grant scope names one logical connection View; encoded-byte, provider-I/O, payload and identifier budgets, source authority and identity preflight still apply to the physical set. Scope edits do not automatically re-review active Observe.
- Context identifies a remote View target by the current Person, connector, connection, requested View and that connection's exact canonical resource before checking grant permissions or detecting duplicate live authority. Unrelated grants do not become candidates; a known target with a consumer, category, operation, purpose, processing or review mismatch remains a blocker. The signed producer preview and binding path rechecks capability and source authority before payload I/O.
- Exact-recipient consent identity includes the logical grant resource, exact observed source resources and current source authority. A changed leaf or epoch requires a new review even when the standing grant is unchanged.
- App derives standing Observe consumers exclusively from trusted shipped Expert manifest capability declarations, independent of installed assignment and binding state. An arbitrary installed/supplied extension receives no automatic first-party consumer. The root Manager has no source-backed domain Tools or standing source consumer. Binding changes neither create nor revoke a grant and cannot alter the first-party intended View permission policy.
- App computes a deterministic `policy_digest` from each intended logical View ID, sorted trusted consumers and categories, Read operation, purpose and actual standing processing restriction. It excludes assignments, source leaves, source/grant authority, producer and subject. Connection review echoes the digest through the client and enable recomputes it before mutation. Conversation stores it as compare-only evidence in each immutable reviewed member, including reviewed absence, and keeps one source revision at bundle level. Resolution and refresh reject digest drift before mutation. The digest is not an authorization epoch; remote producer audience remains signed source-transport evidence, not model-recipient consent.

## Provenance and coverage travel with evidence

Source-backed model/tool inputs retain enough identity to determine:

- Person and source/producer;
- authority/grant dependency;
- observation/projection revision;
- coverage and freshness;
- consumer/purpose/data class;
- exact processing recipient in the pre-cutover implementation; ADR 0034 replaces this with the source processing boundary and required Health transform provenance.

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

The parent records DelegationIntent before handoff and later records the authenticated immutable Task receipt as DelegationResult. It never copies Task ModelResults into the parent journal. Aggregation includes a distinct Task execution once, even when a continuation replays its receipt; changed evidence for the same execution is corruption. Observer timeout or cancellation after delegation cannot manufacture an Unadmitted/Rejected zero-charge result. Only owner-verified absence of Task admission supports that result.

A continuation first reconciles unresolved delegation from actual stored Task evidence. A Working Task remains pending; recovery never reruns its endpoint. Ancestor adoption authenticates the original request and exact batch/cursor lineage without rewriting the Task's original parent. A recovered Blocked Task retains replay/accounting and its pending cursor, is omitted from provider history, and blocks the child before any new dispatch so Conversation can publish the authenticated review group.

### Learner recovery and memory review

A Learner claim has an immutable Person/device/budget binding and one authenticated canonical journal. Fresh claims are admitted only from Queued or explicitly Deferred work. Expired Running recovery preserves the same claim for a genuinely empty journal or known acknowledged Output. Incomplete, unresolved or unknown execution becomes Failed with an Interrupted issue and cannot generate a fresh model call.

Staging proves the exact proposal against that claim's stored Output and head. Its immutable stage receipt retains the complete original request even though the candidate key deliberately omits mutable payload/time/revision fields; changed same-key requests conflict. Staging never activates memory. User decisions have immutable command receipts and apply the Knowledge-owned review plan atomically. Public review and decision projections expose safe content, validity and actual acknowledgement identity without storage hashes, idempotency keys or raw provenance.

## External writes and uncertain outcomes

Consequential external effects require an admitted manual instruction or exact Actions review/standing authority, current target/source checks, and a durable immutable pre-dispatch intent. Manual Calendar effects and Expert proposals share one encrypted Actions record. Missing, locked or corrupt encrypted state never selects an alternate plaintext repository or silently resets policy.

An Expert proposal names an exact immutable `TaskExecutionReceiptRef` and artifact. The Vault adapter loads the actual committed Task receipt and record in the Actions admission/dispatch transaction, and Experts' pure validator proves Person/device, terminal Task, admission, selected sources and exact artifact bytes. Actions validates its proposal schema, captured Calendar contributor and requested effect. Current Registry binding or enable state is separate from historical provenance. The current source/grant and Actions authority still gate first dispatch. One admitted proposal artifact can name only one effect identity; a new command cannot replay it as another write.

The native EventKit boundary retains a preparation bound to the exact effect, source, Person/device, host generation and expiry. After encrypted `Executing` commits, one consumed capability crosses the write boundary. EventKit's recomputed `ObservationFingerprint` is an explicit native prewrite condition; it is never relabeled a provider version and does not claim atomic compare-and-write. Exact target, permission and resource checks still have the native check-to-write race. Gateway mirror fingerprints do not grant write capability.

A known native acknowledgement settles a committed effect even if a later observation becomes unavailable. A positive prewrite rejection proves `NotApplied`; timeout, cancellation after dispatch, malformed receipt or lost response leaves `Unknown`. The bounded native cache retains immutable invocation identity and never redispatches a cached invocation. Explicit recovery reads causal receipts first. Only Create may use a unique matching execution marker as bounded positive recovery evidence; matching Update fields or absent Delete targets do not prove who performed the effect. Insufficient evidence remains uncertain, and startup performs no external replay.

A succeeded effect and its pending Day collection ticket commit together. Day records the exact execution ID, receipt digest and normalized collection intent with its projection change, checking current source revision, incarnation and resource membership. Repeated collection rejoins that receipt. Collection failure leaves the Action succeeded with collection pending; one native acknowledgement does not mark whole-calendar coverage fresh.

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
