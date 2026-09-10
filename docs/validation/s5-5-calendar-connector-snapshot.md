# S5.5 Durable Calendar Connector Snapshot

> Date: 2026-09-10  
> Acceptance status: integration foundation only; S5.5-C1 remains pending

## Delivered boundary

- The existing durable Calendar mirror now projects into the common versioned
  `ConnectorSnapshot` contract instead of requiring a provider-specific status reader.
- Fixture and EventKit connections publish the same `calendar.timeline` View descriptor,
  device execution location, mirror retention, five-minute freshness, bounded item/byte
  limits and per-item provenance requirement.
- Calendar read and create are separate Observe and Act capability descriptors. The
  snapshot grants read only; write authority is never inferred from a connected source.
- Ready, pending, degraded, unavailable and disconnected states derive from persisted
  source status. Stale data and permission/provider failures remain typed, while a healthy
  source can keep a Schedule Situation runnable during another source's failure.
- Source handles hash provider-native calendar identifiers. Credential values, external
  IDs and calendar names do not enter the common snapshot.
- Calendar failure observation time is now persisted with aggregate and per-source state,
  cleared by a successful import and preserved across reopen.
- Added a read-only `connections` protocol/FFI operation that can inspect connector health
  without creating or unlocking the Agent vault. Unknown mutation or credential fields are
  rejected by the strict action contract.
- Added strict Dart projections and a shared Connections section in Data & privacy. It shows
  provider, execution location, available View count, last success, typed degraded reason and
  granted read scopes while explicitly separating action approval.

## Automated evidence

Run from the repository root:

```sh
cargo test -p floe-core --test connected_calendar
cargo test -p floe-agent --test connected_context
cargo test -p floe-protocol --test protocol
cargo test -p floe-ffi --lib vault_host::tests::connections_are_inspectable_without_initializing_a_vault
cargo check --workspace
cargo test --workspace
cd apps/client
flutter test test/features/agent/agent_connections_test.dart test/features/server/settings_screen_test.dart
flutter analyze
```

The Calendar integration suite covers four cases:

1. A reopened durable mirror produces a conforming provider-neutral snapshot.
2. A per-calendar permission failure reports degraded state while the healthy Calendar
   View keeps the explicit foreground Situation runnable.
3. Disconnect survives reopen, clears grants and Views, and reconnect starts pending at a
   new revision without restoring old evidence.
4. A projection at the five-minute freshness boundary becomes typed stale/unavailable
   evidence and cannot satisfy a required Situation View.

The protocol/FFI boundary covers read-only inspection without vault initialization. Three
focused Flutter tests cover strict parsing, escalation rejection and the degraded-source UI.

## Remaining gate

Only Calendar implements this production projection, and the snapshot is not yet consumed as
the discovery source for every adapter. Disconnect/reconnect controls still use the existing
Calendar path, and no signed live EventKit lifecycle was rerun for this checkpoint. Therefore
S5.5-C1 and all other S5.5 criteria remain pending.
