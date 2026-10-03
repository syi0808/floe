# S2 Day product Calendar read contract proposal

Design only, based on source checkpoint `7f62cd72` and the accepted architecture plan. No S2 production implementation, compilation or validation is claimed. This is a bounded supplement for the canonical contract, not a second execution plan. The coordinator accepted the locked-Vault distinction and the request, limit, commit-fence and revision decisions recorded below on 2026-10-02.

## 1. Existing constraints and gaps

- Canonical §4.1 puts `CalendarAcquisitionPort` in Day and its implementation in Context. Day resolves refresh persistence, reconciliation and mirror CAS; Context resolves configured sources and orchestrates acquisition. Day must not import Context, Connections or Access.
- `access/application/calendar_read.rs::CalendarReadAccessAdmission` currently requires a real grant and fixes `GrantPurpose::Assistant`. Neither this type nor a fabricated `floe.day` consumer can represent a direct product refresh.
- `server/internal/views/calendar.go::CalendarView` is a bounded assistant projection. It lacks event origin, external revision and lossless civil-date schedule information. Google and Microsoft adapters currently select only the fields that projection uses and truncate titles. It cannot be reinterpreted as a complete Day mirror.
- `day/domain/calendar.rs` currently puts one connection/provider on the entire mirror. Event origins lack connection identity. This cannot represent multiple accounts without collisions or first-source selection.
- Vault appendix §1–2 keeps Day and source metadata outside encrypted agent state. Its §3 blocks grant-dependent source use while locked. App appendix §8 explicitly locks external Actions, not all Day display or local CRUD.

## 2. Authority and caller graph

Direct Day display has the closed purpose `DayCalendarRefresh`. Access alone admits it against the admitted Person/device, exact configured source/resources, current source authority, current OS/provider permission, bounded query and expiry. It is a separate authority kind from assistant Observe. It creates no `DataAccessGrant`, `GrantConsumer`, `GrantPurpose::Assistant`, model projection, Action approval or processing expansion. Paused/disabled Observe does not disable configured product display; a pending source mutation or identity change does.

The final call graph is:

1. Flutter emits `day.refresh(command_id, DayQuery)`; FFI only converts and forwards the host-admitted actor.
2. Day durably admits/rejoins the refresh operation, captures the exact mirror expectation and starts owned work. `day.refresh.get` only reads the operation.
3. Context implements Day's acquisition port using independent `ContextCore` product handles: Connections source inventory/facts, Access product-read authority, native and Gateway Calendar batch adapters, and a clock.
4. Context resolves every configured Calendar source for this Person. Access admits each exact source read. The provider/native adapter acquires bounded records. Access validates read/release continuity.
5. Context returns typed per-source/per-calendar results and exact commit expectations. Day validates the result against its admitted operation, reconciles by full origin, and makes one short mirror/operation CAS.
6. Day projects safe display state. Assistant reads of any resulting cache still obtain a fresh assistant grant and dependency through Context. Product-read provenance is never accepted as model permission.

App constructs and injects handles. Go authenticates, checks exact signed authority and current source identity, performs provider I/O and normalizes records. Neither owns a second product-read policy engine. Pure source-continuity predicates remain in Access/Connections and are shared by the product and assistant paths.

## 3. Day owner contract

Use the existing DayQuery display fields as a typed owner value: `date: NaiveDate`, `timezone_offset_seconds: i32`, `end_timezone_offset_seconds: Option<i32>`, `now: DateTime<Utc>`. `now` affects display projection only. Admission, expiry and observation timestamps always use the owner clock. Refresh initially covers exactly the requested civil day, including the two supplied endpoint offsets; it does not invent a prefetch window.

