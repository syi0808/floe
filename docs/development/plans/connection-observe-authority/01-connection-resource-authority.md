# 01: Connection-owned source and resource authority

Prerequisite: 00 complete.

This checkpoint makes Connections the semantic owner of native Calendar connection identity, current resource scope and SourceAuthority. Day stops owning those facts.

## Exit state

At completion:

1. Connections owns the current native Calendar connection record and resource set;
2. resource/subject changes advance SourceAuthority at the Connections boundary;
3. Day owns only Calendar mirror/domain data and freshness, not connection permission state;
4. App/Context read native connection state through a Connections-owned port/service;
5. no second authoritative Calendar resource list remains;
6. old Day connection mutation APIs/types/tests are removed.

GrantSourceBinding still carries SourceAuthority until 02; that is the only explicitly bounded transitional fact and is removed next. Do not create an adapter preserving Day ownership.

## 01-A: establish the Connections contract

Primary package:

- crates/modules/connections

Add one owner model for source state. Prefer these semantics (names may only change if an existing current type already has identical meaning):

SourceConnection:
- person_id: PersonId
- connector_id: ConnectorId
- connection_id: ConnectionId
- execution_owner_id: ExecutionOwnerId
- state: ConnectionState
- revision: u64 for connection/configuration CAS
- source_authority: SourceAuthority
- resources: sorted unique Vec<ConnectionResource>
- native_subject_fingerprint: Option<String> only for device-native subjects that require it

ConnectionResource:
- handle: ResourceHandle
- label: bounded display metadata

Rules:

- resource handles are authoritative IDs; labels are not authority;
- stable identity fields cannot mutate in place;
- account/provider/execution-owner replacement creates a new ConnectionId;
- resource membership change advances SourceAuthority;
- native subject fingerprint change advances SourceAuthority;
- sync freshness/event changes do not advance SourceAuthority;
- revision advances for durable connection configuration mutation and remains distinct from source_authority;
- empty resources are allowed only for connection states where the connector is connected but exposes no usable source; Observe read then reports unavailable/review as appropriate rather than manufacturing a resource.

Add a Connections-owned repository port for the native source record. Implement it on TursoStore in crates/adapters/vault/src/repositories, in a Connections-specific repository module. Storage is physical; Connections owns validation/CAS semantics.

Because local dev data is disposable, replace the persisted Calendar connection representation directly. Do not write a decoder for the old Day-owned CalendarConnection payload.

## 01-B: split Day Calendar mirror from connection state

Current owners to change:

- crates/modules/day/src/domain/calendar.rs
- crates/modules/day/src/application/observations.rs
- crates/modules/day/src/ports/timeline_repository.rs
- crates/adapters/vault/src/repositories/day.rs
- crates/adapters/vault/src/engine.rs

Remove from Day-owned durable state:

- connection_id as authority owner state;
- device_id/execution owner;
- disconnected lifecycle;
- CalendarScope as connection permission;
- CalendarSelection authoritative list;
- connection revision;
- SourceAuthority.

Retain Day-owned facts needed for domain projection:

- imported events/records;
- per-source freshness/status if still required for Day UX;
- CalendarRange;
- provider event identity needed to reconcile mirror records;
- source/calendar handle on event provenance.

CalendarMirror must not be a second copy of SourceConnection. It may carry non-authoritative source handles needed to relate mirrored records to the owning connection.

Move/delete DayService connection mutation functions whose purpose is connection lifecycle/scope:

- select_calendar / select_calendars;
- set_calendar_scope;
- disconnect_calendar;
- discover_calendars where it expands authoritative connection resources.

Rehome those operations to Connections/App connection services. Keep Day import/reconciliation operations only.

## 01-C: cut App and Context callers

Inspect and migrate:

- crates/app/src/core.rs Calendar convenience methods;
- crates/app/src/vault_host/calendar_access.rs;
- crates/app/src/vault_host/review_snapshot.rs;
- crates/app/src/vault_host/interaction_owners.rs;
- crates/modules/context/src/application/native_calendar.rs;
- crates/modules/context/src/application/source_candidates.rs;
- native connection/settings composition;
- Flutter/native publication commands that currently call Day connection setters.

Use the Connections owner for current connection/resource state. Do not expose Vault storage details upward.

Where Context needs a current Calendar connection, define a narrow Connections-backed reader port/value; do not return a Day CalendarMirror and ask Context to extract authority from it.

## 01-D: native subject ownership

Today reviewed_native_subject_fingerprint is persisted beside a Calendar grant. Move the current trusted native subject identity to the Connections-owned source state.

Connection/setup flow:

1. inspect OS/provider subject;
2. establish/update SourceConnection resources and subject fingerprint;
3. advance SourceAuthority if either authoritative fact changed;
4. grant operations later consume that current source state but do not own the fingerprint.

At runtime the provider/native check must still prove that the device subject matches the Connections-owned fingerprint before source data is accepted. Do not weaken the double-check/generation fence.

The old grant-policy storage is not deleted until 05 because ConsumerPolicyAuthority still exists temporarily, but it must no longer be the semantic owner of native subject identity after this checkpoint.

## 01-E: storage deletion

Delete old Day-owned connection columns/payload meaning when the new repository is active. Fresh profiles are acceptable.

Do not:
- copy old records into both schemas;
- add optional old/new record fields;
- silently recreate source authority after a corrupt/open failure.

Tests may use an explicit fresh profile.

## Tests

Add/replace owner tests for:

1. resource add/remove advances SourceAuthority exactly once;
2. label-only/freshness-only update does not advance SourceAuthority;
3. stable identity change requires a new connection rather than mutation;
4. stale expected revision conflicts;
5. source record persists across reopen;
6. Day event sync does not mutate SourceAuthority;
7. Day mirror contains no connection permission authority;
8. native subject change advances SourceAuthority;
9. foreign Person/device cannot read/mutate another connection.

Delete tests whose sole assertion is that Day owns Calendar connection revision/scope.

## Residual audit

Search for:

~~~
floe_day::CalendarConnection
CalendarSelection
set_calendar_scope
select_calendars
disconnect_calendar
source_authority
calendar_mirror
~~~

Every authority-bearing Calendar connection match outside Connections/App composition/provider boundary must be justified or removed.

## Verification

At minimum:

~~~
cargo test -p floe-connections
cargo test -p floe-day
cargo test -p floe-vault
cargo test -p floe-context native_calendar
cargo test -p floe-app calendar
python3 tools/architecture/check_boundaries.py
git diff --check
~~~

Update docs/architecture/modules.md and authority-recovery.md in the same implementation change to state that Connections owns native source/resource authority and Day owns mirror/domain state.
