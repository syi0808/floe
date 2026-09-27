# 01: Connection-owned source and resource authority

Prerequisite: 00 complete.

Status: Not started.

Planning base: `main` at `9847c4fff9eda343e09694018394e4f2451a7db0` on 2026-09-28.

Checkpoint 00 proved the production defect rather than merely inferring it: Calendar Observe still derives grant consumers from exact leaf Expert selections, while native Calendar connection/resource authority is stored inside Day. Checkpoint 01 fixes only the ownership half of that architecture. Connections becomes the canonical owner of native Calendar connection identity, lifecycle, current resource scope, local configuration revision, `SourceAuthority`, and current trusted native subject identity. Day is reduced to imported Calendar domain data, mirror freshness/status, and projection state.

This is an ownership cutover, not a compatibility migration. Internal development state is disposable. Establish one Connections-owned source record, move every in-scope caller to it, separate Day mirror concurrency from connection CAS, then delete the old Day authority path in this checkpoint.

Line numbers below are planning-base anchors on `9847c4ff`. The execution agent must re-resolve symbols on the actual start HEAD before editing.

## Exit state

Checkpoint 01 is complete only when:

1. `floe-connections` owns one canonical durable native Calendar source record.
2. That record owns Person, connector, connection, execution owner, lifecycle, current resources, resource mode, Connections CAS revision, `SourceAuthority`, and current native subject fingerprint where applicable.
3. Resource membership or native subject identity changes advance `SourceAuthority` exactly once at the Connections boundary.
4. Label-only changes may update configuration revision but do not advance `SourceAuthority`.
5. Calendar sync freshness, event imports, transient provider failures, and mirror writes cannot advance `SourceAuthority` or the Connections configuration revision.
6. Stable source identity cannot be rewritten in place. Account/connector/execution-owner replacement establishes a new `ConnectionId`.
7. Callers submit an expected Connections revision for CAS; callers do not manufacture the next local revision.
8. A producer/server revision, if retained, is explicitly distinct from the local Connections CAS revision.
9. Day owns no connection lifecycle, resource mode, authoritative Calendar selection, source configuration revision, `SourceAuthority`, execution owner, or native subject fingerprint.
10. Day uses a separate mirror revision only for Day persistence concurrency where needed.
11. App, Context, Access admission, Actions, source candidate discovery, review snapshots, interaction refresh, and connector overview read current native Calendar authority from Connections rather than extracting it from `CalendarMirror`.
12. Calendar source setup/disconnect/inventory mutation no longer travels through `DayMutation` or `DayService`.
13. Flutter source configuration uses a Connections-owned gateway/model. Day gateway remains a mirror/sync boundary.
14. Current trusted native subject identity is stored with the Connections source record, not in `CalendarGrantPolicy`.
15. `CalendarGrantPolicy` may remain temporarily for `ConsumerPolicyAuthority` until checkpoint 05, but no longer owns or persists `reviewed_native_subject_fingerprint`.
16. `GrantSourceBinding.source_authority` remains until checkpoint 02; no new compatibility field or duplicate authority is added.
17. Calendar grant leaf resources remain until checkpoint 02/03.
18. Calendar Expert candidates remain leaf-scoped until checkpoint 03; 01 changes their owner/input type only.
19. Remote Calendar special authorization remains until checkpoint 04.
20. `ConsumerPolicyAuthority` remains until checkpoint 05.
21. Final `ConnectionObserve` product-wire simplification remains checkpoint 07 work.
22. Old Day authority types, setters, persistence meaning, DTO variants, tests, and forwarding paths are deleted or rewritten.
23. `docs/architecture/modules.md` and `docs/architecture/authority-recovery.md` describe the implemented owner split.
24. Required tests, boundary checks, residual searches, and deletion gates pass.
25. README records 01 Complete and 02 remains Not started.

The only intentional authority transition after 01 is the already-planned `GrantSourceBinding.source_authority` field. It is removed by 02. Do not create another transition wrapper.

## Final ownership model

### Connections owns source truth

Target semantic record:

~~~
SourceConnection
  person_id
  connector_id
  connection_id
  execution_owner_id
  lifecycle/state
  revision                    # local Connections configuration CAS
  source_authority
  resource_mode               # selected vs provider-current/all semantics
  resources[]                 # sorted unique current allowed resources
  native_subject_fingerprint  # native sources only
~~~

Target resource:

~~~
ConnectionResource
  handle      # authoritative opaque resource identity
  label       # bounded display metadata only
~~~

Exact Rust names may change only when an existing type already represents identical semantics. Do not reuse `floe_day::CalendarConnection` or `CalendarMirror` as the Connections contract.