```rust
enum MirrorExpectation { Absent, Present(Revision) }

struct CalendarRefreshRequest {
    actor: OwnerActor,
    refresh_operation_id: Uuid,
    query: DayQuery,
    expected_mirror_revision: MirrorExpectation,
}

trait CalendarAcquisitionPort: Send + Sync {
    fn acquire<'a>(&'a self, request: CalendarRefreshRequest,
        scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<CalendarAcquisition, CalendarRefreshError>>;
}

DayService::new(repository, Arc<dyn CalendarAcquisitionPort>, clock)
DayService::refresh_day(actor, command_id, DayQuery, scope)
    -> Result<DayRefreshSnapshot, DayError>
DayService::get_refresh(actor, operation_ref, scope)
    -> Result<DayRefreshSnapshot, DayError>
```

The coordinator accepted `refresh_operation_id` as the persisted Day operation ID, not a new per-poll UUID or `ExecutionScope::scope_id()`. The accepted `MirrorExpectation` replaces invalid `Revision(0)` for proven absence. These are required precisions beyond the earlier request shorthand; neither field comes from Flutter.

`CalendarAcquisition` carries `{refresh_operation_id, person_id, device_id, range, inventory, sources, completed_at}`. `inventory` is the sorted complete set of relevant configured source references/revisions observed by Context, with a canonical digest. A bounded inventory query must distinguish complete inventory from overflow; it cannot silently stop at the first N sources.

Each `CalendarSourceAcquisition` carries:

- exact `connection_id`, connector/provider and execution owner;
- local source revision and `SourceAuthority`, selected resource identity/digest and private subject/provider identity expectation;
- acquisition/read ID, observation/expiry times and exact range;
- one result per selected calendar: `Complete { records, observed_at }` or `Failed { reason, observed_at }`;
- the final source-version expectation that the mirror commit must compare.

The source authority/subject fields are private owner/repository evidence, not blindly serialized product DTOs. Result reasons are closed: permission denied, source changed, source fenced, unavailable, Vault locked, budget exceeded, deadline exceeded, cancelled. Actor/admission/storage/invalid-contract failure, inventory drift and a successful-source commit-fence mismatch fail the whole operation. Normal provider failure remains explicit per source/resource in the resulting Day snapshot when the inventory and successful-source fences remain stable. Host loss/reopen yields Interrupted and never reissues unfinished acquisition automatically.

Partial pagination is not a complete calendar result. Context accumulates one resource's pages within the admitted total budget, validates duplicates and continuity, and emits Complete only on terminal coverage. Failure discards that resource's incomplete replacement batch and preserves its prior healthy cache. Other resources can commit their successful replacements. A complete empty batch legitimately clears events in its exact interval; a failed batch never does.

The accepted hard ceilings are 64 configured sources, 256 selected calendars in total, 10,000 records and 4 MiB normalized payload per refresh; one day per request, at most 128 records/1 MiB per remote page, and a 60-second operation deadline. Access clamps source permits to the remaining operation budget/deadline and preserves the existing per-native-call deadline clamp. Overflow is an explicit failed acquisition, never truncation or an omitted inventory member. These are accepted S2 contract limits, not claims about implemented or compiled code.

### Durable operation and commit

Day's repository owns these primitives using boxed futures:

```rust
admit_refresh(RefreshAdmission) -> RefreshAdmissionResult // New | Existing
claim_refresh(RefreshClaim) -> RefreshRecord
read_refresh(RefreshLookup) -> Option<RefreshRecord>
commit_refresh(RefreshCommit) -> RefreshRecord
finish_refresh(RefreshFailureCommit) -> RefreshRecord
interrupt_refreshes(RefreshExecutorReplacement) -> Vec<RefreshRecord>
```

Admission atomically persists Person/device, command ID, normalized intent digest, operation ID, original query, mirror expectation and executor generation. Same-command replay returns the original record before source I/O; changed intent conflicts. The exact intent digest is SHA-256 of the canonical typed record `{kind:"day_refresh", person_id, device_id, command_id, date, timezone_offset_seconds, effective_end_timezone_offset_seconds}`, with the end offset normalized to the start offset when absent. Display-only `now` is excluded, as are newly observed source facts, mirror expectations and executor/runtime generations. The first admitted query's display `now` is persisted; retrying the same command with a later display `now` rejoins the same record and returns its stored result without reprojecting or rereading sources. A separate Day snapshot query may request a fresh display projection. A changed date or effective offset is a changed intent and conflicts. Owner-clock time controls every security-relevant check.

Claim transitions Pending to Running once. Read is pure. `commit_refresh` compares operation revision/executor generation, original mirror expectation, complete inventory and the exact successful-source expectations, then atomically persists the new mirror and Completed snapshot. CAS conflict is a typed outcome; it does not repeat provider I/O or silently rebase.

The Turso adapter's final short transaction must also compare the complete current configured inventory, exact successful-source rows and absence of source-operation fences for data being installed. This is a mechanical compare of owner-issued expectations, using Connections' pure source-fence predicate, not Day/Vault inventing configuration policy. No provider/OS/keychain await occurs inside it. Any inventory drift or successful-source fence drift rejects the whole mirror commit with Day Conflict and the closed reason SourceChanged. The prior mirror, including its status/coverage, remains unchanged; the refresh operation records the failure and requires an explicit fresh refresh command. There is no partial commit, retry, rebase or silent inclusion of a concurrently added source in this case.

When inventory and successful-source fences are stable, a normal provider failure preserves that resource's prior cached records while other successful resources and explicit failure status commit together. Thus provider partial failure and authority-snapshot drift have distinct outcomes.

### Multi-source mirror

Replace the single-source `CalendarMirrorState` with per-source state keyed by `(connection_id, provider)` and per-calendar status keyed by the complete origin. Add `connection_id` to `CalendarSource` on every imported event. Reconciliation keys are `(connection_id, provider, calendar_id, external_id)`, never title/time alone. Mirror revision remains one independent CAS revision. Source revision/authority never advances because of refresh success, partial results or failure.

The safe Day Calendar projection contains source display reference/label, availability, last successful observation, attempted interval and per-resource failure/coverage. Raw source authority, account fingerprints, physical calendar/event IDs, external revisions, Gateway routing and authorization bytes stay private. Actions later resolves the selected opaque Day event reference through the Day owner to load exact private event evidence and revalidate it; product DTOs do not supply provider preconditions. Disconnected or changed source cache can be shown as stale/unavailable, but cannot restore source selection, current authority, assistant permission or a write capability.

## 4. Access product-read contract

Add `access/domain/product_calendar_read.rs`, `ports/product_source_authority.rs` and `application/product_calendar_read.rs`. `ProductCalendarReadAuthority` is an Access-owned capability, independently constructible over source-fact ports and clock. It does not require an unlocked GrantRepository merely to evaluate native product reads. It is not an App policy wrapper or another authority owner.

```rust
enum ProductReadPurpose { DayCalendarRefresh }

struct ProductCalendarReadRequest {
    actor: OwnerActor,
    refresh_operation_id: Uuid,
    read_operation_id: Uuid,
    source: GrantSourceBinding,
    range_start: DateTime<Utc>,
    range_end: DateTime<Utc>,
    limits: CalendarReadLimits,
}

trait ProductSourceAuthority: Send + Sync {
    fn observe_current<'a>(&'a self, actor: &'a OwnerActor,
        source: &'a GrantSourceBinding, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<ProductSourceObservation, AgentFailure>>;
}

ProductCalendarReadAuthority::admit(request, scope)
    -> Result<ProductCalendarReadPermit, AgentFailure>
ProductCalendarReadAuthority::revalidate_for_dispatch(&permit, scope)
    -> Result<ProductCalendarDispatchFence, AgentFailure>
ProductCalendarReadAuthority::release(permit, result_binding, scope)
    -> Result<ProductCalendarReadReceipt, AgentFailure>
```

Context selects a source identity; it does not supply trusted authority/resource fields. The inward port reloads current configured source state and source-operation fences, performs the actual metadata-only OS/provider probe, and returns `ProductSourceObservation { expectation: SourceExpectation, observed_at, permission }`. `SourceExpectation.revision` must be present here. Permission is a closed typed native/provider read observation, never a UI assertion. Shared Access validation compares exact actor/device, source identity, source authority, selected physical resources, provider revision and native subject or verified Gateway binding. Source state must be serving and unfenced.