Prefer stable connector identity such as `calendar.event_kit`, `calendar.android`, or `calendar.fixture` rather than embedding provider-specific behavior into the generic Connections record. Derive `CalendarProvider` at the Calendar boundary.

Resource mode should have generic source semantics, e.g. `Selected` / `AllAvailable`, rather than making the generic owner depend on `CalendarScope`. Calendar setup/wire can map `CalendarScope` while that shared enum still exists elsewhere.

### Revision meanings are separated

After 01:

~~~
SourceConnection.revision
  local source-configuration CAS
  Connections advances it

SourceAuthority
  current source/resource/native-subject epoch
  Connections advances it only when source truth changes

CalendarMirror.revision
  Day mirror/import CAS
  unrelated to SourceAuthority
~~~

A client must never calculate `next_revision = current_revision + 1` and submit it as authority. Existing Flutter code doing this must be removed.

Existing server connector `connection_revision` is producer evidence and must not be reused as local `SourceConnection.revision`. If retained, name it separately, e.g. `observed_producer_revision`.

### Authority advancement matrix

| Change | Connections revision | SourceAuthority |
|---|---:|---:|
| create source | initialize | new |
| add/remove resource | advance | advance |
| source mode change changing effective source truth | advance | advance |
| label-only rename | advance if persisted | unchanged |
| native subject fingerprint change | advance | advance |
| identical fingerprint/configuration | unchanged | unchanged |
| disconnect/revoke | advance | advance |
| successful event sync | unchanged | unchanged |
| transient ProviderUnavailable | unchanged | unchanged |
| Day mirror freshness/error update | unchanged | unchanged |

`PermissionDenied` changes source authority only when it proves the recorded source/subject can no longer be read. That decision belongs to Connections. A transient fetch failure cannot rotate source authority.

`CalendarUnavailable` removes a resource only after authoritative inventory reconciliation proves membership changed. One failed read is not itself authority to rewrite resource scope.

### Day owns only mirror/domain state

Target concept:

~~~
CalendarMirror
  mirror_revision
  source_connection_id        # provenance/reference only
  source/provider marker      # only when needed for event provenance
  source_statuses             # observed sync state, not resource authority
  last_success_at / last_range
  error / error_at
  events
~~~

`CalendarMirror` must not contain or reconstruct:

~~~
execution owner
disconnected source lifecycle
resource mode
authoritative resources
SourceConnection revision
SourceAuthority
native subject fingerprint
~~~

Day may receive exact current resource handles as one validated import input and use them to check batch completeness, but it must not persist them as a second authoritative source list.

## Planning-base source map

### Day authority currently

| File / lines | Current symbol | Problem |
|---|---|---|
| `crates/modules/day/src/domain/calendar.rs:77-80` | `CalendarSelection` | Source configuration lives in Day. |
| `crates/modules/day/src/domain/calendar.rs:83-103` | `CalendarConnection` | Identity, execution owner, lifecycle, scope, resources, revision, `SourceAuthority` and sync status are one type. |
| `crates/modules/day/src/domain/calendar.rs:115-118` | `CalendarMirror` | Stores `CalendarConnection` wholesale alongside events. |
| `crates/modules/day/src/application/observations.rs:65-73` | `calendar_connection` | Source authority is read through Day mirror. |
| `observations.rs:76-122` | `select_calendar` / `select_calendars` | Day creates source configuration. |
| `observations.rs:125-236` | `set_calendar_scope` | Day owns source CAS and `SourceAuthority` evolution. |
| `observations.rs:238-260` | `disconnect_calendar` | Day changes lifecycle/resources/authority. |
| `observations.rs:262-329` | `discover_calendars` | Day expands resource authority. |
| `observations.rs:331+` | `record_calendar_failure` | Day can rotate `SourceAuthority`. |
| `observations.rs:413+` | `import_calendar_sources` | Sync mutation and source authority are mixed. |
| `observations.rs:520-545` | import completion | Sync increments the same revision used for source configuration. |
| `observations.rs:548+` | `calendar_at_revision` / `next_authority` | Day owns source CAS/epoch logic. |

### Day persistence

| File / lines | Current behavior |
|---|---|
| `crates/modules/day/src/ports/timeline_repository.rs:60+` | `calendar_mirror` / `put_calendar_mirror` store mixed state. |
| `crates/adapters/vault/src/repositories/day.rs:119-137` | TursoStore adapter exposes mixed mirror. |
| `crates/adapters/vault/src/engine.rs:37` | `calendar_mirrors` table initialization. |
| `engine.rs:56-93` | mixed mirror load/bounded load. |
| `engine.rs:95-147` | mixed mirror CAS write. |