The permit has private fields and no product serialization or public constructor. It retains the exact observation, purpose, owner runtime generation, operation/read identity, range, limits, original cancellation/deadline and expiry. A fresh scope cannot extend it. `result_binding` binds the exact request and normalized payload digest/count/bytes; it cannot replace the admitted scope. Release consumes the permit only after fresh source/permission/identity observation and body binding checks. Its receipt is product-read audit/commit provenance only; it has no conversion into `ContextDependency`, `CalendarReadAccessAdmission`, `PreparedModelPlan` or an Action approval.

At S2, change the existing Access signer to accept a closed command enum with separate `AssistantView` and `DayCalendarRefresh` variants. Product signing requires this retained product permit and independently verified challenge claims; assistant signing still requires the actual live grant. Do not make grant/consumer fields nullable on the existing assistant command or let a purpose string bypass its grant validation.

## 5. Native and Gateway acquisition/release fences

### Native

The existing `CalendarBroker` is the OS transport. Use `InspectSubject` before admission and the existing `ReadEvents` payload after dispatch revalidation. Retain an outstanding host request bound to request/read ID, admitted actor/runtime, host registration/epoch, exact connection/revision, selected calendar IDs, range and deadline. Native completion echoes those fields; callback bodies cannot create requests or choose sources.

Require current read permission and matching native subject before/after the actual read. Normalize exact per-calendar batches, then perform the final native subject/permission probe and source-row/fence comparison before release. OS permission or native identity changes discard the acquired replacement data. The native host can report observations; Access decides their effect. No new public trusted-calendar publication route is introduced.

A metadata observation is point-in-time evidence, not an impossible transaction over EventKit and SQLite. The final short source-row/mirror CAS closes local configuration races. Source/OS changes after a valid release make retained display data stale and deny subsequent reads; they never create assistant permission.

### Gateway private wire

Add the real provider result contract `calendar.mirror` with private routes:

- `POST /v1/calendar/mirror/source-preview` — signed current exact configured source/physical resources/provider identity facts; no read authorization.
- `POST /v1/calendar/mirror/admit` — issue product-read challenge after provider preflight.
- `POST /v1/calendar/mirror/read` — consume exact Rust-signed admission once, acquire one bounded page and stage bytes.
- `POST /v1/calendar/mirror/release` — consume exact Rust-signed release once and return the staged page.

These reuse the existing Trust, source owner, challenge registry, staged-result accounting, replay protection and source lifecycle locks. They do not duplicate the authority engine. Internally use a closed authority-request variant `DayCalendarRefresh` alongside the existing assistant-view variant. Route and challenge-kind checks must agree.

Product challenge operations are exactly `day_calendar_admission` and `day_calendar_release`; purpose is exactly `day_refresh`, result kind exactly `calendar.mirror`. The signed canonical bytes include:

- schema/operation/challenge ID/nonce/key ID, issued/expiry timestamps;
- exact Person/client/device and producer audience/binding identity;
- refresh operation ID, source read ID and per-page ID;
- connector/connection/execution owner, local expected source revision, provider connection revision, source incarnation/epoch, provider identity/generation;
- sorted configured physical calendar IDs, query SHA-256, exact interval, page/total record and byte limits;
- for release only, consumed admission ID and exact staged result SHA-256.

Product challenges forbid `grant`, `consumer` and assistant-purpose fields. Assistant challenge parsers reject the product operation/purpose/result kind. Neither route can replay the other's proof. The strict decoder rejects unknown/duplicate/case-variant fields and compares independently decoded claims with the retained Access permit before signing. The producer signature, current Vault pin, current paired credential generation and original owner key are independently checked. There is no synthetic grant ID, model profile or new credential creation.

Local source revision and provider connection revision are distinct. Go echoes the Rust-bound local revision but validates its own actual provider revision; Rust compares both against the retained exact expectation. Neither side substitutes one revision for the other.

Each page query names one admitted physical calendar, exact interval, bounded limit and opaque continuation. A continuation is a Go-owned authenticated or server-stored value bound to read ID, source/account/resource generations, interval and remaining total budget. It is never a caller-selected URL, a new resource or an authority claim. Product DTOs never see it.

The result is a strict `calendar.mirror` page with the exact read/page/source/physical-resource binding, observed/expiry times, interval and `Complete { records } | More { records, cursor } | Failed { reason }`. A record carries genuine provider event identity, full bounded title and tagged schedule: `timed {starts_at,ends_at,timezone}` or `all_day {start_date,end_date_exclusive}`. Preserve the provider's civil dates rather than reverse engineering them from UTC milliseconds. Read-only remote support advertises no Calendar write action; do not fabricate modify permission.

The accepted private external-revision contract is `CalendarExternalRevision::ProviderOpaque(String) | ObservationFingerprint([u8; 32])`. Its strict wire union is `{kind:"provider_opaque",value:String}` or `{kind:"observation_fingerprint",sha256:lowercase_hex_64}`. Preserve an actual opaque provider revision when one is available. Otherwise hash the canonical normalized event observation with SHA-256 and label it ObservationFingerprint. Do not manufacture integer versions. An observation fingerprint supports equality/provenance only and is never a provider conditional-write precondition. Actions must independently establish the provider's genuine write precondition or return unsupported/unavailable; neither the fingerprint nor the product DTO can substitute for it.

Go revalidates authenticated principal/issuer, exact live source revision/authority/resources and provider identity before acquisition, after provider I/O, and before staged release. Network identity preflight stays outside owner locks, followed by a short exact identity-generation fence. Rust revalidates local source facts and Gateway binding before signing admission, before signing release and after decoding the released result. Any drift discards that page; timeout/permission/credential failure never becomes an empty complete result or native fallback.

## 6. Locked-Vault and construction lifetime

This distinction is coordinator-approved for S2:

- Day snapshot/local captures, events, tasks and notes remain available from the independent Day repository while Vault is locked.
- Native configured-source product refresh remains available through the independent Access product-read capability and current OS permission. It has no Observe dependency. A durable unresolved source operation still fences that source even while Vault is locked.
- Gateway product refresh needs the existing encrypted signing key and producer pin. A locked/unavailable generation yields typed VaultLocked/Unavailable for that source. It does not use a plaintext signer, a cached signature, a replacement key or absence fallback.
- Mixed native/Gateway refresh can produce native results plus explicit Gateway VaultLocked coverage. Prior Gateway cache is not rewritten as fresh. Grant reviews, assistant reads and external Actions keep their accepted ready-Vault requirements.

Construct host-lifetime Day/product Context handles before encrypted owner generations. Gateway signing/trust uses an explicit generation lease port: acquiring a lease returns Ready or typed Locked/Unavailable; a lease binds one immutable existing Vault generation and fails after seal. It must not expose an optional whole service, mutable service locator or late Context/Day initialization cycle. Lock closes that generation's Gateway admission and cancels its in-flight reads; native product work belongs to the admitted host lifetime. Host closure cancels both. Reopening does not revive a prior permit or replay an interrupted Day refresh.

## 7. Canonical reconciliation before implementation

The coordinator has accepted the durable `refresh_operation_id`, explicit `MirrorExpectation`, hard ceilings, whole-commit SourceChanged/Conflict on inventory or successful-source fence drift, and tagged external-revision contract. Merge those decisions and the display-`now` replay rule into the canonical appendix before implementation. The source set must be complete or explicitly over budget; no first-source or silent truncation policy is acceptable.

The remaining private-wire choice is the exact product challenge schema declaration. Implement its Rust/Go codecs together, preserving distinct operation tags and forbidding grant/consumer fields on product proofs. Google/Microsoft readers must acquire the actual revision/civil-date metadata needed by the accepted normalized contract; they must not adapt the existing lossy assistant projection into invented mirror records.

No additional user policy decision is needed for the accepted owner/locked-Vault separation. These design decisions do not authorize S2 production edits before G1 and are not compilation evidence.