### Connections lacks a source owner

Current package:

~~~
crates/modules/connections/src/api.rs
crates/modules/connections/src/application/*
crates/modules/connections/src/ports/remote_control.rs
crates/modules/connections/src/lib.rs
~~~

`application/connected_context.rs` defines `ConnectionState` and connector projection contracts. `api.rs::CalendarConnectionRef` is a remote catalog reference only. There is no durable source owner/service/repository. Do not reinterpret `CalendarConnectionRef` as `SourceConnection`.

### App forwards source authority to Day

`crates/app/src/calendar_facade.rs` anchors:

| Lines | Function | Disposition |
|---|---|---|
| 13-20 | `calendar_connection` | REWRITE to Connections. |
| 23-33 | `select_calendar` | DELETE/rehome. |
| 36-46 | `select_calendars` | DELETE/rehome. |
| 49-70 | `set_calendar_scope` | DELETE/rehome; expected revision only. |
| 73-81 | `disconnect_calendar` | Rehome to Connections. |
| 84-93 | `discover_calendars` | Rehome inventory reconciliation. |
| 96-108 | `record_calendar_failure` | Split source authority from mirror health. |
| 123+ | `import_calendar_sources` | Keep Day import after source validation. |

### Source configuration is a Day command

`crates/app/src/day_services.rs`:

- `DayMutation::DisconnectCalendar` line 25.
- `DayMutation::SetCalendarScope` line 28.
- `DayMutation::DiscoverCalendars` line 35.
- `DayMutation::ImportCalendarSources` line 39.
- `DayMutation::CalendarFailed` line 51.
- dispatch at lines 187-239.

`crates/bindings/protocol/src/dto/day_mutation.rs` repeats the same source configuration variants.

`crates/bindings/ffi/src/day_wire.rs:16-50` converts them.

Source setup/disconnect/inventory reconciliation must leave the Day wire in 01.

### Flutter currently makes Day authoritative

Relevant anchors:

- `apps/client/lib/features/day/domain/day_models.dart:224+`: `CalendarConnection` mixes source authority with sync state.
- `native_day_gateway.dart:130-158`: `selectCalendars` computes `(current.calendar?.revision ?? 0) + 1` client-side.
- `native_day_gateway.dart:161-187`: `bindCalendarConnection` uses producer `connectionRevision` as local revision.
- `native_day_gateway.dart:200+`: discovery mutates source scope through Day.
- `native_day_gateway.dart:325+`: disconnect through Day.
- `connector_screen.dart:104`: device source comes from Day snapshot.
- `connector_screen.dart:270-335`: server Calendar binding goes through `CalendarGateway`/Day.
- `connector_screen.dart:818+`: Use with Floe reads source authority/resources from Day `CalendarConnection`.
- `personal_day_screen.dart:206,281,369,379,436`: `snapshot.calendar` is used as source state.
- `calendar_action_controller.dart:42+`: action eligibility uses Day connection revision/resources.

### Context, Access, Actions read Day authority

`crates/modules/context/src/application/native_calendar.rs`:
- line 77 `CalendarConnectionReader` returns Day `CalendarConnection`.
- line 108 `admit_current_native_calendar_read`.
- line 227 `evidence`.
- line 240 `connection_calendar_ids`.
- line 301 `preview_native_calendar_subject`.

`crates/modules/context/src/application/source_candidates.rs`:
- `SourceCandidateRequest` holds `CalendarConnection`.
- `calendar.timeline` branch line 150.
- leaf loop line 162.

`crates/modules/context/src/application/observations.rs:175` validates publication against Day `CalendarConnection`.

`crates/app/src/local_context.rs::LocalContextHost::execute` accepts `Option<&floe_day::CalendarConnection>`.

`crates/app/src/vault_host/calendar_access.rs`:
- line 43 `CoreCalendarConnections`.
- line 48 reader returns Day connection.
- `VaultNativeCalendarGrants` takes Day connection.
- line 598 `usable_native_connection`.
- line 620 `native_grant_source`.

`crates/modules/actions/src/ports/mod.rs` imports Day `CalendarConnection` and `ActionRepository` exposes `calendar_connection`.

`crates/adapters/vault/src/repositories/actions.rs:202-208` reconstructs it from `mirror.connection`.

`crates/modules/actions/src/application/expert.rs:452` and around 522 consume it.

### Native subject is currently grant-policy state

`crates/adapters/vault/src/vault/calendar_grant_policy.rs`:
- `CalendarGrantPolicy` line 21.
- `reviewed_native_subject_fingerprint` line 26.
- policy table has a fingerprint column.
- `evolve_calendar_consumer_policy` includes fingerprint in policy epoch evolution.

`crates/adapters/vault/src/vault/calendar_grants.rs`:
- `CalendarGrantAdmission` includes reviewed fingerprint.
- `authorize_current_native_calendar_grant` compares current probe to `CalendarGrantPolicy` fingerprint.
- `review_native_calendar_grant` writes fingerprint to `CalendarGrantPolicy`.

After 01 current subject identity comes from Connections.

## 01-A — baseline and inventory

Before edits:

~~~
git fetch origin
git status --short --branch
git rev-parse HEAD
git rev-parse origin/main
git log -1 --oneline
python3 tools/architecture/check_boundaries.py
~~~

Read:

~~~
AGENTS.md
.agents/skills/architecture-change/SKILL.md
.agents/skills/code-change-verification/SKILL.md
docs/development/plans/connection-observe-authority/README.md
docs/development/plans/connection-observe-authority/01-connection-resource-authority.md
docs/architecture/invariants.md
docs/architecture/modules.md
docs/architecture/authority-recovery.md
~~~

Capture pre-edit residual inventory:

~~~
rg -n "floe_day::CalendarConnection|CalendarConnection" crates apps/client
rg -n "CalendarSelection|CalendarScope" crates/modules/day crates/app apps/client
rg -n "set_calendar_scope|select_calendar|select_calendars|disconnect_calendar|discover_calendars" crates apps/client
rg -n "calendar_mirror|put_calendar_mirror" crates
rg -n "reviewed_native_subject_fingerprint|calendar_grant_policy" crates
rg -n "source_authority" crates/modules/day crates/app crates/modules/context crates/modules/actions
rg -n "DayMutation::(DisconnectCalendar|SetCalendarScope|DiscoverCalendars)" crates
~~~

Classify every authority-bearing result before editing.

## 01-B — establish the Connections source contract

Primary files:

~~~
crates/modules/connections/src/lib.rs
crates/modules/connections/src/application/mod.rs
crates/modules/connections/src/ports/mod.rs
~~~

Recommended new files:

~~~
crates/modules/connections/src/source.rs
crates/modules/connections/src/application/source_connections.rs
crates/modules/connections/src/ports/source_repository.rs
~~~

Names may follow local conventions, but ownership may not.

### SourceConnection validation

Constructor/restore validates:

- `PersonId`.
- `ConnectorId` / `ConnectionId` / `ExecutionOwnerId`.
- revision > 0.
- valid `SourceAuthority`.
- sorted unique valid resource handles.
- bounded labels.
- valid resource mode.
- serving native source has a valid subject fingerprint when the connector requires it.
- disconnected/revoked state is not serving.
- no credentials/tokens/provider payload.

Optional fingerprint is allowed only because non-native sources genuinely do not use it.

### Owner mutations

Provide narrow owner operations for establish/create, replace resources, reconcile inventory, update native subject, disconnect/revoke, and read current.

All mutations on existing state take expected revision. Connections computes new revision and `SourceAuthority`.

Do not expose a broad replace-arbitrary-SourceConnection API.

### SourceAuthority rules

Centralize comparisons in Connections:
- resource handles determine source scope.
- labels do not.
- native fingerprint does.
- stable identity mismatch is not an update.
- lifecycle removal/disconnect does.
- identical state is no-op.

Storage/adapters do not decide epoch advancement.

## 01-C — add Connections-owned durable persistence

Add:

~~~
crates/adapters/vault/src/repositories/connections.rs
~~~

Export from `repositories/mod.rs` and initialize from `engine.rs`.

Recommended shape:

~~~
source_connections
  connection_id PRIMARY KEY
  person_id
  connector_id
  revision
  payload
~~~

Add only real uniqueness/index rules. Indexed identity/revision must agree with payload.

Repository supports exact load, current load by Person/connector where needed, create, and CAS update.

CAS:
1. begin immediate transaction.
2. read exact row.
3. validate Person/connection/expected revision.
4. domain service has produced valid next state.
5. update under expected revision/payload fence.
6. require one row.
7. commit/rollback.

Tests: reopen, stale CAS, concurrent conflict, malformed payload, cross-Person denial, indexed/payload mismatch.

No dual write, old decoder, lazy migration, or CalendarMirror fallback.

## 01-D — add App Connections source commands

`crates/app/src/connection_services.rs` is currently read-only. Add a typed source command surface owned by Connections.

Allowed inputs:
- connector/source configuration intent.
- stable connection ID when binding a known source.
- expected local Connections revision.
- resource mode/resources.
- observed producer revision under a distinct field only if required.

Forbidden:
- PersonId from payload.
- arbitrary native device/execution owner.
- caller-computed next local revision.
- SourceAuthority.
- consumers/purpose/processing.
- ConsumerPolicyAuthority.
- credentials/tokens.

Person/device derive from `CallerContext`.

For server Calendar binding, producer `connection_revision` stays producer evidence; it must not become local `SourceConnection.revision`.

Keep `connections.overview` read-only. Source mutation gets a separate typed command.

## 01-E — split Day mirror from source state

Modify:

~~~
crates/modules/day/src/domain/calendar.rs
crates/modules/day/src/application/observations.rs
crates/modules/day/src/ports/timeline_repository.rs
crates/adapters/vault/src/repositories/day.rs
crates/adapters/vault/src/engine.rs
~~~

Delete Day ownership of:
- `CalendarConnection` authority type.
- source execution owner.
- disconnected lifecycle.
- `CalendarScope` / source resource mode.
- authoritative resources.
- source configuration revision.
- `SourceAuthority`.

If `CalendarSelection` has no Day-only use, delete it.

Add Day-only `mirror_revision` only if CAS is necessary.

Import APIs currently taking `expected_revision` must use Day mirror revision after cutover, never `SourceConnection.revision`.

### Batch completeness

Replace comparison against `mirror.connection.calendars` with:

~~~
App reloads SourceConnection
  -> current resources
  -> passes exact handles as validation input to Day import
  -> Day checks one batch per expected handle
  -> Day persists events/statuses only
~~~

Status keys are observed state, not permission scope.

### Failure handling

- `ProviderUnavailable` -> Day status only.
- transient `CalendarUnavailable` -> Day status unless inventory proves removal.
- permission loss invalidating source -> Connections mutation/SourceAuthority.
- Day never calls `SourceAuthority::advance`.

## 01-F — migrate Context and connector projection

Modify:

~~~
crates/modules/context/src/application/native_calendar.rs
crates/modules/context/src/application/native_calendar_view.rs
crates/modules/context/src/application/source_candidates.rs
crates/modules/context/src/application/observations.rs
crates/modules/context/src/application/calendar_connector.rs
~~~

Replace `CalendarConnectionReader` returning Day type with a Connections-owned source reader/value.

Native read/preview gets provider, execution owner, resources, source revision, `SourceAuthority`, subject expectation from `SourceConnection`.

Preserve:
- generation/double-read fence.
- exact resource validation.
- cancellation/deadline.
- source re-read after I/O.
- Access admission.

Continuity compares current Connections identity/revision/authority, not Day mirror equality.

Important boundary: source candidates remain leaf-scoped until 03. In 01 the loop uses `SourceConnection.resources`; do not implement one `calendar.timeline:<connection>` candidate yet.

`publish_calendar_observation` validates against Connections source, not Day mirror.

Change `project_calendar_connector` to compose:

~~~
SourceConnection -> identity/lifecycle/resources
CalendarMirror   -> freshness/failures/items
~~~

A missing/stale mirror can change projected health but cannot mutate source state.

## 01-G — migrate App, Access, review, Experts, and Actions

### App Calendar facade

Rewrite `calendar_facade.rs`: source reads/mutations -> Connections; imports -> Day.

Remove forwarding-only Day source helpers after caller cutover.

### vault_host/calendar_access.rs

Migrate `CoreCalendarConnections`, reader implementation, `VaultNativeCalendarGrants` input, `usable_native_connection`, `native_grant_source`, and overview source state to Connections.

`GrantSourceBinding.source_authority` remains temporarily, populated from Connections until 02.

Grant resources remain leaf IDs until 02/03.

### review_snapshot / interaction_owners

Replace Day connection lookups with Connections source lookup and keep current drift checks.

Do not remove per-leaf review members, target-derived policy, or `ConsumerPolicyAuthority`; those are 03/05.

### expert_binding_settings / expert_host

Move input/fixtures to Connections state while preserving leaf candidate semantics.

### Actions

Delete `ActionRepository.calendar_connection` proxy that extracts `mirror.connection`.

Give Actions a separate Connections source reader/port. Do not keep source state on `ActionRepository` just because `TursoStore` implements both repositories.

`validate_context_calendar_source` takes `SourceConnection`.

Preserve:
- Read != Act.
- exact approved destination.
- binding/source/grant fences.
- durable pre-dispatch intent.
- idempotency.
- uncertain-result reconciliation.
- no blind retry.

Native Calendar provider allowed IDs come from Connections, not Day mirror.

## 01-H — move native subject identity to Connections

For device-native Calendar:

1. source config establishes current resource handles.
2. App probes exact native subject.
3. existing double-read/generation checks return trusted fingerprint.
4. App reloads SourceConnection.
5. Connections CAS stores fingerprint.
6. changed fingerprint advances `SourceAuthority` once.
7. identical fingerprint does not.
8. grant review/admission reloads Connections state.

A native source without trusted fingerprint is not fully serving for Observe reads.

Until 07, client `expected_native_subject_fingerprint` may remain compare-only review evidence. Backend compares it with current Connections fingerprint; caller does not define stored source identity.

### Narrow CalendarGrantPolicy

Modify:

~~~
crates/adapters/vault/src/vault/calendar_grant_policy.rs
crates/adapters/vault/src/vault/calendar_grants.rs
~~~

Remove:
- `CalendarGrantPolicy.reviewed_native_subject_fingerprint`.
- fingerprint column in fresh schema.
- decode/validation for it.
- fingerprint as `ConsumerPolicyAuthority` evolution input.
- `CalendarGrantAdmission` reviewed fingerprint if caller-zero.

Keep:
- grant/person identity.
- `ConsumerPolicyAuthority`.
- table/schema until 05.

No old policy decoder.

`authorize_current_native_calendar_grant` compares source/provider stamp fingerprint to current Connections fingerprint, then independently validates grant/policy state.

## 01-I — move source configuration wire and Flutter to Connections

This is source ownership wire work, not checkpoint 07 Observe cleanup.

### Remove source config from DayMutation

Delete:
- `DisconnectCalendar`.
- `SetCalendarScope`.
- `DiscoverCalendars`.

Files:

~~~
crates/bindings/protocol/src/dto/day_mutation.rs
crates/app/src/day_services.rs
crates/bindings/ffi/src/day_wire.rs
~~~

`CalendarFailed` must no longer have `SourceAuthority` semantics. Keep only Day sync failure meaning if still needed.

`ImportCalendar` / `ImportCalendarSources` stay Day operations using mirror revision.

### Add typed Connections source mutation

Add the smallest Connections-owned App/protocol route for configure/reconcile/disconnect.

Validate identity through AppHost/CallerContext. Do not put SourceAuthority, credentials, grant policy, or caller-computed next revision on wire.

Same-snapshot App/protocol/FFI/Flutter cutover; no old decoder.

### Split Flutter gateway/model

Target:

~~~
Connections source gateway
  inspect current source
  configure/select resources
  bind/reconcile source
  disconnect source
  reconcile inventory

Day sync gateway
  load Day
  sync/import events
  update derived mirror status
~~~

Do not retain forwarding methods only for compatibility.

Move authority model out of `features/day/domain`. Day client model keeps mirror/status only.

Update `ConnectorScreen`, action UI/controller, and `PersonalDayScreen` to consume Connections source state.

Checkpoint 07 still owns final generic `ConnectionObserve`, selected/granted resource cleanup, and Calendar Access DTO elimination. Do not preempt it unless a symbol becomes caller-zero.

## 01-J — delete old Day authority surface

After cutover delete/rewrite:

~~~
floe_day::CalendarConnection
DayService::calendar_connection
DayService::select_calendar
DayService::select_calendars
DayService::set_calendar_scope
DayService::disconnect_calendar
DayService::discover_calendars
Day source next_authority helper
App forwarding helpers
DayMutation source configuration variants
Day authority CalendarConnectionDto
calendar_connection_to_dto / calendar_connection_from_dto
Flutter Day CalendarConnection authority model
tests asserting Day owns source revision/scope/authority
~~~

No deprecated aliases/re-exports.

## Test migration matrix

### floe-connections owner tests

Add tests for:

1. create source -> valid revision/authority.
2. identical configure -> no change.
3. add resource -> revision + authority once.
4. remove resource -> revision + authority once.
5. reorder-only input -> no authority change.
6. label-only rename -> no authority change.
7. subject change -> authority once.
8. identical subject -> no authority change.
9. disconnect/revoke -> authority advance/non-serving.
10. transient health cannot mutate source.
11. stable identity replacement rejected/new ConnectionId required.
12. stale expected revision conflict.
13. foreign Person/device rejected.
14. sorted unique resources.
15. corrupt authority/revision fails closed.

### Connections repository

Test persist/reopen, stale CAS, concurrent conflict, malformed payload, indexed/payload mismatch, and no CalendarMirror fallback.

### Day

Move authority/scope/disconnect/discovery tests out of:
- `crates/modules/day/tests/calendar.rs`.
- `crates/modules/day/tests/calendar_sources.rs`.

Retain/rewrite event reconciliation, range, freshness, partial batch, mirror CAS tests.

Add integration proof that successful sync, partial sync, and `ProviderUnavailable` do not alter SourceConnection revision/authority.

### App

Rewrite `crates/app/tests/connected_calendar.rs`:
- configure through Connections.
- import Day separately.
- reopen both states.
- connector snapshot composes source + mirror.
- resource edit changes source authority.
- sync changes freshness only.
- disconnect changes source lifecycle without mirror becoming authority.
- new source identity does not reuse authority.

### Context

Migrate native calendar fixture to `SourceConnection`.

Retain >128 resources, foreign identity denial, source continuity, post-I/O re-read, subject/generation fences.

Keep leaf candidate behavior test until 03.

### Vault/Access

Prove:
- native admission compares source stamp with Connections fingerprint.
- policy row stores no fingerprint.
- subject change changes source authority.
- `ConsumerPolicyAuthority` remains until 05.
- `GrantSourceBinding` still carries authority until 02.

### Actions

Prove:
- source read comes from Connections.
- old mirror cannot authorize removed resource.
- stale source blocks publication/dispatch.
- durable intent and uncertainty recovery unchanged.

### Protocol/Flutter

Prove:
- source configure sends expected revision, not next revision.
- source command sends no SourceAuthority.
- removed Day source variants are rejected.
- Connections projection owns resources/revision.
- Day response has mirror/status meaning only.
- ConnectorScreen uses Connections state.
- Day sync cannot change source revision.
- Use with Floe still works with source authority outside Day.
- no old Day source decoder.

## Residual and deletion audit

After cutover:

~~~
rg -n "floe_day::CalendarConnection" crates apps
rg -n "\bCalendarConnection\b" crates/modules/day crates/app crates/modules/context crates/modules/actions apps/client
rg -n "set_calendar_scope|select_calendar|select_calendars|disconnect_calendar|discover_calendars" crates apps/client
rg -n "DayMutation::(DisconnectCalendar|SetCalendarScope|DiscoverCalendars)" crates
rg -n "CalendarConnectionDto|calendar_connection_to_dto|calendar_connection_from_dto" crates
rg -n "mirror\.connection|calendar_mirror\(.*\).*connection" crates
rg -n "reviewed_native_subject_fingerprint" crates
rg -n "source_authority" crates/modules/day
rg -n "calendar\.revision|snapshot\.calendar|current\.calendar" apps/client/lib
~~~

Required:
- no source authority mutation in Day.
- no source reconstruction from mirror.
- no Day source setup/disconnect.
- no client-computed next local source revision.
- no grant-policy-owned subject fingerprint.
- no duplicate durable source resource list in Day.
- no old Day source decoder.

Allowed residuals: Connections source owner, shared contracts retained for 02, leaf Expert selection retained for 03, remote Calendar retained for 04, `ConsumerPolicyAuthority` retained for 05, Observe product shape retained for 07, provider/native exact Calendar IDs, and Day event provenance.

Record every non-obvious residual.

## Explicit checkpoint boundaries

Do NOT in 01:

- remove `GrantSourceBinding.source_authority` — 02.
- convert DataAccessGrant leaf resources to logical View — 02/03.
- convert leaf Calendar Expert candidates to one connection/View candidate — 03.
- remove `selected_shipped_consumers` / consumer intersection — 03.
- delete remote Calendar stack — 04.
- delete `ConsumerPolicyAuthority` or Calendar policy table — 05.
- implement final generic `ConnectionObserve` / no-Calendar-ID Observe toggle — 07.
- weaken source subject/generation fencing, exact-recipient consent, Act authority, durable write intent, cancellation, or recovery.

A later-checkpoint symbol may be deleted early only if 01 makes it truly caller-zero without changing that later checkpoint's semantics. Record it.

## Suggested implementation slices

### 01-A Connections owner contract
- SourceConnection / ConnectionResource / mode.
- owner service/errors.
- repository port.
- unit tests.

Suggested commit: `connections: own source resource authority`

### 01-B durable source repository
- `source_connections` storage.
- Turso repository.
- CAS/reopen/corruption tests.

Suggested commit: `vault: persist connection source authority`

### 01-C internal caller cutover
- App source facade/service.
- Context.
- Access source input.
- review/interactions/Experts.
- Actions source reader.
- connector projection source+mirror composition.

Suggested commit: `app: route calendar authority through connections`

### 01-D native subject cutover
- fingerprint in SourceConnection.
- subject epoch rules.
- policy fingerprint field deletion.
- admission compares with Connections.

Suggested commit: `access: bind native subject to connection source`

### 01-E Day mirror split
- Day-only mirror.
- mirror revision.
- import/reconciliation.
- remove Day source setters/epoch logic.

Suggested commit: `day: reduce calendar state to mirror data`

### 01-F source wire/client cutover
- Connections source command.
- remove source config from DayMutation.
- FFI.
- Flutter source gateway/model/UI/actions migration.

Suggested commit: `client: move calendar source config to connections`

### 01-G deletion/docs/closure
- residual purge.
- architecture docs.
- evidence.
- README 01 Complete.

Suggested commit: `docs: complete connection observe checkpoint 01`

Slices may be combined when that produces a smaller final system. Do not add a wrapper merely to keep obsolete code compiling between commits.

## Verification

During iteration use normal incremental Cargo.

Minimum close gate:

~~~
cargo test -p floe-connections
cargo test -p floe-day
cargo test -p floe-vault
cargo test -p floe-context native_calendar
cargo test -p floe-context source_candidates
cargo test -p floe-actions
cargo test -p floe-app calendar
cargo test -p floe-protocol
cargo test -p floe-ffi
cargo check --workspace
python3 tools/architecture/check_boundaries.py
python3 tools/architecture/check_expert_extensibility.py
git diff --check
~~~

Flutter:

~~~
cd apps/client
flutter analyze
flutter test test/features/day
flutter test test/features/connections
flutter test test/features/actions
~~~

Persisted meaning and wire shape change, so use a fresh Floe development profile for manual/native verification. Do not add migration compatibility.

Apple-native smoke when available:
- configure/select EventKit resources.
- inspect Connections source.
- sync Day events.
- verify sync does not rotate SourceAuthority.
- edit resources and verify SourceAuthority changes.
- inspect native subject.
- verify fingerprint is stored at Connections.
- verify Use with Floe review no longer reads source authority from Day.

Report unavailable prerequisites as SKIPPED.

A final `CARGO_INCREMENTAL=0 cargo test --workspace --no-fail-fast` is recommended because 01 changes a workspace-wide owner/type boundary. If skipped, record why; checkpoint 09 remains the authoritative full repository verification.

## Architecture documentation convergence

In implementation update:

~~~
docs/architecture/modules.md
docs/architecture/authority-recovery.md
~~~

After 01 they must state:

- Connections owns native source identity, lifecycle, resources, local configuration revision, SourceAuthority, and current native subject identity.
- Day owns Calendar mirror/domain/freshness only.
- Day sync/failure cannot independently rotate SourceAuthority.
- Context/Access/Actions read current source through Connections.
- Registry binding remains configuration, not permission.
- standing Observe remains Access-owned.
- provider/native subject drift fails closed through Connections.
- `GrantSourceBinding` source epoch remains only as bounded pre-02 implementation detail where relevant.

Do not rewrite ADR history in 01. Overall durable rationale remains checkpoint 09 unless implementation reveals a new durable decision.

## Close procedure

Before marking complete:

1. rerun residual searches.
2. classify every surviving old-owner match.
3. verify no compatibility path/old decoder.
4. verify no Day source persistence fallback.
5. run verification.
6. append execution evidence here.
7. update README: 00 Complete, 01 Complete, 02 Not started.
8. commit closure.
9. stop; do not begin 02.

Evidence must record:
- date/start HEAD/origin-main.
- actual SourceConnection files/names.
- repository/table/CAS.
- revision/SourceAuthority/mirror revision rules.
- final Day mirror.
- final source command/wire.
- native subject path.
- migrated callers.
- deleted surfaces/tests.
- retained 02/03/04/05/07 transitions.
- residuals.
- docs.
- commands/outcomes.
- skipped native checks.
- commit SHA(s).
- clean worktree.

## Required agent report

Report:

1. start HEAD / origin-main / final HEAD.
2. SourceConnection final contract and owner path.
3. durable repository/table and CAS.
4. revision vs SourceAuthority vs mirror revision.
5. App/protocol/Flutter source configuration cutover.
6. Day mirror final shape and deleted authority surface.
7. Context/Access/Actions/Experts migration.
8. native subject ownership and CalendarGrantPolicy narrowing.
9. connector overview composition.
10. tests moved/rewritten/deleted.
11. residual audit and bounded later-checkpoint matches.
12. architecture docs updated.
13. commands and real outcomes.
14. skipped checks with reason.
15. checkpoint commit SHA(s).
16. clean worktree.
17. confirmation checkpoint 02 was not started.
